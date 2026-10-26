//! As operações longas: o Studio as pede ao servidor do Lace
//! ([`crate::server`]) e repassa à interface, por um `Channel` do Tauri, as
//! notificações que o servidor manda enquanto elas rodam e, no fim, o
//! resultado. Cancelar é o `$/cancel` do protocolo.
//!
//! As notificações (`phase`, `events`, `output_omitted`, `build`,
//! `install`) chegam do servidor já em lotes e com o teto de linhas da
//! entrega do core (`lace_core::delivery`): aqui elas só ganham o `type`
//! que a interface lê, e a linha do console de um `install` vai também como
//! `cli_output`.
//!
//! Só uma operação roda por vez (erro `busy`): duas no mesmo projeto
//! dividiriam o `.lace/Temp`, e uma instalação no meio de um fluxo trocaria
//! as ferramentas dele.

use std::panic::AssertUnwindSafe;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::error::{IpcError, IpcResult, codes};
use crate::server::Listener;
use crate::state::{ActiveJob, AppState};

/// O que a interface recebe pelo canal de uma operação, além das
/// notificações do servidor.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobMessage {
    /// A operação começou.
    Started {
        /// O identificador.
        job: u32,
        /// O comando equivalente.
        command: String,
    },
    /// Uma linha para o console da operação: a de cada passo de uma
    /// instalação ou atualização.
    CliOutput {
        /// `stdout` ou `stderr`.
        stream: &'static str,
        /// A linha.
        line: String,
    },
    /// A operação terminou e este é o resultado (um `FlowOutcome`, ou o de
    /// uma instalação ou atualização).
    Finished {
        /// O resultado.
        outcome: Value,
    },
    /// O Lace não conseguiu rodar a operação.
    Failed {
        /// O erro.
        error: IpcError,
    },
}

/// O canal de uma operação, para as mensagens do Studio e as notificações
/// do servidor, na ordem em que acontecem.
#[derive(Clone)]
pub struct Relay {
    channel: Channel<Value>,
}

impl Relay {
    /// O canal da interface.
    pub fn new(channel: Channel<Value>) -> Self {
        Relay { channel }
    }

    /// Uma mensagem do Studio.
    pub fn send(&self, message: JobMessage) {
        if let Ok(value) = serde_json::to_value(message) {
            let _ = self.channel.send(value);
        }
    }

    /// Uma notificação do servidor (`phase`, `events`, `output_omitted`,
    /// `build`, `install`), como veio, com o `type` que a interface lê. A
    /// linha do console de um `install` vai depois, como `cli_output`.
    pub fn notification(&self, method: &str, params: Value) {
        let (message, line) = message_of(method, params);
        if let Some(message) = message {
            let _ = self.channel.send(message);
        }
        if let Some(line) = line {
            self.send(JobMessage::CliOutput {
                stream: "stderr",
                line,
            });
        }
    }
}

/// A mensagem da interface para uma notificação do servidor, e a linha do
/// console que ela traz, quando é um `install` com linha.
fn message_of(method: &str, mut params: Value) -> (Option<Value>, Option<String>) {
    let Some(fields) = params.as_object_mut() else {
        return (None, None);
    };
    let line = if method == "install" {
        fields
            .remove("line")
            .and_then(|line| line.as_str().map(str::to_owned))
    } else {
        None
    };
    fields.insert("type".to_owned(), Value::String(method.to_owned()));
    (Some(params), line)
}

/// Um pedido ao servidor que roda como operação: o comando que o console
/// mostra, o método do protocolo e os parâmetros.
pub struct Operation {
    /// O comando equivalente (`lace-studio sim -p soma`, `lace install ...`).
    pub command: String,
    /// O método (`flow.run`, `learn.check`, `install.add`, `update.apply`).
    pub method: &'static str,
    /// Os parâmetros.
    pub params: Value,
}

/// Começa `operation` no servidor e devolve o identificador. O canal recebe
/// `started`, as notificações do pedido e, no fim, a mensagem que `finish`
/// faz da resposta. A vaga é liberada antes dessa última mensagem, para a
/// interface poder começar outra operação assim que a receber.
pub fn start(
    app: &AppHandle,
    operation: Operation,
    channel: Channel<Value>,
    finish: impl FnOnce(IpcResult<Value>) -> JobMessage + Send + 'static,
) -> IpcResult<u32> {
    let state = app.state::<AppState>();
    let settings = state.settings.get();
    let id = reserve(&state)?;
    let relay = Relay::new(channel);
    relay.send(JobMessage::Started {
        job: id,
        command: operation.command,
    });
    let worker = app.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("job-{id}"))
        .spawn(move || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let notifications = relay.clone();
                let listener: Listener = Box::new(move |method, params| {
                    notifications.notification(method, params);
                });
                let reply = worker.state::<AppState>().server.call_with(
                    &settings,
                    operation.method,
                    operation.params,
                    listener,
                    |request| attach(&worker, id, request),
                );
                finish(reply)
            }));
            let message = result.unwrap_or_else(|_| JobMessage::Failed {
                error: IpcError::new(codes::INTERNAL, "The operation panicked; see the log"),
            });
            release(&worker, id);
            relay.send(message);
        });
    if let Err(error) = spawned {
        release(app, id);
        return Err(IpcError::new(
            codes::INTERNAL,
            format!("Could not start the worker thread: {error}"),
        ));
    }
    Ok(id)
}

/// Reserva a vaga da operação. Erro `busy` se já houver uma rodando.
pub fn reserve(state: &AppState) -> IpcResult<u32> {
    let mut job = state.job.lock().expect("job lock");
    if job.is_some() {
        return Err(IpcError::new(
            codes::BUSY,
            "Another operation is running; wait for it or cancel it",
        ));
    }
    let id = state.next_id();
    *job = Some(ActiveJob {
        id,
        request: None,
        cancelled: false,
    });
    Ok(id)
}

/// Libera a vaga, se ainda for desta operação.
pub fn release(app: &AppHandle, id: u32) {
    let state = app.state::<AppState>();
    let mut job = state.job.lock().expect("job lock");
    if job.as_ref().is_some_and(|j| j.id == id) {
        *job = None;
    }
}

/// O pedido da operação `id` saiu para o servidor: um cancelamento pedido
/// antes disso vai para ele agora.
fn attach(app: &AppHandle, id: u32, request: u64) {
    let state = app.state::<AppState>();
    let cancelled = {
        let mut job = state.job.lock().expect("job lock");
        match job.as_mut() {
            Some(active) if active.id == id => {
                active.request = Some(request);
                active.cancelled
            }
            _ => false,
        }
    };
    if cancelled {
        state.server.cancel(request);
    }
}

/// Começa um fluxo (build, check, sim, synth, esquemático, placa, exercício)
/// e devolve o identificador da operação. `request` é o `FlowRequest` do
/// protocolo. O progresso e o resultado chegam por `channel`, terminando
/// sempre com `finished` ou `failed`.
#[tauri::command]
pub async fn flow_start(app: AppHandle, request: Value, channel: Channel<Value>) -> IpcResult<u32> {
    let planned = crate::state::blocking(app.clone(), move |_, state| {
        let spf = state.spf()?;
        crate::flows::plan(&state.server, &state.settings.get(), &spf, request)
    })
    .await?;
    let (operation, finisher) = planned;
    start(&app, operation, channel, move |reply| {
        finisher.finish(reply)
    })
}

/// Cancela a operação que está rodando. Com `job`, só se for ela. O
/// resultado chega pelo canal da operação, com `status: cancelled`.
#[tauri::command]
pub fn flow_cancel(app: AppHandle, job: Option<u32>) -> bool {
    let state = app.state::<AppState>();
    let request = {
        let mut guard = state.job.lock().expect("job lock");
        match guard.as_mut() {
            Some(active) if job.is_none_or(|id| id == active.id) => {
                active.cancelled = true;
                active.request
            }
            _ => return false,
        }
    };
    if let Some(request) = request {
        state.server.cancel(request);
    }
    true
}

/// O identificador da operação rodando, se houver.
#[tauri::command]
pub fn flow_running(app: AppHandle) -> Option<u32> {
    let state = app.state::<AppState>();
    let guard = state.job.lock().expect("job lock");
    guard.as_ref().map(|j| j.id)
}

/// Cancela a operação rodando e espera até `wait` ela soltar a vaga. Usado
/// ao fechar o Studio: sem isso, as ferramentas continuariam rodando no
/// servidor até ele ser fechado.
pub fn cancel_and_wait(app: &AppHandle, wait: Duration) {
    if !flow_cancel(app.clone(), None) {
        return;
    }
    let until = Instant::now() + wait;
    while Instant::now() < until && flow_running(app.clone()).is_some() {
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn notifications_keep_the_names_the_interface_reads() {
        let (message, line) = message_of(
            "output_omitted",
            json!({ "step": "simulate", "tool": "vvp", "lines": 3 }),
        );
        let message = message.unwrap();
        assert_eq!(message["type"], "output_omitted");
        assert_eq!(message["step"], "simulate");
        assert_eq!(message["lines"], 3);
        assert_eq!(line, None);
        let (message, _) = message_of("phase", json!({ "phase": "build" }));
        assert_eq!(
            message.unwrap(),
            json!({ "type": "phase", "phase": "build" })
        );
        // A linha do console de um install sai da mensagem e vira cli_output.
        let (message, line) = message_of(
            "install",
            json!({ "event": { "event": "verifying" }, "line": "  Verifying the executables" }),
        );
        let message = message.unwrap();
        assert_eq!(message["type"], "install");
        assert!(message.get("line").is_none());
        assert_eq!(line.as_deref(), Some("  Verifying the executables"));
        let value = serde_json::to_value(JobMessage::Started {
            job: 7,
            command: "lace-studio build".into(),
        })
        .unwrap();
        assert_eq!(value["type"], "started");
        assert_eq!(value["job"], 7);
    }
}
