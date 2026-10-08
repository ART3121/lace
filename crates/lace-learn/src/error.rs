//! Os erros do lace-learn.

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::LaceError;

/// O que pode dar errado ao carregar uma trilha, abrir a pasta de
/// exercícios ou corrigir um exercício. As mensagens são em inglês, como as
/// do Lace; o [`code`](LearnError::code) é o que as interfaces traduzem.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LearnError {
    /// Um erro do lace-core: ferramenta, projeto, I/O de uma operação.
    #[error(transparent)]
    Lace(#[from] LaceError),

    /// Ler ou gravar um arquivo falhou.
    #[error("{action} {path}: {source}")]
    Io {
        /// O que se tentava fazer (`"Reading"`, `"Writing"`).
        action: &'static str,
        /// O arquivo ou a pasta.
        path: Utf8PathBuf,
        /// O erro do sistema.
        #[source]
        source: std::io::Error,
    },

    /// Sem o componente `lace-learn` no bundle e sem `LACE_LEARN_DIR`.
    #[error("The lace-learn component is not installed")]
    ComponentMissing,

    /// A pasta das trilhas não tem a trilha pedida.
    #[error("Track '{track}' not found in {dir}")]
    TrackNotFound {
        /// A trilha pedida (`verilog`).
        track: String,
        /// A pasta das trilhas.
        dir: Utf8PathBuf,
    },

    /// Um arquivo da trilha falta ou não se entende: quem escreve
    /// exercícios conserta com o `lace learn dev check`.
    #[error("Invalid track file {path}: {reason}")]
    InvalidTrack {
        /// O arquivo ou a pasta.
        path: Utf8PathBuf,
        /// O que está errado.
        reason: String,
    },

    /// Nem `start` nem nenhuma pasta acima dele é uma pasta de exercícios.
    #[error("No lace learn workspace in {0} or in a folder above it")]
    NoWorkspace(Utf8PathBuf),

    /// A pasta pedida para os exercícios já existe e não está vazia.
    #[error("{0} already exists and is not empty")]
    WorkspaceExists(Utf8PathBuf),

    /// O estado da pasta de exercícios (`.lace-learn.json`) não se entende.
    #[error("Invalid workspace state {path}: {reason}")]
    InvalidState {
        /// O `.lace-learn.json`.
        path: Utf8PathBuf,
        /// O que está errado.
        reason: String,
    },

    /// A trilha não tem exercício com esse nome.
    #[error("No exercise named '{0}'")]
    UnknownExercise(String),
}

impl LearnError {
    /// O código estável do erro, em `snake_case`, para o JSON e para as
    /// interfaces traduzirem. Um erro do lace-core leva o código dele.
    pub fn code(&self) -> &'static str {
        match self {
            LearnError::Lace(error) => error.code(),
            LearnError::Io { .. } => "io",
            LearnError::ComponentMissing => "learn_component_missing",
            LearnError::TrackNotFound { .. } => "track_not_found",
            LearnError::InvalidTrack { .. } => "invalid_track",
            LearnError::NoWorkspace(_) => "no_learn_workspace",
            LearnError::WorkspaceExists(_) => "learn_workspace_exists",
            LearnError::InvalidState { .. } => "invalid_learn_state",
            LearnError::UnknownExercise(_) => "unknown_exercise",
        }
    }

    /// Para `map_err`: o erro de I/O com o que se fazia e o caminho.
    pub(crate) fn io(action: &'static str, path: &Utf8Path) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.to_owned();
        move |source| LearnError::Io {
            action,
            path,
            source,
        }
    }

    /// Um arquivo da trilha inválido.
    pub(crate) fn track(path: &Utf8Path, reason: impl Into<String>) -> Self {
        LearnError::InvalidTrack {
            path: path.to_owned(),
            reason: reason.into(),
        }
    }
}

/// O `Result` do lace-learn.
pub type Result<T> = std::result::Result<T, LearnError>;
