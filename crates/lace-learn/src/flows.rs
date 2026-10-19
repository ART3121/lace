//! Os fluxos do `lace learn`: corrigir exercícios e abrir a onda da última
//! correção com o layout do exercício. É o que a CLI e o Studio faziam cada
//! um por si; aqui a regra existe uma vez. A tela de texto, a TUI do modo
//! watch e as confirmações ficam na CLI.

use lace_core::flows::{LayoutPolicy, WaveOpened, WaveRequest, WaveSource};
use lace_core::{Control, Toolchain};

use crate::layout::LAYOUT_FILE;
use crate::{Exercise, Grade, LearnError, Result, Verdict, Workspace, grade};

/// O resultado de uma rodada de correção.
#[derive(Debug, Clone)]
pub struct LearnCheckOutcome {
    /// A correção de cada exercício pedido, na ordem; termina no primeiro
    /// cancelado.
    pub results: Vec<Grade>,
    /// Quantos exercícios da trilha estão resolvidos depois da rodada.
    pub solved: usize,
    /// Quantos exercícios a trilha tem.
    pub total: usize,
}

impl LearnCheckOutcome {
    /// Todos os exercícios pedidos ficaram resolvidos.
    pub fn succeeded(&self) -> bool {
        self.results.iter().all(Grade::solved)
    }

    /// A rodada foi cancelada no meio.
    pub fn cancelled(&self) -> bool {
        self.results
            .last()
            .is_some_and(|g| g.verdict == Verdict::Cancelled)
    }
}

/// Corrige `exercises` na ordem, gravando cada resultado na pasta de
/// exercícios ([`Workspace::record`]: resolvido marca, cancelado não muda
/// nada, errado desmarca) e avisando `on_grade` depois de cada um. Uma
/// correção cancelada encerra a rodada.
///
/// O `control` é o que o chamador quiser: a CLI e o Studio passam um sem
/// receptor de eventos, porque o resumo que o testbench escreve é para a
/// correção ler, não para mostrar ao vivo.
///
/// # Erros
///
/// Os de [`grade()`] (ferramenta que falta, projeto que não abre) e de
/// [`Workspace::record`]. Código errado do aluno não é erro: é o [`Grade`].
pub fn check(
    toolchain: &Toolchain,
    workspace: &mut Workspace,
    exercises: &[Exercise],
    control: &Control,
    on_grade: &mut dyn FnMut(&Grade),
) -> Result<LearnCheckOutcome> {
    let mut results = Vec::with_capacity(exercises.len());
    for exercise in exercises {
        let graded = grade(toolchain, workspace, exercise, control)?;
        workspace.record(&graded)?;
        on_grade(&graded);
        let cancelled = graded.verdict == Verdict::Cancelled;
        results.push(graded);
        if cancelled {
            break;
        }
    }
    Ok(LearnCheckOutcome {
        results,
        solved: workspace.solved_count(),
        total: workspace.track().len(),
    })
}

/// Abre no surfer-aurora a onda da última correção de `exercise`, com o
/// layout do exercício (`.lace-learn/wave.sucl`) quando ele existe, e
/// confere que o Surfer subiu ([`lace_core::flows::open_wave`]).
///
/// # Erros
///
/// [`LearnError::NoWaveform`] se o exercício ainda não foi corrigido; os de
/// abrir o Surfer.
pub fn open_wave(
    toolchain: &Toolchain,
    workspace: &Workspace,
    exercise: &Exercise,
) -> Result<WaveOpened> {
    let waveform = workspace.waveform(exercise);
    if !waveform.is_file() {
        return Err(LearnError::NoWaveform {
            exercise: exercise.name.clone(),
            path: waveform,
        });
    }
    let layout = workspace.exercise_dir(exercise).join(LAYOUT_FILE);
    let request = WaveRequest {
        source: WaveSource::File { path: waveform },
        layout: if layout.is_file() {
            LayoutPolicy::File { path: layout }
        } else {
            LayoutPolicy::None
        },
    };
    Ok(lace_core::flows::open_wave(toolchain, None, &request)?)
}
