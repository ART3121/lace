//! A entrega do progresso de uma operação a uma interface: os eventos do
//! [`Control`] em lotes, com teto, numa thread própria.
//!
//! O receptor de eventos do core roda no meio da operação e tem de ser
//! rápido (API.md, 5.8). O [`Dispatcher`] dá a qualquer cliente o desenho
//! que o Lace Studio usava por conta própria: o receptor só põe o evento numa
//! fila, e uma thread de entrega junta os eventos em lotes (no máximo a cada
//! [`Limits::flush_every`] ou [`Limits::max_batch`] eventos) e os entrega em
//! ordem, pela função que o cliente deu, com as fases e os resultados de
//! build entre os lotes.
//!
//! As linhas de saída têm teto ([`Throttle`]): um testbench que imprime sem
//! parar mandaria mais do que uma interface dá conta de mostrar, e a memória
//! cresceria. O que passa do teto fica de fora e é contado
//! ([`Notification::OutputOmitted`]). Os outros eventos (começo e fim de
//! passo) sempre passam.
//!
//! A fila entre o receptor e a thread é limitada ([`Limits::queue`]). Cheia,
//! o receptor espera até [`Limits::send_timeout`] e, se a entrega continuar
//! parada, descarta a linha e a conta como omitida: a ferramenta nunca fica
//! presa para sempre atrás de um cliente que parou de ler, e o cancelamento
//! continua chegando.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

use crate::flows::{Phase, Progress};
use crate::{BuildResult, CancelToken, Control, Event};
use crate::{Step, Tool};

/// Os números da entrega. O padrão é o que o Lace Studio calibrou.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Intervalo máximo entre dois lotes de eventos.
    pub flush_every: Duration,
    /// Tamanho máximo de um lote.
    pub max_batch: usize,
    /// Quantas linhas de saída passam de uma vez, numa rajada: uma listagem
    /// de memória que termina logo chega inteira.
    pub burst: f64,
    /// Quantas linhas por segundo passam depois da rajada.
    pub rate: f64,
    /// Depois de começar a cortar, as linhas só voltam a passar com estas
    /// fichas no balde: em vez de um aviso a cada linha que passa, um bloco
    /// de linhas e um aviso de tempos em tempos.
    pub resume: f64,
    /// Cada tantos bytes de uma linha contam como mais uma linha: uma linha
    /// longa pesa o que ocupa.
    pub line_bytes: usize,
    /// Quantas mensagens cabem na fila entre o receptor e a thread de
    /// entrega.
    pub queue: usize,
    /// Quanto o receptor espera pela fila cheia antes de descartar uma linha.
    pub send_timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            flush_every: Duration::from_millis(30),
            max_batch: 500,
            burst: 20_000.0,
            rate: 5_000.0,
            resume: 2_500.0,
            line_bytes: 1024,
            queue: 4096,
            send_timeout: Duration::from_millis(500),
        }
    }
}

/// O que sai de uma operação enquanto ela roda, na ordem em que aconteceu.
/// Os nomes são os que o Lace Studio manda à interface dele.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Notification {
    /// Uma fase começou.
    Phase {
        /// Qual.
        phase: Phase,
    },
    /// Eventos do core, na ordem em que aconteceram.
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
    /// Progresso de uma instalação ou atualização do Lace (o evento do
    /// instalador, serializado por ele; o core não conhece o tipo).
    Install {
        /// O evento, em JSON.
        event: Value,
    },
}

/// O teto das linhas de saída, um balde de fichas: começa com
/// [`Limits::burst`] fichas, ganha [`Limits::rate`] por segundo até esse
/// teto, e cada linha gasta uma (mais uma a cada [`Limits::line_bytes`]).
/// Sem ficha, a linha fica de fora e é contada, até o balde ter
/// [`Limits::resume`] fichas de novo. Os outros eventos sempre passam.
#[derive(Debug)]
pub struct Throttle {
    burst: f64,
    rate: f64,
    resume: f64,
    line_bytes: usize,
    tokens: f64,
    refilled: Instant,
    /// As linhas de fora desde a última que passou, e de onde vieram.
    omitted: Option<(Step, Tool, u64)>,
}

impl Throttle {
    /// Um balde cheio em `now`.
    pub fn new(limits: &Limits, now: Instant) -> Self {
        Throttle {
            burst: limits.burst,
            rate: limits.rate,
            resume: limits.resume,
            line_bytes: limits.line_bytes.max(1),
            tokens: limits.burst,
            refilled: now,
            omitted: None,
        }
    }

    /// O que mandar por `event`: se ele passa (`true`) e, antes dele, o aviso
    /// de quantas linhas ficaram de fora desde a última que passou.
    pub fn admit(&mut self, event: &Event, now: Instant) -> (Option<Notification>, bool) {
        if let Event::Output {
            step, tool, line, ..
        } = event
        {
            let elapsed = now.saturating_duration_since(self.refilled).as_secs_f64();
            self.tokens = (self.tokens + elapsed * self.rate).min(self.burst);
            self.refilled = now;
            let cost = 1.0 + (line.len() / self.line_bytes) as f64;
            let needed = if self.omitted.is_some() {
                cost.max(self.resume)
            } else {
                cost
            };
            if self.tokens < needed {
                return (self.omit(*step, *tool), false);
            }
            self.tokens -= cost;
        }
        (self.take_omitted(), true)
    }

    /// Conta uma linha de `step` como omitida sem passar pelo balde: a fila
    /// de entrega não a aceitou.
    pub fn drop_line(&mut self, step: Step, tool: Tool) -> Option<Notification> {
        self.omit(step, tool)
    }

    fn omit(&mut self, step: Step, tool: Tool) -> Option<Notification> {
        match &mut self.omitted {
            Some((s, t, lines)) if *s == step && *t == tool => {
                *lines += 1;
                None
            }
            _ => {
                // Outro passo: o que ficou de fora do anterior sai antes.
                let before = self.take_omitted();
                self.omitted = Some((step, tool, 1));
                before
            }
        }
    }

    /// O aviso das linhas omitidas até aqui, se houve, zerando a conta.
    pub fn take_omitted(&mut self) -> Option<Notification> {
        self.omitted
            .take()
            .map(|(step, tool, lines)| Notification::OutputOmitted { step, tool, lines })
    }
}

/// O que chega à thread de entrega.
enum Msg {
    Event(Event),
    Message(Notification),
    Stop,
}

/// A entrega em lotes numa thread própria. Quem recebe as notificações é a
/// função dada em [`Dispatcher::start`], chamada nessa thread, em ordem.
///
/// ```no_run
/// use lace_core::delivery::{Dispatcher, Limits};
/// use lace_core::*;
///
/// # fn main() -> Result<()> {
/// # let toolchain = Toolchain::open("/x")?;
/// # let project = Project::open("/p")?;
/// let dispatcher = Dispatcher::start(Limits::default(), |notification| {
///     // mandar para a interface
///     let _ = serde_json::to_string(&notification);
/// });
/// let cancel = CancelToken::new();
/// let control = dispatcher.control(cancel.clone());
/// let progress = dispatcher.progress();
/// let request = flows::FlowRequest::Check { file: None, processor: None, lint: false };
/// let options = flows::FlowOptions::new("lace check");
/// let mut project = project;
/// let outcome = flows::run(&toolchain, &mut project, &request, &options, &progress, &control)?;
/// dispatcher.finish();
/// # let _ = outcome;
/// # Ok(()) }
/// ```
pub struct Dispatcher {
    tx: SyncSender<Msg>,
    limits: Limits,
    thread: Option<JoinHandle<()>>,
}

impl Dispatcher {
    /// Começa a thread de entrega. `deliver` recebe cada notificação, na
    /// ordem, nessa thread.
    pub fn start(limits: Limits, mut deliver: impl FnMut(Notification) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::sync_channel(limits.queue.max(1));
        let thread = std::thread::Builder::new()
            .name("lace-delivery".into())
            .spawn(move || forward(rx, limits, &mut deliver))
            .expect("spawn the delivery thread");
        Dispatcher {
            tx,
            limits,
            thread: Some(thread),
        }
    }

    /// O [`Control`] de uma operação: cancela com `cancel`, e cada evento
    /// passa pelo teto e pelos lotes antes de chegar a `deliver`.
    pub fn control(&self, cancel: CancelToken) -> Control {
        let tx = self.tx.clone();
        let limits = self.limits;
        let throttle = Arc::new(Mutex::new(Throttle::new(&limits, Instant::now())));
        Control::new().with_cancel(cancel).on_event(move |event| {
            let now = Instant::now();
            let (omitted, admitted) = throttle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .admit(event, now);
            if let Some(message) = omitted {
                // O aviso é raro: espera mais por ele; num cliente parado por
                // completo, perde-se só a contagem.
                let _ = push(&tx, Msg::Message(message), now + limits.send_timeout * 10);
            }
            if !admitted {
                return;
            }
            // A fila cheia espera um pouco pela entrega; se ela continua
            // parada, a linha fica de fora, contada, e a ferramenta segue.
            if let Some(Msg::Event(Event::Output { step, tool, .. })) =
                push(&tx, Msg::Event(event.clone()), now + limits.send_timeout)
            {
                throttle
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .drop_line(step, tool);
            }
        })
    }

    /// A função de progresso dos fluxos: as fases e os resultados de build
    /// viram notificações, na ordem, entre os lotes; o resto do
    /// [`Progress`] é da interface de texto e não passa.
    pub fn progress(&self) -> impl Fn(Progress<'_>) + Send + Sync + 'static {
        let tx = self.tx.clone();
        move |progress| {
            let message = match progress {
                Progress::Phase(phase) => Notification::Phase { phase },
                Progress::Built(result) => Notification::Build {
                    result: result.clone(),
                },
                _ => return,
            };
            let _ = tx.send(Msg::Message(message));
        }
    }

    /// Manda uma notificação fora de lote (o lote pendente sai antes dela).
    pub fn notify(&self, notification: Notification) {
        let _ = self.tx.send(Msg::Message(notification));
    }

    /// Esvazia o que falta, encerra a thread e espera ela terminar. O
    /// `Drop` faz o mesmo.
    pub fn finish(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Dispatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Põe `message` na fila, esperando até `deadline` se ela estiver cheia.
/// Devolve a mensagem de volta se não coube a tempo; `None` se entrou ou se
/// a thread de entrega já terminou.
fn push(tx: &SyncSender<Msg>, mut message: Msg, deadline: Instant) -> Option<Msg> {
    loop {
        match tx.try_send(message) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => return None,
            Err(TrySendError::Full(back)) => {
                if Instant::now() >= deadline {
                    return Some(back);
                }
                message = back;
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

fn forward(rx: Receiver<Msg>, limits: Limits, deliver: &mut dyn FnMut(Notification)) {
    let mut batch: Vec<Event> = Vec::new();
    let mut deadline: Option<Instant> = None;
    loop {
        let wait = deadline
            .map(|d| d.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600));
        match rx.recv_timeout(wait) {
            Ok(Msg::Event(event)) => {
                batch.push(event);
                deadline.get_or_insert_with(|| Instant::now() + limits.flush_every);
                if batch.len() >= limits.max_batch {
                    flush(&mut batch, &mut deadline, deliver);
                }
            }
            Ok(Msg::Message(message)) => {
                flush(&mut batch, &mut deadline, deliver);
                deliver(message);
            }
            Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => {
                flush(&mut batch, &mut deadline, deliver);
                break;
            }
            Err(RecvTimeoutError::Timeout) => flush(&mut batch, &mut deadline, deliver),
        }
    }
}

/// Entrega o lote pendente, se houver, e zera o prazo.
fn flush(
    batch: &mut Vec<Event>,
    deadline: &mut Option<Instant>,
    deliver: &mut dyn FnMut(Notification),
) {
    if !batch.is_empty() {
        let events = std::mem::take(batch);
        deliver(Notification::Events { events });
    }
    *deadline = None;
}

/// Corta o `stdout` e o `stderr` de cada passo de um resultado em JSON em
/// `max` bytes, guardando o fim, que é onde estão o erro e o `$finish`. A
/// interface já recebeu cada linha pelos eventos; o resultado leva o fim,
/// para o resumo, sem mandar megabytes de uma vez.
pub fn trim_step_output(value: &mut Value, max: usize) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if (key == "stdout" || key == "stderr")
                    && let Value::String(text) = child
                    && text.len() > max
                {
                    let mut cut = text.len() - max;
                    while !text.is_char_boundary(cut) {
                        cut += 1;
                    }
                    *text = format!(
                        "[... {} bytes omitted; the full output went to the console ...]\n{}",
                        cut,
                        &text[cut..]
                    );
                } else {
                    trim_step_output(child, max);
                }
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| trim_step_output(item, max)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Stream, Termination};

    const MAX: usize = 256 * 1024;

    #[test]
    fn trims_long_step_output_keeping_the_end() {
        let long = "x".repeat(MAX + 10) + "fim";
        let mut value = serde_json::json!({
            "builds": [{ "steps": [{ "stdout": long, "stderr": "curto" }] }]
        });
        trim_step_output(&mut value, MAX);
        let stdout = value["builds"][0]["steps"][0]["stdout"].as_str().unwrap();
        assert!(stdout.starts_with("[... 13 bytes omitted"));
        assert!(stdout.ends_with("fim"));
        assert_eq!(value["builds"][0]["steps"][0]["stderr"], "curto");
    }

    #[test]
    fn trimming_respects_utf8_boundaries() {
        // 'ç' tem dois bytes: o corte não pode cair no meio dele.
        let long = "ç".repeat(MAX);
        let mut value = serde_json::json!({ "stdout": long });
        trim_step_output(&mut value, MAX);
        assert!(value["stdout"].as_str().unwrap().ends_with('ç'));
    }

    fn output(step: Step, line: &str) -> Event {
        Event::Output {
            step,
            tool: Tool::Vvp,
            stream: Stream::Stdout,
            line: line.to_owned(),
            diagnostic: false,
        }
    }

    fn finished(step: Step) -> Event {
        Event::StepFinished {
            step,
            tool: Tool::Vvp,
            termination: Termination::Cancelled,
            duration_ms: 1,
        }
    }

    fn omitted(message: Option<Notification>) -> Option<u64> {
        match message {
            Some(Notification::OutputOmitted { lines, .. }) => Some(lines),
            Some(_) => panic!("outra mensagem"),
            None => None,
        }
    }

    #[test]
    fn output_beyond_the_burst_is_counted_and_reported_in_place() {
        let limits = Limits::default();
        let start = Instant::now();
        let mut throttle = Throttle::new(&limits, start);
        let burst = limits.burst as u64;
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
        let mut throttle = Throttle::new(&limits, start);
        throttle.tokens = 0.0;
        throttle.admit(&output(Step::Simulate, "x"), start);
        let (message, admitted) = throttle.admit(&finished(Step::Simulate), start);
        assert_eq!(omitted(message), Some(1));
        assert!(admitted);
    }

    #[test]
    fn a_sustained_flood_passes_at_the_rate() {
        let limits = Limits::default();
        let start = Instant::now();
        let mut throttle = Throttle::new(&limits, start);
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
        let mut throttle = Throttle::new(&limits, start);
        throttle.tokens = 10.0;
        let long = "x".repeat(64 * 1024);
        assert!(!throttle.admit(&output(Step::Simulate, &long), start).1);
    }

    #[test]
    fn the_dispatcher_batches_events_and_keeps_messages_in_order() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let sink = received.clone();
        let limits = Limits {
            flush_every: Duration::from_millis(20),
            max_batch: 3,
            ..Limits::default()
        };
        let dispatcher = Dispatcher::start(limits, move |n| sink.lock().unwrap().push(n));
        let control = dispatcher.control(CancelToken::new());
        let progress = dispatcher.progress();
        progress(Progress::Phase(Phase::Simulate));
        for i in 0..4 {
            control.emit(output(Step::Simulate, &format!("linha {i}")));
        }
        dispatcher.notify(Notification::Install {
            event: serde_json::json!({ "event": "verifying" }),
        });
        dispatcher.finish();
        let received = received.lock().unwrap();
        // Fase, um lote de 3, o resto do lote antes da mensagem, a mensagem.
        assert!(matches!(
            received[0],
            Notification::Phase {
                phase: Phase::Simulate
            }
        ));
        assert!(matches!(&received[1], Notification::Events { events } if events.len() == 3));
        assert!(matches!(&received[2], Notification::Events { events } if events.len() == 1));
        assert!(matches!(received[3], Notification::Install { .. }));
        assert_eq!(received.len(), 4);
    }

    #[test]
    fn a_stalled_consumer_does_not_block_the_tool_forever() {
        // A entrega dorme: a fila (de 2) enche, e as linhas a mais são
        // descartadas depois de um tempo curto, contadas como omitidas.
        let limits = Limits {
            queue: 2,
            send_timeout: Duration::from_millis(20),
            flush_every: Duration::from_millis(5),
            ..Limits::default()
        };
        let delivered = Arc::new(Mutex::new(0usize));
        let count = delivered.clone();
        let dispatcher = Dispatcher::start(limits, move |n| {
            std::thread::sleep(Duration::from_millis(200));
            if let Notification::Events { events } = n {
                *count.lock().unwrap() += events
                    .iter()
                    .filter(|e| matches!(e, Event::Output { .. }))
                    .count();
            } else if let Notification::OutputOmitted { lines, .. } = n {
                *count.lock().unwrap() += lines as usize;
            }
        });
        let control = dispatcher.control(CancelToken::new());
        let started = Instant::now();
        for i in 0..40 {
            control.emit(output(Step::Simulate, &format!("linha {i}")));
        }
        // Com 40 linhas e 20 ms de espera por linha, no máximo um segundo.
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
        control.emit(finished(Step::Simulate));
        dispatcher.finish();
        // Tudo chegou, ou como linha ou como omitida.
        assert_eq!(*delivered.lock().unwrap(), 40);
    }

    #[test]
    fn notification_json_has_the_studio_names() {
        let value = serde_json::to_value(Notification::OutputOmitted {
            step: Step::Simulate,
            tool: Tool::Vvp,
            lines: 3,
        })
        .unwrap();
        assert_eq!(value["type"], "output_omitted");
        assert_eq!(value["lines"], 3);
        let value = serde_json::to_value(Notification::Phase {
            phase: Phase::RtlViewer,
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "type": "phase", "phase": "rtl_viewer" })
        );
    }
}
