//! As operações longas: rodam numa thread própria, mandam o progresso para
//! a interface por um `Channel` do Tauri e podem ser canceladas.
//!
//! É o desenho que a API do Lace recomenda para uma GUI (API.md, 5.8): a
//! operação roda numa thread de trabalho com um `Control` que leva o
//! `CancelToken` e um receptor de eventos; a interface guarda o
//! identificador e chama [`flow_cancel`].
//!
//! O receptor de eventos do Core roda no meio da operação e tem de ser
//! rápido. Ele só manda o evento por um canal; uma segunda thread junta os
//! eventos em lotes (no máximo a cada 30 ms ou 500 eventos) e os entrega à
//! interface. Uma simulação que escreve milhares de linhas por segundo vira
//! algumas dezenas de mensagens por segundo.
//!
//! As linhas de saída têm teto ([`Throttle`]): o Tauri guarda cada mensagem
//! até a interface buscá-la, e um testbench que imprime sem parar mandava
//! mais do que ela dá conta de mostrar. A memória do Studio crescia (de 53 MB
//! a 193 MB em 30 s), e o Parar levava 10 s para aparecer, atrás da fila. O
//! que passa do teto fica de fora, e a interface recebe quantas linhas foram.
//!
//! Só uma operação roda por vez (erro `busy`): duas no mesmo projeto
//! dividiriam o `.lace/Temp`.

use std::panic::AssertUnwindSafe;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use lace_core::{BuildResult, CancelToken, Control, Event, Step, Tool};
use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::error::{IpcError, IpcResult, codes};
use crate::flows::{self, FlowRequest, Phase, Progress};
use crate::state::{ActiveJob, AppState};

/// Intervalo máximo entre dois lotes de eventos.
const FLUSH_EVERY: Duration = Duration::from_millis(30);
/// Tamanho máximo de um lote.
const MAX_BATCH: usize = 500;
/// Quanto do `stdout` e do `stderr` de cada passo vai no resultado final. A
/// interface já recebeu cada linha pelos eventos; o resultado leva o fim,
/// para o resumo, sem mandar megabytes de uma vez.
const MAX_STEP_OUTPUT: usize = 256 * 1024;
/// Quantas linhas de saída passam de uma vez para a interface, numa rajada:
/// as que cada console guarda (`LOG_LINES`, em `console/consoles.ts`). Uma
/// listagem de memória que termina logo chega inteira.
const OUTPUT_BURST: f64 = 20_000.0;
/// Quantas linhas por segundo passam depois da rajada.
const OUTPUT_RATE: f64 = 5_000.0;
/// Depois de começar a cortar, as linhas só voltam a passar com estas fichas
/// no balde (meio segundo delas): em vez de um aviso a cada linha que passa,
/// um bloco de linhas e um aviso a cada meio segundo.
const OUTPUT_RESUME: f64 = OUTPUT_RATE / 2.0;
/// Cada tantos bytes de uma linha contam como mais uma linha: uma linha
/// longa (até 64 KiB, as de quem escreve sem `\n`) pesa o que ocupa.
const OUTPUT_LINE_BYTES: usize = 1024;

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
    /// Linhas de saída que ficaram de fora ([`Throttle`]), no lugar onde
    /// estariam: antes da próxima linha que passou ou do fim do passo.
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

/// O que chega à thread que entrega os lotes.
enum Msg {
    Event(Event),
    Message(JobMessage),
}

/// O teto das linhas de saída que vão para a interface, um balde de fichas:
/// começa com [`OUTPUT_BURST`] fichas, ganha [`OUTPUT_RATE`] por segundo até
/// esse teto, e cada linha gasta uma (mais uma a cada [`OUTPUT_LINE_BYTES`]).
/// Sem ficha, a linha fica de fora e é contada, até o balde ter
/// [`OUTPUT_RESUME`] fichas de novo. Os outros eventos (começo e fim de
/// passo) sempre passam.
struct Throttle {
    tokens: f64,
    refilled: Instant,
    /// As linhas de fora desde a última que passou, e de onde vieram.
    omitted: Option<(Step, Tool, u64)>,
}

impl Throttle {
    fn new(now: Instant) -> Self {
        Throttle {
            tokens: OUTPUT_BURST,
            refilled: now,
            omitted: None,
        }
    }

    /// O que mandar à interface por `event`: nada, se ele ficou de fora; e,
    /// antes dele, quantas linhas ficaram de fora desde a última que passou.
    fn admit(&mut self, event: &Event, now: Instant) -> (Option<JobMessage>, bool) {
        if let Event::Output {
            step, tool, line, ..
        } = event
        {
            let elapsed = now.saturating_duration_since(self.refilled).as_secs_f64();
            self.tokens = (self.tokens + elapsed * OUTPUT_RATE).min(OUTPUT_BURST);
            self.refilled = now;
            let cost = 1.0 + (line.len() / OUTPUT_LINE_BYTES) as f64;
            let needed = if self.omitted.is_some() {
                cost.max(OUTPUT_RESUME)
            } else {
                cost
            };
            if self.tokens < needed {
                match &mut self.omitted {
                    Some((s, t, lines)) if s == step && t == tool => *lines += 1,
                    _ => {
                        // Outro passo: o que ficou de fora do anterior sai antes.
                        let before = self.take_omitted();
                        self.omitted = Some((*step, *tool, 1));
                        return (before, false);
                    }
                }
                return (None, false);
            }
            self.tokens -= cost;
        }
        (self.take_omitted(), true)
    }

    fn take_omitted(&mut self) -> Option<JobMessage> {
        self.omitted
            .take()
            .map(|(step, tool, lines)| JobMessage::OutputOmitted { step, tool, lines })
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

/// Começa a entrega em lotes para `channel` e devolve quem manda para ela.
/// A thread termina quando todos os `Sender` forem soltos.
fn start_forwarder(channel: Channel<JobMessage>) -> Sender<Msg> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("job-forwarder".into())
        .spawn(move || forward(rx, channel))
        .expect("spawn the forwarder thread");
    tx
}

fn forward(rx: Receiver<Msg>, channel: Channel<JobMessage>) {
    let mut batch: Vec<Event> = Vec::new();
    let mut deadline: Option<Instant> = None;
    let flush = |batch: &mut Vec<Event>, deadline: &mut Option<Instant>| {
        if !batch.is_empty() {
            let events = std::mem::take(batch);
            let _ = channel.send(JobMessage::Events { events });
        }
        *deadline = None;
    };
    loop {
        let wait = deadline
            .map(|d| d.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600));
        match rx.recv_timeout(wait) {
            Ok(Msg::Event(event)) => {
                batch.push(event);
                deadline.get_or_insert_with(|| Instant::now() + FLUSH_EVERY);
                if batch.len() >= MAX_BATCH {
                    flush(&mut batch, &mut deadline);
                }
            }
            Ok(Msg::Message(message)) => {
                flush(&mut batch, &mut deadline);
                let _ = channel.send(message);
            }
            Err(RecvTimeoutError::Timeout) => flush(&mut batch, &mut deadline),
            Err(RecvTimeoutError::Disconnected) => {
                flush(&mut batch, &mut deadline);
                break;
            }
        }
    }
}

/// Começa um fluxo (build, check, sim, synth, esquemático) e devolve o
/// identificador da operação. O progresso e o resultado chegam por
/// `channel`, terminando sempre com `finished` ou `failed`.
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
    let tx = start_forwarder(channel);
    let _ = tx.send(Msg::Message(JobMessage::Started {
        job: id,
        command: request.command_line(),
    }));

    let worker_app = app.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("flow-{id}"))
        .spawn(move || {
            let events = tx.clone();
            let throttle = Mutex::new(Throttle::new(Instant::now()));
            let control = Control::new().with_cancel(cancel).on_event(move |event| {
                let (omitted, admitted) = throttle
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .admit(event, Instant::now());
                if let Some(message) = omitted {
                    let _ = events.send(Msg::Message(message));
                }
                if admitted {
                    let _ = events.send(Msg::Event(event.clone()));
                }
            });
            let progress_tx = tx.clone();
            let progress = move |p: Progress| {
                let message = match p {
                    Progress::Phase(phase) => JobMessage::Phase { phase },
                    Progress::Built(result) => JobMessage::Build { result },
                };
                let _ = progress_tx.send(Msg::Message(message));
            };
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                flows::run(&request, &settings, &spf, &control, &progress)
            }));
            let message = match result {
                Ok(Ok(outcome)) => match serde_json::to_value(&outcome) {
                    Ok(mut value) => {
                        trim_step_output(&mut value);
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
            let _ = tx.send(Msg::Message(message));
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

/// Corta o `stdout` e o `stderr` de cada passo em [`MAX_STEP_OUTPUT`]
/// bytes, guardando o fim, que é onde estão o erro e o `$finish`.
fn trim_step_output(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if (key == "stdout" || key == "stderr")
                    && let Value::String(text) = child
                    && text.len() > MAX_STEP_OUTPUT
                {
                    let mut cut = text.len() - MAX_STEP_OUTPUT;
                    while !text.is_char_boundary(cut) {
                        cut += 1;
                    }
                    *text = format!(
                        "[... {} bytes omitted; the full output went to the console ...]\n{}",
                        cut,
                        &text[cut..]
                    );
                } else {
                    trim_step_output(child);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(trim_step_output),
        _ => {}
    }
}

/// Manda mensagens de uma operação que não é um fluxo do Core (a CLI): o
/// mesmo canal, sem lotes.
pub struct DirectSender {
    tx: Sender<Msg>,
}

impl DirectSender {
    /// Começa a entrega para `channel`.
    pub fn new(channel: Channel<JobMessage>) -> Self {
        DirectSender {
            tx: start_forwarder(channel),
        }
    }

    /// Manda uma mensagem.
    pub fn send(&self, message: JobMessage) {
        let _ = self.tx.send(Msg::Message(message));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_long_step_output_keeping_the_end() {
        let long = "x".repeat(MAX_STEP_OUTPUT + 10) + "fim";
        let mut value = serde_json::json!({
            "builds": [{ "steps": [{ "stdout": long, "stderr": "curto" }] }]
        });
        trim_step_output(&mut value);
        let stdout = value["builds"][0]["steps"][0]["stdout"].as_str().unwrap();
        assert!(stdout.starts_with("[... 13 bytes omitted"));
        assert!(stdout.ends_with("fim"));
        assert_eq!(value["builds"][0]["steps"][0]["stderr"], "curto");
    }

    fn output(step: Step, line: &str) -> Event {
        Event::Output {
            step,
            tool: Tool::Vvp,
            stream: lace_core::Stream::Stdout,
            line: line.to_owned(),
            diagnostic: false,
        }
    }

    fn finished(step: Step) -> Event {
        Event::StepFinished {
            step,
            tool: Tool::Vvp,
            termination: lace_core::Termination::Cancelled,
            duration_ms: 1,
        }
    }

    fn omitted(message: Option<JobMessage>) -> Option<u64> {
        match message {
            Some(JobMessage::OutputOmitted { lines, .. }) => Some(lines),
            Some(_) => panic!("outra mensagem"),
            None => None,
        }
    }

    #[test]
    fn output_beyond_the_burst_is_counted_and_reported_in_place() {
        let start = Instant::now();
        let mut throttle = Throttle::new(start);
        let burst = OUTPUT_BURST as u64;
        // A rajada passa inteira, no mesmo instante.
        for _ in 0..burst {
            let (message, admitted) = throttle.admit(&output(Step::Simulate, "x"), start);
            assert!(message.is_none() && admitted);
        }
        // O resto fica de fora, contado.
        for _ in 0..1000 {
            let (message, admitted) = throttle.admit(&output(Step::Simulate, "x"), start);
            assert!(message.is_none() && !admitted);
        }
        // Uma ficha que volta não basta: o corte vai até a reserva.
        let soon = start + Duration::from_millis(10);
        assert!(!throttle.admit(&output(Step::Simulate, "x"), soon).1);
        // Com ela, voltam a passar, depois do aviso de quantas ficaram.
        let later = start + Duration::from_millis(600);
        let (message, admitted) = throttle.admit(&output(Step::Simulate, "x"), later);
        assert_eq!(omitted(message), Some(1001));
        assert!(admitted);
        // O fim do passo sempre passa, e traz o que ficou de fora antes dele.
        let mut throttle = Throttle::new(start);
        throttle.tokens = 0.0;
        throttle.admit(&output(Step::Simulate, "x"), start);
        let (message, admitted) = throttle.admit(&finished(Step::Simulate), start);
        assert_eq!(omitted(message), Some(1));
        assert!(admitted);
    }

    #[test]
    fn a_sustained_flood_passes_at_the_rate() {
        let start = Instant::now();
        let mut throttle = Throttle::new(start);
        throttle.tokens = 0.0;
        // Um milhão de linhas em dez segundos: passam umas 5 000 por segundo,
        // em blocos, com um aviso a cada meio segundo, e não a cada linha.
        let (mut passed, mut notices) = (0, 0);
        for i in 0..1_000_000u64 {
            let now = start + Duration::from_micros(i * 10);
            let (message, admitted) = throttle.admit(&output(Step::Simulate, "linha"), now);
            passed += u64::from(admitted);
            notices += u64::from(message.is_some());
        }
        assert!((45_000..=50_500).contains(&passed), "{passed}");
        assert!((15..=25).contains(&notices), "{notices}");
        // Uma linha longa pesa o que ocupa.
        let mut throttle = Throttle::new(start);
        throttle.tokens = 10.0;
        let long = "x".repeat(64 * 1024);
        assert!(!throttle.admit(&output(Step::Simulate, &long), start).1);
    }

    #[test]
    fn trimming_respects_utf8_boundaries() {
        // 'ç' tem dois bytes: o corte não pode cair no meio dele.
        let long = "ç".repeat(MAX_STEP_OUTPUT);
        let mut value = serde_json::json!({ "stdout": long });
        trim_step_output(&mut value);
        assert!(value["stdout"].as_str().unwrap().ends_with('ç'));
    }
}
