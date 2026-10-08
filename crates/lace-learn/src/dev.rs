//! O `lace learn dev check`: confere uma trilha antes de ela ir para o
//! componente. Para cada exercício: as portas de `start.v` e de
//! `solution.v` são as mesmas, o enunciado tem dica, o testbench sai, o
//! `start.v` não passa e a `solution.v` passa.

use camino::Utf8Path;
use lace_core::Control;
use schemars::JsonSchema;
use serde::Serialize;

use crate::error::Result;
use crate::grade::{Grade, Verdict, grade};
use crate::testbench;
use crate::track::{Exercise, Track, interface_in};
use crate::workspace::Workspace;

/// O resultado da conferência.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct DevReport {
    /// A trilha.
    pub track: String,
    /// Cada exercício, na ordem.
    pub exercises: Vec<DevExercise>,
}

impl DevReport {
    /// Nenhum exercício com problema.
    pub fn ok(&self) -> bool {
        self.exercises.iter().all(|e| e.problems.is_empty())
    }
}

/// Um exercício conferido.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct DevExercise {
    /// O nome.
    pub name: String,
    /// O que está errado; vazio quando está tudo certo.
    pub problems: Vec<String>,
    /// Como o `start.v` se saiu (não pode ser `solved`).
    pub start: Option<Verdict>,
    /// Como a `solution.v` se saiu (tem que ser `solved`).
    pub solution: Option<Verdict>,
}

/// Confere a trilha numa pasta de exercícios criada em `scratch` (vazia ou
/// inexistente), chamando `progress` a cada exercício conferido.
///
/// # Erros
///
/// Os de criar a pasta e os de rodar as ferramentas; um exercício errado
/// não é erro, é um problema no relatório.
pub fn check_track(
    toolchain: &lace_core::Toolchain,
    track: &Track,
    scratch: &Utf8Path,
    control: &Control,
    mut progress: impl FnMut(&DevExercise),
) -> Result<DevReport> {
    // Primeiro o que não precisa simular; quem falha aqui não entra na
    // pasta de exercícios.
    let mut entries: Vec<DevExercise> = track
        .exercises()
        .map(|e| DevExercise {
            name: e.name.clone(),
            problems: static_problems(e),
            start: None,
            solution: None,
        })
        .collect();
    let mut runnable = track.clone();
    for chapter in &mut runnable.chapters {
        chapter.exercises.retain(|e| {
            entries
                .iter()
                .any(|d| d.name == e.name && d.problems.is_empty())
        });
    }
    runnable.chapters.retain(|c| !c.exercises.is_empty());

    if runnable.chapters.is_empty() {
        for entry in &entries {
            progress(entry);
        }
        return Ok(DevReport {
            track: track.id.clone(),
            exercises: entries,
        });
    }
    let workspace = Workspace::init(scratch, runnable)?;
    for entry in &mut entries {
        if entry.problems.is_empty() {
            let exercise = workspace.track().require(&entry.name)?.clone();
            let start = grade(toolchain, &workspace, &exercise, control)?;
            entry.start = Some(start.verdict);
            if start.solved() {
                entry
                    .problems
                    .push("start.v already passes the testbench".into());
            }
            let student = workspace.student_file(&exercise);
            let solution = std::fs::read_to_string(exercise.solution_file()).map_err(
                crate::error::LearnError::io("Reading", &exercise.solution_file()),
            )?;
            std::fs::write(&student, solution)
                .map_err(crate::error::LearnError::io("Writing", &student))?;
            let checked = grade(toolchain, &workspace, &exercise, control)?;
            entry.solution = Some(checked.verdict);
            if !checked.solved() {
                entry
                    .problems
                    .push(format!("solution.v does not pass: {}", explain(&checked)));
            }
            workspace.reset(&exercise)?;
        }
        progress(entry);
    }
    Ok(DevReport {
        track: track.id.clone(),
        exercises: entries,
    })
}

/// O que se confere sem simular.
fn static_problems(exercise: &Exercise) -> Vec<String> {
    let mut problems = Vec::new();
    if exercise.hints.is_empty() {
        problems.push("prompt.md has no hint (## Dica)".to_owned());
    }
    let solution = match exercise.interface() {
        Ok(interface) => Some(interface),
        Err(e) => {
            problems.push(e.to_string());
            None
        }
    };
    match interface_in(&exercise.start_file(), &exercise.module) {
        Ok(start) => {
            if let Some(solution) = &solution
                && start.ports != solution.ports
            {
                problems.push("start.v and solution.v declare different ports".to_owned());
            }
        }
        Err(e) => problems.push(e.to_string()),
    }
    if let Some(solution) = &solution
        && !exercise.custom_testbench
        && let Err(e) = testbench::generate(exercise, solution)
    {
        problems.push(e.to_string());
    }
    problems
}

/// Por que a solução não passou, numa linha.
fn explain(grade: &Grade) -> String {
    if let Some(d) = grade.diagnostics.first() {
        let place = match (&d.file, d.line) {
            (Some(file), Some(line)) => {
                format!("{}:{line}: ", file.file_name().unwrap_or_default())
            }
            _ => String::new(),
        };
        return format!("{:?}, {place}{}", grade.verdict, d.message);
    }
    let wrong: Vec<String> = grade
        .outputs
        .iter()
        .filter(|o| o.mismatches > 0)
        .map(|o| format!("{} wrong in {} samples", o.name, o.mismatches))
        .collect();
    if wrong.is_empty() {
        format!("{:?}", grade.verdict)
    } else {
        format!("{:?}, {}", grade.verdict, wrong.join(", "))
    }
}
