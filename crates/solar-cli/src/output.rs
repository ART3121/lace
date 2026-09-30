//! Formatação da saída: texto para gente ou JSON para máquina.
//!
//! Com `--json`, cada comando escreve um único objeto JSON no stdout e os
//! métodos de texto não escrevem nada. Sem `--json`, a cor só aparece quando
//! o stdout é um terminal (e respeita `NO_COLOR`), via `anstream`.

use anstream::println;
use anstyle::{AnsiColor, Style};
use camino::Utf8Path;
use serde::Serialize;
use solar_core::{
    Artifact, BuildResult, CheckResult, Diagnostic, FileMismatch, FileRole, Language, Project,
    RunningProcess, SchematicResult, Severity, SimulationResult, SolarError, Status, Step,
    StepReport, SynthesisResult, Termination, Tool, Toolchain, component,
};

const ERROR: Style = AnsiColor::Red.on_default().bold();
const WARNING: Style = AnsiColor::Yellow.on_default().bold();
const OK: Style = AnsiColor::Green.on_default().bold();
const DIM: Style = Style::new().dimmed();
const BOLD: Style = Style::new().bold();

pub struct Output {
    json: bool,
    verbose: bool,
}

/// Um trecho de texto com estilo.
fn paint(style: Style, text: impl std::fmt::Display) -> String {
    format!("{style}{text}{style:#}")
}

impl Output {
    pub fn new(json: bool, verbose: bool) -> Self {
        Output { json, verbose }
    }

    /// Escreve `value` como JSON, só no modo `--json`.
    pub fn json(&self, value: &impl Serialize) -> anyhow::Result<()> {
        if self.json {
            println!("{}", serde_json::to_string_pretty(value)?);
        }
        Ok(())
    }

    /// Um erro que impediu o comando de rodar. No JSON, `code` é o de
    /// `SolarError::code()`, ou `"cli"` para erros da própria linha de
    /// comando.
    pub fn error(&self, error: &anyhow::Error) {
        let code = error
            .chain()
            .find_map(|e| e.downcast_ref::<SolarError>())
            .map_or("cli", SolarError::code);
        if self.json {
            let report =
                serde_json::json!({ "error": { "code": code, "message": format!("{error:#}") } });
            println!("{report}");
        } else {
            anstream::eprintln!("{}: {error:#}", paint(ERROR, "erro"));
        }
    }

    /// Uma linha de confirmação, com o caminho envolvido.
    pub fn message(&self, what: &str, path: &Utf8Path) -> anyhow::Result<()> {
        if self.json {
            return self.json(&serde_json::json!({ "message": what, "path": path }));
        }
        println!("{what}: {path}");
        Ok(())
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
        self.report(Summary {
            title: &format!("build {}", result.processor),
            detail: &detail,
            status: result.status,
            steps: &result.steps,
            diagnostics: &result.diagnostics,
            artifacts: &result.artifacts,
            root,
        });
    }

    pub fn simulation(&self, result: &SimulationResult, root: &Utf8Path) {
        if self.json {
            return;
        }
        let detail = format!("{:?}", result.simulator).to_lowercase();
        self.report(Summary {
            title: &format!("simulação {}", result.top),
            detail: &detail,
            status: result.status,
            steps: &result.steps,
            diagnostics: &result.diagnostics,
            artifacts: &result.artifacts,
            root,
        });
        for missing in &result.missing_inputs {
            println!(
                "  {}: o testbench lê {}, que não existe",
                paint(WARNING, "aviso"),
                relative(missing, root)
            );
        }
        if self.verbose
            && let Some(run) = result.steps.iter().find(|s| s.step == Step::Simulate)
        {
            for line in run.stdout.lines() {
                println!("  {} {line}", paint(DIM, "|"));
            }
        }
    }

    pub fn check(&self, result: &CheckResult, root: &Utf8Path) {
        if self.json {
            return;
        }
        self.report(Summary {
            title: &format!("checagem {}", result.top),
            detail: "iverilog -t null",
            status: result.status,
            steps: &result.steps,
            diagnostics: &result.diagnostics,
            artifacts: &[],
            root,
        });
    }

    pub fn synthesis(&self, result: &SynthesisResult, root: &Utf8Path) {
        if self.json {
            return;
        }
        let detail = format!("{} módulos", result.modules.len());
        self.report(Summary {
            title: &format!("síntese {}", result.top),
            detail: &detail,
            status: result.status,
            steps: &result.steps,
            diagnostics: &result.diagnostics,
            artifacts: &result.artifacts,
            root,
        });
        if self.verbose && !result.modules.is_empty() {
            println!("  módulos: {}", result.modules.join(", "));
        }
    }

    pub fn schematic(&self, result: &SchematicResult, root: &Utf8Path) {
        if self.json {
            return;
        }
        self.report(Summary {
            title: &format!("esquemático {}", result.module),
            detail: "yosys show + dot",
            status: result.status,
            steps: &result.steps,
            diagnostics: &result.diagnostics,
            artifacts: &result.artifacts,
            root,
        });
    }

    pub fn opened(&self, surfer: &RunningProcess) {
        if self.json {
            return;
        }
        println!(
            "surfer-aurora aberto (pid {}), log em {}",
            surfer.id(),
            surfer.log_file()
        );
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
        if self.json {
            let map: serde_json::Map<_, _> = tools
                .iter()
                .map(|(tool, path)| {
                    let value = match path {
                        Ok(p) => serde_json::json!({ "path": p, "system": tool.is_system() }),
                        Err(e) => serde_json::json!({ "error": { "code": e.code(), "message": e.to_string() } }),
                    };
                    (tool.binary_name().to_owned(), value)
                })
                .collect();
            return self.json(&serde_json::json!({
                "root": toolchain.root(),
                "bundle": manifest.bundle,
                "platform": manifest.platform,
                "components": manifest.components,
                "not_installed": component::ALL
                    .iter()
                    .filter(|c| toolchain.component(c).is_none())
                    .collect::<Vec<_>>(),
                "tools": map,
                "system_compiler": toolchain.system_compiler(),
                "verify": mismatches,
            }));
        }
        println!(
            "{} {} ({}) em {}",
            paint(BOLD, "bundle"),
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
                println!("  {:<14} {}", name, paint(DIM, "não instalado"));
            }
        }
        println!("{}", paint(BOLD, "ferramentas"));
        for (tool, path) in tools {
            let name = tool.binary_name();
            match path {
                Ok(path) if tool.is_system() => println!(
                    "  {} {name:<13} {path}  {}",
                    paint(OK, "ok"),
                    paint(DIM, "(sistema)")
                ),
                Ok(path) => println!("  {} {name:<13} {path}", paint(OK, "ok")),
                Err(SolarError::ComponentMissing(c)) => {
                    println!(
                        "  {} {name:<13} {}",
                        paint(DIM, "--"),
                        paint(DIM, format!("{c} não instalado"))
                    )
                }
                Err(error) => println!("  {} {name:<13} {error}", paint(ERROR, "!!")),
            }
        }
        match toolchain.system_compiler() {
            Some(c) => println!(
                "{} {} (make {}, perl {})",
                paint(BOLD, "compilador do sistema"),
                c.cxx,
                c.make,
                c.perl
            ),
            None => println!(
                "{} {}",
                paint(BOLD, "compilador do sistema"),
                paint(
                    WARNING,
                    "não encontrado: o Verilator não roda (solar config set-compiler)"
                )
            ),
        }
        if let Some(mismatches) = mismatches {
            if mismatches.is_empty() {
                println!(
                    "{} {} executáveis conferem com o manifesto",
                    paint(OK, "ok"),
                    toolchain.verified_files()
                );
            }
            for m in mismatches {
                let what = if m.actual.is_some() {
                    "hash diferente"
                } else {
                    "ausente"
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

    pub fn files(&self, project: &Project) -> anyhow::Result<()> {
        let synth = project.files(FileRole::Synthesizable);
        let tbs = project.files(FileRole::Testbench);
        if self.json {
            return self.json(&serde_json::json!({
                "synthesizable": synth,
                "testbench": tbs,
                "top_level": project.top_level(),
                "selected_testbench": project.testbench(),
            }));
        }
        self.files_text(project);
        Ok(())
    }

    fn files_text(&self, project: &Project) {
        let root = project.root();
        let top = project.top_level();
        let tb = project.testbench();
        for (title, role, selected, mark) in [
            ("sintetizáveis", FileRole::Synthesizable, &top, "topo"),
            ("testbenches", FileRole::Testbench, &tb, "simulado"),
        ] {
            println!("{}", paint(BOLD, title));
            let files = project.files(role);
            if files.is_empty() {
                println!("  {}", paint(DIM, "nenhum"));
            }
            for f in &files {
                let tag = if Some(&f.path) == selected.as_ref() {
                    format!("  {}", paint(OK, mark))
                } else {
                    String::new()
                };
                println!("  {}{tag}", relative(&f.path, root));
            }
        }
    }

    pub fn processors(&self, project: &Project) -> anyhow::Result<()> {
        if self.json {
            return self.json(&processors_json(project));
        }
        self.processors_text(project);
        Ok(())
    }

    fn processors_text(&self, project: &Project) {
        if project.processors().is_empty() {
            println!("  {}", paint(DIM, "nenhum processador"));
        }
        for p in project.processors() {
            let built = if p.is_built() {
                paint(OK, "compilado")
            } else {
                paint(DIM, "não compilado")
            };
            println!(
                "  {:<16} {:<3} {:>4} MHz {:>8} clocks{}  {built}",
                p.name,
                language_name(p.language),
                p.frequency_mhz,
                p.clocks,
                if p.show_arrays { "  arrays" } else { "" },
            );
        }
    }

    pub fn status(&self, project: &Project) -> anyhow::Result<()> {
        if self.json {
            return self.json(&serde_json::json!({
                "name": project.name(),
                "spf": project.spf_path(),
                "root": project.root(),
                "processors": processors_json(project),
                "synthesizable": project.files(FileRole::Synthesizable),
                "testbench": project.files(FileRole::Testbench),
                "top_level": project.top_level(),
                "selected_testbench": project.testbench(),
            }));
        }
        println!("{} {}", paint(BOLD, "projeto"), project.name());
        println!("  {}", project.spf_path());
        println!("{}", paint(BOLD, "processadores"));
        self.processors_text(project);
        self.files_text(project);
        Ok(())
    }

    pub fn config(
        &self,
        path: &Utf8Path,
        file: Option<&crate::settings::ConfigFile>,
    ) -> anyhow::Result<()> {
        if self.json {
            return self.json(&serde_json::json!({ "path": path, "config": file }));
        }
        println!("{} {path}", paint(BOLD, "configuração"));
        match file {
            None => println!(
                "  {}",
                paint(
                    DIM,
                    "não existe; crie com `solar config init --toolchain <DIR>`"
                )
            ),
            Some(file) => println!("{}", serde_json::to_string_pretty(file)?),
        }
        Ok(())
    }

    fn report(&self, summary: Summary<'_>) {
        let Summary {
            title,
            detail,
            status,
            steps,
            diagnostics,
            artifacts,
            root,
        } = summary;
        println!(
            "{title}: {} {}",
            headline(status, steps),
            paint(DIM, format!("({detail})"))
        );
        for d in diagnostics {
            if d.severity == Severity::Info && !self.verbose {
                continue;
            }
            println!("  {}", format_diagnostic(d));
        }
        let succeeded = status == Status::Succeeded;
        for artifact in artifacts {
            let show = if succeeded {
                artifact.required || self.verbose
            } else {
                artifact.required && !artifact.fresh
            };
            if show {
                let mark = if artifact.fresh {
                    String::new()
                } else {
                    paint(DIM, " (não gerado)")
                };
                println!("  -> {}{mark}", relative(&artifact.path, root));
            }
        }
    }
}

fn processors_json(project: &Project) -> serde_json::Value {
    project
        .processors()
        .iter()
        .map(|p| {
            let mut value = serde_json::to_value(p).expect("Processor serializa");
            value["built"] = p.is_built().into();
            value
        })
        .collect()
}

struct Summary<'a> {
    title: &'a str,
    detail: &'a str,
    status: Status,
    steps: &'a [StepReport],
    diagnostics: &'a [Diagnostic],
    artifacts: &'a [Artifact],
    root: &'a Utf8Path,
}

fn language_name(language: Language) -> &'static str {
    match language {
        Language::Cmm => "C±",
        Language::Cpp => "C",
        _ => "?",
    }
}

fn relative<'a>(path: &'a Utf8Path, root: &Utf8Path) -> &'a Utf8Path {
    path.strip_prefix(root).unwrap_or(path)
}

fn headline(status: Status, steps: &[StepReport]) -> String {
    match status {
        Status::Succeeded => paint(OK, "ok"),
        Status::Incomplete => paint(WARNING, "incompleto: faltaram artefatos"),
        status => {
            let how = steps.last().map(|s| match s.termination {
                Termination::Exited(code) => format!("{} saiu com código {code}", s.tool),
                Termination::Signaled(sig) => format!("{} morreu com o sinal {sig}", s.tool),
                Termination::Exception(code) => {
                    format!("{} quebrou com a exceção {code:#010x}", s.tool)
                }
                _ => format!("{} terminou de forma desconhecida", s.tool),
            });
            let word = if status == Status::Crashed {
                "quebrou"
            } else {
                "falhou"
            };
            format!("{}: {}", paint(ERROR, word), how.unwrap_or_default())
        }
    }
}

/// `arquivo:linha[:coluna]: erro: mensagem`, o formato que editores e
/// terminais transformam em link.
fn format_diagnostic(d: &Diagnostic) -> String {
    let severity = match d.severity {
        Severity::Error => paint(ERROR, "erro"),
        Severity::Warning => paint(WARNING, "aviso"),
        Severity::Info => paint(DIM, "info"),
        _ => paint(DIM, "saída"),
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
