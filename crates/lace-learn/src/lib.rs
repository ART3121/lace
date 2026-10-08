//! # lace-learn
//!
//! Exercícios de Verilog no estilo do rustlings, corrigidos pelo
//! [`lace_core`]. É a API que o `lace learn` e a vista Exercícios do Lace
//! Studio usam; as trilhas (os exercícios em si) vêm do componente
//! `lace-learn` do bundle, ou da pasta de [`TRACKS_ENV`].
//!
//! Cada exercício vira um projeto Lace na pasta de exercícios do aluno
//! ([`Workspace`]): o módulo que ele escreve, e em `.lace-learn/` um
//! testbench que aplica o mesmo estímulo a ele e a uma referência (a
//! solução, com os módulos renomeados) e compara as saídas a cada amostra.
//! A correção ([`grade`](fn@grade)) roda o `check` e a simulação do
//! lace-core e lê o resumo que o testbench escreve.
//!
//! ```no_run
//! use lace_core::{Control, Toolchain};
//! use lace_learn::{Workspace, grade, load_track, tracks_dir};
//!
//! let toolchain = Toolchain::open("/opt/lace/toolchain")?;
//! let tracks = tracks_dir(Some(&toolchain))?;
//! let track = load_track(&tracks, lace_learn::DEFAULT_TRACK, None)?;
//! let mut workspace = Workspace::init(camino::Utf8Path::new("lace-learn"), track)?;
//!
//! let exercise = workspace.current().clone();
//! let result = grade(&toolchain, &workspace, &exercise, &Control::new())?;
//! if result.solved() {
//!     workspace.set_solved(&exercise.name, true)?;
//! }
//! # Ok::<(), lace_learn::LearnError>(())
//! ```

#![deny(clippy::print_stdout, clippy::print_stderr)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod dev;
mod error;
pub mod grade;
pub mod layout;
mod reference;
pub mod testbench;
mod text;
pub mod track;
pub mod workspace;

pub use dev::{DevExercise, DevReport, check_track};
pub use error::{LearnError, Result};
pub use grade::{Finding, Grade, OutputCheck, Verdict, grade};
pub use track::{
    Chapter, DEFAULT_TRACK, Exercise, Kind, Reset, Spec, Stimulus, TRACKS_ENV, Track,
    available_tracks, load_track, tracks_dir,
};
pub use workspace::{ExerciseStatus, Workspace};
