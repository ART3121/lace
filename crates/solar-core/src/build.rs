//! `build`: do fonte de um processador ao Verilog sintetizável.
//!
//! Pipeline, com os argumentos e diretórios de trabalho da AURORA
//! (`js/compilation/builders/*.ts`), `P` = diretório do processador e
//! `T` = diretório temporário:
//!
//! ```text
//! C±: cmmcomp -i <nome>.cmm -n <nome> -p P -m <Macros> -t T [-A] -en   cwd P
//! C:  cpppp   -i <fonte> -o T/pp.cpp -I <Header> -I P/Software         cwd T
//!     cppcomp -i T/pp.cpp -p P -n <nome> -t T                          cwd P
//! os dois:
//!     appcomp -i P/Software/<nome>.asm -t T -en                        cwd T
//!     asmcomp -i <mesmo .asm> -p P -d <HDL> -m <Macros> -t T
//!             -f <MHz> -c <clocks> -en                                 cwd T
//! ```
//!
//! O `-i` do `cmmcomp` é só o nome do arquivo (ele procura em `P/Software/`);
//! os demais recebem caminho completo. O `cpppp` e o `cppcomp` não aceitam
//! `-en`: a flag só existe nos três compiladores do fluxo C±.
//!
//! Tudo é compilado direto no destino final: o `asmcomp` embute caminhos
//! absolutos no Verilog, então compilar em outro lugar e copiar quebraria a
//! simulação.

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::error::{Result, SolarError};
use crate::pipeline::{
    Artifact, ArtifactKind, ArtifactTracker, PlannedStep, Runner, Status, Step, StepReport,
    final_status,
};
use crate::process::Invocation;
use crate::project::{Language, Processor};
use crate::toolchain::{Tool, Toolchain};
use crate::{paths, source};

/// Valores que sobrepõem os do `.spf` só neste build, sem gravar nada no
/// projeto. `None` em um campo mantém o valor do processador.
///
/// ```
/// use solar_core::BuildOptions;
///
/// let mut options = BuildOptions::default();
/// options.frequency_mhz = Some(50);
/// options.clocks = Some(10_000);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BuildOptions {
    /// Frequência de operação em MHz (`-f` do `asmcomp`). Entra no testbench
    /// gerado como o período do clock.
    pub frequency_mhz: Option<u32>,
    /// Quantos clocks o testbench gerado simula (`-c` do `asmcomp`).
    pub clocks: Option<u32>,
    /// Exporta os arrays do programa para a simulação (`-A` do `cmmcomp`).
    /// Só tem efeito em C±.
    pub show_arrays: Option<bool>,
}

/// O resultado de [`build`]. Ver o módulo `pipeline` para os campos comuns a
/// toda operação.
///
/// Em JSON (resumido):
///
/// ```json
/// { "processor": "soma", "language": "cmm", "status": "succeeded",
///   "failed_step": null, "frequency_mhz": 100, "clocks": 2000,
///   "steps": [ { "step": "compile", "tool": "cmmcomp", "...": "..." } ],
///   "diagnostics": [],
///   "artifacts": [ { "kind": "verilog", "path": "/p/soma/Hardware/soma.v",
///                    "required": true, "fresh": true } ] }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildResult {
    /// O processador compilado.
    pub processor: String,
    /// A linguagem do fonte, que decidiu o pipeline.
    pub language: Language,
    /// Como o build terminou.
    pub status: Status,
    /// O passo que falhou ou quebrou, quando for o caso.
    pub failed_step: Option<Step>,
    /// A frequência usada (do `.spf` ou de [`BuildOptions`]).
    pub frequency_mhz: u32,
    /// Os clocks usados (do `.spf` ou de [`BuildOptions`]).
    pub clocks: u32,
    /// Um relatório por compilador executado. Em C±: `compile`,
    /// `pre_assemble`, `assemble`. Em C: `preprocess` antes dos três.
    pub steps: Vec<StepReport>,
    /// Todas as mensagens de todos os passos, na ordem em que saíram. Hoje
    /// os compiladores param no primeiro erro, mas a API já é uma lista.
    pub diagnostics: Vec<Diagnostic>,
    /// Obrigatórios: `Software/<nome>.asm`, `Hardware/<nome>.v`,
    /// `Hardware/<nome>_data.mif`, `Hardware/<nome>_inst.mif` e o testbench
    /// `<temp>/<nome>_tb.v`. Intermediários (`pc_<nome>_mem.txt`,
    /// `trad_cmm.txt`, `trad_opcode.txt`, `cmm_log.txt`, `app_log.txt`,
    /// `pp.cpp`), quando existem.
    pub artifacts: Vec<Artifact>,
}

impl BuildResult {
    /// `status == Succeeded`.
    pub fn succeeded(&self) -> bool {
        self.status == Status::Succeeded
    }

    /// O primeiro diagnóstico de erro, que é o que o usuário precisa ver.
    pub fn first_error(&self) -> Option<&Diagnostic> {
        self.diagnostics
            .iter()
            .find(|d| d.severity == crate::Severity::Error)
    }
}

/// Compila `processor` com `toolchain`: do fonte (`.cmm` ou `.cpp`) ao
/// Verilog sintetizável, às memórias e ao testbench.
///
/// Antes de rodar qualquer compilador, o Solar:
///
/// 1. confere o nome do fonte e o nome declarado nele (as armadilhas abaixo);
/// 2. confere que nenhum arquivo que os compiladores vão abrir ou criar passa
///    de [`YANC_PATH_LIMIT`](crate::YANC_PATH_LIMIT) (259 no Windows, por causa
///    do MAX_PATH; 1000 nos outros sistemas);
/// 3. cria `Software/`, `Hardware/`, `Simulation/` e o diretório temporário
///    ([`Processor::ensure_dirs`]).
///
/// Depois roda os passos em sequência e para no primeiro que falhar.
///
/// # Armadilhas de nome do YANC
///
/// - O `cmmcomp` reabre `Software/<processador>.cmm` no fim para gerar
///   `trad_cmm.txt`, usando o nome do processador e não o do arquivo. Por
///   isso o fonte precisa se chamar `<processador>.<ext>`.
/// - O `asmcomp` batiza os arquivos de `Hardware/` com o `#PRNAME` do fonte,
///   não com o nome passado em `-n`. Um `#PRNAME` diferente gera
///   `Hardware/<outro>.v`; sem `#PRNAME`, gera um nome de lixo. Por isso o
///   Solar exige `#PRNAME <processador>` em C± e, em C, que o
///   `#pragma yanc prname`, se houver, case com o processador.
///
/// # Erros
///
/// Erro de compilação do usuário volta como `Ok` com `status = Failed` e os
/// diagnósticos. `Err` fica para o que impede o build de começar:
///
/// - [`SolarError::InvalidSource`]: fonte ausente, com o nome errado ou com
///   `#PRNAME` ausente ou diferente;
/// - [`SolarError::PathTooLong`]: projeto num diretório fundo demais;
/// - [`SolarError::ComponentMissing`]: falta um compilador do YANC;
/// - [`SolarError::Io`]: um diretório não pôde ser criado;
/// - [`SolarError::Spawn`]: um compilador não pôde ser executado.
///
/// # Exemplo
///
/// ```no_run
/// use solar_core::{BuildOptions, Project, Toolchain, build};
///
/// let toolchain = Toolchain::open("/opt/solar/toolchain")?;
/// let project = Project::open("/home/eu/projetos/soma")?;
/// let result = build(&toolchain, project.require_processor("soma")?, &BuildOptions::default())?;
/// match result.first_error() {
///     None => println!("ok: {:?}", result.artifacts),
///     Some(e) => println!("{}:{}: {}", e.file.as_ref().unwrap(), e.line.unwrap_or(0), e.message),
/// }
/// # Ok::<(), solar_core::SolarError>(())
/// ```
pub fn build(
    toolchain: &Toolchain,
    processor: &Processor,
    options: &BuildOptions,
) -> Result<BuildResult> {
    let _span = tracing::info_span!("build", processor = %processor.name).entered();

    let plan = Plan::new(toolchain, processor, options)?;
    processor.ensure_dirs()?;

    let mut tracker = ArtifactTracker::new();
    for (kind, path, required) in plan.artifact_paths() {
        tracker.expect(kind, path, required);
    }

    let mut runner = Runner::new();
    for planned in plan.steps()? {
        if !runner.run(planned)? {
            break;
        }
    }

    let artifacts = tracker.finish();
    let result = BuildResult {
        processor: processor.name.clone(),
        language: processor.language,
        status: final_status(runner.status, &artifacts),
        failed_step: runner.failed_step,
        frequency_mhz: plan.frequency_mhz,
        clocks: plan.clocks,
        steps: runner.steps,
        diagnostics: runner.diagnostics,
        artifacts,
    };
    tracing::info!(status = ?result.status, "build terminou");
    Ok(result)
}

/// O que [`build_processors`] faz quando um processador não compila.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OnFailure {
    /// Compila todos, para mostrar todos os erros de uma vez. É o que um
    /// "compilar tudo" quer.
    Continue,
    /// Para no primeiro que falhar. É o que se quer antes de simular ou
    /// sintetizar: com um processador quebrado, o resto não serve.
    Stop,
}

/// Compila vários processadores em sequência, na ordem dada.
///
/// `on_result` é chamado depois de cada build, para a interface mostrar o
/// progresso enquanto os próximos compilam. A lista devolvida tem um
/// resultado por processador compilado (menos que `processors` se
/// `on_failure` for [`OnFailure::Stop`] e algum falhar).
///
/// ```no_run
/// use solar_core::{BuildOptions, OnFailure, Project, Toolchain, build_processors};
/// # let toolchain = Toolchain::open("/opt/solar/toolchain")?;
/// let project = Project::open("/p/demo")?;
/// let results = build_processors(
///     &toolchain,
///     project.buildable_processors(),
///     &BuildOptions::default(),
///     OnFailure::Stop,
///     |r| println!("{}: {:?}", r.processor, r.status),
/// )?;
/// # Ok::<(), solar_core::SolarError>(())
/// ```
///
/// # Erros
///
/// Os mesmos de [`build`]; o primeiro `Err` interrompe a sequência.
pub fn build_processors<'a>(
    toolchain: &Toolchain,
    processors: impl IntoIterator<Item = &'a Processor>,
    options: &BuildOptions,
    on_failure: OnFailure,
    mut on_result: impl FnMut(&BuildResult),
) -> Result<Vec<BuildResult>> {
    let mut results = Vec::new();
    for processor in processors {
        let result = build(toolchain, processor, options)?;
        on_result(&result);
        let failed = !result.succeeded();
        results.push(result);
        if failed && on_failure == OnFailure::Stop {
            break;
        }
    }
    Ok(results)
}

/// Tudo o que o build precisa, validado antes de executar qualquer coisa.
struct Plan<'a> {
    toolchain: &'a Toolchain,
    processor: &'a Processor,
    hdl: Utf8PathBuf,
    macros: Utf8PathBuf,
    headers: Utf8PathBuf,
    frequency_mhz: u32,
    clocks: u32,
    show_arrays: bool,
}

impl<'a> Plan<'a> {
    fn new(
        toolchain: &'a Toolchain,
        processor: &'a Processor,
        options: &BuildOptions,
    ) -> Result<Self> {
        validate_source(processor)?;
        let hdl = toolchain.hdl_dir()?;
        let macros = toolchain.macros_dir()?;
        let headers = toolchain.headers_dir()?;
        for dir in [&hdl, &macros, &headers] {
            paths::check_yanc_limit_with_margin(dir, paths::TOOLCHAIN_FILE_MARGIN)?;
        }
        let plan = Plan {
            toolchain,
            processor,
            hdl,
            macros,
            headers,
            frequency_mhz: options.frequency_mhz.unwrap_or(processor.frequency_mhz),
            clocks: options.clocks.unwrap_or(processor.clocks),
            show_arrays: options.show_arrays.unwrap_or(processor.show_arrays),
        };
        // Todo arquivo que algum compilador abre ou cria, com o nome completo.
        paths::check_yanc_limit(&processor.source)?;
        for (_, path, _) in plan.artifact_paths() {
            paths::check_yanc_limit(&path)?;
        }
        Ok(plan)
    }

    fn asm_file(&self) -> Utf8PathBuf {
        self.processor
            .software_dir()
            .join(format!("{}.asm", self.processor.name))
    }

    fn preprocessed_file(&self) -> Utf8PathBuf {
        self.processor.temp_dir.join("pp.cpp")
    }

    fn steps(&self) -> Result<Vec<PlannedStep>> {
        let p = self.processor;
        let tc = self.toolchain;
        let name = p.name.as_str();
        let asm = self.asm_file();
        let mut steps = Vec::new();

        match p.language {
            Language::Cmm => {
                let file_name = p.source.file_name().expect("validado em validate_source");
                let mut inv = Invocation::new(tc.tool(Tool::Cmmcomp)?, &p.dir)
                    .arg("-i")
                    .arg(file_name)
                    .arg("-n")
                    .arg(name)
                    .arg("-p")
                    .path_arg(&p.dir)
                    .arg("-m")
                    .path_arg(&self.macros)
                    .arg("-t")
                    .path_arg(&p.temp_dir);
                if self.show_arrays {
                    inv = inv.arg("-A");
                }
                steps.push(PlannedStep {
                    step: Step::Compile,
                    tool: Tool::Cmmcomp,
                    invocation: inv.arg("-en"),
                    source: Some(p.source.clone()),
                });
            }
            Language::Cpp => {
                let pp = self.preprocessed_file();
                steps.push(PlannedStep {
                    step: Step::Preprocess,
                    tool: Tool::Cpppp,
                    invocation: Invocation::new(tc.tool(Tool::Cpppp)?, &p.temp_dir)
                        .arg("-i")
                        .path_arg(&p.source)
                        .arg("-o")
                        .path_arg(&pp)
                        .arg("-I")
                        .path_arg(&self.headers)
                        .arg("-I")
                        .path_arg(&p.software_dir()),
                    source: None,
                });
                steps.push(PlannedStep {
                    step: Step::Compile,
                    tool: Tool::Cppcomp,
                    invocation: Invocation::new(tc.tool(Tool::Cppcomp)?, &p.dir)
                        .arg("-i")
                        .path_arg(&pp)
                        .arg("-p")
                        .path_arg(&p.dir)
                        .arg("-n")
                        .arg(name)
                        .arg("-t")
                        .path_arg(&p.temp_dir),
                    // O cppcomp cita o próprio arquivo (o pp.cpp) nas mensagens.
                    source: None,
                });
            }
        }

        steps.push(PlannedStep {
            step: Step::PreAssemble,
            tool: Tool::Appcomp,
            invocation: Invocation::new(tc.tool(Tool::Appcomp)?, &p.temp_dir)
                .arg("-i")
                .path_arg(&asm)
                .arg("-t")
                .path_arg(&p.temp_dir)
                .arg("-en"),
            source: Some(asm.clone()),
        });
        steps.push(PlannedStep {
            step: Step::Assemble,
            tool: Tool::Asmcomp,
            invocation: Invocation::new(tc.tool(Tool::Asmcomp)?, &p.temp_dir)
                .arg("-i")
                .path_arg(&asm)
                .arg("-p")
                .path_arg(&p.dir)
                .arg("-d")
                .path_arg(&self.hdl)
                .arg("-m")
                .path_arg(&self.macros)
                .arg("-t")
                .path_arg(&p.temp_dir)
                .arg("-f")
                .arg(self.frequency_mhz.to_string())
                .arg("-c")
                .arg(self.clocks.to_string())
                .arg("-en"),
            source: Some(asm),
        });
        Ok(steps)
    }

    /// Os artefatos que este build deve produzir: (tipo, caminho, obrigatório).
    fn artifact_paths(&self) -> Vec<(ArtifactKind, Utf8PathBuf, bool)> {
        let p = self.processor;
        let n = p.name.as_str();
        let hw = p.hardware_dir();
        let tmp = &p.temp_dir;
        let mut list = vec![
            (ArtifactKind::Assembly, self.asm_file(), true),
            (ArtifactKind::Verilog, hw.join(format!("{n}.v")), true),
            (
                ArtifactKind::DataMemory,
                hw.join(format!("{n}_data.mif")),
                true,
            ),
            (
                ArtifactKind::InstructionMemory,
                hw.join(format!("{n}_inst.mif")),
                true,
            ),
            (ArtifactKind::Testbench, tmp.join(format!("{n}_tb.v")), true),
            (
                ArtifactKind::ProgramCounterMap,
                tmp.join(format!("pc_{n}_mem.txt")),
                false,
            ),
            (
                ArtifactKind::SourceTranslation,
                tmp.join("trad_cmm.txt"),
                false,
            ),
            (
                ArtifactKind::OpcodeTranslation,
                tmp.join("trad_opcode.txt"),
                false,
            ),
            (ArtifactKind::CompilerLog, tmp.join("cmm_log.txt"), false),
            (
                ArtifactKind::PreAssemblerLog,
                tmp.join("app_log.txt"),
                false,
            ),
        ];
        if p.language == Language::Cpp {
            list.push((
                ArtifactKind::PreprocessedSource,
                self.preprocessed_file(),
                false,
            ));
        }
        list
    }
}

/// As armadilhas de nome do YANC, conferidas antes de rodar qualquer coisa.
///
/// 1. O `cmmcomp` reabre `Software/<-n>.cmm` no fim para gerar
///    `trad_cmm.txt`, então o arquivo precisa se chamar `<nome>.cmm`.
/// 2. O `asmcomp` batiza os artefatos pelo nome declarado no fonte
///    (`#PRNAME`), não pelo `-n`. Ver o módulo `source`.
fn validate_source(p: &Processor) -> Result<()> {
    let invalid = |reason: String| SolarError::InvalidSource {
        path: p.source.clone(),
        reason,
    };
    let expected = format!("{}.{}", p.name, p.language.extension());
    if p.source.file_name() != Some(expected.as_str()) {
        return Err(invalid(format!(
            "o arquivo precisa se chamar {expected}: o YANC reabre o fonte pelo nome do processador"
        )));
    }
    let text = match std::fs::read_to_string(&p.source) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(invalid("o arquivo não existe".into()));
        }
        Err(e) => return Err(SolarError::io("lendo fonte", &p.source)(e)),
    };

    match (p.language, source::declared_name(&text, p.language)) {
        (_, Some(declared)) if declared != p.name => Err(invalid(format!(
            "o fonte declara o processador '{declared}', mas ele se chama '{}'; \
             o asmcomp daria esse nome aos arquivos de Hardware/",
            p.name
        ))),
        (Language::Cmm, None) => Err(invalid(format!(
            "falta a diretiva #PRNAME {}: sem ela o asmcomp gera arquivos com nome inválido",
            p.name
        ))),
        _ => Ok(()),
    }
}
