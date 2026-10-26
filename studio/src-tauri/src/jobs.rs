//! As operações longas: rodam numa thread própria, mandam o progresso para
//! a interface por um `Channel` do Tauri e podem ser canceladas.
//!
//! É o desenho que a API do Lace recomenda para uma GUI (API.md, 5.8), com a
//! entrega que o core fornece (`lace_core::delivery`): a operação roda numa
//! thread de trabalho com um `Control` que leva o `CancelToken` e um
//! receptor de eventos; o receptor só põe o evento numa fila, e a thread de
//! entrega do `Dispatcher` junta os eventos em lotes, aplica o teto de linhas
//! e os entrega aqui, onde viram [`JobMessage`] para a interface.
//!
//! Só uma operação roda por vez (erro `busy`): duas no mesmo projeto
//! dividiriam o `.lace/Temp`.

use std::panic::AssertUnwindSafe;
use std::time::{Duration, Instant};

use lace_core::delivery::{Dispatcher, Limits, Notification, trim_step_output};
use lace_core::flows::Phase;
use lace_core::{BuildResult, CancelToken, Event, Step, Tool};
use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::error::{IpcError, IpcResult, codes};
use crate::flows::{self, FlowRequest};
use crate::state::{ActiveJob, AppState};

/// Quanto do `stdout` e do `stderr` de cada passo vai no resultado final. A
/// interface já recebeu cada linha pelos eventos; o resultado leva o fim,
/// para o resumo, sem mandar megabytes de uma vez.
const MAX_STEP_OUTPUT: usize = 256 * 1024;

/// O que a interface recebe pelo canal de uma operação, em ordem.
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
    /// Uma fase começou.
    Phase {
        /// Qual.
        phase: Phase,
    },
    /// Eventos do Core, na ordem em que aconteceram.
    Events {
        /// Os eventos (`step_started`, `output`, `step_finished`).
        events: Vec<Event>,
    },
    /// Linhas de saída que ficaram de fora (o teto da entrega), no lugar
    /// onde estariam: antes da próxima linha que passou ou do fim do passo.
    OutputOmitted {
        /// O passo que as escreveu.
        step: Step,
        /// A ferramenta.
        tool: Tool,
        /// Quantas.
        lines: u64,
    },
    /// Um processador terminou de compilar.
    Build {
        /// O resultado.
        result: BuildResult,
    },
    /// Progresso de uma instalação ou atualização do Lace (o evento do
    /// instalador, em JSON).
    Install {
        /// O evento.
        event: Value,
    },
    /// Uma linha da CLI `lace` (`lace install`, `lace update`).
    CliOutput {
        /// `stdout` ou `stderr`.
        stream: &'static str,
        /// A linha.
        line: String,
    },
    /// A operação terminou e este é o resultado (um `FlowOutcome`, ou o de
    /// uma chamada da CLI).
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

impl From<Notification> for JobMessage {
    fn from(notification: Notification) -> Self {
        match notification {
            Notification::Phase { phase } => JobMessage::Phase { phase },
            Notification::Events { events } => JobMessage::Events { events },
            Notification::OutputOmitted { step, tool, lines } => {
                JobMessage::OutputOmitted { step, tool, lines }
            }
            Notification::Build { result } => JobMessage::Build { result },
            Notification::Install { event, .. } => JobMessage::Install { event },
        }
    }
}

/// Reserva a vaga da operação. Erro `busy` se já houver uma rodando.
pub fn reserve(state: &AppState) -> IpcResult<(u32, CancelToken)> {
    let mut job = state.job.lock().expect("job lock");
    if job.is_some() {
        return Err(IpcError::new(
            codes::BUSY,
            "Another operation is running; wait for it or cancel it",
        ));
    }
    let id = state.next_id();
    let cancel = CancelToken::new();
    *job = Some(ActiveJob {
        id,
        cancel: cancel.clone(),
    });
    Ok((id, cancel))
}

/// Libera a vaga, se ainda for desta operação.
pub fn release(app: &AppHandle, id: u32) {
    let state = app.state::<AppState>();
    let mut job = state.job.lock().expect("job lock");
    if job.as_ref().is_some_and(|j| j.id == id) {
        *job = None;
    }
}

/// Começa um fluxo (build, check, sim, synth, esquemático, placa, exercício)
/// e devolve o identificador da operação. O progresso e o resultado chegam
/// por `channel`, terminando sempre com `finished` ou `failed`.
#[tauri::command]
pub async fn flow_start(
    app: AppHandle,
    request: FlowRequest,
    channel: Channel<JobMessage>,
) -> IpcResult<u32> {
    let state = app.state::<AppState>();
    let spf = state.spf()?;
    let settings = state.settings.get();
    // A placa FPGA é experimental: desligada, o fluxo nem começa.
    if let Some(feature) = request.feature() {
        lace_core::experimental::require(feature)?;
    }
    let (id, cancel) = reserve(&state)?;
    let _ = channel.send(JobMessage::Started {
        job: id,
        command: request.command_line(),
    });

    let worker_app = app.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("flow-{id}"))
        .spawn(move || {
            let deliver = channel.clone();
            let dispatcher = Dispatcher::start(Limits::default(), move |notification| {
                let _ = deliver.send(JobMessage::from(notification));
            });
            let control = dispatcher.control(cancel);
            let progress = dispatcher.progress();
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                flows::run(&request, &settings, &spf, &control, &progress)
            }));
            // Tudo que o fluxo disse sai antes do resultado.
            dispatcher.finish();
            let message = match result {
                Ok(Ok(outcome)) => match serde_json::to_value(&outcome) {
                    Ok(mut value) => {
                        trim_step_output(&mut value, MAX_STEP_OUTPUT);
                        JobMessage::Finished { outcome: value }
                    }
                    Err(error) => JobMessage::Failed {
                        error: error.into(),
                    },
                },
                Ok(Err(error)) => JobMessage::Failed { error },
                Err(_) => JobMessage::Failed {
                    error: IpcError::new(codes::INTERNAL, "The operation panicked; see the log"),
                },
            };
            // A vaga é liberada antes da última mensagem, para a interface
            // poder começar outra operação assim que receber o resultado.
            release(&worker_app, id);
            let _ = channel.send(message);
        });
    if let Err(error) = spawned {
        release(&app, id);
        return Err(IpcError::new(
            codes::INTERNAL,
            format!("Could not start the worker thread: {error}"),
        ));
    }
    Ok(id)
}

/// Cancela a operação que está rodando. Com `job`, só se for ela. O
/// resultado chega pelo canal da operação, com `status: cancelled`.
#[tauri::command]
pub fn flow_cancel(app: AppHandle, job: Option<u32>) -> bool {
    let state = app.state::<AppState>();
    let guard = state.job.lock().expect("job lock");
    match guard.as_ref() {
        Some(active) if job.is_none_or(|id| id == active.id) => {
            active.cancel.cancel();
            true
        }
        _ => false,
    }
}

/// O identificador da operação rodando, se houver.
#[tauri::command]
pub fn flow_running(app: AppHandle) -> Option<u32> {
    let state = app.state::<AppState>();
    let guard = state.job.lock().expect("job lock");
    guard.as_ref().map(|j| j.id)
}

/// Cancela a operação rodando e espera até `wait` ela soltar a vaga. Usado
/// ao fechar o Studio: sem isso, as ferramentas continuariam rodando (num
/// grupo de processos próprio, no Unix) depois que ele saísse.
pub fn cancel_and_wait(app: &AppHandle, wait: Duration) {
    if !flow_cancel(app.clone(), None) {
        return;
    }
    let until = Instant::now() + wait;
    while Instant::now() < until && flow_running(app.clone()).is_some() {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Manda mensagens de uma operação que não é um fluxo do Core (a CLI): o
/// mesmo canal, sem lotes.
pub struct DirectSender {
    channel: Channel<JobMessage>,
}

impl DirectSender {
    /// A entrega direta para `channel`.
    pub fn new(channel: Channel<JobMessage>) -> Self {
        DirectSender { channel }
    }

    /// Manda uma mensagem.
    pub fn send(&self, message: JobMessage) {
        let _ = self.channel.send(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_messages_keep_the_names_the_interface_reads() {
        let value = serde_json::to_value(JobMessage::from(Notification::OutputOmitted {
            step: Step::Simulate,
            tool: Tool::Vvp,
            lines: 3,
        }))
        .unwrap();
        assert_eq!(value["type"], "output_omitted");
        assert_eq!(value["step"], "simulate");
        assert_eq!(value["lines"], 3);
        let value = serde_json::to_value(JobMessage::from(Notification::Phase {
            phase: Phase::Build,
        }))
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "type": "phase", "phase": "build" })
        );
        let value = serde_json::to_value(JobMessage::Started {
            job: 7,
            command: "lace-studio build".into(),
        })
        .unwrap();
        assert_eq!(value["type"], "started");
        assert_eq!(value["job"], 7);
    }
}
