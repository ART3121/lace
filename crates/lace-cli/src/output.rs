//! Formatação da saída: texto para gente ou JSON para máquina.
//!
//! Com `--json`, cada comando escreve um único objeto JSON no stdout e os
//! métodos de texto não escrevem nada. Com `--events`, o stdout tem um
//! objeto JSON por linha: os eventos das ferramentas enquanto rodam
//! (`lace_core::Event`) e, na última linha, `{"event": "result", "result":
//! ...}` com o mesmo objeto do `--json`. Em texto, a cor só aparece quando o
//! stdout é um terminal (e respeita `NO_COLOR`), via `anstream`.

use crate::progress::Progress;
use crate::report::{
    ErrorInfo, ErrorReport, NewReport, PortValues, ProcessorStatus, RemoveReport, Report,
    StatusReport, ToolEntry, ToolsReport, TopReport,
};
use std::time::Duration;

use anstream::println;
use anstyle::{AnsiColor, Style};
use camino::{Utf8Path, Utf8PathBuf};
use lace_core::fpga::{FpgaBuildResult, FpgaProgramResult, ResourceUsage};
use lace_core::history::{Availability, MetricComparison, RunComparison, RunMetadata, RunSummary};
use lace_core::{
    AddedFile, Artifact, ArtifactKind, BuildResult, CancelToken, CheckResult, Control, Diagnostic,
    Event, FileMismatch, FileRole, HierarchyResult, LaceError, Language, ModuleInstance, MovedPath,
    Platform, PreparedLayout, Processor, Project, ProjectFile, RunningProcess, SchematicResult,
    Severity, SimulationResult, Simulator, Status, Step, StepReport, SynthesisMetric,
    SynthesisResult, Termination, TestStatus, Tool, Toolchain, component,
};
use serde::Serialize;

pub(crate) const ERROR: Style = AnsiColor::Red.on_default().bold();
pub(crate) const WARNING: Style = AnsiColor::Yellow.on_default().bold();
pub(crate) const OK: Style = AnsiColor::Green.on_default().bold();
pub(crate) const DIM: Style = Style::new().dimmed();
pub(crate) const BOLD: Style = Style::new().bold();

/// Quantos valores de uma porta de saída cabem numa linha de texto; o resto
/// fica no arquivo (e no JSON, que traz todos).
const SHOWN_VALUES: usize = 32;

/// Para quem a saída é escrita.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Texto, para gente.
    Text,
    /// `--json`: um objeto no fim.
    Json,
    /// `--events`: um objeto por linha, enquanto roda.
    Events,
}

pub struct Output {
    /// Saída para máquina (`--json` ou `--events`): os métodos de texto não
    /// escrevem nada.
    json: bool,
    events: bool,
    verbose: bool,
}

/// A última linha do `--events`.
#[derive(Serialize)]
struct ResultLine<'a, T: Report> {
    event: &'static str,
    result: &'a T,
}

/// Um trecho de texto com estilo.
pub(crate) fn paint(style: Style, text: impl std::fmt::Display) -> String {
    format!("{style}{text}{style:#}")
}

impl Output {
    pub fn new(mode: Mode, verbose: bool) -> Self {
        Output {
            json: mode != Mode::Text,
            events: mode == Mode::Events,
            verbose,
        }
    }

    /// Saída em texto, para gente.
    pub fn is_text(&self) -> bool {
        !self.json
    }

    /// O `Control` das operações: cancela com `cancel` e mostra o que as
    /// ferramentas escrevem enquanto rodam. Em texto, a saída da simulação
    /// (o `$display` do testbench) e, com -v, a de todos os passos, e num
    /// terminal a barra de progresso de cada passo (`progress.rs`); com
    /// `--events`, cada evento numa linha JSON. Com `--json`, nada: sem
    /// receptor, o vvp guarda o stdout em buffer, que é mais rápido.
    pub fn control(&self, cancel: CancelToken) -> Control {
        let control = Control::new().with_cancel(cancel);
        if self.events {
            control.on_event(|event| {
                if let Ok(line) = serde_json::to_string(event) {
                    println!("{line}");
                }
            })
        } else if self.json {
            control
        } else {
            let verbose = self.verbose;
            let progress = Progress::for_terminal();
            control.on_event(move |event| live_text(event, verbose, progress.as_deref()))
        }
    }

    /// Escreve `value` como JSON: o objeto inteiro com `--json`, a linha de
    /// resultado com `--events`, nada em texto. Só aceita os tipos de
    /// `report`, que têm schema em `docs/schema/`.
    pub fn json(&self, value: &impl Report) -> anyhow::Result<()> {
        if self.events {
            let line = ResultLine {
                event: "result",
                result: value,
            };
            println!("{}", serde_json::to_string(&line)?);
        } else if self.json {
            println!("{}", serde_json::to_string_pretty(value)?);
        }
        Ok(())
    }

    /// Um erro que impediu o comando de rodar, com a dica do que fazer
    /// quando há uma. No JSON, `code` é o de `LaceError::code()`, ou `"cli"`
    /// para erros da própria linha de comando, e `hint` só aparece com dica.
    pub fn error(&self, error: &anyhow::Error) {
        // O erro do lace-learn que embrulha um do lace-core é transparente:
        // o do lace-core não aparece sozinho na cadeia.
        let learn = error
            .chain()
            .find_map(|e| e.downcast_ref::<lace_learn::LearnError>());
        let lace = error
            .chain()
            .find_map(|e| e.downcast_ref::<LaceError>())
            .or(match learn {
                Some(lace_learn::LearnError::Lace(e)) => Some(e),
                _ => None,
            });
        let (code, hint) = match (lace, learn) {
            (Some(lace), _) => (lace.code(), hint(lace)),
            (None, Some(learn)) => crate::learn::error_info(learn),
            (None, None) => ("cli", None),
        };
        if self.json {
            let report = ErrorReport {
                error: ErrorInfo {
                    code: code.to_owned(),
                    message: chain_text(error),
                    hint,
                },
            };
            if self.events {
                let _ = self.json(&report);
            } else if let Ok(line) = serde_json::to_string(&report) {
                println!("{line}");
            }
        } else {
            anstream::eprintln!("{}: {}", paint(ERROR, "Error"), chain_text(error));
            if let Some(hint) = hint {
                anstream::eprintln!("  {hint}");
            }
        }
    }

    /// Um aviso que não impede o comando, no stderr.
    pub(crate) fn warning(&self, text: &str) {
        if !self.json {
            anstream::eprintln!("{}: {text}", paint(WARNING, "Warning"));
        }
    }

    /// Uma linha de confirmação em texto, com o caminho envolvido. O JSON
    /// do comando sai à parte.
    pub fn message(&self, what: &str, path: &Utf8Path) {
        if !self.json {
            println!("{what}: {path}");
        }
    }

    /// O projeto novo.
    pub fn created(&self, project: &Project) -> anyhow::Result<()> {
        if self.json {
            return self.json(&NewReport {
                message: "Project created".into(),
                path: project.spf_path().to_owned(),
                root: project.root().to_owned(),
            });
        }
        println!("Project created: {}", project.spf_path());
        // Um projeto dentro da pasta de outro, por exemplo.
        for issue in project.issues() {
            self.warning(&issue.message);
        }
        Ok(())
    }

    /// `lace order`: a lista na ordem nova, numerada, com a marca do topo ou
    /// do testbench escolhido.
    pub fn ordered(&self, files: &[ProjectFile], moved: &Utf8Path, root: &Utf8Path) {
        if self.json {
            return;
        }
        let list = match files.first().map(|f| f.role) {
            Some(FileRole::Testbench) => "Testbenches",
            _ => "Modules",
        };
        println!(
            "{} {}",
            paint(BOLD, list),
            paint(DIM, "(the order the compilers read them)")
        );
        for (i, file) in files.iter().enumerate() {
            let mark = if file.path == moved { "  <-" } else { "" };
            println!(
                "  {:>2}. {}{}",
                i + 1,
                relative(&file.path, root),
                paint(DIM, mark)
            );
        }
    }

    /// Um arquivo de `lace add`.
    pub fn added(&self, file: &AddedFile, root: &Utf8Path) {
        if self.json {
            return;
        }
        let (what, mark) = match file.role {
            FileRole::Testbench => ("Testbench", "selected testbench"),
            _ => ("Module", "top"),
        };
        let how = if file.created { "created" } else { "added" };
        let mark = if file.selected {
            format!(" {}", paint(OK, format!("({mark})")))
        } else {
            String::new()
        };
        println!("{what} {how}: {}{mark}", relative(&file.path, root));
    }

    /// Os arquivos de `lace remove`, como foram digitados, e os que não
    /// estavam registrados.
    pub fn removed(
        &self,
        project: &Utf8Path,
        removed: &[(&Utf8Path, Utf8PathBuf)],
        unknown: &[(&Utf8Path, Utf8PathBuf)],
    ) -> anyhow::Result<()> {
        if self.json {
            let paths = |list: &[(&Utf8Path, Utf8PathBuf)]| -> Vec<Utf8PathBuf> {
                list.iter().map(|(_, path)| path.clone()).collect()
            };
            return self.json(&RemoveReport {
                project: project.to_owned(),
                removed: paths(removed),
                not_registered: paths(unknown),
            });
        }
        for (typed, _) in removed {
            println!("Removed from the project: {typed}");
        }
        for (typed, _) in unknown {
            self.warning(&format!("{typed} was not in the project"));
        }
        Ok(())
    }

    /// Um caminho de `lace move` e os arquivos do projeto que foram com ele.
    pub fn moved(&self, moved: &MovedPath, root: &Utf8Path) {
        if self.json {
            return;
        }
        println!(
            "Moved: {} -> {}",
            relative(&moved.from, root),
            relative(&moved.to, root)
        );
        for file in &moved.files {
            let what = match (file.role, file.top_level) {
                (FileRole::Testbench, true) => "selected testbench",
                (FileRole::Testbench, false) => "testbench",
                (_, true) => "top",
                _ => "module",
            };
            println!(
                "  {} {}",
                relative(&file.path, root),
                paint(DIM, format!("(still in the project, {what})"))
            );
        }
    }

    /// A árvore de `lace hierarchy`: o design e cada testbench, os
    /// processadores que ficaram de fora e o resumo das elaborações. Os
    /// módulos da biblioteca SAPHO aparecem fechados; com -v, inteiros.
    pub fn hierarchy(&self, result: &HierarchyResult, not_built: &[&str], root: &Utf8Path) {
        if self.json {
            return;
        }
        let sections = result
            .design
            .iter()
            .map(|design| ("Design".to_owned(), design))
            .chain(result.testbenches.iter().map(|tb| {
                let title = match (&tb.processor, &tb.testbench) {
                    (Some(name), _) => format!("Testbench of processor {name}"),
                    (None, Some(file)) => format!("Testbench {}", relative(file, root)),
                    (None, None) => "Testbench".to_owned(),
                };
                (title, tb)
            }));
        for (title, elaboration) in sections {
            println!("{}", paint(BOLD, title));
            if elaboration.status != Status::Succeeded {
                println!("  {}", paint(ERROR, "did not elaborate"));
            }
            for node in &elaboration.roots {
                self.instance(node, 1, root);
            }
        }
        for name in not_built {
            self.warning(&format!(
                "Processor {name} is not built and was left out; build it with: lace build -p {name}"
            ));
        }
        self.summary(Summary {
            title: "Hierarchy",
            detail: "iverilog",
            status: result.status,
            failed_step: result.failed_step,
            steps: &result.steps,
            planned: &[],
            diagnostics: &result.diagnostics,
            artifacts: &[],
            duration_ms: result.duration_ms,
            root,
            independent_steps: true,
            limit_ms: None,
        });
    }

    /// Uma linha da árvore: `instância: módulo`, ou só o nome quando os dois
    /// são iguais.
    fn instance(&self, node: &ModuleInstance, depth: usize, root: &Utf8Path) {
        let indent = "  ".repeat(depth);
        let module = if node.name == node.module {
            String::new()
        } else {
            format!(": {}", node.module)
        };
        let closed = node.library && !self.verbose && !node.children.is_empty();
        let mut notes = Vec::new();
        if node.library {
            notes.push("SAPHO".to_owned());
        }
        if closed {
            notes.push(format!(
                "{} instances inside, -v shows them",
                descendants(node)
            ));
        }
        if self.verbose
            && let Some(file) = &node.file
        {
            let line = node.line.map_or_else(String::new, |l| format!(":{l}"));
            notes.push(format!("{}{line}", relative(file, root)));
        }
        let notes = if notes.is_empty() {
            String::new()
        } else {
            format!(" {}", paint(DIM, format!("({})", notes.join(", "))))
        };
        let name = if node.library {
            paint(DIM, &node.name)
        } else {
            node.name.clone()
        };
        println!("{indent}{name}{}{notes}", paint(DIM, module));
        if !closed {
            for child in &node.children {
                self.instance(child, depth + 1, root);
            }
        }
    }

    /// O módulo de topo e o arquivo dele.
    pub fn top(&self, project: &Project) -> anyhow::Result<()> {
        let file = project.top_level();
        let module = match &file {
            Some(_) => project.top_module(),
            None => Ok(None),
        };
        if self.json {
            let (name, error) = match &module {
                Ok(name) => (name.clone(), None),
                Err(e) => (None, Some(error_info(e))),
            };
            return self.json(&TopReport {
                top_level: file,
                module: name,
                module_error: error,
            });
        }
        let root = project.root();
        match (file, module) {
            (None, _) => {
                println!("No top module");
                println!("  {}", paint(DIM, "Choose it with: lace top <file|module>"));
            }
            (Some(file), Ok(Some(name))) => {
                println!("Top module: {name} ({})", relative(&file, root));
            }
            (Some(file), Ok(None)) => println!("Top file: {}", relative(&file, root)),
            // O arquivo continua sendo o topo; o nome do módulo é que não
            // dá para saber (vários módulos, nenhum com o nome do arquivo).
            (Some(file), Err(e)) => {
                println!("Top file: {}", relative(&file, root));
                println!("  {}: {e}", paint(WARNING, "Warning"));
            }
        }
        Ok(())
    }

    /// `lace build` num projeto sem processadores.
    pub fn no_processors(&self) {
        if self.json {
            return;
        }
        println!("The project has no SAPHO processors");
        println!("  {}", paint(DIM, "Create one with: lace proc add <name>"));
    }

    /// O relatório que a operação gravou no histórico.
    pub fn recorded(&self, id: &str) {
        if !self.json {
            println!(
                "{}",
                paint(
                    DIM,
                    format!("Report {id}: lace report show {}", short_id(id))
                )
            );
        }
    }

    /// A operação rodou, mas o relatório não pôde ser gravado.
    pub fn report_not_saved(&self, error: &LaceError) {
        self.warning(&format!("Report not saved: {error}"));
    }

    /// `lace report` e `lace report show`: o `report.txt`, sem mudança.
    pub fn report_text(&self, text: &str) {
        if !self.json {
            print!("{text}");
        }
    }

    /// `lace report list`.
    pub fn report_list(&self, reports: &[RunSummary]) {
        if self.json {
            return;
        }
        if reports.is_empty() {
            println!("No reports yet");
            println!(
                "  {}",
                paint(DIM, "lace build, check, sim and synth each store one")
            );
            return;
        }
        println!(
            "{}",
            paint(
                BOLD,
                format!(
                    "{:<12} {:<10} {:<18} {:<12} {:<12} {:<21} {}",
                    "ID", "STATUS", "TOP", "SYNTHESIS", "SIMULATION", "TIMESTAMP", "COMMAND"
                )
            )
        );
        for r in reports {
            if !r.readable {
                println!("{:<12} {}", r.id, paint(WARNING, "unreadable record.json"));
                continue;
            }
            let status = r.status.map_or("-", status_word);
            let synthesis = if r.synthesis { "available" } else { "-" };
            let simulation = match r.simulation {
                Availability::Available => "available",
                Availability::Partial => "partial",
                _ => "-",
            };
            println!(
                "{:<12} {:<10} {:<18} {:<12} {:<12} {:<21} {}",
                r.id,
                status,
                r.top.as_deref().unwrap_or("-"),
                synthesis,
                simulation,
                r.timestamp.as_deref().unwrap_or("-"),
                r.command.as_deref().unwrap_or("-"),
            );
        }
    }

    /// `lace report clean`: o que saiu e quantos ficaram.
    pub fn reports_removed(&self, removed: &[String], kept: usize) {
        if self.json {
            return;
        }
        match removed {
            [] => println!("No reports to remove"),
            [one] => println!("Removed {one}"),
            few if few.len() <= 5 => println!("Removed {} reports: {}", few.len(), few.join(", ")),
            [first, .., last] => {
                println!("Removed {} reports, from {first} to {last}", removed.len())
            }
        }
        if removed.is_empty() {
            return;
        }
        match kept {
            0 => self.next("The history is empty; report numbers are not reused"),
            1 => self.next("1 report left: lace report list"),
            n => self.next(&format!("{n} reports left: lace report list")),
        }
    }

    /// `lace report compare`.
    pub fn report_comparison(&self, c: &RunComparison, summary_only: bool) {
        if self.json {
            return;
        }
        println!("{}\n", paint(BOLD, "REPORT COMPARISON"));
        if summary_only {
            println!("Current:   {}", c.current_id);
            println!("Baseline:  {}", c.baseline_id);
            println!(
                "Top:       {}",
                c.current.top.as_deref().unwrap_or("not reported")
            );
        } else {
            comparison_header("Current report", &c.current_id, &c.current);
            println!();
            comparison_header("Baseline report", &c.baseline_id, &c.baseline);
            comparison_synthesis(c);
            comparison_timings(c);
        }
        comparison_summary(c);
        if !c.warnings.is_empty() {
            println!("\n{}\n", paint(BOLD, "CONTEXT WARNINGS"));
            for warning in &c.warnings {
                println!("  {}: {warning}.", paint(WARNING, "Warning"));
            }
            println!(
                "  {}",
                paint(
                    DIM,
                    "When contexts differ, a change in time or size is not necessarily a regression."
                )
            );
        }
    }

    pub fn build(&self, result: &BuildResult, root: &Utf8Path) {
        if self.json {
            return;
        }
        let detail = format!(
            "{}, {} MHz, {} clocks",
            language_name(result.language),
            result.frequency_mhz,
            result.clocks
        );
        let planned: &[(Step, Tool)] = match result.language {
            Language::Cpp => &[
                (Step::Preprocess, Tool::Cpppp),
                (Step::Compile, Tool::Cppcomp),
                (Step::PreAssemble, Tool::Appcomp),
                (Step::Assemble, Tool::Asmcomp),
            ],
            _ => &[
                (Step::Compile, Tool::Cmmcomp),
                (Step::PreAssemble, Tool::Appcomp),
                (Step::Assemble, Tool::Asmcomp),
            ],
        };
        self.summary(Summary {
            title: &format!("Build of {}", result.processor),
            detail: &detail,
            status: result.status,
            failed_step: result.failed_step,
            steps: &result.steps,
            planned,
            diagnostics: &result.diagnostics,
            artifacts: &result.artifacts,
            duration_ms: result.duration_ms,
            root,
            independent_steps: false,
            limit_ms: None,
        });
    }

    /// O resumo da simulação, a saída do testbench, os valores das portas
    /// (com -p) e as entradas que faltaram.
    pub fn simulation(
        &self,
        result: &SimulationResult,
        outputs: &[PortValues],
        root: &Utf8Path,
        timeout: Option<Duration>,
    ) {
        if self.json {
            return;
        }
        let (detail, planned): (&str, &[(Step, Tool)]) = match result.simulator {
            Simulator::Verilator => (
                "Verilator",
                &[
                    (Step::Verilate, Tool::Verilator),
                    (Step::Simulate, Tool::Verilator),
                ],
            ),
            _ => (
                "Icarus",
                &[
                    (Step::Elaborate, Tool::Iverilog),
                    (Step::Simulate, Tool::Vvp),
                ],
            ),
        };
        // A simulação rápida diz que é, porque não deixa onda.
        let title = if result.fast {
            format!("Fast simulation of {}", result.top)
        } else {
            format!("Simulation of {}", result.top)
        };
        self.summary(Summary {
            title: &title,
            detail,
            status: result.status,
            failed_step: result.failed_step,
            steps: &result.steps,
            planned,
            diagnostics: &result.diagnostics,
            artifacts: &result.artifacts,
            duration_ms: result.duration_ms,
            root,
            independent_steps: false,
            limit_ms: timeout.map(|t| u64::try_from(t.as_millis()).unwrap_or(u64::MAX)),
        });
        // O stdout do testbench ($display) já saiu enquanto a simulação
        // rodava (`live_text`).
        if result.status == Status::TimedOut {
            println!(
                "  {}",
                paint(
                    DIM,
                    "Does the testbench reach $finish? Without it the simulation never ends. Otherwise, raise --timeout"
                )
            );
        }
        for port in outputs {
            let mut line = if let Some(error) = &port.error {
                paint(WARNING, format!("unreadable: {error}"))
            } else if port.values.is_empty() {
                paint(DIM, "no values")
            } else {
                port.values
                    .iter()
                    .take(SHOWN_VALUES)
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            if port.values.len() > SHOWN_VALUES {
                line += &paint(
                    DIM,
                    format!(
                        " ... ({} more in {})",
                        port.values.len() - SHOWN_VALUES,
                        relative(&port.path, root)
                    ),
                );
            }
            println!("  Output {}: {line}", port.port);
        }
        for missing in &result.missing_inputs {
            println!(
                "  {}: Create {} with one value per line (the testbench reads it)",
                paint(WARNING, "Warning"),
                relative(missing, root)
            );
        }
        // cocotb: um teste por linha, e quantos passaram. As falhas já saíram
        // nos diagnósticos, com a linha do `.py`.
        if let Some(tests) = &result.tests {
            for case in &tests.cases {
                let (style, state) = match case.status {
                    TestStatus::Passed => (OK, "passed"),
                    TestStatus::Failed => (ERROR, "failed"),
                    _ => (DIM, "skipped"),
                };
                println!("    {} {}", paint(style, format!("{state:<9}")), case.name);
            }
            let total = tests.passed + tests.failed + tests.skipped;
            let style = if tests.failed > 0 || total == 0 {
                ERROR
            } else {
                OK
            };
            println!(
                "  {}: {} of {total} passed",
                paint(style, "Tests"),
                tests.passed
            );
        }
    }

    pub fn check(&self, result: &CheckResult, lint: bool, root: &Utf8Path) {
        if self.json {
            return;
        }
        let title = match result.targets.as_slice() {
            [] => "Check".to_owned(),
            few if few.len() <= TITLE_TARGETS => format!("Check of {}", few.join(", ")),
            many => format!(
                "Check of {} roots: {} and {} more",
                many.len(),
                many[..TITLE_TARGETS].join(", "),
                many.len() - TITLE_TARGETS
            ),
        };
        // Sem o Verilator o lint não roda, e o título não diz que rodou.
        let detail = if lint && result.steps.iter().any(|s| s.step == Step::Lint) {
            "iverilog -tnull, verilator --lint-only"
        } else {
            "iverilog -tnull"
        };
        self.summary(Summary {
            title: &title,
            detail,
            status: result.status,
            failed_step: result.failed_step,
            steps: &result.steps,
            planned: &[],
            diagnostics: &result.diagnostics,
            artifacts: &[],
            duration_ms: result.duration_ms,
            root,
            independent_steps: false,
            limit_ms: None,
        });
    }

    pub fn synthesis(&self, result: &SynthesisResult, root: &Utf8Path) {
        if self.json {
            return;
        }
        // Sem netlist (a síntese falhou) não há módulo para contar.
        let detail = match result.modules.len() {
            0 => "yosys".to_owned(),
            1 => "1 module".to_owned(),
            n => format!("{n} modules"),
        };
        self.summary(Summary {
            title: &format!("Synthesis of {}", result.top),
            detail: &detail,
            status: result.status,
            failed_step: result.failed_step,
            steps: &result.steps,
            planned: &[(Step::Synthesize, Tool::Yosys)],
            diagnostics: &result.diagnostics,
            artifacts: &result.artifacts,
            duration_ms: result.duration_ms,
            root,
            independent_steps: false,
            limit_ms: None,
        });
        if let Some(statistics) = &result.statistics {
            let cells = statistics
                .cells
                .map_or("not reported".to_owned(), |c| c.to_string());
            let wires = statistics
                .wire_bits
                .map_or("not reported".to_owned(), |w| w.to_string());
            println!(
                "  {} {cells} cells, {wires} wire bits {}",
                paint(BOLD, "Statistics:"),
                paint(DIM, "(details: lace report)")
            );
        }
        if self.verbose && !result.modules.is_empty() {
            println!("  Modules: {}", result.modules.join(", "));
        }
    }

    /// A compilação para a placa: os passos do Quartus, os ajustes das
    /// ligações, os recursos da FPGA e o tempo de cada clock.
    pub fn fpga_build(&self, result: &FpgaBuildResult, root: &Utf8Path) {
        if self.json {
            return;
        }
        let detail = format!(
            "Quartus {}",
            result.quartus.version.as_deref().unwrap_or("Prime")
        );
        self.summary(Summary {
            title: &format!("FPGA build of {} for {}", result.top, result.board),
            detail: &detail,
            status: result.status,
            failed_step: result.failed_step,
            steps: &result.steps,
            planned: &[
                (Step::Synthesize, Tool::Quartus),
                (Step::Fit, Tool::Quartus),
                (Step::Bitstream, Tool::Quartus),
                (Step::Timing, Tool::Quartus),
            ],
            diagnostics: &result.diagnostics,
            artifacts: &result.artifacts,
            duration_ms: result.duration_ms,
            root,
            independent_steps: false,
            limit_ms: None,
        });
        for note in &result.notes {
            println!("  {} {note}", paint(WARNING, "note:"));
        }
        // Os detalhes (o que compõe os elementos lógicos) só com -v.
        let shown: Vec<_> = result
            .resources
            .iter()
            .filter(|r| self.verbose || !r.detail)
            .collect();
        if !shown.is_empty() {
            println!("  {}", paint(BOLD, "Resources:"));
            let name = |r: &ResourceUsage| {
                if r.detail {
                    format!("  {}", r.name)
                } else {
                    r.name.clone()
                }
            };
            let width = shown.iter().map(|r| name(r).len()).max().unwrap_or(0);
            for r in shown {
                let amount = match r.available {
                    Some(total) if total > 0 => {
                        // Como o Quartus: arredondado, e "< 1" abaixo de 1%.
                        let percent = if r.used > 0 && r.used * 100 < total {
                            "<1".to_owned()
                        } else {
                            ((r.used * 100 + total / 2) / total).to_string()
                        };
                        format!("{} / {total} ({percent}%)", r.used)
                    }
                    Some(total) => format!("{} / {total}", r.used),
                    None => r.used.to_string(),
                };
                println!("    {:<width$}  {amount}", name(r));
            }
        }
        if let Some(timing) = &result.timing {
            let verdict = if timing.met {
                paint(OK, "met")
            } else {
                paint(
                    WARNING,
                    "not met: the design may fail on the board at this clock",
                )
            };
            println!("  {} {verdict}", paint(BOLD, "Timing:"));
            for clock in &timing.clocks {
                let mut parts = Vec::new();
                if let Some(mhz) = clock.target_mhz {
                    parts.push(format!("needs {mhz} MHz"));
                }
                if let Some(fmax) = clock.fmax_mhz {
                    parts.push(format!("reaches {fmax} MHz"));
                }
                if let Some(slack) = clock.setup_slack_ns {
                    parts.push(format!("setup slack {slack} ns"));
                }
                if let Some(slack) = clock.hold_slack_ns {
                    parts.push(format!("hold slack {slack} ns"));
                }
                println!("    {}  {}", clock.clock, parts.join(", "));
            }
        }
    }

    /// A gravação na placa.
    pub fn fpga_program(&self, result: &FpgaProgramResult, root: &Utf8Path) {
        if self.json {
            return;
        }
        self.summary(Summary {
            title: &format!("Programming of {}", result.board),
            detail: &result.cable,
            status: result.status,
            failed_step: result.failed_step,
            steps: &result.steps,
            planned: &[(Step::Program, Tool::Quartus)],
            diagnostics: &result.diagnostics,
            artifacts: &[],
            duration_ms: result.duration_ms,
            root,
            independent_steps: false,
            limit_ms: None,
        });
        if result.succeeded() {
            println!(
                "  {}",
                paint(
                    DIM,
                    "The FPGA keeps the design until the board is powered off"
                )
            );
        }
    }

    pub fn schematic(&self, result: &SchematicResult, root: &Utf8Path) {
        if self.json {
            return;
        }
        self.summary(Summary {
            title: &format!("Schematic of {}", result.module),
            detail: "yosys show + dot",
            status: result.status,
            failed_step: result.failed_step,
            steps: &result.steps,
            planned: &[(Step::Graph, Tool::Yosys), (Step::Render, Tool::Dot)],
            diagnostics: &result.diagnostics,
            artifacts: &result.artifacts,
            duration_ms: result.duration_ms,
            root,
            independent_steps: false,
            limit_ms: None,
        });
    }

    pub fn opened(&self, surfer: &RunningProcess, layout: Option<&PreparedLayout>) {
        if self.json {
            return;
        }
        println!(
            "surfer-aurora opened (pid {}), log at {}",
            surfer.id(),
            surfer.log_file()
        );
        if let Some(saved) = layout.and_then(|l| l.saved.as_ref()) {
            let path = crate::commands::from_shell(&saved.path);
            if saved.customized {
                println!(
                    "Layout: {path}, as you saved it (lace wave --reset-layout opens the generated one)"
                );
            } else {
                println!("Layout: {path}; save it in Surfer (Ctrl+S) to keep your changes");
            }
        }
        if let Some(l) = layout
            && !l.layout.selection.is_empty()
        {
            println!(
                "Only the {} chosen signals and scopes (lace wave signals)",
                l.layout.selection.len()
            );
        }
        for p in layout.map_or(&[][..], |l| l.layout.processors.as_slice()) {
            let mut shown = vec![format!("{} variables", p.variables)];
            if p.assembly {
                shown.push("assembly".to_owned());
            }
            if p.source {
                shown.push("source lines".to_owned());
            }
            println!("  {} ({}): {}", p.processor, p.instance, shown.join(", "));
            if p.outdated {
                self.warning(&format!(
                    "{} was built again after this simulation: the PC and the source line show as numbers; simulate again to see the assembly (lace sim -p {})",
                    p.processor, p.processor
                ));
            }
        }
    }

    /// O bundle: versões, ferramentas, compilador do sistema e, com
    /// `--verify`, os executáveis cujo hash não confere.
    pub fn tools(
        &self,
        toolchain: &Toolchain,
        mismatches: Option<&[FileMismatch]>,
    ) -> anyhow::Result<()> {
        let tools: Vec<_> = Tool::all()
            .iter()
            .map(|&t| (t, toolchain.tool(t)))
            .collect();
        let manifest = toolchain.manifest();
        // O Perl do Verilator vem do sistema no Linux e no macOS, e do
        // bundle no Windows; o Quartus, sempre do sistema.
        let bundled = toolchain.system_compiler().is_some_and(|c| c.bundled);
        let from_system = |tool: Tool| tool.is_system() && !(tool == Tool::Perl && bundled);
        if self.json {
            let tools = tools
                .iter()
                .map(|(tool, path)| {
                    let entry = match path {
                        Ok(path) => ToolEntry::Found {
                            path: path.clone(),
                            system: from_system(*tool),
                        },
                        Err(e) => ToolEntry::Missing {
                            error: error_info(e),
                        },
                    };
                    (tool.binary_name().to_owned(), entry)
                })
                .collect();
            return self.json(&ToolsReport {
                root: toolchain.root().to_owned(),
                bundle: manifest.bundle.clone(),
                platform: manifest.platform.clone(),
                components: manifest.components.clone(),
                not_installed: component::ALL
                    .iter()
                    .filter(|c| toolchain.component(c).is_none())
                    .map(|c| (*c).to_owned())
                    .collect(),
                tools,
                system_compiler: toolchain.system_compiler().cloned(),
                quartus: toolchain.quartus().cloned(),
                verify: mismatches.map(<[FileMismatch]>::to_vec),
            });
        }
        println!(
            "{} {} ({}) in {}",
            paint(BOLD, "Bundle"),
            manifest.bundle,
            manifest.platform,
            toolchain.root()
        );
        for c in &manifest.components {
            println!(
                "  {:<14} {:<16} {}",
                c.name,
                c.version,
                paint(DIM, &c.source)
            );
        }
        for name in component::ALL {
            if toolchain.component(name).is_none() {
                println!("  {:<14} {}", name, paint(DIM, "not installed"));
            }
        }
        println!("{}", paint(BOLD, "Tools"));
        for (tool, path) in tools {
            let name = tool.binary_name();
            match path {
                Ok(path) if from_system(tool) => println!(
                    "  {} {name:<14} {path}  {}",
                    paint(OK, "OK"),
                    paint(DIM, "(system)")
                ),
                Ok(path) => println!("  {} {name:<14} {path}", paint(OK, "OK")),
                Err(LaceError::ComponentMissing(c)) => {
                    println!(
                        "  {} {name:<14} {}",
                        paint(DIM, "--"),
                        paint(DIM, format!("{c} not installed"))
                    )
                }
                // Só as placas Intel precisam dele; a linha do Quartus
                // abaixo diz como apontar um.
                Err(LaceError::QuartusMissing) => println!(
                    "  {} {name:<14} {}",
                    paint(DIM, "--"),
                    paint(DIM, "not found on the system")
                ),
                Err(error) => println!("  {} {name:<14} {error}", paint(ERROR, "!!")),
            }
        }
        match toolchain.system_compiler() {
            Some(c) => println!(
                "{} {} (make {}, perl {}){}",
                paint(BOLD, "Verilator compiler"),
                c.cxx,
                c.make,
                c.perl,
                paint(
                    DIM,
                    if c.bundled {
                        "  (bundle)"
                    } else {
                        "  (system)"
                    }
                )
            ),
            // No Windows o compilador vem com o componente verilator: sem
            // ele, a linha do verilator acima já diz o que falta.
            None if toolchain.platform() == Platform::WindowsX64 => {}
            None => println!(
                "{} {}",
                paint(BOLD, "Verilator compiler"),
                paint(
                    WARNING,
                    "not found on the system, so Verilator cannot run (set its location with --compiler <DIR> or LACE_COMPILER)"
                )
            ),
        }
        match toolchain.quartus() {
            Some(q) => println!(
                "{} {} {}{}",
                paint(BOLD, "Quartus Prime"),
                q.version.as_deref().unwrap_or("?"),
                q.root,
                paint(DIM, "  (system)")
            ),
            // O Quartus não existe para macOS.
            None if toolchain.platform() == Platform::MacosArm64 => {}
            None => println!(
                "{} {}",
                paint(BOLD, "Quartus Prime"),
                paint(
                    DIM,
                    "not found; Intel FPGA boards need it (set its location with --quartus <DIR> or LACE_QUARTUS)"
                )
            ),
        }
        if let Some(mismatches) = mismatches {
            if mismatches.is_empty() {
                println!(
                    "{} {} executables match the manifest",
                    paint(OK, "OK"),
                    toolchain.verified_files()
                );
            }
            for m in mismatches {
                let what = if m.actual.is_some() {
                    "hash mismatch"
                } else {
                    "missing"
                };
                println!(
                    "  {} {}: {what} ({})",
                    paint(ERROR, "!!"),
                    m.path,
                    m.component
                );
            }
        }
        Ok(())
    }

    /// O projeto: Verilog registrado (topo e testbench escolhido marcados),
    /// os `.v` da pasta que não estão registrados e os processadores. Num
    /// projeto vazio, como começar nos dois fluxos.
    /// `here`: o processador da pasta em que o comando rodou.
    pub fn status(&self, project: &Project, here: Option<&Processor>) -> anyhow::Result<()> {
        let synth = project.files(FileRole::Synthesizable);
        let tbs = project.files(FileRole::Testbench);
        let unregistered = project.unregistered_verilog();
        // Um arquivo de topo sem módulo identificável não impede o resumo; o
        // erro aparece em `lace top` e em quem precisa do nome.
        let top_module = project.top_module().ok().flatten();
        if self.json {
            return self.json(&StatusReport {
                name: project.name().to_owned(),
                spf: project.spf_path().to_owned(),
                root: project.root().to_owned(),
                synthesizable: synth,
                testbench: tbs,
                top_level: project.top_level(),
                top_module,
                selected_testbench: project.testbench(),
                testbench_module: project.testbench_module().ok().flatten(),
                unregistered,
                here: here.map(|p| p.name.clone()),
                processors: project
                    .processors()
                    .iter()
                    .map(|p| ProcessorStatus {
                        processor: p.clone(),
                        built: p.is_built(),
                    })
                    .collect(),
                issues: project.issues(),
            });
        }
        let root = project.root();
        println!("{} {}", paint(BOLD, "Project"), project.name());
        println!("  {}", project.spf_path());
        for issue in project.issues() {
            println!(
                "  {}",
                paint(WARNING, format!("warning: {}", issue.message))
            );
        }
        let empty = synth.is_empty() && tbs.is_empty() && project.processors().is_empty();
        if !empty {
            let top_mark = match &top_module {
                Some(module) => format!("top: {module}"),
                None => "top".to_owned(),
            };
            file_section("Modules", &synth, project.top_level(), &top_mark, root);
            file_section("Testbenches", &tbs, project.testbench(), "selected", root);
        }
        if !unregistered.is_empty() {
            println!("{}", paint(BOLD, "Untracked files"));
            for file in &unregistered {
                println!("  {}", relative(file, root));
            }
            println!("  {}", paint(DIM, "Add one with: lace add <file>"));
        }
        if !project.processors().is_empty() {
            println!("{}", paint(BOLD, "Processors"));
            processors_text(project, here);
        }
        if empty {
            println!("{}", paint(BOLD, "Empty project"));
            print_steps(&[
                (
                    "lace add <name>.v <name>_tb.v".to_owned(),
                    "Verilog: module and testbench, then lace sim",
                ),
                (
                    "lace proc add <name>".to_owned(),
                    "SAPHO: processor, then lace sim -p <name>",
                ),
            ]);
        }
        Ok(())
    }

    /// O fim de uma operação: o que terminou e em quanto tempo, cada passo
    /// (feito, falhou, não rodou), as mensagens das ferramentas e os
    /// artefatos, gerados ou não.
    fn summary(&self, summary: Summary<'_>) {
        let Summary {
            title,
            detail,
            status,
            failed_step,
            steps,
            planned,
            diagnostics,
            artifacts,
            duration_ms,
            root,
            independent_steps,
            limit_ms,
        } = summary;
        println!(
            "{} {}: {}",
            paint(BOLD, title),
            paint(DIM, format!("({detail})")),
            headline(status, failed_step, steps, duration_ms, limit_ms)
        );

        // As colunas são alinhadas antes da cor: os códigos de cor não
        // ocupam espaço na tela, mas contariam no alinhamento.
        for (i, step) in steps.iter().enumerate() {
            let last = !independent_steps && i + 1 == steps.len();
            let (style, state) = step_state(step, last, status, failed_step);
            println!(
                "    {} {:<13} {:<10} {}",
                paint(style, format!("{state:<9}")),
                step_name(step.step),
                step.tool.to_string(),
                paint(DIM, format!("{:>9}", duration(step.duration_ms)))
            );
        }
        // O que estava planejado e não chegou a rodar.
        for (step, tool) in planned {
            if !steps.iter().any(|s| s.step == *step) {
                println!(
                    "    {} {:<13} {}",
                    paint(DIM, format!("{:<9}", "not run")),
                    step_name(*step),
                    paint(DIM, tool.to_string())
                );
            }
        }

        for d in diagnostics {
            if d.severity == Severity::Info && !self.verbose {
                continue;
            }
            println!("  {}", format_diagnostic(d));
        }

        let generated: Vec<&Artifact> = artifacts
            .iter()
            .filter(|a| a.fresh && (a.required || self.verbose))
            .collect();
        let missing: Vec<&Artifact> = artifacts
            .iter()
            .filter(|a| a.required && !a.fresh)
            .collect();
        // Numa falha, o que saiu pode estar pela metade (o `.asm` de um
        // `cmmcomp` que parou no erro).
        let written = if status == Status::Succeeded {
            paint(OK, "Generated")
        } else {
            paint(DIM, "Written before it stopped")
        };
        artifact_list(&written, &generated, root, false);
        artifact_list(&paint(WARNING, "Not generated"), &missing, root, true);
    }

    /// Uma fase que não rodou porque a anterior falhou.
    pub fn not_run(&self, what: &str, builds: &[BuildResult]) {
        if self.json {
            return;
        }
        let why = builds
            .iter()
            .find(|b| !b.succeeded())
            .map_or_else(String::new, |b| {
                format!(": the build of {} did not finish", b.processor)
            });
        println!("{} {}{why}", paint(BOLD, what), paint(WARNING, "not run"));
    }

    /// Uma dica de próximo passo, apagada.
    pub fn next(&self, text: &str) {
        if !self.json {
            println!("  {}", paint(DIM, text));
        }
    }
}

/// Os artefatos com o papel e o caminho, alinhados. Os que não foram
/// gerados mostram se sobrou um de uma execução anterior.
fn artifact_list(title: &str, artifacts: &[&Artifact], root: &Utf8Path, missing: bool) {
    if artifacts.is_empty() {
        return;
    }
    println!("  {title}:");
    for a in artifacts {
        let path = relative(&a.path, root);
        let note = if missing && a.path.exists() {
            paint(DIM, "  (left from an earlier run)")
        } else {
            String::new()
        };
        println!("    {:<19} {path}{note}", artifact_label(a.kind));
    }
}

/// O papel de um artefato, para gente.
fn artifact_label(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Assembly => "assembly",
        ArtifactKind::Verilog => "processor Verilog",
        ArtifactKind::DataMemory => "data memory",
        ArtifactKind::InstructionMemory => "instruction memory",
        ArtifactKind::Testbench => "testbench",
        ArtifactKind::PreprocessedSource => "preprocessed C",
        ArtifactKind::ProgramCounterMap => "PC map",
        ArtifactKind::SourceTranslation => "source map",
        ArtifactKind::OpcodeTranslation => "opcode map",
        ArtifactKind::CompilerLog => "compiler log",
        ArtifactKind::PreAssemblerLog => "pre-assembler log",
        ArtifactKind::IcarusImage => "Icarus image",
        ArtifactKind::VerilatedModel => "Verilator model",
        ArtifactKind::Waveform => "waveform",
        ArtifactKind::SimulationOutput => "output",
        ArtifactKind::Netlist => "netlist",
        ArtifactKind::SchematicGraph => "schematic graph",
        ArtifactKind::Schematic => "schematic",
        ArtifactKind::SynthesisStatistics => "statistics",
        ArtifactKind::BoardTop => "board top",
        ArtifactKind::QuartusProject => "Quartus project",
        ArtifactKind::SramObject => "bitstream (.sof)",
        ArtifactKind::RawBinary => "bitstream (.rbf)",
        ArtifactKind::SerialVectorFormat => "JTAG vectors (.svf)",
        _ => "file",
    }
}

/// Como um passo que rodou terminou, e a cor. Um passo que saiu com 0
/// também falhou quando foi ele que reprovou a operação (o `vvp` depois de
/// um `$error`): é o último a rodar e o `failed_step`.
fn step_state(
    step: &StepReport,
    last: bool,
    status: Status,
    failed_step: Option<Step>,
) -> (Style, &'static str) {
    match step.termination {
        Termination::Exited(0)
            if last && status == Status::Failed && Some(step.step) == failed_step =>
        {
            (ERROR, "failed")
        }
        Termination::Exited(0) => (OK, "done"),
        Termination::Cancelled => (WARNING, "cancelled"),
        Termination::TimedOut => (ERROR, "timed out"),
        Termination::Signaled(_) | Termination::Exception(_) => (ERROR, "crashed"),
        _ => (ERROR, "failed"),
    }
}

fn step_name(step: Step) -> String {
    serde_json::to_value(step)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.replace('_', " ")))
        .unwrap_or_default()
}

/// `850 ms`, `1.23 s`.
fn duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.2} s", ms as f64 / 1000.0)
    }
}

/// O erro com as causas, como o `{:#}` do anyhow, sem repetir uma causa que
/// a mensagem de cima já termina com ela (o `Io` do Core escreve a causa e
/// também a devolve como `source`: "No such file or directory (os error 2):
/// No such file or directory (os error 2)").
fn chain_text(error: &anyhow::Error) -> String {
    let mut text = String::new();
    for cause in error.chain() {
        let part = cause.to_string();
        if text.ends_with(&part) {
            continue;
        }
        if !text.is_empty() {
            text.push_str(": ");
        }
        text.push_str(&part);
    }
    text
}

/// Quantas raízes o título da verificação nomeia; acima disso, o resto vira
/// "e mais N" (um projeto de 1500 módulos soltos não ocupa uma tela).
const TITLE_TARGETS: usize = 5;

/// O que fazer depois de um erro, quando o código diz. As mensagens do Core
/// são neutras; os nomes de comando ficam aqui.
fn hint(error: &LaceError) -> Option<String> {
    Some(match error {
        LaceError::NoTopLevel(_) => "Choose it with: lace top <file|module>".into(),
        LaceError::NoTestbench(_) => {
            "Create it with: lace add <name>_tb.v, or a cocotb one with: lace add test_<name>.py"
                .into()
        }
        LaceError::NoCocotbToplevel(_) => {
            "Add a line `# aurora-toplevel: <module>` to the testbench, or choose the top with: lace top <file|module>".into()
        }
        LaceError::CocotbUnavailable { .. } => "Reinstall it with: lace install cocotb".into(),
        LaceError::EmptyProject(_) => {
            "Add one with: lace add <file.v>, or create a processor with: lace proc add <name>"
                .into()
        }
        // Sem o Verilog do processador (o check não compila), compilar basta;
        // sem o testbench (a onda de quem nunca simulou), é simular.
        LaceError::NotBuilt { processor, missing }
            if missing.file_name() == Some(format!("{processor}.v").as_str()) =>
        {
            format!("Build it with: lace build -p {processor}")
        }
        LaceError::NotBuilt { processor, .. } => {
            format!("Build and simulate with: lace sim -p {processor}")
        }
        LaceError::AmbiguousModule { files, .. } => format!(
            "Choose it by file: lace top {}",
            files.first().map(|f| f.as_str()).unwrap_or("<file>")
        ),
        LaceError::ComponentMissing(component) => {
            format!("Install it with: lace install {component}")
        }
        LaceError::NonAsciiPath { .. } => {
            "Move the project to a folder without accents or other non-ASCII characters".into()
        }
        LaceError::ProcessorNotFound { name, available } if available.is_empty() => {
            format!("Create it with: lace proc add {name}")
        }
        LaceError::SystemCompilerMissing => {
            "Or set its location with --compiler <DIR> or LACE_COMPILER".into()
        }
        LaceError::ProjectNotFound(_) => {
            "Create one with: lace new <name>, or point to it with -C <folder>".into()
        }
        LaceError::NoReports(_) => "lace build, check, sim and synth each store a report".into(),
        LaceError::ReportNotFound(_) => "List them with: lace report list".into(),
        LaceError::BoardNotFound { .. } => "List them with: lace fpga boards".into(),
        LaceError::NoFpgaConfig(_) => {
            "Write fpga.json next to the .spf with the board and the connections, e.g. {\"board\": \"de2-115\", \"connect\": {\"clk\": \"CLOCK_50\", \"rst\": \"!KEY[0]\"}}; see the signals with: lace fpga boards <board>".into()
        }
        LaceError::InvalidBoard { .. } => "This is a bug in Lace; please report it".into(),
        LaceError::NoBitstream(_) => "Build for the board first with: lace fpga build".into(),
        LaceError::StaleBitstream { .. } => {
            "Build for the board again with: lace fpga build, then program".into()
        }
        LaceError::NoCable => {
            "Connect the board through its USB-Blaster port, turn it on and set its RUN/PROG switch to RUN; on Windows the USB-Blaster driver is in the drivers folder of Quartus, and on Linux the cable needs a udev rule (see docs/FPGA.md)".into()
        }
        LaceError::QuartusMissing => {
            "Install Quartus Prime Lite (Windows or Linux) with the device support of the board, or set its folder with --quartus <DIR> or LACE_QUARTUS".into()
        }
        _ => return None,
    })
}

/// `run-000042` como o usuário pode digitar: `42`.
fn short_id(id: &str) -> String {
    id.strip_prefix("run-")
        .map(|n| n.trim_start_matches('0'))
        .filter(|n| !n.is_empty())
        .unwrap_or(id)
        .to_owned()
}

fn status_word(status: Status) -> &'static str {
    match status {
        Status::Succeeded => "PASS",
        Status::Failed => "FAIL",
        Status::Crashed => "CRASH",
        Status::Incomplete => "INCOMPLETE",
        Status::Cancelled => "CANCELLED",
        Status::TimedOut => "TIMEOUT",
        _ => "UNKNOWN",
    }
}

fn comparison_header(title: &str, id: &str, m: &RunMetadata) {
    let not = "not reported";
    println!("{}", paint(BOLD, title));
    println!("  ID:         {id}");
    println!("  Timestamp:  {}", m.timestamp);
    println!("  Command:    {}", m.command);
    println!("  Top:        {}", m.top.as_deref().unwrap_or(not));
    let simulator = m.simulation.as_ref().map_or_else(
        || not.to_owned(),
        |s| {
            let name = serde_json::to_value(s.simulator)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            match &s.component_version {
                Some(v) => format!("{name} {v}"),
                None => name,
            }
        },
    );
    println!("  Simulator:  {simulator}");
    let synthesis = m
        .synthesis
        .as_ref()
        .map_or(not.to_owned(), |s| match &s.component_version {
            Some(v) => format!("yosys {v}"),
            None => "yosys".to_owned(),
        });
    println!("  Synthesis:  {synthesis}");
}

fn comparison_synthesis(c: &RunComparison) {
    println!("\n{}\n", paint(BOLD, "GENERIC SYNTHESIS STATISTICS"));
    let Some(s) = &c.synthesis else {
        println!(
            "Comparison unavailable: one or both reports have no comparable synthesis statistics."
        );
        return;
    };
    println!(
        "{:<22} {:>14} {:>14} {:>12} {:>12}",
        "Metric", "Baseline", "Current", "Change", "Percent"
    );
    for m in &s.metrics {
        integer_row(m.metric.label(), &m.comparison);
    }
    println!("\n{}\n", paint(BOLD, "CELL USAGE"));
    println!(
        "{:<22} {:>14} {:>14} {:>12} {:>12}",
        "Cell type", "Baseline", "Current", "Change", "Percent"
    );
    for cell in &s.cell_types {
        integer_row(&cell.cell_type, &cell.usage);
    }
}

fn comparison_timings(c: &RunComparison) {
    println!("\n{}\n", paint(BOLD, "SIMULATION TIMINGS"));
    let Some(t) = &c.simulation else {
        println!(
            "Comparison unavailable: one or both reports have no comparable simulation timings."
        );
        return;
    };
    println!(
        "{:<18} {:>16} {:>16} {:>14} {:>12}",
        "Metric", "Baseline", "Current", "Change", "Percent"
    );
    time_row("Compilation", &t.compile, TimeBase::Milliseconds);
    time_row("Execution", &t.execution, TimeBase::Milliseconds);
    time_row("Total", &t.total, TimeBase::Milliseconds);
    time_row("Simulated time", &t.simulated, TimeBase::Femtoseconds);
}

fn comparison_summary(c: &RunComparison) {
    println!("\n{}\n", paint(BOLD, "COMPARISON SUMMARY"));
    println!("Synthesis");
    match c
        .synthesis
        .as_ref()
        .and_then(|s| Some((s, s.metric(SynthesisMetric::Cells)?)))
    {
        Some((s, cells)) => {
            println!(
                "  Cells:              {} -> {}",
                integer(cells.baseline),
                integer(cells.current)
            );
            println!(
                "  Change:             {} ({})",
                integer_change(cells),
                percent(cells)
            );
            println!("  Cell types added:   {}", s.added_cell_types);
            println!("  Cell types removed: {}", s.removed_cell_types);
        }
        None => println!("  Comparison unavailable"),
    }
    println!("\nSimulation");
    match &c.simulation {
        Some(t) => {
            for (label, m) in [("Execution", &t.execution), ("Total", &t.total)] {
                let unit = TimeBase::Milliseconds.unit_for(m);
                println!(
                    "  {:<20}{} -> {}",
                    format!("{label}:"),
                    time(m.baseline, unit),
                    time(m.current, unit)
                );
                println!(
                    "  Change:             {} ({})",
                    time_change(m, unit),
                    percent(m)
                );
            }
        }
        None => println!("  Comparison unavailable"),
    }
}

fn integer(value: Option<u64>) -> String {
    value.map_or("not reported".to_owned(), |v| v.to_string())
}

fn integer_change(m: &MetricComparison) -> String {
    match m.delta {
        None => "-".into(),
        Some(0) => "0".into(),
        Some(d) => format!("{d:+}"),
    }
}

/// A porcentagem da referência: `-` sem um dos lados, `new` de zero para
/// outro número.
fn percent(m: &MetricComparison) -> String {
    if m.baseline.is_none() || m.current.is_none() {
        return "-".into();
    }
    match m.percent {
        None => "new".into(),
        Some(0.0) => "0.0%".into(),
        Some(p) => format!("{p:+.1}%"),
    }
}

fn integer_row(label: &str, m: &MetricComparison) {
    let label = if label.chars().count() > 22 {
        println!("{label}");
        ""
    } else {
        label
    };
    println!(
        "{label:<22} {:>14} {:>14} {:>12} {:>12}",
        integer(m.baseline),
        integer(m.current),
        integer_change(m),
        percent(m)
    );
}

/// Em que unidade um tempo foi guardado.
#[derive(Clone, Copy)]
enum TimeBase {
    Milliseconds,
    Femtoseconds,
}

impl TimeBase {
    /// A maior unidade em que o maior dos dois valores dá pelo menos 1, para
    /// as duas colunas e a mudança ficarem na mesma unidade.
    fn unit_for(self, m: &MetricComparison) -> (&'static str, u64) {
        let units: &[(&str, u64)] = match self {
            TimeBase::Milliseconds => &[("s", 1000), ("ms", 1)],
            TimeBase::Femtoseconds => &[
                ("s", 1_000_000_000_000_000),
                ("ms", 1_000_000_000_000),
                ("us", 1_000_000_000),
                ("ns", 1_000_000),
                ("ps", 1_000),
                ("fs", 1),
            ],
        };
        let largest = m.baseline.unwrap_or(0).max(m.current.unwrap_or(0));
        *units
            .iter()
            .find(|(_, d)| largest >= *d)
            .unwrap_or(units.last().expect("unidades"))
    }
}

fn time(value: Option<u64>, (suffix, divisor): (&str, u64)) -> String {
    match value {
        None => "not reported".into(),
        Some(v) if divisor == 1 => format!("{v} {suffix}"),
        Some(v) => format!("{:.1} {suffix}", v as f64 / divisor as f64),
    }
}

fn time_change(m: &MetricComparison, unit: (&str, u64)) -> String {
    match m.delta {
        None => "-".into(),
        Some(d) => {
            let sign = match d.signum() {
                1 => "+",
                -1 => "-",
                _ => "",
            };
            format!("{sign}{}", time(Some(d.unsigned_abs()), unit))
        }
    }
}

fn time_row(label: &str, m: &MetricComparison, base: TimeBase) {
    let unit = base.unit_for(m);
    println!(
        "{label:<18} {:>16} {:>16} {:>14} {:>12}",
        time(m.baseline, unit),
        time(m.current, unit),
        time_change(m, unit),
        percent(m)
    );
}

/// Comandos de exemplo alinhados, com a explicação apagada ao lado.
fn print_steps(steps: &[(String, &str)]) {
    for (command, what) in steps {
        if what.is_empty() {
            println!("  {command}");
        } else {
            println!("  {command:<30} {}", paint(DIM, what));
        }
    }
}

fn file_section(
    title: &str,
    files: &[ProjectFile],
    selected: Option<Utf8PathBuf>,
    mark: &str,
    root: &Utf8Path,
) {
    println!("{}", paint(BOLD, title));
    if files.is_empty() {
        println!("  {}", paint(DIM, "None"));
    }
    for file in files {
        let mut line = format!("  {}", relative(&file.path, root));
        if selected.as_ref() == Some(&file.path) {
            line += &format!("  {}", paint(OK, mark));
        }
        // Registrado mas apagado do disco: a checagem e a simulação vão
        // recusar o projeto até ele voltar ou sair (lace remove).
        if !file.path.is_file() {
            line += &format!("  {}", paint(WARNING, "does not exist"));
        }
        println!("{line}");
    }
}

fn processors_text(project: &Project, here: Option<&Processor>) {
    for p in project.processors() {
        let built = if p.is_built() {
            paint(OK, "built")
        } else {
            paint(DIM, "not built")
        };
        let mark = if here.is_some_and(|h| h.name == p.name) {
            format!("  {}", paint(BOLD, "(this folder)"))
        } else {
            String::new()
        };
        println!(
            "  {:<16} {:<3} {:>4} MHz {:>8} clocks{}  {built}{mark}",
            p.name,
            language_name(p.language),
            p.frequency_mhz,
            p.clocks,
            if p.show_arrays { "  arrays" } else { "" },
        );
    }
    if let Some(here) = here {
        println!(
            "  {}",
            paint(
                DIM,
                format!(
                    "Here, build, check, sim, wave, synth and proc set act on {}",
                    here.name
                )
            )
        );
    }
}

/// Um erro do Core como aparece dentro de outro JSON.
fn error_info(error: &LaceError) -> ErrorInfo {
    ErrorInfo {
        code: error.code().to_owned(),
        message: error.to_string(),
        hint: hint(error),
    }
}

struct Summary<'a> {
    title: &'a str,
    detail: &'a str,
    status: Status,
    failed_step: Option<Step>,
    steps: &'a [StepReport],
    /// Os passos que a operação roda, na ordem, para dizer quais não
    /// rodaram. Vazio quando eles variam (a verificação).
    planned: &'a [(Step, Tool)],
    diagnostics: &'a [Diagnostic],
    artifacts: &'a [Artifact],
    duration_ms: u64,
    root: &'a Utf8Path,
    /// Cada passo vale por si: um que saiu com 0 deu certo, mesmo sendo o
    /// último de uma operação que falhou. É a hierarquia, que elabora cada
    /// testbench à parte e continua depois de uma falha.
    independent_steps: bool,
    /// O prazo pedido (`--timeout`), em milissegundos: o título diz que a
    /// ferramenta parou nele, e não depois (o encerramento leva um instante).
    limit_ms: Option<u64>,
}

fn language_name(language: Language) -> &'static str {
    match language {
        Language::Cmm => "C±",
        Language::Cpp => "C",
        _ => "?",
    }
}

/// Quantas instâncias há abaixo de `node`, em qualquer profundidade.
fn descendants(node: &ModuleInstance) -> usize {
    node.children.iter().map(|c| 1 + descendants(c)).sum()
}

fn relative<'a>(path: &'a Utf8Path, root: &Utf8Path) -> &'a Utf8Path {
    path.strip_prefix(root).unwrap_or(path)
}

/// Como a operação terminou, numa frase: `finished in 0.09 s`, `failed
/// after 0.04 s: iverilog exited with code 2`.
fn headline(
    status: Status,
    failed_step: Option<Step>,
    steps: &[StepReport],
    ms: u64,
    limit_ms: Option<u64>,
) -> String {
    let after = duration(ms);
    // O passo que o Lace encerrou é o último que rodou.
    let stopped = steps
        .last()
        .filter(|s| Some(s.step) == failed_step && !s.termination.success());
    match status {
        Status::Succeeded => paint(OK, format!("finished in {after}")),
        Status::Incomplete => paint(
            WARNING,
            format!("incomplete after {after}: the tools ran, but some files were not generated"),
        ),
        Status::Cancelled => match stopped {
            Some(s) => paint(
                WARNING,
                format!("cancelled after {after}, {} stopped", s.tool),
            ),
            None => paint(WARNING, "cancelled before starting"),
        },
        Status::TimedOut => {
            let which = stopped.map_or_else(String::new, |s| match limit_ms {
                Some(limit) => format!(", {} stopped at the {} limit", s.tool, seconds(limit)),
                None => format!(", {} stopped after {}", s.tool, seconds(s.duration_ms)),
            });
            paint(ERROR, format!("timed out{which}"))
        }
        status => {
            // O passo que falhou, e não o último: a checagem roda o mesmo
            // passo uma vez por testbench.
            let failed = steps
                .iter()
                .find(|s| Some(s.step) == failed_step && !s.termination.success())
                .or_else(|| steps.iter().rev().find(|s| Some(s.step) == failed_step))
                .or(steps.last());
            let how = failed.map_or_else(String::new, |s| match s.termination {
                // O vvp sai com 0 depois de um $error; quem marcou a falha
                // foram os diagnósticos.
                Termination::Exited(0) => format!(": {} reported an error", s.tool),
                Termination::Exited(code) => format!(": {} exited with code {code}", s.tool),
                Termination::Signaled(sig) => format!(": {} was killed by signal {sig}", s.tool),
                Termination::Exception(code) => {
                    format!(": {} crashed with exception {code:#010x}", s.tool)
                }
                _ => format!(": {} ended for an unknown reason", s.tool),
            });
            let word = if status == Status::Crashed {
                "crashed"
            } else {
                "failed"
            };
            paint(ERROR, format!("{word} after {after}{how}"))
        }
    }
}

/// `12 s`, `1.5 s`: a duração como se fala.
fn seconds(ms: u64) -> String {
    if ms.is_multiple_of(1000) {
        format!("{} s", ms / 1000)
    } else {
        format!("{:.1} s", ms as f64 / 1000.0)
    }
}

/// A saída das ferramentas enquanto rodam, em texto: o que o testbench
/// escreve, sempre; com -v, o comando e toda a saída de cada passo. As
/// mensagens do próprio simulador (`$finish called at`, `ERROR:`) ficam para
/// o resumo do fim, que as mostra como diagnóstico. Num terminal, a barra de
/// progresso acompanha cada passo, e as linhas passam por ela para não se
/// misturarem.
fn live_text(event: &Event, verbose: bool, progress: Option<&Progress>) {
    let print = |line: String| match progress {
        Some(p) => p.println(&line),
        None => println!("{line}"),
    };
    match event {
        Event::StepStarted {
            step,
            tool,
            command,
        } => {
            if verbose {
                print(format!(
                    "  {}",
                    paint(DIM, format!("$ {}", command.display_command()))
                ));
            }
            if let Some(p) = progress {
                p.start(*step, *tool);
            }
        }
        Event::StepFinished { .. } => {
            if let Some(p) = progress {
                p.stop();
            }
        }
        Event::Output { line, .. } if verbose => {
            print(format!("  {}", paint(DIM, format!("| {line}"))))
        }
        Event::Output {
            step: Step::Simulate,
            line,
            diagnostic: false,
            ..
        } => print(format!("  {} {line}", paint(DIM, "|"))),
        _ => {}
    }
}

/// `arquivo:linha[:coluna]: error: mensagem`, o formato que editores e
/// terminais transformam em link.
fn format_diagnostic(d: &Diagnostic) -> String {
    let severity = match d.severity {
        Severity::Error => paint(ERROR, "error"),
        Severity::Warning => paint(WARNING, "warning"),
        Severity::Info => paint(DIM, "info"),
        _ => paint(DIM, "output"),
    };
    match (&d.file, d.line, d.column) {
        (Some(file), Some(line), Some(col)) => {
            format!("{file}:{line}:{col}: {severity}: {}", d.message)
        }
        (Some(file), Some(line), None) => format!("{file}:{line}: {severity}: {}", d.message),
        (Some(file), None, _) => format!("{file}: {severity}: {}", d.message),
        _ => format!("{}: {severity}: {}", d.tool, d.message),
    }
}
