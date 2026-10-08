//! `lace learn`: os exercícios de Verilog no estilo do rustlings, com o
//! crate `lace-learn`. Sem subcomando, o modo watch: corrige o exercício
//! atual a cada gravação e responde às teclas. Os subcomandos fazem cada
//! coisa uma vez, para scripts e para quem não tem terminal interativo.
//!
//! As mensagens são em inglês, como o resto da CLI; os enunciados e as
//! dicas vêm da trilha, em português.

use std::io::{IsTerminal, Write};
use std::time::Duration;

use anyhow::bail;
use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{Control, RunningProcess, Severity, Toolchain, ViewerOptions};
use lace_learn::{
    DevExercise, Exercise, Finding, Grade, LearnError, Verdict, Workspace, check_track, grade,
    load_track, tracks_dir,
};

use crate::output::{BOLD, DIM, ERROR, OK, Output, WARNING, paint};
use crate::report::{
    LearnCheckReport, LearnHintReport, LearnInitReport, LearnListReport, LearnResetReport,
    LearnWaveReport,
};
use crate::settings;
use crate::{Cli, LearnArgs, LearnCommand, LearnDevCommand};

/// A pasta que o `lace learn init` cria sem nome.
pub const DEFAULT_DIR: &str = "lace-learn";

/// Quanto esperar para saber se o Surfer abriu.
const SURFER_GRACE: Duration = Duration::from_millis(1500);

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
    let root = Workspace::discover(&settings::absolute(&cli.project)?)?;
    let tracks = tracks_dir(toolchain)?;
    Ok(Workspace::open(&root, &tracks, None)?)
}

/// O exercício pedido pelo nome, ou o atual.
fn exercise(workspace: &Workspace, name: Option<&str>) -> anyhow::Result<Exercise> {
    Ok(match name {
        Some(name) => workspace.track().require(name)?.clone(),
        None => workspace.current().clone(),
    })
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
            "Start with: cd {} && lace learn",
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
    let mut results = Vec::new();
    for exercise in &exercises {
        let grade = grade(&toolchain, &workspace, exercise, control)?;
        workspace.record(&grade)?;
        if out.is_text() {
            if all {
                let (style, word) = verdict_word(grade.verdict);
                println(&format!("{:<10} {}", paint(style, word), exercise.name));
            } else {
                print_lines(&result_text(&grade, &workspace, exercise));
            }
        }
        let cancelled = grade.verdict == Verdict::Cancelled;
        results.push(grade);
        if cancelled {
            break;
        }
    }
    let solved = workspace.solved_count();
    let total = workspace.track().len();
    if out.is_text() && all {
        println(&format!("\n{solved} of {total} exercises solved"));
    }
    let ok = results.iter().all(Grade::solved);
    out.json(&LearnCheckReport {
        results,
        solved,
        total,
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
fn status_row(s: &lace_learn::ExerciseStatus, selected: bool) -> String {
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
    let (surfer, layout) = open_wave(&toolchain, &workspace, &exercise)?;
    out.message(
        "Waveform opened in surfer-aurora",
        &workspace.waveform(&exercise),
    );
    out.json(&LearnWaveReport {
        exercise: exercise.name.clone(),
        waveform: workspace.waveform(&exercise),
        layout,
        pid: surfer.id(),
    })?;
    Ok(true)
}

/// Abre a onda da última correção no Surfer, com o layout do exercício.
pub(crate) fn open_wave(
    toolchain: &Toolchain,
    workspace: &Workspace,
    exercise: &Exercise,
) -> anyhow::Result<(RunningProcess, Option<Utf8PathBuf>)> {
    let wave = workspace.waveform(exercise);
    if !wave.is_file() {
        bail!(
            "Exercise {} has no waveform yet: check it first with: lace learn check {}",
            exercise.name,
            exercise.name
        );
    }
    let layout = workspace
        .exercise_dir(exercise)
        .join(lace_learn::layout::LAYOUT_FILE);
    let mut options = ViewerOptions::default();
    let layout = layout.is_file().then_some(layout);
    options.layout = layout.clone();
    let mut surfer = lace_core::open_waveform(toolchain, &wave, &options)?;
    surfer.ensure_started(SURFER_GRACE)?;
    Ok((surfer, layout))
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
        None => tracks_dir(Some(&toolchain))?.join(lace_learn::DEFAULT_TRACK),
    };
    let track = lace_learn::Track::load(&dir, None)?;
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
        _ => String::new(),
    }
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
        bail!("The watch mode has no JSON output; check an exercise with: lace learn check --json");
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("The watch mode needs a terminal; check an exercise with: lace learn check");
    }
    let toolchain = cli.toolchain.resolve()?;
    let workspace = open(cli, Some(&toolchain))?;
    crate::learn_tui::run(toolchain, workspace, control.cancel_token().clone())?;
    Ok(true)
}

/// O erro do lace-learn com o código e a dica, para `Output::error`.
pub fn error_info(error: &LearnError) -> (&'static str, Option<String>) {
    let hint = match error {
        LearnError::ComponentMissing => Some(format!(
            "Install it with: lace install {}",
            lace_core::component::LACE_LEARN
        )),
        LearnError::NoWorkspace(_) => {
            Some("Create one with: lace learn init, or point to it with -C <folder>".into())
        }
        LearnError::UnknownExercise(_) => Some("List them with: lace learn list".into()),
        LearnError::WorkspaceExists(_) => {
            Some("Choose another folder: lace learn init <folder>".into())
        }
        LearnError::TrackNotFound { .. } => Some("The installed track is: verilog".into()),
        _ => None,
    };
    (error.code(), hint)
}
