//! Comandos do bundle: o equivalente do `lace tools`, a conferência dos
//! hashes, e instalar componentes e atualizar o Lace.
//!
//! Instalar e atualizar são a biblioteca do instalador (`lace_installer`),
//! chamada em processo, na thread da vaga única das operações: a mesma
//! regra da CLI e do `lace serve`, sem rodar o `lace` como filho. O
//! progresso chega à interface como `install` (o evento do instalador) e,
//! para o console, como `cli_output` com a linha que a CLI escreveria; o
//! resultado final tem a forma de sempre (`{flow, succeeded, cancelled,
//! exit_code, result}`), com o código de saída que a CLI daria.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{CancelToken, Toolchain, component};
use lace_installer::add::{self, Payload};
use lace_installer::install::Event;
use lace_installer::payload::Index;
use lace_installer::release::{Bar, Network, Remote};
use lace_installer::update::{
    self as updater, AppliedUpdate, ChildOutput, Hooks, Prepared, Reopen, RunOptions, Silent,
    UpdateReport,
};
use serde_json::{Value, json};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::error::{IpcError, IpcResult, codes};
use crate::jobs::{self, DirectSender, JobMessage};
use crate::state::{AppState, blocking};
use crate::toolchain::{self, ToolchainInfo};

/// Quanto esperar cada consulta de versão do `update.check`.
const CHECK_TIMEOUT: Duration = Duration::from_secs(90);

/// O bundle, as ferramentas e o compilador do Verilator.
#[tauri::command]
pub async fn toolchain_info(app: AppHandle) -> IpcResult<ToolchainInfo> {
    blocking(app, |_, state| Ok(toolchain::info(&state.settings.get()))).await
}

/// `lace update --check`: o Lace, o bundle e cada aplicativo comparados com
/// a última release e com o upstream. Precisa de rede; cada consulta tem
/// prazo. `prefix` vem preenchido quando o bundle é o de uma instalação do
/// instalador: é por ele que a interface decide o botão de atualizar.
#[tauri::command]
pub async fn lace_update_check(app: AppHandle) -> IpcResult<UpdateReport> {
    blocking(app, |_, state| {
        let toolchain = toolchain::require(&state.settings.get())?;
        let mut network = Network::from_env()?;
        network.timeout = Some(CHECK_TIMEOUT);
        let remote = Arc::new(Remote(network));
        let options = RunOptions {
            check: true,
            full: false,
            from: None,
            reopen: None,
            bar: Bar::Quiet,
            child_output: ChildOutput::Capture,
            reg: None,
        };
        Ok(updater::run(
            &remote,
            lace_installer::LACE_VERSION,
            Some(toolchain.manifest()),
            prefix_of(&toolchain),
            &options,
            &mut Silent,
            &mut |_| {},
            &CancelToken::new(),
        )?)
    })
    .await
}

/// `lace install <componentes>`: instala aplicativos do bundle na
/// instalação do Lace, sem reinstalar. Roda como operação (veja
/// [`start_installer_job`]); cancelar vale até o último pedaço entrar, e o
/// que entrou sai.
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
    let command = format!("lace install {} --json", components.join(" "));
    start_installer_job(
        &app,
        "install",
        command,
        channel,
        move |toolchain, cancel, on| {
            let prefix = prefix_of(toolchain)?;
            updater::recover_interrupted(&prefix)?;
            updater::clean_leftovers(prefix.as_std_path());
            let network = Network::from_env()?;
            let remote = Arc::new(Remote(network.clone()));
            let prepared = add::prepare(
                remote,
                &network,
                &prefix,
                Payload::Release {
                    version: lace_installer::LACE_VERSION,
                },
                cancel,
            )?;
            let index = prepared.index.clone();
            let report = add::add_components(
                &prepared,
                &prefix,
                &components,
                &mut |event| on(&event, Some(&index)),
                cancel,
            )?;
            Ok(serde_json::to_value(report).unwrap_or(Value::Null))
        },
    )
}

/// `lace update --yes`: troca a instalação pela da última release do Lace,
/// com o bundle e os mesmos componentes. Vai por componentes quando dá: só
/// os pedaços com arquivos que mudaram são baixados e trocados, também no
/// Windows, e o resultado diz o que mudou (`method: "components"`,
/// `changes`). Sem isso, no Linux e no macOS o instalador da release roda e
/// termina junto; no Windows, o assistente de instalação abre e termina
/// depois que o Studio sai (`action` no fim: `updated`, `wizard_opened` ou
/// `up_to_date`).
///
/// Quem confirma é a interface, antes de chamar. Cancelar vale até a troca
/// dos arquivos; durante a troca o pedido é ignorado, e uma troca
/// interrompida é desfeita na próxima operação.
///
/// Atualizado, a interface reinicia o Studio ([`crate::exit`]). Com o
/// assistente do Windows, ela fecha o Studio para ele trocar os arquivos, e
/// o assistente abre o Studio quando termina ([`Reopen::Studio`]).
#[tauri::command]
pub async fn lace_update(app: AppHandle, channel: Channel<JobMessage>) -> IpcResult<u32> {
    let command = if cfg!(windows) {
        "lace update --yes --json --reopen-studio"
    } else {
        "lace update --yes --json"
    };
    start_installer_job(
        &app,
        "update",
        command.to_owned(),
        channel,
        |toolchain, cancel, on| {
            let remote = Arc::new(Remote(Network::from_env()?));
            let reg = reg_exe();
            let options = RunOptions {
                check: false,
                full: false,
                from: None,
                reopen: cfg!(windows).then_some(Reopen::Studio),
                bar: Bar::Quiet,
                child_output: ChildOutput::Capture,
                reg: reg.as_deref(),
            };
            let index: Rc<RefCell<Option<Index>>> = Rc::new(RefCell::new(None));
            let mut hooks = Labels {
                index: Rc::clone(&index),
            };
            let report = updater::run(
                &remote,
                lace_installer::LACE_VERSION,
                Some(toolchain.manifest()),
                prefix_of(toolchain),
                &options,
                &mut hooks,
                &mut |event| on(&event, index.borrow().as_ref()),
                cancel,
            )?;
            Ok(serde_json::to_value(report).unwrap_or(Value::Null))
        },
    )
}

/// Guarda o índice da versão nova, para os rótulos do console.
struct Labels {
    index: Rc<RefCell<Option<Index>>>,
}

impl Hooks for Labels {
    fn plan(&mut self, prepared: &Prepared) {
        *self.index.borrow_mut() = Some(prepared.index.clone());
    }

    fn applied(&mut self, _prepared: &Prepared, applied: &AppliedUpdate, _prefix: &Utf8Path) {
        if let Some(warning) = &applied.registry_warning {
            tracing::info!("{warning}");
        }
    }
}

/// O que roda numa operação do instalador: recebe o bundle, o token de
/// cancelamento e para onde mandar cada evento (com o índice, para os
/// rótulos, quando já se sabe).
type InstallerWork = dyn FnOnce(
        &Toolchain,
        &CancelToken,
        &mut dyn FnMut(&Event, Option<&Index>),
    ) -> Result<Value, lace_installer::Error>
    + Send;

/// Roda uma operação do instalador como operação do Studio: reserva a vaga,
/// manda `started`, cada evento (`install`) com a linha dele para o console
/// (`cli_output`) e, no fim, `finished` com `{flow, succeeded, cancelled,
/// exit_code, result}`, onde `result` é o relatório, ou `{error}` quando
/// falhou, com o código de saída que a CLI daria (0, 2 ou 130).
fn start_installer_job(
    app: &AppHandle,
    flow: &'static str,
    command: String,
    channel: Channel<JobMessage>,
    work: impl FnOnce(
        &Toolchain,
        &CancelToken,
        &mut dyn FnMut(&Event, Option<&Index>),
    ) -> Result<Value, lace_installer::Error>
    + Send
    + 'static,
) -> IpcResult<u32> {
    let state = app.state::<AppState>();
    let toolchain = toolchain::require(&state.settings.get())?;
    let (id, cancel) = jobs::reserve(&state)?;
    let sender = Arc::new(DirectSender::new(channel));
    sender.send(JobMessage::Started { job: id, command });
    let worker_app = app.clone();
    let work: Box<InstallerWork> = Box::new(work);
    std::thread::Builder::new()
        .name(format!("installer-{id}"))
        .spawn(move || {
            let events = Arc::clone(&sender);
            let mut on = move |event: &Event, index: Option<&Index>| {
                events.send(JobMessage::Install {
                    event: serde_json::to_value(event).unwrap_or(Value::Null),
                });
                if let Some(line) = console_line(event, index) {
                    events.send(JobMessage::CliOutput {
                        stream: "stderr",
                        line,
                    });
                }
            };
            let result = work(&toolchain, &cancel, &mut on);
            jobs::release(&worker_app, id);
            let outcome = match result {
                Ok(result) => json!({
                    "flow": flow,
                    "succeeded": true,
                    "cancelled": false,
                    "exit_code": 0,
                    "result": result,
                }),
                Err(error) if error.is_cancelled() => json!({
                    "flow": flow,
                    "succeeded": false,
                    "cancelled": true,
                    "exit_code": 130,
                    "result": Value::Null,
                }),
                Err(error) => json!({
                    "flow": flow,
                    "succeeded": false,
                    "cancelled": false,
                    "exit_code": 2,
                    "result": { "error": { "code": error.code(), "message": error.to_string(), "hint": Value::Null } },
                }),
            };
            sender.send(JobMessage::Finished { outcome });
        })
        .map_err(|error| {
            jobs::release(app, id);
            IpcError::new(
                codes::INTERNAL,
                format!("Could not start the worker thread: {error}"),
            )
        })?;
    Ok(id)
}

/// A linha que a CLI escreveria no stderr para um evento do instalador, para
/// o console; `None` para o que é só barra de progresso.
fn console_line(event: &Event, index: Option<&Index>) -> Option<String> {
    let label = |name: &str| -> String {
        if name == updater::LACE {
            return "Lace".to_owned();
        }
        index
            .and_then(|i| i.component(name))
            .map_or_else(|| name.to_owned(), |c| c.label.clone())
    };
    Some(match event {
        Event::Chunk {
            index: at,
            count,
            components,
            download,
        } => {
            let what = if components.is_empty() {
                "Lace".to_owned()
            } else {
                components
                    .iter()
                    .map(|c| label(c))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            if *download > 0 {
                format!(
                    "  [{at}/{count}] {what} ({})",
                    lace_installer::mib(*download)
                )
            } else {
                format!("  [{at}/{count}] {what}")
            }
        }
        Event::Replacing { files } => format!("  Replacing {files} files"),
        Event::Verifying => "  Verifying the executables".to_owned(),
        Event::Fallback { reason } | Event::Warning { message: reason } => reason.clone(),
        Event::FallbackDownload { url, .. } => format!("Downloading {url}"),
        Event::WizardSelection {
            warning: Some(warning),
            ..
        } => format!("Warning: {warning}"),
        Event::InstallerOutput { line } => line.clone(),
        _ => return None,
    })
}

/// A pasta da instalação de que o bundle faz parte (`<instalação>/toolchain`
/// está nela), se é uma instalação do instalador: o Studio não está em
/// `bin/`, então chega por aqui, e não pelo próprio executável.
fn prefix_of(toolchain: &Toolchain) -> Result<Utf8PathBuf, lace_installer::Error> {
    let prefix = toolchain
        .root()
        .parent()
        .map(Utf8Path::to_owned)
        .ok_or_else(|| lace_installer::Error::NoInstallation {
            exe: toolchain.root().to_owned(),
        })?;
    lace_installer::installation::check(&prefix)?;
    Ok(prefix)
}

/// O `reg.exe` do Windows, pelo `SystemRoot`.
fn reg_exe() -> Option<Utf8PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    Some(
        std::env::var("SystemRoot")
            .map(|root| Utf8PathBuf::from(format!("{root}\\System32\\reg.exe")))
            .unwrap_or_else(|_| Utf8PathBuf::from("reg.exe")),
    )
}
