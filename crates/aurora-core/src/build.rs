//! `build`: do fonte de um processador ao Verilog sintetizável, com o YANC
//! 7.1 ou mais novo (ver o módulo `yanc`).
//!
//! `D` = pasta do processador (`<raiz>/<nome>`, o fonte e os arquivos da
//! pessoa) e `O` = pasta de saída (`D/<nome>`), a estrutura do próprio YANC:
//!
//! ```text
//! C±: cmmcomp D/<nome>.cmm -o O [--wave-arrays] --lang=en     cwd D
//! C:  cppcomp D/<nome>.cpp -o O --lang=en                     cwd D
//! os dois:
//!     asmcomp O/<nome>.asm -f <MHz> -c <clocks> --lang=en     cwd O
//! ```
//!
//! O compilador grava em `O` o `.asm`, `prog_log.txt` (com a pasta do fonte,
//! `srcdir`, onde o `asmcomp` acha os arquivos que inicializam arrays),
//! `src_lines.txt` e `pc_<nome>_mem.txt`; o `asmcomp`, ao lado do `.asm`, o
//! Verilog, as memórias, o testbench, `app_log.txt` e `trad_opcode.txt`. O
//! Verilog e o testbench citam os arquivos pelo caminho relativo a `O`: as
//! memórias, a tabela do PC, `output_<n>.txt` e `toma_<n>.txt` pelo nome,
//! `input_<n>.txt` e `cade_<n>.txt` como `../input_<n>.txt`. Nada é copiado
//! nem reescrito: quem roda o testbench roda em `O` ([`simulate`](crate::simulate)),
//! e quem lê o Verilog em outro lugar copia antes as memórias e a tabela
//! ([`Processor::files_read_by_verilog`]).
//!
//! O `cppcomp` procura os `#include` na pasta do fonte e, por último, no
//! `Header/` ao lado dele. A AURORA pede as mensagens em inglês (`--lang=en`)
//! mesmo com `YANC_LANG=pt` no ambiente: o parser trabalha sobre um idioma
//! só.

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::control::Control;
use crate::diagnostics::Diagnostic;
use crate::error::{AuroraError, Result};
use crate::pipeline::{
    Artifact, ArtifactKind, ArtifactTracker, PlannedStep, Runner, Status, Step, StepReport,
    final_status,
};
use crate::process::Invocation;
use crate::project::{Language, Processor};
use crate::toolchain::{Tool, Toolchain};
use crate::{paths, yanc};

/// Valores que sobrepõem os do `.spf` só neste build, sem gravar nada no
/// projeto. `None` em um campo mantém o valor do processador.
///
/// ```
/// use aurora_core::BuildOptions;
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
    /// Cada elemento de array vira um sinal da onda (`--wave-arrays` do
    /// `cmmcomp`). Só tem efeito em C±.
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
///   "artifacts": [ { "kind": "verilog", "path": "/p/soma/soma/soma.v",
///                    "required": true, "fresh": true } ] }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
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
    /// Um relatório por compilador executado: `compile` e `assemble`
    /// ([`build_steps`]).
    pub steps: Vec<StepReport>,
    /// Todas as mensagens de todos os passos, na ordem em que saíram. Hoje
    /// os compiladores param no primeiro erro, mas a API já é uma lista.
    pub diagnostics: Vec<Diagnostic>,
    /// Obrigatórios, na pasta de saída: `<nome>.asm`, `<nome>.v`,
    /// `<nome>_data.mif`, `<nome>_inst.mif` e o testbench `<nome>_tb.v`.
    /// Intermediários (`pc_<nome>_mem.txt`, `src_lines.txt`,
    /// `trad_opcode.txt`, `prog_log.txt`, `app_log.txt`), quando existem.
    pub artifacts: Vec<Artifact>,
    /// Quanto o build levou, do começo ao fim, em milissegundos.
    pub duration_ms: u64,
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
/// Verilog sintetizável, às memórias e ao testbench, todos na pasta de saída
/// do processador (`<nome>/<nome>/`).
///
/// Antes de rodar qualquer compilador, a AURORA confere:
///
/// 1. que o YANC do bundle é o 7.1 ou mais novo;
/// 2. que o processador está na estrutura do YANC 7.1, e não na de antes,
///    com `Software/`, `Hardware/` e `Simulation/`
///    ([`Project::convert_layout`](crate::Project::convert_layout));
/// 3. que o fonte existe e se chama `<processador>.<ext>`: o YANC dá ao
///    processador o nome do arquivo;
/// 4. que nenhum arquivo que os compiladores vão abrir ou criar passa de
///    [`YANC_PATH_LIMIT`](crate::YANC_PATH_LIMIT) (259 no Windows, por causa
///    do MAX_PATH; 1000 nos outros sistemas);
///
/// e cria a pasta de saída e a temporária ([`Processor::ensure_dirs`]).
/// Depois roda os passos em sequência e para no primeiro que falhar, ou
/// quando `control` pede o cancelamento (`status = Cancelled`).
///
/// Um `#PRNAME` ou `#pragma yanc prname` que sobrou de um fonte da 6.x é erro
/// do compilador, que diz para apagar a linha; a AURORA deixa a mensagem vir
/// dele, com a linha, como qualquer erro do fonte.
///
/// # Erros
///
/// Erro de compilação do usuário volta como `Ok` com `status = Failed` e os
/// diagnósticos. `Err` fica para o que impede o build de começar:
///
/// - [`AuroraError::YancTooOld`]: o bundle tem um YANC até a 6.x;
/// - [`AuroraError::LegacyLayout`]: o processador está na estrutura de antes
///   do YANC 7.1, com o fonte fora do lugar novo
///   ([`Processor::needs_conversion`](crate::Processor::needs_conversion));
/// - [`AuroraError::InvalidName`]: processador do `.spf` com nome que o YANC
///   não compila (ver [`validate_processor_name`](crate::validate_processor_name));
/// - [`AuroraError::InvalidSource`]: fonte ausente ou com o nome errado;
/// - [`AuroraError::PathTooLong`]: projeto num diretório fundo demais;
/// - [`AuroraError::ComponentMissing`]: falta um compilador do YANC;
/// - [`AuroraError::Io`]: um diretório não pôde ser criado;
/// - [`AuroraError::Spawn`]: um compilador não pôde ser executado.
///
/// # Exemplo
///
/// ```no_run
/// use aurora_core::{BuildOptions, Control, Project, Toolchain, build};
///
/// let toolchain = Toolchain::open("/opt/aurora/toolchain")?;
/// let project = Project::open("/home/eu/projetos/soma")?;
/// let soma = project.require_processor("soma")?;
/// let result = build(&toolchain, soma, &BuildOptions::default(), &Control::default())?;
/// match result.first_error() {
///     None => println!("ok: {:?}", result.artifacts),
///     Some(e) => println!("{}:{}: {}", e.file.as_ref().unwrap(), e.line.unwrap_or(0), e.message),
/// }
/// # Ok::<(), aurora_core::AuroraError>(())
/// ```
pub fn build(
    toolchain: &Toolchain,
    processor: &Processor,
    options: &BuildOptions,
    control: &Control,
) -> Result<BuildResult> {
    let _span = tracing::info_span!("build", processor = %processor.name).entered();
    let started = std::time::Instant::now();

    let plan = Plan::new(toolchain, processor, options)?;
    processor.ensure_dirs()?;
    let _lock = crate::paths::run_lock(&processor.temp_dir, &processor.name)?;

    let mut tracker = ArtifactTracker::new();
    for (kind, path, required) in plan.artifact_paths() {
        tracker.expect(kind, path, required);
    }

    let mut runner = Runner::new(control);
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
        duration_ms: crate::pipeline::elapsed_ms(started),
    };
    tracing::info!(status = ?result.status, "Build finished");
    Ok(result)
}

/// Os passos que [`build`] roda num processador em `language`, na ordem,
/// com a ferramenta de cada um. Serve para mostrar o que não chegou a rodar
/// quando um passo falha.
///
/// ```
/// use aurora_core::{Language, Step, Tool, build_steps};
///
/// assert_eq!(
///     build_steps(Language::Cmm),
///     [(Step::Compile, Tool::Cmmcomp), (Step::Assemble, Tool::Asmcomp)]
/// );
/// ```
pub fn build_steps(language: Language) -> &'static [(Step, Tool)] {
    match language {
        Language::Cpp => &[
            (Step::Compile, Tool::Cppcomp),
            (Step::Assemble, Tool::Asmcomp),
        ],
        _ => &[
            (Step::Compile, Tool::Cmmcomp),
            (Step::Assemble, Tool::Asmcomp),
        ],
    }
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
/// `on_failure` for [`OnFailure::Stop`] e algum falhar, ou se o
/// cancelamento for pedido: um build cancelado encerra a sequência mesmo com
/// [`OnFailure::Continue`]).
///
/// ```no_run
/// use aurora_core::{BuildOptions, Control, OnFailure, Project, Toolchain, build_processors};
/// # let toolchain = Toolchain::open("/opt/aurora/toolchain")?;
/// let project = Project::open("/p/demo")?;
/// let results = build_processors(
///     &toolchain,
///     project.buildable_processors(),
///     &BuildOptions::default(),
///     OnFailure::Stop,
///     &Control::default(),
///     |r| println!("{}: {:?}", r.processor, r.status),
/// )?;
/// # Ok::<(), aurora_core::AuroraError>(())
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
    control: &Control,
    mut on_result: impl FnMut(&BuildResult),
) -> Result<Vec<BuildResult>> {
    let mut results = Vec::new();
    for processor in processors {
        let result = build(toolchain, processor, options, control)?;
        on_result(&result);
        let failed = !result.succeeded();
        let cancelled = result.status == Status::Cancelled;
        results.push(result);
        if cancelled || (failed && on_failure == OnFailure::Stop) {
            break;
        }
    }
    Ok(results)
}

/// Tudo o que o build precisa, validado antes de executar qualquer coisa.
struct Plan<'a> {
    toolchain: &'a Toolchain,
    processor: &'a Processor,
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
        yanc::require(toolchain)?;
        if processor.needs_conversion() {
            return Err(AuroraError::LegacyLayout {
                processor: processor.name.clone(),
                dir: processor.dir.clone(),
            });
        }
        validate_source(processor)?;
        // As pastas da biblioteca que as ferramentas acham ao lado delas: um
        // bundle sem elas para aqui, e não no meio do build.
        for dir in [
            toolchain.hdl_dir()?,
            toolchain.macros_dir()?,
            toolchain.headers_dir()?,
        ] {
            paths::check_yanc_limit_with_margin(&dir, paths::TOOLCHAIN_FILE_MARGIN)?;
        }
        let plan = Plan {
            toolchain,
            processor,
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

    fn steps(&self) -> Result<Vec<PlannedStep>> {
        let p = self.processor;
        let tc = self.toolchain;
        // O inglês das mensagens, que a AURORA interpreta, mesmo com
        // `YANC_LANG=pt` no ambiente.
        let lang = "--lang=en";
        // O fonte com o caminho completo: a pasta dele é a dos arquivos da
        // pessoa, que o compilador anota em `prog_log.txt` para o `asmcomp`.
        let compile = match p.language {
            Language::Cmm => {
                let mut inv = Invocation::new(tc.tool(Tool::Cmmcomp)?, &p.dir)
                    .path_arg(&p.source)
                    .arg("-o")
                    .path_arg(&p.output_dir);
                if self.show_arrays {
                    inv = inv.arg("--wave-arrays");
                }
                PlannedStep {
                    step: Step::Compile,
                    tool: Tool::Cmmcomp,
                    invocation: inv.arg(lang),
                    source: Some(p.source.clone()),
                    timeout: None,
                }
            }
            Language::Cpp => PlannedStep {
                step: Step::Compile,
                tool: Tool::Cppcomp,
                invocation: Invocation::new(tc.tool(Tool::Cppcomp)?, &p.dir)
                    .path_arg(&p.source)
                    .arg("-o")
                    .path_arg(&p.output_dir)
                    .arg(lang),
                // O cppcomp cita o arquivo de cada mensagem (o fonte, ou o
                // cabeçalho incluído onde está o erro).
                source: None,
                timeout: None,
            },
        };
        // As mensagens do asmcomp citam linhas do `.asm`.
        let assemble = PlannedStep {
            step: Step::Assemble,
            tool: Tool::Asmcomp,
            invocation: Invocation::new(tc.tool(Tool::Asmcomp)?, &p.output_dir)
                .path_arg(&p.assembly_path())
                .arg("-f")
                .arg(self.frequency_mhz.to_string())
                .arg("-c")
                .arg(self.clocks.to_string())
                .arg(lang),
            source: Some(p.assembly_path()),
            timeout: None,
        };
        let steps = vec![compile, assemble];
        debug_assert!(
            steps
                .iter()
                .map(|s| (s.step, s.tool))
                .eq(build_steps(p.language).iter().copied())
        );
        Ok(steps)
    }

    /// Os artefatos que este build deve produzir: (tipo, caminho, obrigatório).
    fn artifact_paths(&self) -> Vec<(ArtifactKind, Utf8PathBuf, bool)> {
        let p = self.processor;
        let n = p.name.as_str();
        let out = &p.output_dir;
        let [data, inst, pc] = p.files_read_by_verilog();
        vec![
            (ArtifactKind::Assembly, p.assembly_path(), true),
            (ArtifactKind::Verilog, p.verilog_path(), true),
            (ArtifactKind::DataMemory, data, true),
            (ArtifactKind::InstructionMemory, inst, true),
            (ArtifactKind::Testbench, p.testbench_path(), true),
            (ArtifactKind::ProgramCounterMap, pc, false),
            (
                ArtifactKind::SourceTranslation,
                out.join("src_lines.txt"),
                false,
            ),
            (
                ArtifactKind::OpcodeTranslation,
                out.join("trad_opcode.txt"),
                false,
            ),
            (ArtifactKind::CompilerLog, out.join("prog_log.txt"), false),
            (
                ArtifactKind::PreAssemblerLog,
                out.join("app_log.txt"),
                false,
            ),
        ]
        .into_iter()
        .inspect(|(_, path, _)| debug_assert!(path.starts_with(out), "{n}: {path}"))
        .collect()
    }
}

/// A regra de nome do YANC 7.1, conferida antes de rodar qualquer coisa: o
/// processador se chama como o arquivo, que por isso precisa se chamar
/// `<nome>.<ext>`, e o nome precisa servir de módulo Verilog.
fn validate_source(p: &Processor) -> Result<()> {
    // Um `.spf` de fora pode trazer nome que o YANC não compila; o erro vem
    // antes de qualquer compilador.
    crate::project::validate_processor_name(&p.name)?;
    let invalid = |reason: String| AuroraError::InvalidSource {
        path: p.source.clone(),
        reason,
    };
    let expected = format!("{}.{}", p.name, p.language.extension());
    if p.source.file_name() != Some(expected.as_str()) {
        return Err(invalid(format!(
            "The file must be named {expected}: YANC names the processor after the file"
        )));
    }
    if !p.source.is_file() {
        return Err(invalid("The file does not exist".into()));
    }
    Ok(())
}
