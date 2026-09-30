//! Solar-Core: a API que orquestra a toolchain do processador SAPHO.
//!
//! O Solar substitui a lógica de orquestração da AURORA por uma biblioteca.
//! As interfaces (a CLI `solar` hoje, uma GUI e um LSP depois) só chamam estas
//! funções e mostram o resultado; nenhuma regra de negócio mora nelas.
//!
//! # O que a API faz
//!
//! | Ferramenta | Função | Resultado |
//! |---|---|---|
//! | arquivos e pastas | [`Project::create`], [`Project::add_processor`], [`Project::add_file`], [`Project::set_top_level`], [`Project::set_testbench`], [`Processor::write_input`] | `.spf` e diretórios no formato da AURORA |
//! | YANC | [`build`] | Verilog, memórias e testbench de um processador |
//! | Icarus Verilog | [`simulate`], [`simulate_project`], [`check_syntax`] | [`SimulationResult`], [`CheckResult`] |
//! | Verilator | [`simulate`], [`simulate_project`] | [`SimulationResult`] |
//! | Yosys | [`synthesize`] | [`SynthesisResult`] (netlist JSON) |
//! | Yosys + Graphviz | [`render_schematic`] | [`SchematicResult`] (SVG) |
//! | surfer-aurora | [`open_waveform`] | [`RunningProcess`] |
//!
//! Todas as ferramentas saem do bundle versionado instalado com o Solar
//! ([`Toolchain`]); nenhuma do `PATH`. A exceção é o compilador C++ do
//! Verilator ([`SystemCompiler`]).
//!
//! # Fluxo típico
//!
//! ```no_run
//! use solar_core::*;
//!
//! // 1. O bundle instalado com o Solar. Nada vem do PATH.
//! let exe = camino::Utf8PathBuf::try_from(std::env::current_exe()?)?;
//! let toolchain = Toolchain::locate(&exe)?;
//!
//! // 2. O projeto: um .spf da AURORA.
//! let project = Project::open("/home/eu/projetos/soma")?;
//! let soma = project.require_processor("soma")?;
//!
//! // 3. Compilar. Erro do usuário não é Err: vem nos diagnósticos.
//! let built = build(&toolchain, soma, &BuildOptions::default())?;
//! if let Some(e) = built.first_error() {
//!     eprintln!("{:?}:{:?}: {}", e.file, e.line, e.message);
//!     return Ok(());
//! }
//!
//! // 4. Simular, desenhar, abrir a onda.
//! let sim = simulate(&toolchain, soma, &SimulationOptions::new(Simulator::Icarus))?;
//! let synth = synthesize(&toolchain, &project, &DesignTarget::Processor("soma".into()))?;
//! if let Some(netlist) = &synth.netlist {
//!     render_schematic(&toolchain, netlist, "soma", &SchematicOptions::default())?;
//! }
//! if let Some(wave) = &sim.waveform {
//!     open_waveform(&toolchain, &wave.path, &ViewerOptions::default())?;
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Dois tipos de falha
//!
//! Toda operação devolve `Result<Resultado, SolarError>`:
//!
//! - `Err(`[`SolarError`]`)`: o Solar não conseguiu rodar (toolchain, projeto,
//!   I/O). É problema do ambiente.
//! - `Ok(resultado)` com [`Status`] diferente de `Succeeded`: as ferramentas
//!   rodaram e recusaram a entrada. É feedback para o usuário, com arquivo e
//!   linha em [`Diagnostic`].
//!
//! # Anatomia de um resultado
//!
//! Todos os resultados de operação têm `status`, `failed_step`, `steps`
//! ([`StepReport`]: comando exato, CWD, ambiente, stdout, stderr, duração) e
//! `diagnostics`; os que produzem arquivos têm `artifacts` ([`Artifact`], com
//! `fresh` dizendo se o arquivo foi escrito agora). Todos implementam
//! `Serialize`: o JSON da CLI (`solar ... --json`) é exatamente esse.
//!
//! # Disco
//!
//! ```text
//! <raiz>/
//!   <projeto>.spf
//!   <processador>/Software/   fonte .cmm ou .cpp, .asm gerado
//!   <processador>/Hardware/   .v e .mif gerados
//!   <processador>/Simulation/ input_<n>.txt (usuário), output_<n>.txt (simulação)
//!   .solar/Temp/<processador>/  intermediários do YANC, testbench, onda, .vvp
//!   .solar/Temp/                simulação do projeto, síntese (synth/<topo>/)
//! ```
//!
//! # Garantias
//!
//! - **Sem I/O de console.** O Core nunca escreve no stdout ou no stderr, não
//!   chama `exit` e não entra em pânico em caminho de erro esperado. O log
//!   sai por [`tracing`](https://docs.rs/tracing); quem configura o
//!   subscriber é o cliente.
//! - **Só o bundle.** Todo programa sai do bundle, num caminho fixo por
//!   plataforma; nada do `PATH`, nada configurável. O processo filho recebe um
//!   ambiente vazio, mais o mínimo que cada ferramenta exige (ver
//!   `docs/BUNDLE.md`).
//! - **Nunca sobrescreve código.** Criar processador ou arquivo com conteúdo
//!   recusa se o arquivo já existir. Artefatos gerados (`Hardware/`, `.asm`,
//!   `Temp/`) são reescritos a cada operação.
//! - **Compatível com a AURORA.** O `.spf` é lido e gravado no formato dela,
//!   preservando o que o Solar não entende.
//! - **Síncrono.** Cada operação bloqueia até as ferramentas terminarem
//!   (menos [`open_waveform`]). Numa GUI, chame numa thread; os tipos são
//!   `Send`. Não há cancelamento nem progresso nesta versão.
//!
//! # Plataformas
//!
//! Linux x64, macOS arm64 e Windows x64, um bundle para cada. Conferido no
//! Linux com o bundle 2026.09.29; no Windows (pelo Wine) os processos, o build
//! e a regressão do YANC; no macOS, a compilação. O que muda por sistema está
//! em `docs/API.md`, seção 9.1.

#![deny(clippy::print_stdout, clippy::print_stderr)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod build;
mod diagnostics;
mod error;
mod files;
mod paths;
mod pipeline;
mod process;
mod project;
mod simulate;
mod source;
mod spf;
mod synth;
mod toolchain;
mod wave;

pub use build::{BuildOptions, BuildResult, OnFailure, build, build_processors};
pub use diagnostics::{Diagnostic, Severity};
pub use error::{Result, SolarError};
pub use files::{FileRole, ProjectFile, read_data_file};
pub use paths::YANC_PATH_LIMIT;
pub use pipeline::{Artifact, ArtifactKind, Status, Step, StepReport};
pub use process::{Invocation, RunningProcess, Termination};
pub use project::{
    DEFAULT_CLOCKS, DEFAULT_FREQUENCY_MHZ, Language, NewProcessor, Processor, ProcessorConfig,
    Project,
};
pub use simulate::{
    SimulationOptions, SimulationResult, Simulator, Waveform, WaveformFormat, missing_inputs,
    simulate, simulate_project,
};
pub use synth::{
    CheckResult, DesignTarget, SchematicOptions, SchematicResult, SynthesisResult, check_syntax,
    render_schematic, synthesize,
};
pub use toolchain::{
    BUNDLE_SCHEMA, BundleComponent, BundleManifest, COMPONENTS_DIR, FileMismatch, MANIFEST_FILE,
    Platform, SystemCompiler, Tool, Toolchain, component,
};
pub use wave::{ViewerOptions, open_waveform};
