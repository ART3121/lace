//! Comandos do bundle: o equivalente do `lace tools`, a conferência dos
//! hashes e o que só a CLI faz (instalar componentes, procurar e instalar
//! atualização).
//!
//! Instalar e atualizar ficam na CLI (`lace install`, `lace update`), no
//! crate `lace-installer`, e não no Core. O Studio chama o `lace` da própria
//! instalação com `--json` em vez de copiar essa lógica (ADR 0001 do Lace:
//! interfaces em outra camada chamam `lace ... --json`).

use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use camino::Utf8Path;
use lace_core::{CancelToken, FileMismatch, component};
use serde_json::{Value, json};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::error::{IpcError, IpcResult, codes};
use crate::jobs::{self, DirectSender, JobMessage};
use crate::state::{AppState, blocking};
use crate::toolchain::{self, ToolchainInfo};

/// O bundle, as ferramentas e o compilador do Verilator.
#[tauri::command]
pub async fn toolchain_info(app: AppHandle) -> IpcResult<ToolchainInfo> {
    blocking(app, |_, state| Ok(toolchain::info(&state.settings.get()))).await
}

/// Confere o SHA-256 de cada executável do bundle (`lace tools --verify`).
/// Vazio: todos conferem.
#[tauri::command]
pub async fn toolchain_verify(app: AppHandle) -> IpcResult<Vec<FileMismatch>> {
    blocking(app, |_, state| {
        Ok(toolchain::require(&state.settings.get())?.verify()?)
    })
    .await
}

/// `lace update --check --json`: o Lace, o bundle e cada aplicativo
/// comparados com a última release e com o upstream. Precisa de rede.
#[tauri::command]
pub async fn lace_update_check(app: AppHandle) -> IpcResult<Value> {
    blocking(app, |_, state| {
        let cli = cli_path(state)?;
        let args = ["update", "--check", "--json"].map(String::from);
        let run = run_cli(
            &cli,
            &args,
            None,
            Some(Duration::from_secs(90)),
            Arc::new(|_, _| {}),
        )?;
        parse_json(&run)
    })
    .await
}

/// `lace install <componentes> --json`: instala aplicativos do bundle na
/// instalação do Lace, sem reinstalar. Roda como operação (veja
/// [`start_cli_job`]).
#[tauri::command]
pub async fn lace_install(
    app: AppHandle,
    components: Vec<String>,
    channel: Channel<JobMessage>,
) -> IpcResult<u32> {
    if components.is_empty() {
        return Err(IpcError::new(
            codes::INVALID_ARGUMENT,
            "No component chosen",
        ));
    }
    if let Some(unknown) = components
        .iter()
        .find(|c| !component::ALL.contains(&c.as_str()))
    {
        return Err(IpcError::new(
            codes::INVALID_ARGUMENT,
            format!("{unknown} is not a component of the bundle"),
        ));
    }
    let mut args = vec!["install".to_owned()];
    args.extend(components);
    args.push("--json".into());
    start_cli_job(&app, "install", args, true, channel)
}

/// `lace update --yes --json`: troca a instalação pela da última release do
/// Lace, com o bundle e os mesmos componentes. No Linux e no macOS, o
/// instalador da release roda e termina junto; no Windows, o assistente de
/// instalação abre e termina depois que o `lace` sai (`action` no fim:
/// `updated`, `wizard_opened` ou `up_to_date`).
///
/// O `--yes` responde a pergunta que a CLI faria no terminal: quem confirma
/// é a interface, antes de chamar. A operação não para no meio: cancelar
/// mataria o `lace` com o instalador, filho dele, trocando o `bin/` e o
/// `toolchain/`, e o instalador seguiria sozinho.
#[tauri::command]
pub async fn lace_update(app: AppHandle, channel: Channel<JobMessage>) -> IpcResult<u32> {
    let args = ["update", "--yes", "--json"].map(String::from).to_vec();
    start_cli_job(&app, "update", args, false, channel)
}

/// Roda a CLI como operação: reserva a vaga, manda o comando (`started`),
/// cada linha que ela escreve (`cli_output`) e, no fim, `finished` com
/// `{flow, succeeded, cancelled, exit_code, result}`, onde `result` é o JSON
/// dela (o relatório, ou `{error}` quando falhou). Sem `cancellable`, o
/// pedido de cancelar não chega ao processo.
fn start_cli_job(
    app: &AppHandle,
    flow: &'static str,
    args: Vec<String>,
    cancellable: bool,
    channel: Channel<JobMessage>,
) -> IpcResult<u32> {
    let state = app.state::<AppState>();
    let cli = cli_path(&state)?;
    let (id, cancel) = jobs::reserve(&state)?;
    let sender = Arc::new(DirectSender::new(channel));
    sender.send(JobMessage::Started {
        job: id,
        command: format!("lace {}", args.join(" ")),
    });
    let worker_app = app.clone();
    std::thread::spawn(move || {
        let lines = sender.clone();
        let on_line = Arc::new(move |stream: &'static str, line: String| {
            lines.send(JobMessage::CliOutput { stream, line });
        });
        let cancel = cancellable.then_some(&cancel);
        let result = run_cli(&cli, &args, cancel, None, on_line);
        jobs::release(&worker_app, id);
        let message = match result {
            Ok(run) => {
                let parsed = serde_json::from_str::<Value>(&run.stdout).unwrap_or(Value::Null);
                JobMessage::Finished {
                    outcome: json!({
                        "flow": flow,
                        "succeeded": run.code == Some(0),
                        "cancelled": run.cancelled,
                        "exit_code": run.code,
                        "result": parsed,
                    }),
                }
            }
            Err(error) => JobMessage::Failed { error },
        };
        sender.send(message);
    });
    Ok(id)
}

/// A CLI da instalação do bundle.
fn cli_path(state: &AppState) -> IpcResult<camino::Utf8PathBuf> {
    let toolchain = toolchain::require(&state.settings.get())?;
    toolchain::lace_cli(&toolchain).ok_or_else(|| {
        IpcError::new(
            codes::CLI,
            format!(
                "The lace command of the installation at {} was not found",
                toolchain.root()
            ),
        )
    })
}

/// O que uma execução da CLI deixou.
struct CliRun {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    cancelled: bool,
}

/// Roda a CLI, entrega cada linha do stderr (e do stdout, se não for JSON)
/// a `on_line` e espera ela terminar, o cancelamento ou o prazo.
fn run_cli(
    cli: &Utf8Path,
    args: &[String],
    cancel: Option<&CancelToken>,
    timeout: Option<Duration>,
    on_line: Arc<dyn Fn(&'static str, String) + Send + Sync>,
) -> IpcResult<CliRun> {
    let mut child = Command::new(cli)
        .args(args)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| IpcError::new(codes::CLI, format!("Could not run {cli}: {e}")))?;

    let stderr = child.stderr.take().expect("piped stderr");
    let stderr_lines = on_line.clone();
    let stderr_reader = std::thread::spawn(move || {
        let mut all = String::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            all.push_str(&line);
            all.push('\n');
            stderr_lines("stderr", line);
        }
        all
    });
    let mut stdout = child.stdout.take().expect("piped stdout");
    let stdout_reader = std::thread::spawn(move || {
        let mut all = String::new();
        let _ = stdout.read_to_string(&mut all);
        all
    });

    let started = Instant::now();
    let mut cancelled = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        let expired = timeout.is_some_and(|t| started.elapsed() > t);
        if cancel.is_some_and(CancelToken::is_cancelled) || expired {
            cancelled = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok(CliRun {
        code: status.and_then(|s| s.code()),
        stdout: stdout_reader.join().unwrap_or_default(),
        stderr: stderr_reader.join().unwrap_or_default(),
        cancelled,
    })
}

/// O JSON que a CLI escreveu, ou um erro com o fim do stderr.
fn parse_json(run: &CliRun) -> IpcResult<Value> {
    if run.cancelled {
        return Err(IpcError::new(codes::CLI, "The lace command took too long"));
    }
    serde_json::from_str(&run.stdout).map_err(|_| {
        let tail: Vec<&str> = run.stderr.lines().rev().take(5).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        IpcError::new(
            codes::CLI,
            format!(
                "lace exited with {:?} without JSON: {}",
                run.code,
                tail.join(" / ")
            ),
        )
    })
}
