//! `aurora learn`: os exercícios de Verilog no estilo do rustlings, com o
//! crate `aurora-learn`. Sem subcomando, o modo watch: corrige o exercício
//! atual a cada gravação e responde às teclas. Os subcomandos fazem cada
//! coisa uma vez, para scripts e para quem não tem terminal interativo.
//!
//! As mensagens são em inglês, como o resto da CLI; os enunciados e as
//! dicas vêm da trilha, em português.

use std::io::{IsTerminal, Write};

use anyhow::bail;
use camino::{Utf8Path, Utf8PathBuf};
use aurora_core::flows::WaveOpened;
use aurora_core::{Control, Severity, Toolchain};
use aurora_learn::flows;
use aurora_learn::{
    DevExercise, Exercise, Finding, Grade, LearnError, Verdict, Workspace, check_track, load_track,
    tracks_dir,
};

use crate::output::{BOLD, DIM, ERROR, OK, Output, WARNING, paint};
use crate::report::{
    LearnCheckReport, LearnHintReport, LearnInitReport, LearnListReport, LearnResetReport,
    LearnWaveReport,
};
use crate::settings;
use crate::{Cli, LearnArgs, LearnCommand, LearnDevCommand};

/// A pasta que o `aurora learn init` cria sem nome.
pub const DEFAULT_DIR: &str = "aurora-learn";

pub fn run(cli: &Cli, args: &LearnArgs, out: &Output, control: &Control) -> anyhow::Result<bool> {
    // Sem a saída ao vivo do testbench: o resumo dele é para a correção ler.
    let quiet = Control::new().with_cancel(control.cancel_token().clone());
    match &args.command {
        None => watch(cli, out, &quiet),
        Some(LearnCommand::Init { dir, track }) => init(cli, dir, track, out),
        Some(LearnCommand::Check { name, all }) => check(cli, name.as_deref(), *all, out, &quiet),
        Some(LearnCommand::List) => list(cli, out),
        Some(LearnCommand::Hint { name }) => hint(cli, name.as_deref(), out),
        Some(LearnCommand::Reset { name, yes }) => reset(cli, name, *yes, out),
        Some(LearnCommand::Wave { name }) => wave(cli, name.as_deref(), out),
        Some(LearnCommand::Dev(LearnDevCommand::Check { track })) => {
            dev_check(cli, track.as_deref(), out, &quiet)
        }
    }
}

/// A pasta de exercícios em que o comando roda (ou a do `-C`), aberta e em
/// dia com a trilha.
fn open(cli: &Cli, toolchain: Option<&Toolchain>) -> anyhow::Result<Workspace> {
    Ok(Workspace::find(
        &settings::absolute(&cli.project)?,
        toolchain,
        None,
    )?)
}

/// O exercício pedido pelo nome, ou o atual.
fn exercise(workspace: &Workspace, name: Option<&str>) -> anyhow::Result<Exercise> {
    Ok(workspace.exercise(name)?)
}

fn init(cli: &Cli, dir: &Utf8Path, track: &str, out: &Output) -> anyhow::Result<bool> {
    let dir = settings::absolute(dir)?;
    let toolchain = cli.toolchain.resolve().ok();
    let tracks = tracks_dir(toolchain.as_ref())?;
    let track = load_track(&tracks, track, None)?;
    let workspace = Workspace::init(&dir, track)?;
    let track = workspace.track();
    if out.is_text() {
        let mut text = String::new();
        text.push_str(&format!("{}\n\n", paint(BOLD, &track.title)));
        text.push_str(&markdown(&track.welcome));
        text.push_str(&format!(
            "\nCreated {} exercises in {}\n",
            track.len(),
            workspace.root()
        ));
        print_lines(&text);
        out.next(&format!(
            "Start with: cd {} && aurora learn",
            from_shell(workspace.root())
        ));
    }
    out.json(&LearnInitReport {
        root: workspace.root().to_owned(),
        track: track.id.clone(),
        exercises: track.len(),
        current: workspace.current().name.clone(),
    })?;
    Ok(true)
}

fn check(
    cli: &Cli,
    name: Option<&str>,
    all: bool,
    out: &Output,
    control: &Control,
) -> anyhow::Result<bool> {
    let toolchain = cli.toolchain.resolve()?;
    let mut workspace = open(cli, Some(&toolchain))?;
    let exercises: Vec<Exercise> = if all {
        workspace.track().exercises().cloned().collect()
    } else {
        vec![exercise(&workspace, name)?]
    };
    let text = out.is_text();
    // O texto de cada resultado só precisa dos caminhos da pasta de
    // exercícios, que não mudam: uma cópia serve enquanto o fluxo grava o
    // estado na original.
    let view = workspace.clone();
    let mut on_grade = |grade: &Grade| {
        if !text {
            return;
        }
        if all {
            let (style, word) = verdict_word(grade.verdict);
            println(&format!("{:<10} {}", paint(style, word), grade.exercise));
        } else if let Some(exercise) = exercises.iter().find(|e| e.name == grade.exercise) {
            print_lines(&result_text(grade, &view, exercise));
        }
    };
    let outcome = flows::check(
        &toolchain,
        &mut workspace,
        &exercises,
        control,
        &mut on_grade,
    )?;
    if text && all {
        println(&format!(
            "\n{} of {} exercises solved",
            outcome.solved, outcome.total
        ));
    }
    let ok = outcome.succeeded();
    out.json(&LearnCheckReport {
        results: outcome.results,
        solved: outcome.solved,
        total: outcome.total,
    })?;
    Ok(ok)
}

fn list(cli: &Cli, out: &Output) -> anyhow::Result<bool> {
    let toolchain = cli.toolchain.resolve().ok();
    let workspace = open(cli, toolchain.as_ref())?;
    let statuses = workspace.statuses();
    let track = workspace.track();
    if out.is_text() {
        println(&format!(
            "{}  {} of {} solved",
            paint(BOLD, &track.title),
            workspace.solved_count(),
            track.len()
        ));
        let mut chapter = String::new();
        for s in &statuses {
            if s.chapter != chapter {
                chapter = s.chapter.clone();
                let title = track
                    .chapters
                    .iter()
                    .find(|c| c.id == chapter)
                    .map_or(chapter.as_str(), |c| c.title.as_str());
                println(&format!("\n{}", paint(BOLD, title)));
            }
            println(&format!("  {}", status_row(s, false)));
        }
    }
    out.json(&LearnListReport {
        root: workspace.root().to_owned(),
        track: track.id.clone(),
        title: track.title.clone(),
        solved: workspace.solved_count(),
        total: track.len(),
        exercises: statuses,
    })?;
    Ok(true)
}

/// Uma linha da lista: a marca do atual, o estado, o nome e o título.
fn status_row(s: &aurora_learn::ExerciseStatus, selected: bool) -> String {
    let pointer = if selected { ">" } else { " " };
    let current = if s.current { "*" } else { " " };
    let state = if s.solved {
        paint(OK, "done")
    } else {
        paint(DIM, "    ")
    };
    format!("{pointer}{current} {state}  {:<22} {}", s.name, s.title)
}

fn hint(cli: &Cli, name: Option<&str>, out: &Output) -> anyhow::Result<bool> {
    let toolchain = cli.toolchain.resolve().ok();
    let workspace = open(cli, toolchain.as_ref())?;
    let exercise = exercise(&workspace, name)?;
    if out.is_text() {
        let mut text = format!("{}\n", paint(BOLD, &exercise.title));
        for (i, hint) in exercise.hints.iter().enumerate() {
            text.push_str(&format!(
                "\n{}\n{}",
                paint(BOLD, format!("Hint {}", i + 1)),
                markdown(hint)
            ));
        }
        print_lines(&text);
    }
    out.json(&LearnHintReport {
        exercise: exercise.name.clone(),
        title: exercise.title.clone(),
        hints: exercise.hints.clone(),
    })?;
    Ok(true)
}

fn reset(cli: &Cli, name: &str, yes: bool, out: &Output) -> anyhow::Result<bool> {
    let toolchain = cli.toolchain.resolve().ok();
    let workspace = open(cli, toolchain.as_ref())?;
    let exercise = exercise(&workspace, Some(name))?;
    let file = workspace.student_file(&exercise);
    if !yes {
        if !std::io::stdin().is_terminal() {
            bail!("Resetting overwrites {file}; confirm with --yes");
        }
        print!(
            "Reset {} to its starting point? Your code in it is lost. [y/N] ",
            from_shell(&file)
        );
        std::io::stdout().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            out.next("Nothing changed");
            return Ok(false);
        }
    }
    workspace.reset(&exercise)?;
    out.message("Exercise reset", &file);
    out.json(&LearnResetReport {
        exercise: exercise.name.clone(),
        file,
    })?;
    Ok(true)
}

fn wave(cli: &Cli, name: Option<&str>, out: &Output) -> anyhow::Result<bool> {
    let toolchain = cli.toolchain.resolve()?;
    let workspace = open(cli, Some(&toolchain))?;
    let exercise = exercise(&workspace, name)?;
    let opened = open_wave(&toolchain, &workspace, &exercise)?;
    out.message("Waveform opened in surfer-aurora", &opened.waveform);
    out.json(&LearnWaveReport {
        exercise: exercise.name.clone(),
        waveform: opened.waveform,
        layout: opened.layout,
        pid: opened.pid,
    })?;
    Ok(true)
}

/// Abre a onda da última correção no Surfer, com o layout do exercício
/// (`aurora_learn::flows::open_wave`).
pub(crate) fn open_wave(
    toolchain: &Toolchain,
    workspace: &Workspace,
    exercise: &Exercise,
) -> anyhow::Result<WaveOpened> {
    Ok(flows::open_wave(toolchain, workspace, exercise)?)
}

fn dev_check(
    cli: &Cli,
    track: Option<&Utf8Path>,
    out: &Output,
    control: &Control,
) -> anyhow::Result<bool> {
    let toolchain = cli.toolchain.resolve()?;
    let dir = match track {
        Some(dir) => settings::absolute(dir)?,
        None => tracks_dir(Some(&toolchain))?.join(aurora_learn::DEFAULT_TRACK),
    };
    let track = aurora_learn::Track::load(&dir, None)?;
    let scratch = tempfile::tempdir()?;
    let scratch = Utf8PathBuf::from_path_buf(scratch.path().join(&track.id))
        .map_err(|p| anyhow::anyhow!("Temporary folder with a non-UTF-8 path: {}", p.display()))?;
    let text = out.is_text();
    let report = check_track(&toolchain, &track, &scratch, control, |e: &DevExercise| {
        if text {
            if e.problems.is_empty() {
                println(&format!("{:<6} {}", paint(OK, "ok"), e.name));
            } else {
                println(&format!(
                    "{:<6} {}: {}",
                    paint(ERROR, "FAIL"),
                    e.name,
                    e.problems.join("; ")
                ));
            }
        }
    })?;
    let failed = report
        .exercises
        .iter()
        .filter(|e| !e.problems.is_empty())
        .count();
    if text {
        let summary = format!(
            "\n{} exercises, {} with problems",
            report.exercises.len(),
            failed
        );
        println(&if failed == 0 {
            paint(OK, summary)
        } else {
            paint(ERROR, summary)
        });
    }
    let ok = report.ok();
    out.json(&report)?;
    Ok(ok)
}

// ------------------------------------------------------------ texto

/// O resultado de uma correção, como o modo watch e o `check` mostram.
fn result_text(grade: &Grade, workspace: &Workspace, exercise: &Exercise) -> String {
    let dir = workspace.exercise_dir(exercise);
    let mut text = String::new();
    let line = |text: &mut String, s: String| {
        text.push_str(&s);
        text.push('\n');
    };
    match grade.verdict {
        Verdict::Solved => {
            line(&mut text, paint(OK, "Solved."));
            line(
                &mut text,
                format!(
                    "The reference solution is in {}; compare it with yours.",
                    relative(&workspace.solution_path(exercise), workspace.root())
                ),
            );
        }
        Verdict::CompileError => line(&mut text, paint(ERROR, "It does not compile.")),
        Verdict::Mismatch => {
            line(
                &mut text,
                paint(
                    ERROR,
                    format!(
                        "Wrong in {} of {} samples.",
                        grade.mismatched, grade.samples
                    ),
                ),
            );
            for output in grade.outputs.iter().filter(|o| o.mismatches > 0) {
                let first = output
                    .first_ns
                    .map(|ns| format!(", first at {ns} ns"))
                    .unwrap_or_default();
                let unknown = if output.unknown > 0 {
                    format!(" ({} in X or Z)", output.unknown)
                } else {
                    String::new()
                };
                line(
                    &mut text,
                    format!(
                        "  {}: wrong in {} samples{first}{unknown}",
                        paint(BOLD, &output.name),
                        output.mismatches
                    ),
                );
            }
            if let Some(first) = &grade.counterexample {
                for l in counterexample_lines(first) {
                    line(&mut text, format!("  {l}"));
                }
            }
        }
        Verdict::TimedOut => line(
            &mut text,
            paint(
                ERROR,
                "The simulation did not finish in time: a loop that never ends, or a combinational loop that never settles.",
            ),
        ),
        Verdict::Incomplete => line(
            &mut text,
            paint(
                ERROR,
                "The simulation stopped before the testbench finished.",
            ),
        ),
        Verdict::Cancelled => line(&mut text, paint(WARNING, "Cancelled.")),
        _ => line(&mut text, format!("{:?}", grade.verdict)),
    }
    for d in &grade.diagnostics {
        let place = match (&d.file, d.line) {
            (Some(file), Some(n)) => format!("{}:{n}: ", relative(file, &dir)),
            (Some(file), None) => format!("{}: ", relative(file, &dir)),
            _ => String::new(),
        };
        let severity = match d.severity {
            Severity::Error => paint(ERROR, "error"),
            Severity::Warning => paint(WARNING, "warning"),
            _ => "note".to_owned(),
        };
        line(&mut text, format!("  {place}{severity}: {}", d.message));
    }
    for finding in &grade.findings {
        line(&mut text, format!("  {}", finding_text(finding)));
    }
    if !grade.output.is_empty() {
        line(&mut text, paint(DIM, "Testbench output:"));
        for out in &grade.output {
            line(&mut text, format!("  | {out}"));
        }
    }
    if grade.verdict == Verdict::Mismatch && grade.waveform.is_some() {
        line(
            &mut text,
            paint(
                DIM,
                "Your outputs and the reference ones are side by side in the waveform.",
            ),
        );
    }
    text
}

/// O que a correção percebeu, para gente.
pub(crate) fn finding_text(finding: &Finding) -> String {
    match finding {
        Finding::InterfaceChanged => "The testbench did not find your module or one of its ports: keep the module name and the port names of the starting file.".to_owned(),
        Finding::UndrivenOutput { output } => {
            format!("{output} is X or Z in every wrong sample: is it assigned?")
        }
        Finding::ResetOnly => "It only goes wrong while the reset is active: check what the reset does (synchronous or asynchronous) and the value after it.".to_owned(),
        Finding::LatchInferred { signal, line } => {
            let at = line.map(|l| format!(", line {l}")).unwrap_or_default();
            format!(
                "{signal} keeps its old value when no branch assigns it: that is a latch{at}. In a combinational always, give it a value in every branch, or a default before the if or case."
            )
        }
        _ => String::new(),
    }
}

/// O contraexemplo em linhas de texto: o instante, as entradas e cada
/// saída, com a esperada e a do aluno.
pub(crate) fn counterexample_lines(first: &aurora_learn::Counterexample) -> Vec<String> {
    let when = match first.cycle {
        Some(cycle) => format!("at {} ns, cycle {cycle}", first.time_ns),
        None => format!("at {} ns", first.time_ns),
    };
    let mut lines = vec![format!("First wrong sample, {when}:")];
    if !first.inputs.is_empty() {
        let inputs: Vec<String> = first
            .inputs
            .iter()
            .map(|i| format!("{} = {}", i.name, i.text))
            .collect();
        lines.push(format!("  inputs   {}", inputs.join(", ")));
    }
    for o in &first.outputs {
        let mark = if o.wrong { "  <- wrong" } else { "" };
        lines.push(format!(
            "  {:<8} expected {}, yours {}{mark}",
            o.name, o.expected, o.actual
        ));
    }
    lines
}

fn verdict_word(verdict: Verdict) -> (anstyle::Style, &'static str) {
    match verdict {
        Verdict::Solved => (OK, "solved"),
        Verdict::Mismatch => (ERROR, "wrong"),
        Verdict::CompileError => (ERROR, "error"),
        Verdict::TimedOut => (ERROR, "timeout"),
        Verdict::Cancelled => (WARNING, "cancelled"),
        _ => (ERROR, "incomplete"),
    }
}

/// O markdown da trilha no terminal: os títulos em negrito, os blocos de
/// código recuados, o resto como está.
fn markdown(text: &str) -> String {
    let mut out = String::new();
    let mut code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            code = !code;
            continue;
        }
        if code {
            out.push_str(&format!("    {}\n", paint(DIM, line)));
        } else if let Some(title) = line
            .strip_prefix("### ")
            .or_else(|| line.strip_prefix("## "))
            .or_else(|| line.strip_prefix("# "))
        {
            out.push_str(&format!("{}\n", paint(BOLD, title)));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// `path` relativo a `base` quando está dentro dela.
fn relative(path: &Utf8Path, base: &Utf8Path) -> String {
    path.strip_prefix(base)
        .map_or_else(|_| path.to_string(), |p| p.to_string())
}

/// `path` como se digitaria a partir do diretório atual.
fn from_shell(path: &Utf8Path) -> String {
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|d| Utf8PathBuf::from_path_buf(d).ok());
    match cwd.as_deref().and_then(|cwd| path.strip_prefix(cwd).ok()) {
        Some(rel) if rel.as_str().is_empty() => ".".into(),
        Some(rel) => rel.to_string(),
        None => path.to_string(),
    }
}

fn println(text: &str) {
    anstream::println!("{text}");
}

/// Escreve um texto de várias linhas.
fn print_lines(text: &str) {
    for line in text.lines() {
        println(line);
    }
}

// ------------------------------------------------------------ modo watch

/// O modo watch: a tela de terminal de `learn_tui.rs`.
fn watch(cli: &Cli, out: &Output, control: &Control) -> anyhow::Result<bool> {
    if !out.is_text() {
        bail!("The watch mode has no JSON output; check an exercise with: aurora learn check --json");
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("The watch mode needs a terminal; check an exercise with: aurora learn check");
    }
    let toolchain = cli.toolchain.resolve()?;
    let workspace = open(cli, Some(&toolchain))?;
    crate::learn_tui::run(toolchain, workspace, control.cancel_token().clone())?;
    Ok(true)
}

/// O erro do aurora-learn com o código e a dica, para `Output::error`.
pub fn error_info(error: &LearnError) -> (&'static str, Option<String>) {
    let hint = match error {
        LearnError::ComponentMissing => Some(format!(
            "Install it with: aurora install {}",
            aurora_core::component::AURORA_LEARN
        )),
        LearnError::NoWorkspace(_) => {
            Some("Create one with: aurora learn init, or point to it with -C <folder>".into())
        }
        LearnError::UnknownExercise(_) => Some("List them with: aurora learn list".into()),
        LearnError::WorkspaceExists(_) => {
            Some("Choose another folder: aurora learn init <folder>".into())
        }
        LearnError::TrackNotFound { .. } => Some("The installed track is: verilog".into()),
        LearnError::NoWaveform { exercise, .. } => {
            Some(format!("Check it first with: aurora learn check {exercise}"))
        }
        _ => None,
    };
    (error.code(), hint)
}
