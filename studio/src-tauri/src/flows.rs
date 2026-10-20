//! Os fluxos do Studio: o pedido da interface vira um fluxo do core
//! (`lace_core::flows`) ou a correção de um exercício do lace-learn
//! (`lace_learn::flows`), e o resultado volta com o que só o Studio
//! acrescenta: a onda numa aba e a correção.
//!
//! A composição em si (compilar antes, parar no primeiro build que falhar,
//! registrar o Verilog gerado, simular, ler as portas, gravar o relatório,
//! abrir a onda) mora no core desde a 0.9: é a mesma da CLI. Aqui ficam só
//! as decisões da interface: o texto `lace-studio ...` do histórico, e se a
//! onda abre em janela ou numa aba (preferência `wave_viewer`).
//!
//! Nada aqui fala com a interface: o progresso sai pela função `progress`
//! do core, e quem a transforma em mensagens é `jobs.rs`.

use std::ops::Deref;

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::experimental::Feature;
use lace_core::flows::{
    self as core, FlowOptions, LayoutPolicy, ProgressFn, WaveOpened, WavePolicy, WaveRequest,
    WaveSource,
};
use lace_core::{Control, Simulator, Status, Toolchain};
use lace_learn::{Grade, Verdict};
use serde::{Deserialize, Serialize};

pub use lace_core::flows::{Phase, Progress};

use crate::error::IpcResult;
use crate::settings::Settings;
use crate::toolchain;

/// O que a interface pede: os fluxos do core, no formato deles, mais a
/// correção de um exercício.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "flow", rename_all = "snake_case")]
pub enum FlowRequest {
    /// `lace build [-p NOME]...`.
    Build {
        /// Os processadores; vazio é todos.
        #[serde(default)]
        processors: Vec<String>,
    },
    /// `lace check [ARQUIVO] [-p NOME] [--lint]`.
    Check {
        /// Só os módulos deste arquivo.
        #[serde(default)]
        file: Option<Utf8PathBuf>,
        /// Só este processador.
        #[serde(default)]
        processor: Option<String>,
        /// Acrescenta o `verilator --lint-only`.
        #[serde(default)]
        lint: bool,
    },
    /// `lace hierarchy [-p NOME]`.
    Hierarchy {
        /// Só este processador.
        #[serde(default)]
        processor: Option<String>,
    },
    /// `lace sim [TESTBENCH] [-p NOME] [--verilator] [--fast] [--timeout S] [--open]`.
    Simulate {
        /// Simula este processador; `None`, a simulação do projeto.
        #[serde(default)]
        processor: Option<String>,
        /// Troca o testbench da simulação do projeto antes de simular.
        #[serde(default)]
        testbench: Option<Utf8PathBuf>,
        /// Icarus ou Verilator.
        simulator: Simulator,
        /// A simulação rápida: sem onda, com o testbench Verilog no Verilator.
        #[serde(default)]
        fast: bool,
        /// Prazo do passo `simulate`, em segundos.
        #[serde(default)]
        timeout_s: Option<u64>,
        /// Abrir a onda se a simulação der certo: em janela ou, com a
        /// preferência `wave_viewer` em `tab`, numa aba (`wave_tab`).
        #[serde(default)]
        open_wave: bool,
    },
    /// `lace synth [-p NOME] [--svg] [--module M]`.
    Synthesize {
        /// Sintetiza só este processador; `None`, o topo do projeto.
        #[serde(default)]
        processor: Option<String>,
        /// Desenha o esquemático depois.
        #[serde(default)]
        schematic: bool,
        /// O módulo do esquemático; `None`, o topo da síntese.
        #[serde(default)]
        module: Option<String>,
        /// Grava o HTML do RTL viewer ao lado do netlist (o Studio desenha o
        /// próprio esquemático, então a interface não pede isso).
        #[serde(default)]
        open_viewer: bool,
    },
    /// Desenha outro módulo de um netlist que já existe. Sem relatório.
    Schematic {
        /// O `hierarchy.json` da síntese.
        netlist: Utf8PathBuf,
        /// O módulo.
        module: String,
        /// Escrever a largura dos barramentos nas arestas.
        #[serde(default = "yes")]
        bus_widths: bool,
    },
    /// `lace fpga build`.
    FpgaBuild,
    /// `lace fpga program [--cable NOME]`.
    FpgaProgram {
        /// O cabo; `None`, o primeiro que o Quartus lista.
        #[serde(default)]
        cable: Option<String>,
    },
    /// `lace learn check NOME`: corrige um exercício da pasta de exercícios e
    /// grava se ele está resolvido. Não grava relatório.
    Learn {
        /// A pasta de exercícios (ou uma pasta dentro dela).
        root: Utf8PathBuf,
        /// O exercício.
        exercise: String,
    },
}

fn yes() -> bool {
    true
}

impl FlowRequest {
    /// O nome curto do fluxo, como vai no resultado.
    pub fn name(&self) -> &'static str {
        match self.core() {
            Some(request) => request.name(),
            None => "learn",
        }
    }

    /// O recurso experimental de que o fluxo faz parte: `jobs.rs` não o
    /// começa com o recurso desligado.
    pub fn feature(&self) -> Option<Feature> {
        self.core().and_then(|request| request.feature())
    }

    /// O comando equivalente da CLI, para o relatório e para o console. O
    /// prefixo é `lace-studio` porque quem rodou foi o Studio, não a CLI.
    pub fn command_line(&self) -> String {
        match self {
            FlowRequest::Learn { exercise, .. } => {
                format!("lace-studio learn check {}", quoted(exercise))
            }
            other => other
                .core()
                .expect("todo pedido que não é learn é do core")
                .command_line("lace-studio"),
        }
    }

    /// O pedido do core, para tudo que não é a correção de um exercício.
    fn core(&self) -> Option<core::FlowRequest> {
        Some(match self {
            FlowRequest::Build { processors } => core::FlowRequest::Build {
                processors: processors.clone(),
            },
            FlowRequest::Check {
                file,
                processor,
                lint,
            } => core::FlowRequest::Check {
                file: file.clone(),
                processor: processor.clone(),
                lint: *lint,
            },
            FlowRequest::Hierarchy { processor } => core::FlowRequest::Hierarchy {
                processor: processor.clone(),
            },
            FlowRequest::Simulate {
                processor,
                testbench,
                simulator,
                fast,
                timeout_s,
                open_wave,
            } => core::FlowRequest::Simulate {
                processor: processor.clone(),
                testbench: testbench.clone(),
                simulator: *simulator,
                fast: *fast,
                timeout_s: *timeout_s,
                open_wave: *open_wave,
            },
            FlowRequest::Synthesize {
                processor,
                schematic,
                module,
                open_viewer,
            } => core::FlowRequest::Synthesize {
                processor: processor.clone(),
                schematic: *schematic,
                module: module.clone(),
                open_viewer: *open_viewer,
            },
            FlowRequest::Schematic {
                netlist,
                module,
                bus_widths,
            } => core::FlowRequest::Schematic {
                netlist: netlist.clone(),
                module: module.clone(),
                bus_widths: *bus_widths,
            },
            FlowRequest::FpgaBuild => core::FlowRequest::FpgaBuild,
            FlowRequest::FpgaProgram { cable } => core::FlowRequest::FpgaProgram {
                cable: cable.clone(),
            },
            FlowRequest::Learn { .. } => return None,
        })
    }
}

fn quoted(text: &str) -> String {
    if text.is_empty() || text.contains(char::is_whitespace) {
        format!("\"{text}\"")
    } else {
        text.to_owned()
    }
}

/// O resultado de um fluxo: o do core, com o que só o Studio acrescenta.
/// Serializa no mesmo nível (os campos do core e os dois daqui).
#[derive(Debug, Clone, Serialize)]
pub struct FlowOutcome {
    /// O resultado do core.
    #[serde(flatten)]
    pub core: core::FlowOutcome,
    /// A onda que a interface abre numa aba (preferência `wave_viewer` em
    /// `tab`, com o cliente web no bundle), no lugar da janela.
    pub wave_tab: Option<Utf8PathBuf>,
    /// A correção de um exercício (fluxo `learn`).
    pub learn: Option<Grade>,
}

impl Deref for FlowOutcome {
    type Target = core::FlowOutcome;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

/// Roda um fluxo inteiro. `Err` é o Lace não conseguir rodar; um fluxo que
/// rodou e falhou é `Ok` com `succeeded: false`.
pub fn run(
    request: &FlowRequest,
    settings: &Settings,
    spf: &Utf8Path,
    control: &Control,
    progress: ProgressFn<'_>,
) -> IpcResult<FlowOutcome> {
    let command = request.command_line();

    // O exercício é um projeto Lace, mas a correção é a do lace-learn: não
    // depende do projeto aberto.
    if let FlowRequest::Learn { root, exercise } = request {
        let toolchain = toolchain::require(settings)?;
        progress(Progress::Phase(Phase::Learn));
        let tracks = crate::commands::learn::tracks(settings)?;
        let root = lace_learn::Workspace::discover(root)?;
        let mut workspace = lace_learn::Workspace::open(&root, &tracks, None)?;
        let exercise = workspace.track().require(exercise)?.clone();
        // Sem a saída ao vivo: o resumo do testbench é para a correção ler.
        let quiet = Control::new().with_cancel(control.cancel_token().clone());
        let checked = lace_learn::flows::check(
            &toolchain,
            &mut workspace,
            std::slice::from_ref(&exercise),
            &quiet,
            &mut |_| {},
        )?;
        let grade = checked.results.into_iter().next();
        let mut outcome = core::FlowOutcome::new("learn", &command);
        match &grade {
            Some(grade) if grade.verdict == Verdict::Cancelled => outcome.worsen(Status::Cancelled),
            Some(grade) if !grade.solved() => outcome.worsen(Status::Failed),
            _ => {}
        }
        return Ok(FlowOutcome {
            core: outcome,
            wave_tab: None,
            learn: grade,
        });
    }

    let core_request = request.core().expect("não é learn");
    let mut project = crate::state::open_project(spf)?;
    let toolchain = toolchain::require(settings)?;
    // A onda numa aba é do Studio: o core não a abre, e a interface recebe
    // o caminho em `wave_tab`.
    let tab = settings.wave_viewer == "tab" && toolchain.surfer_web_dir().is_ok();
    let mut options = FlowOptions::new(command);
    options.wave = if tab {
        WavePolicy::None
    } else {
        WavePolicy::Window
    };
    let outcome = core::run(
        &toolchain,
        &mut project,
        &core_request,
        &options,
        progress,
        control,
    )?;
    let wave_tab = match &core_request {
        core::FlowRequest::Simulate {
            open_wave: true, ..
        } if tab => outcome
            .simulation
            .as_ref()
            .and_then(|s| s.waveform.as_ref())
            .map(|w| w.path.clone()),
        _ => None,
    };
    Ok(FlowOutcome {
        core: outcome,
        wave_tab,
        learn: None,
    })
}

/// Abre uma onda no surfer-aurora, como o `lace wave`: a onda de um
/// exercício do `lace learn` abre com o layout da correção; as outras, com o
/// layout dos processadores SAPHO quando têm processador. Confere que o
/// Surfer não fechou logo, e uma thread espera ele fechar, para o processo
/// não ficar zumbi.
pub fn open_wave(toolchain: &Toolchain, waveform: &Utf8Path) -> IpcResult<WaveOpened> {
    let layout = match lace_learn::layout::layout_of(waveform) {
        Some(path) => LayoutPolicy::File { path },
        None => LayoutPolicy::Generated,
    };
    let request = WaveRequest {
        source: WaveSource::File {
            path: waveform.to_owned(),
        },
        layout,
    };
    Ok(core::open_wave(toolchain, None, &request)?)
}
