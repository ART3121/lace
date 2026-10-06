//! Os fluxos: compilar, verificar, simular, sintetizar e desenhar.
//!
//! Cada fluxo é a mesma composição de funções do Core que o comando da CLI
//! faz (`crates/lace-cli/src/commands.rs`): antes da simulação e da
//! síntese, compila os processadores (`build_processors`, parando no
//! primeiro que falhar); roda a operação, lê as portas de saída, grava o
//! relatório no histórico do projeto e, na simulação, abre a onda. A
//! verificação não compila: roda o Icarus sobre o que está no disco.
//!
//! A ADR 0001 do Lace diz que regra posta na CLI é regra que a GUI tem de
//! copiar. Esta cópia é o que ela prevê, e está isolada aqui para sair
//! inteira quando o Core ganhar os fluxos compostos (ver
//! `docs/ARCHITECTURE.md`, "Pendências no Lace").
//!
//! Nada aqui fala com a interface: o progresso sai pela função `progress`,
//! e quem a transforma em mensagens é `jobs.rs`.

use std::time::{Duration, SystemTime};

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::history::{self, Operation};
use lace_core::{
    BuildOptions, BuildResult, CheckOptions, CheckResult, Control, DesignTarget, OnFailure,
    Processor, Project, SchematicOptions, SchematicResult, SimulationOptions, SimulationResult,
    Simulator, SynthesisResult, Toolchain, ViewerOptions,
};
use serde::{Deserialize, Serialize};

use crate::error::{IpcError, IpcResult};
use crate::settings::Settings;
use crate::toolchain;

/// Quanto esperar para saber se o surfer-aurora abriu: sem display ou com
/// onda inválida, ele fecha em menos de um segundo (o mesmo da CLI).
pub const SURFER_GRACE: Duration = Duration::from_millis(1500);

/// O que a interface pede.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "flow", rename_all = "snake_case")]
pub enum FlowRequest {
    /// `lace build [-p NOME]...`: compila os processadores pedidos (vazio:
    /// todos), mesmo que um falhe, para mostrar todos os erros.
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
        /// Só este processador: compila ele e verifica o Verilog dele e o
        /// testbench que o YANC gerou (`CheckOptions::processor`), no lugar
        /// do projeto. É o alvo da barra de ferramentas.
        #[serde(default)]
        processor: Option<String>,
        /// Acrescenta o `verilator --lint-only`.
        #[serde(default)]
        lint: bool,
    },
    /// `lace sim [TESTBENCH] [-p NOME] [--verilator] [--timeout S] [--open]`.
    Simulate {
        /// Simula este processador com o testbench do YANC; `None`, a
        /// simulação do projeto.
        #[serde(default)]
        processor: Option<String>,
        /// Troca o testbench da simulação do projeto antes de simular.
        #[serde(default)]
        testbench: Option<Utf8PathBuf>,
        /// Icarus ou Verilator.
        simulator: Simulator,
        /// Prazo do passo `simulate`, em segundos.
        #[serde(default)]
        timeout_s: Option<u64>,
        /// Abrir a onda no surfer-aurora se a simulação der certo.
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
    },
    /// Desenha outro módulo de um netlist que já existe, sem sintetizar de
    /// novo. Não tem comando equivalente na CLI e não grava relatório.
    Schematic {
        /// O `hierarchy.json` da síntese.
        netlist: Utf8PathBuf,
        /// O módulo.
        module: String,
        /// Escrever a largura dos barramentos nas arestas.
        #[serde(default = "yes")]
        bus_widths: bool,
    },
}

fn yes() -> bool {
    true
}

impl FlowRequest {
    /// O nome curto do fluxo, como vai no resultado.
    pub fn name(&self) -> &'static str {
        match self {
            FlowRequest::Build { .. } => "build",
            FlowRequest::Check { .. } => "check",
            FlowRequest::Simulate { .. } => "simulate",
            FlowRequest::Synthesize { .. } => "synthesize",
            FlowRequest::Schematic { .. } => "schematic",
        }
    }

    /// O comando equivalente da CLI, para o relatório e para o console. O
    /// prefixo é `lace-studio` porque quem rodou foi o Studio, não a CLI.
    pub fn command_line(&self) -> String {
        let mut parts = vec!["lace-studio".to_owned()];
        match self {
            FlowRequest::Build { processors } => {
                parts.push("build".into());
                for p in processors {
                    parts.extend(["-p".into(), p.clone()]);
                }
            }
            FlowRequest::Check {
                file,
                processor,
                lint,
            } => {
                parts.push("check".into());
                parts.extend(file.iter().map(|f| quoted(f.as_str())));
                if let Some(p) = processor {
                    parts.extend(["-p".into(), p.clone()]);
                }
                if *lint {
                    parts.push("--lint".into());
                }
            }
            FlowRequest::Simulate {
                processor,
                testbench,
                simulator,
                timeout_s,
                open_wave,
            } => {
                parts.push("sim".into());
                parts.extend(testbench.iter().map(|t| quoted(t.as_str())));
                if let Some(p) = processor {
                    parts.extend(["-p".into(), p.clone()]);
                }
                if *simulator == Simulator::Verilator {
                    parts.push("--verilator".into());
                }
                if let Some(t) = timeout_s {
                    parts.extend(["--timeout".into(), t.to_string()]);
                }
                if *open_wave {
                    parts.push("--open".into());
                }
            }
            FlowRequest::Synthesize {
                processor,
                schematic,
                module,
            } => {
                parts.push("synth".into());
                if let Some(p) = processor {
                    parts.extend(["-p".into(), p.clone()]);
                }
                if *schematic {
                    parts.push("--svg".into());
                }
                if let Some(m) = module {
                    parts.extend(["--module".into(), m.clone()]);
                }
            }
            FlowRequest::Schematic { module, .. } => {
                parts.extend(["schematic".into(), module.clone()]);
            }
        }
        parts.join(" ")
    }
}

fn quoted(text: &str) -> String {
    if text.is_empty() || text.contains(char::is_whitespace) {
        format!("\"{text}\"")
    } else {
        text.to_owned()
    }
}

/// O resultado de um fluxo. Os campos de uma fase que não rodou ficam
/// `null` (ou vazios), como no `--json` da CLI.
#[derive(Debug, Clone, Serialize)]
pub struct FlowOutcome {
    /// O fluxo (`build`, `check`, `simulate`, `synthesize`, `schematic`).
    pub flow: &'static str,
    /// O comando equivalente.
    pub command: String,
    /// Tudo deu certo.
    pub succeeded: bool,
    /// Os builds feitos antes (ou o próprio build).
    pub builds: Vec<BuildResult>,
    /// O resultado da verificação.
    pub check: Option<CheckResult>,
    /// O resultado da simulação.
    pub simulation: Option<SimulationResult>,
    /// Os valores de cada porta de saída do processador simulado.
    pub outputs: Vec<PortValues>,
    /// O resultado da síntese.
    pub synthesis: Option<SynthesisResult>,
    /// O resultado do esquemático.
    pub schematic: Option<SchematicResult>,
    /// Por que o esquemático não foi desenhado depois da síntese (o módulo
    /// pedido não está no netlist, `module_not_found`); a síntese em si vale.
    pub schematic_error: Option<IpcError>,
    /// A onda aberta no surfer-aurora.
    pub wave: Option<WaveOpened>,
    /// A onda que a interface abre numa aba (preferência `wave_viewer` em
    /// `tab`, com o cliente web no bundle), no lugar da janela.
    pub wave_tab: Option<Utf8PathBuf>,
    /// Por que a onda não abriu (a simulação em si deu certo).
    pub wave_error: Option<IpcError>,
    /// O relatório gravado no histórico (`run-000042`).
    pub report: Option<String>,
    /// Por que o relatório não foi gravado (a operação não falha por isso).
    pub report_error: Option<String>,
}

impl FlowOutcome {
    fn new(request: &FlowRequest) -> Self {
        FlowOutcome {
            flow: request.name(),
            command: request.command_line(),
            succeeded: false,
            builds: Vec::new(),
            check: None,
            simulation: None,
            outputs: Vec::new(),
            synthesis: None,
            schematic: None,
            schematic_error: None,
            wave: None,
            wave_tab: None,
            wave_error: None,
            report: None,
            report_error: None,
        }
    }
}

/// Os valores de uma porta de saída.
#[derive(Debug, Clone, Serialize)]
pub struct PortValues {
    /// A porta.
    pub port: u32,
    /// O `output_<porta>.txt`.
    pub path: Utf8PathBuf,
    /// Os valores, um por linha do arquivo.
    pub values: Vec<i64>,
    /// Por que não deu para ler (linha que não é inteiro).
    pub error: Option<IpcError>,
}

/// A onda aberta no surfer-aurora.
#[derive(Debug, Clone, Serialize)]
pub struct WaveOpened {
    /// O arquivo da onda.
    pub waveform: Utf8PathBuf,
    /// O PID do surfer-aurora.
    pub pid: u32,
    /// O log do surfer-aurora.
    pub log: Utf8PathBuf,
    /// Os processadores compilados de novo depois desta simulação
    /// (`WaveProcessor::outdated`): o PC e a linha aparecem como números.
    pub outdated: Vec<String>,
}

/// A fase que começou, para a interface mostrar o que está acontecendo.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Compilando processadores (YANC).
    Build,
    /// Verificando o Verilog.
    Check,
    /// Simulando.
    Simulate,
    /// Sintetizando.
    Synthesize,
    /// Desenhando o esquemático.
    Schematic,
    /// Abrindo a onda.
    Wave,
}

/// O progresso de um fluxo.
pub enum Progress {
    /// Uma fase começou.
    Phase(Phase),
    /// Um processador terminou de compilar.
    Built(BuildResult),
}

/// Roda um fluxo inteiro. `Err` é o Lace não conseguir rodar; um fluxo que
/// rodou e falhou é `Ok` com `succeeded: false`.
pub fn run(
    request: &FlowRequest,
    settings: &Settings,
    spf: &Utf8Path,
    control: &Control,
    progress: &dyn Fn(Progress),
) -> IpcResult<FlowOutcome> {
    let started = SystemTime::now();
    let mut outcome = FlowOutcome::new(request);
    let mut project = Project::open(spf)?;

    if let FlowRequest::Schematic {
        netlist,
        module,
        bus_widths,
    } = request
    {
        let toolchain = toolchain::require(settings)?;
        progress(Progress::Phase(Phase::Schematic));
        let mut options = SchematicOptions::default();
        options.bus_widths = *bus_widths;
        let result = lace_core::render_schematic(&toolchain, netlist, module, &options, control)?;
        outcome.succeeded = result.succeeded();
        outcome.schematic = Some(result);
        return Ok(outcome);
    }

    if let FlowRequest::Build { processors } = request
        && processors.is_empty()
        && project.processors().is_empty()
    {
        // Um projeto só de Verilog não tem o que compilar, e isso não é
        // erro (a CLI avisa e sai com 0).
        outcome.succeeded = true;
        return Ok(outcome);
    }

    let toolchain = toolchain::require(settings)?;
    if let FlowRequest::Simulate {
        testbench: Some(testbench),
        ..
    } = request
    {
        project.set_testbench(testbench)?;
    }
    let project = project;

    // Os processadores a compilar antes, e se a falha de um para o resto.
    let (targets, on_failure): (Vec<&Processor>, OnFailure) = match request {
        FlowRequest::Build { processors } if processors.is_empty() => {
            (project.processors().iter().collect(), OnFailure::Continue)
        }
        FlowRequest::Build { processors } => (
            processors
                .iter()
                .map(|name| project.require_processor(name))
                .collect::<Result<_, _>>()?,
            OnFailure::Continue,
        ),
        // A verificação só roda o Icarus sobre o que está no disco.
        FlowRequest::Check { .. } => (Vec::new(), OnFailure::Stop),
        FlowRequest::Simulate {
            processor: Some(name),
            ..
        }
        | FlowRequest::Synthesize {
            processor: Some(name),
            ..
        } => (vec![project.require_processor(name)?], OnFailure::Stop),
        _ => (project.buildable_processors(), OnFailure::Stop),
    };

    if !targets.is_empty() {
        progress(Progress::Phase(Phase::Build));
    }
    outcome.builds = lace_core::build_processors(
        &toolchain,
        targets,
        &BuildOptions::default(),
        on_failure,
        control,
        |result| progress(Progress::Built(result.clone())),
    )?;
    let built = outcome.builds.iter().all(BuildResult::succeeded);
    let mut operation = Operation::new(&outcome.command, started).with_builds(&outcome.builds);

    if !built || matches!(request, FlowRequest::Build { .. }) {
        outcome.succeeded = built;
        (outcome.report, outcome.report_error) = record(&project, &toolchain, &operation);
        return Ok(outcome);
    }

    match request {
        FlowRequest::Check {
            file,
            processor,
            lint,
        } => {
            progress(Progress::Phase(Phase::Check));
            let mut options = CheckOptions::default();
            options.file = file.clone();
            options.processor = processor.clone();
            options.lint = *lint;
            let result = lace_core::check(&toolchain, &project, &options, control)?;
            outcome.succeeded = result.succeeded();
            outcome.check = Some(result);
        }
        FlowRequest::Simulate {
            processor,
            simulator,
            timeout_s,
            ..
        } => {
            progress(Progress::Phase(Phase::Simulate));
            let mut options = SimulationOptions::new(*simulator);
            options.timeout = timeout_s.map(Duration::from_secs);
            let processor = processor
                .as_deref()
                .map(|name| project.require_processor(name))
                .transpose()?;
            let result = match processor {
                Some(p) => lace_core::simulate(&toolchain, p, &options, control)?,
                None => lace_core::simulate_project(&toolchain, &project, &options, control)?,
            };
            if let Some(p) = processor
                && result.succeeded()
            {
                outcome.outputs = port_values(p, &result);
            }
            outcome.succeeded = result.succeeded();
            outcome.simulation = Some(result);
        }
        FlowRequest::Synthesize {
            processor,
            schematic,
            module,
        } => {
            progress(Progress::Phase(Phase::Synthesize));
            let target = match processor {
                Some(name) => DesignTarget::Processor(name.clone()),
                None => DesignTarget::TopLevel,
            };
            let result = lace_core::synthesize(&toolchain, &project, &target, control)?;
            if *schematic && let Some(netlist) = &result.netlist {
                progress(Progress::Phase(Phase::Schematic));
                let module = module.as_deref().unwrap_or(&result.top);
                match lace_core::render_schematic(
                    &toolchain,
                    netlist,
                    module,
                    &SchematicOptions::default(),
                    control,
                ) {
                    Ok(svg) => outcome.schematic = Some(svg),
                    // Um módulo fora do netlist: a síntese e as estatísticas
                    // ficam, e a interface explica e oferece outro módulo.
                    Err(error @ lace_core::LaceError::ModuleNotFound { .. }) => {
                        outcome.schematic_error = Some(error.into());
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            outcome.succeeded =
                result.succeeded() && outcome.schematic.as_ref().is_none_or(|s| s.succeeded());
            outcome.synthesis = Some(result);
        }
        FlowRequest::Build { .. } | FlowRequest::Schematic { .. } => unreachable!(),
    }

    if let Some(check) = &outcome.check {
        operation = operation.with_check(check);
    }
    if let Some(simulation) = &outcome.simulation {
        operation = operation.with_simulation(simulation);
    }
    if let Some(synthesis) = &outcome.synthesis {
        operation = operation.with_synthesis(synthesis);
    }
    if let Some(schematic) = &outcome.schematic {
        operation = operation.with_schematic(schematic);
    }
    let (report, report_error) = record(&project, &toolchain, &operation);
    (outcome.report, outcome.report_error) = (report, report_error);

    // Como o botão Wave da AURORA e o `lace sim --open`: a onda abre
    // quando a simulação a entrega (deu certo, ou um teste cocotb falhou com
    // a simulação indo até o fim) e o relatório foi gravado.
    if let FlowRequest::Simulate {
        open_wave: true, ..
    } = request
        && let Some(wave) = outcome
            .simulation
            .as_ref()
            .and_then(|s| s.waveform.as_ref())
    {
        progress(Progress::Phase(Phase::Wave));
        if settings.wave_viewer == "tab" && toolchain.surfer_web_dir().is_ok() {
            outcome.wave_tab = Some(wave.path.clone());
        } else {
            match open_wave(&toolchain, &wave.path) {
                Ok(opened) => outcome.wave = Some(opened),
                Err(error) => outcome.wave_error = Some(error),
            }
        }
    }
    Ok(outcome)
}

/// Grava a operação no histórico e devolve o relatório ou o motivo de não
/// ter gravado. Uma falha ao gravar não muda o resultado do fluxo.
fn record(
    project: &Project,
    toolchain: &Toolchain,
    operation: &Operation,
) -> (Option<String>, Option<String>) {
    match history::record(project, toolchain, operation) {
        Ok(record) => (Some(record.id), None),
        Err(error) => (None, Some(error.to_string())),
    }
}

/// Os valores de cada porta de saída que a simulação do processador
/// escreveu, na ordem das portas. A porta sai do nome do arquivo, como na
/// CLI (`port_values`), porque o `Processor` não guarda o `#NUIOOU`.
fn port_values(processor: &Processor, result: &SimulationResult) -> Vec<PortValues> {
    let dir = processor.simulation_dir();
    let mut ports: Vec<u32> = result
        .outputs
        .iter()
        .filter(|path| path.parent() == Some(dir.as_path()))
        .filter_map(|path| {
            path.file_name()?
                .strip_prefix("output_")?
                .strip_suffix(".txt")?
                .parse()
                .ok()
        })
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
        .into_iter()
        .map(|port| {
            let (values, error) = match processor.read_output_values(port) {
                Ok(values) => (values, None),
                Err(error) => (Vec::new(), Some(IpcError::from(error))),
            };
            PortValues {
                port,
                path: processor.output_path(port),
                values,
                error,
            }
        })
        .collect()
}

/// Abre uma onda no surfer-aurora, com o layout dos processadores SAPHO
/// quando ela tem processador (como o `lace wave`), e confere que ele não
/// fechou logo. Sem layout (a onda não é VCD nem FST, ou ele falhou), abre a
/// onda crua.
///
/// O Surfer continua aberto depois desta chamada; uma thread espera ele
/// fechar, para o processo não ficar zumbi (`<defunct>`) até o Studio sair.
pub fn open_wave(toolchain: &Toolchain, waveform: &Utf8Path) -> IpcResult<WaveOpened> {
    let mut outdated = Vec::new();
    let options = match lace_core::prepare_wave_layout(waveform) {
        Ok(Some(layout)) => {
            outdated = layout
                .layout
                .processors
                .iter()
                .filter(|p| p.outdated)
                .map(|p| p.processor.clone())
                .collect();
            ViewerOptions::with_layout(&layout)
        }
        Ok(None) => ViewerOptions::default(),
        Err(error) => {
            tracing::warn!("No processor layout for {waveform}: {error}");
            ViewerOptions::default()
        }
    };
    let mut surfer = lace_core::open_waveform(toolchain, waveform, &options)?;
    surfer.ensure_started(SURFER_GRACE)?;
    let opened = WaveOpened {
        waveform: waveform.to_owned(),
        pid: surfer.id(),
        log: surfer.log_file().to_owned(),
        outdated,
    };
    std::thread::spawn(move || {
        let _ = surfer.wait();
    });
    Ok(opened)
}
