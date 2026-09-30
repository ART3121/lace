//! Peças comuns a toda operação que encadeia ferramentas (build, simulação,
//! síntese, esquemático): o relatório de cada passo, o status final e o
//! rastreio de artefatos.
//!
//! Todo resultado de operação ([`BuildResult`](crate::BuildResult),
//! [`SimulationResult`](crate::SimulationResult), ...) tem a mesma espinha:
//!
//! | Campo | Conteúdo |
//! |---|---|
//! | `status` | [`Status`]: como a operação terminou |
//! | `failed_step` | o [`Step`] que falhou ou quebrou, se algum |
//! | `steps` | um [`StepReport`] por programa executado, na ordem |
//! | `diagnostics` | as mensagens de todos os passos, já interpretadas |
//! | `artifacts` | os [`Artifact`]s esperados, com `fresh` dizendo se foram gerados agora |
//!
//! Os passos rodam em sequência e param no primeiro que não terminar com
//! código 0: se o `cmmcomp` falhar, `steps` tem um elemento só.

use std::time::SystemTime;

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;

use crate::diagnostics::{self, Diagnostic, Severity};
use crate::error::Result;
use crate::process::{self, Invocation, Termination};
use crate::toolchain::Tool;

/// Um passo de uma operação. Em JSON, em `snake_case` (`"pre_assemble"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Step {
    /// `cpppp`, só no fluxo C.
    Preprocess,
    /// `cmmcomp` ou `cppcomp`: fonte para assembly.
    Compile,
    /// `appcomp`: conta instruções e variáveis.
    PreAssemble,
    /// `asmcomp`: assembly para Verilog e memórias.
    Assemble,
    /// `iverilog -t null`: só confere se o Verilog elabora.
    CheckSyntax,
    /// `iverilog`: Verilog para o `.vvp` do Icarus.
    Elaborate,
    /// `verilator --binary`: Verilog para um executável C++.
    Verilate,
    /// `vvp` ou o executável do Verilator rodando o testbench.
    Simulate,
    /// `yosys`: Verilog para netlist JSON.
    Synthesize,
    /// `yosys show`: netlist para o grafo do esquemático (`.dot`).
    Graph,
    /// `dot`: grafo do esquemático para SVG.
    Render,
}

/// O que aconteceu num passo: o comando exato, como terminou e tudo o que
/// escreveu. É o registro de auditoria da operação; os `diagnostics` do
/// resultado são a versão interpretada de `stdout` e `stderr`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StepReport {
    /// Qual passo.
    pub step: Step,
    /// A ferramenta executada. Para o executável que o Verilator gera, é
    /// [`Tool::Verilator`].
    pub tool: Tool,
    /// O que foi executado, com CWD e ambiente.
    pub command: Invocation,
    /// Como o processo terminou.
    pub termination: Termination,
    /// Tudo que o processo escreveu no stdout, decodificado como UTF-8 (bytes
    /// inválidos viram `U+FFFD`). Numa simulação, inclui o `$display` do
    /// testbench.
    pub stdout: String,
    /// Tudo que o processo escreveu no stderr.
    pub stderr: String,
    /// Duração, do início do processo até o fim, em milissegundos.
    pub duration_ms: u64,
}

/// Como uma operação terminou. Em JSON, em `snake_case` (`"succeeded"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Status {
    /// Todos os passos terminaram com código 0 e os artefatos foram gerados.
    Succeeded,
    /// Uma ferramenta recusou a entrada (código de saída diferente de 0).
    Failed,
    /// Uma ferramenta morreu (sinal no Unix, exceção no Windows). É defeito da
    /// ferramenta, não do código do usuário.
    Crashed,
    /// As ferramentas terminaram bem, mas faltou algum artefato obrigatório.
    Incomplete,
}

/// O papel de um arquivo produzido por uma operação. Em JSON, em
/// `snake_case` (`"data_memory"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ArtifactKind {
    /// `Software/<nome>.asm`
    Assembly,
    /// `Hardware/<nome>.v`, o processador sintetizável.
    Verilog,
    /// `Hardware/<nome>_data.mif`
    DataMemory,
    /// `Hardware/<nome>_inst.mif`
    InstructionMemory,
    /// `<temp>/<nome>_tb.v`, gerado pelo asmcomp fora de `Simulation/`.
    Testbench,
    /// `<temp>/pp.cpp`, saída do `cpppp`.
    PreprocessedSource,
    /// `<temp>/pc_<nome>_mem.txt`, liga endereço de instrução a linha do fonte.
    ProgramCounterMap,
    /// `<temp>/trad_cmm.txt`
    SourceTranslation,
    /// `<temp>/trad_opcode.txt`
    OpcodeTranslation,
    /// `<temp>/cmm_log.txt`
    CompilerLog,
    /// `<temp>/app_log.txt`
    PreAssemblerLog,
    /// O `.vvp` do Icarus.
    IcarusImage,
    /// O executável que o Verilator gerou.
    VerilatedModel,
    /// Onda da simulação (VCD ou FST).
    Waveform,
    /// `Simulation/output_<n>.txt`, escrito pelo testbench.
    SimulationOutput,
    /// Netlist JSON do Yosys.
    Netlist,
    /// Grafo do esquemático (`.dot`), gerado pelo Yosys.
    SchematicGraph,
    /// Esquemático SVG, desenhado pelo `dot`.
    Schematic,
}

/// Um arquivo que a operação deveria produzir.
///
/// `fresh` é calculado comparando o horário de modificação antes e depois da
/// operação. Um `.v` que sobrou de um build anterior aparece com
/// `fresh = false`, e não como sucesso. Artefatos obrigatórios aparecem
/// sempre; intermediários, só se existirem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Artifact {
    /// O papel do arquivo.
    pub kind: ArtifactKind,
    /// Caminho absoluto.
    pub path: Utf8PathBuf,
    /// Sem ele a operação não está completa.
    pub required: bool,
    /// Existe e foi escrito por esta operação (não é sobra de uma anterior).
    pub fresh: bool,
}

/// Um passo pronto para rodar.
pub(crate) struct PlannedStep {
    pub step: Step,
    pub tool: Tool,
    pub invocation: Invocation,
    /// Arquivo ao qual se referem as mensagens com linha e sem arquivo.
    pub source: Option<Utf8PathBuf>,
}

impl PlannedStep {
    pub fn new(step: Step, tool: Tool, invocation: Invocation) -> Self {
        PlannedStep {
            step,
            tool,
            invocation,
            source: None,
        }
    }
}

/// Executa passos em sequência, acumulando relatórios e diagnósticos, e para
/// no primeiro que não terminar com código 0.
pub(crate) struct Runner {
    pub steps: Vec<StepReport>,
    pub diagnostics: Vec<Diagnostic>,
    pub status: Status,
    pub failed_step: Option<Step>,
}

impl Runner {
    pub fn new() -> Self {
        Runner {
            steps: Vec::new(),
            diagnostics: Vec::new(),
            status: Status::Succeeded,
            failed_step: None,
        }
    }

    /// Roda o passo e diz se ele terminou bem. Depois de uma falha, não roda
    /// mais nada e devolve `false`.
    pub fn run(&mut self, planned: PlannedStep) -> Result<bool> {
        if self.status != Status::Succeeded {
            return Ok(false);
        }
        tracing::info!(step = ?planned.step, tool = %planned.tool, "passo");
        let output = process::run(&planned.invocation)?;
        self.diagnostics.extend(diagnostics::parse(
            planned.tool,
            &output.stdout,
            &output.stderr,
            planned.source.as_deref(),
        ));
        let termination = output.termination;
        self.steps.push(StepReport {
            step: planned.step,
            tool: planned.tool,
            command: planned.invocation,
            termination,
            stdout: output.stdout,
            stderr: output.stderr,
            duration_ms: u64::try_from(output.duration.as_millis()).unwrap_or(u64::MAX),
        });
        let status = match termination {
            Termination::Exited(0) => return Ok(true),
            Termination::Exited(_) => Status::Failed,
            Termination::Signaled(_) | Termination::Exception(_) | Termination::Unknown => {
                Status::Crashed
            }
        };
        self.status = status;
        self.failed_step = Some(planned.step);
        Ok(false)
    }

    /// Algumas ferramentas terminam com 0 mesmo tendo reportado erro. Marca como falha o passo
    /// mais recente se houver diagnóstico de erro dele.
    pub fn fail_on_error_diagnostics(&mut self, tool: Tool) {
        let has_error = self
            .diagnostics
            .iter()
            .any(|d| d.tool == tool && d.severity == Severity::Error);
        if self.status == Status::Succeeded && has_error {
            self.status = Status::Failed;
            self.failed_step = self.steps.last().map(|s| s.step);
        }
    }
}

/// Lembra o horário de modificação de cada artefato antes da operação, para
/// saber depois quais foram realmente reescritos. Sem isso, um `.v` que sobrou
/// de um build anterior pareceria sucesso.
pub(crate) struct ArtifactTracker {
    entries: Vec<(ArtifactKind, Utf8PathBuf, bool, Option<SystemTime>)>,
}

impl ArtifactTracker {
    pub fn new() -> Self {
        ArtifactTracker {
            entries: Vec::new(),
        }
    }

    pub fn expect(&mut self, kind: ArtifactKind, path: impl Into<Utf8PathBuf>, required: bool) {
        let path = path.into();
        let before = mtime(&path);
        self.entries.push((kind, path, required, before));
    }

    /// Os artefatos, com `fresh` calculado. Intermediário que não existe fica
    /// de fora; obrigatório que não existe aparece com `fresh = false`.
    pub fn finish(self) -> Vec<Artifact> {
        self.entries
            .into_iter()
            .filter_map(|(kind, path, required, before)| {
                let after = mtime(&path);
                let fresh = after.is_some() && after != before;
                (required || after.is_some()).then_some(Artifact {
                    kind,
                    path,
                    required,
                    fresh,
                })
            })
            .collect()
    }
}

fn mtime(path: &Utf8Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// `Incomplete` quando todos os passos deram certo mas falta artefato.
pub(crate) fn final_status(status: Status, artifacts: &[Artifact]) -> Status {
    if status == Status::Succeeded && artifacts.iter().any(|a| a.required && !a.fresh) {
        Status::Incomplete
    } else {
        status
    }
}
