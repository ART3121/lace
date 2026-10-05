//! Cancelar uma operação e acompanhar o que as ferramentas escrevem enquanto
//! ela roda.
//!
//! Toda operação que executa ferramentas ([`build`](crate::build),
//! [`check`](crate::check), [`simulate`](crate::simulate),
//! [`simulate_project`](crate::simulate_project),
//! [`synthesize`](crate::synthesize),
//! [`render_schematic`](crate::render_schematic)) recebe um [`Control`] como
//! último argumento. Com `Control::default()` ela roda até o fim sem avisar
//! nada.
//!
//! Numa GUI, a operação roda numa thread e a interface fica com um clone do
//! [`CancelToken`]:
//!
//! ```no_run
//! use std::sync::mpsc;
//! use lace_core::*;
//!
//! # let toolchain = Toolchain::open("/opt/lace/toolchain")?;
//! let project = Project::open("/p/demo")?;
//! let cancel = CancelToken::new();
//! let (events, received) = mpsc::channel();
//! let control = Control::new()
//!     .with_cancel(cancel.clone())
//!     .on_event(move |event| {
//!         let _ = events.send(event.clone());
//!     });
//!
//! let worker = std::thread::spawn(move || {
//!     let options = SimulationOptions::new(Simulator::Icarus);
//!     simulate_project(&toolchain, &project, &options, &control)
//! });
//! // Na thread da interface: mostra cada linha e, no botão de parar,
//! // chama cancel.cancel().
//! for event in received {
//!     if let Event::Output { line, .. } = event {
//!         println!("{line}");
//!     }
//! }
//! let result = worker.join().expect("a thread terminou")?;
//! println!("{:?}", result.status);
//! # Ok::<(), LaceError>(())
//! ```
//!
//! Cancelar encerra o processo que estiver rodando, com tudo o que ele
//! iniciou (no Unix, o grupo de processos; no Windows, a árvore), e não
//! inicia os passos seguintes. A operação volta como `Ok`, com
//! [`Status::Cancelled`](crate::Status::Cancelled) e os passos que chegaram a
//! rodar.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use schemars::JsonSchema;
use serde::Serialize;

use crate::pipeline::Step;
use crate::process::{Invocation, Termination};
use crate::toolchain::Tool;

/// O pedido de cancelamento de uma operação. Quem pede (a interface) e quem
/// executa (a operação) ficam cada um com um clone: os clones são o mesmo
/// pedido.
///
/// Um pedido feito não se desfaz. Para a próxima operação, crie outro.
///
/// ```
/// use lace_core::CancelToken;
///
/// let token = CancelToken::new();
/// let da_interface = token.clone();
/// da_interface.cancel();
/// assert!(token.is_cancelled());
/// ```
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// Um pedido ainda não feito.
    pub fn new() -> Self {
        CancelToken::default()
    }

    /// Pede o cancelamento. Pode ser chamado de qualquer thread, quantas
    /// vezes for.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Se o cancelamento já foi pedido.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Liga o pedido a uma flag que outro código já marca, como a de um
/// tratador de sinal (`signal_hook::flag::register`): o Ctrl+C vira
/// cancelamento.
impl From<Arc<AtomicBool>> for CancelToken {
    fn from(flag: Arc<AtomicBool>) -> Self {
        CancelToken(flag)
    }
}

/// De qual saída do processo veio uma linha. Em JSON: `"stdout"` ou
/// `"stderr"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Stream {
    /// Saída padrão. Numa simulação, é onde sai o `$display` do testbench.
    Stdout,
    /// Saída de erro.
    Stderr,
}

/// O que aconteceu durante uma operação, avisado na hora em que acontece.
///
/// Em JSON, com o tipo no campo `event`:
///
/// ```json
/// { "event": "step_started", "step": "simulate", "tool": "vvp", "command": { "...": "..." } }
/// { "event": "output", "step": "simulate", "tool": "vvp", "stream": "stdout",
///   "line": "t=10 y=1", "diagnostic": false }
/// { "event": "step_finished", "step": "simulate", "tool": "vvp",
///   "termination": { "kind": "exited", "value": 0 }, "duration_ms": 12 }
/// ```
///
/// Os mesmos dados chegam depois no resultado da operação
/// ([`StepReport`](crate::StepReport)); os eventos só os adiantam.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Event {
    /// Um passo começou: o processo vai ser iniciado.
    StepStarted {
        /// Qual passo.
        step: Step,
        /// A ferramenta.
        tool: Tool,
        /// O que vai ser executado.
        command: Invocation,
    },
    /// Uma linha que o processo escreveu, sem o fim de linha. Linhas de
    /// stdout e de stderr chegam na ordem em que o Lace as leu, que pode não
    /// ser exatamente a ordem em que o processo as escreveu.
    Output {
        /// O passo que escreveu.
        step: Step,
        /// A ferramenta que escreveu.
        tool: Tool,
        /// stdout ou stderr.
        stream: Stream,
        /// A linha, decodificada como UTF-8 (bytes inválidos viram `U+FFFD`).
        line: String,
        /// A linha é uma mensagem da ferramenta, que volta interpretada em
        /// `diagnostics` no resultado: tudo do stderr, todo o stdout dos
        /// compiladores, e no stdout de uma simulação as linhas do próprio
        /// simulador (`$finish called at`, `VCD info:`, o `ERROR:` de um
        /// `$error`). `false` é saída do programa, como o `$display` do
        /// testbench. Decidido linha a linha: a continuação indentada de uma
        /// mensagem conta como `false`.
        diagnostic: bool,
    },
    /// O processo do passo terminou.
    StepFinished {
        /// Qual passo.
        step: Step,
        /// A ferramenta.
        tool: Tool,
        /// Como terminou.
        termination: Termination,
        /// Duração, em milissegundos.
        duration_ms: u64,
    },
}

/// Quem recebe os eventos.
type EventSink = Box<dyn Fn(&Event) + Send + Sync>;

/// Como acompanhar e interromper uma operação: o [`CancelToken`] e quem
/// recebe os [`Event`]s. `Control::default()` não cancela e não avisa nada.
///
/// O receptor de eventos roda na thread da operação, no meio dela: ele deve
/// ser rápido (mandar o evento por um canal, escrever uma linha) e não pode
/// chamar outra operação do Lace.
#[derive(Default)]
pub struct Control {
    cancel: CancelToken,
    sink: Option<EventSink>,
}

impl Control {
    /// Sem cancelamento e sem eventos, como `Control::default()`.
    pub fn new() -> Self {
        Control::default()
    }

    /// Usa `token` como pedido de cancelamento.
    pub fn with_cancel(mut self, token: CancelToken) -> Self {
        self.cancel = token;
        self
    }

    /// Chama `sink` a cada [`Event`].
    pub fn on_event(mut self, sink: impl Fn(&Event) + Send + Sync + 'static) -> Self {
        self.sink = Some(Box::new(sink));
        self
    }

    /// O pedido de cancelamento desta operação.
    pub fn cancel_token(&self) -> &CancelToken {
        &self.cancel
    }

    /// Se o cancelamento já foi pedido.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Se alguém recebe eventos. Sem receptor, a operação nem monta o evento.
    pub(crate) fn has_sink(&self) -> bool {
        self.sink.is_some()
    }

    pub(crate) fn emit(&self, event: Event) {
        if let Some(sink) = &self.sink {
            sink(&event);
        }
    }
}

impl fmt::Debug for Control {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Control")
            .field("cancelled", &self.is_cancelled())
            .field("events", &self.sink.is_some())
            .finish()
    }
}
