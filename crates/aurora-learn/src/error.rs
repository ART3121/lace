//! Os erros do aurora-learn.

use aurora_core::AuroraError;
use camino::{Utf8Path, Utf8PathBuf};

/// O que pode dar errado ao carregar uma trilha, abrir a pasta de
/// exercícios ou corrigir um exercício. As mensagens são em inglês, como as
/// da AURORA; o [`code`](LearnError::code) é o que as interfaces traduzem.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LearnError {
    /// Um erro do aurora-core: ferramenta, projeto, I/O de uma operação.
    #[error(transparent)]
    Aurora(#[from] AuroraError),

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

    /// Sem o componente `aurora-learn` no bundle e sem `AURORA_LEARN_DIR`.
    #[error("The aurora-learn component is not installed")]
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
    /// exercícios conserta com o `aurora learn dev check`.
    #[error("Invalid track file {path}: {reason}")]
    InvalidTrack {
        /// O arquivo ou a pasta.
        path: Utf8PathBuf,
        /// O que está errado.
        reason: String,
    },

    /// Nem `start` nem nenhuma pasta acima dele é uma pasta de exercícios.
    #[error("No aurora learn workspace in {0} or in a folder above it")]
    NoWorkspace(Utf8PathBuf),

    /// A pasta pedida para os exercícios já existe e não está vazia.
    #[error("{0} already exists and is not empty")]
    WorkspaceExists(Utf8PathBuf),

    /// O estado da pasta de exercícios (`.aurora-learn.json`) não se entende.
    #[error("Invalid workspace state {path}: {reason}")]
    InvalidState {
        /// O `.aurora-learn.json`.
        path: Utf8PathBuf,
        /// O que está errado.
        reason: String,
    },

    /// A trilha não tem exercício com esse nome.
    #[error("No exercise named '{0}'")]
    UnknownExercise(String),

    /// O exercício ainda não foi corrigido: não há onda para abrir. O texto
    /// não diz como corrigir; cada interface tem o seu caminho (`aurora learn
    /// check`, o botão do Studio).
    #[error("Exercise {exercise} has no waveform yet; check it first")]
    NoWaveform {
        /// O exercício.
        exercise: String,
        /// Onde a onda ficaria.
        path: Utf8PathBuf,
    },
}

impl LearnError {
    /// O código estável do erro, em `snake_case`, para o JSON e para as
    /// interfaces traduzirem. Um erro do aurora-core leva o código dele.
    pub fn code(&self) -> &'static str {
        match self {
            LearnError::Aurora(error) => error.code(),
            LearnError::Io { .. } => "io",
            LearnError::ComponentMissing => "learn_component_missing",
            LearnError::TrackNotFound { .. } => "track_not_found",
            LearnError::InvalidTrack { .. } => "invalid_track",
            LearnError::NoWorkspace(_) => "no_learn_workspace",
            LearnError::WorkspaceExists(_) => "learn_workspace_exists",
            LearnError::InvalidState { .. } => "invalid_learn_state",
            LearnError::UnknownExercise(_) => "unknown_exercise",
            LearnError::NoWaveform { .. } => "no_waveform",
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

/// O `Result` do aurora-learn.
pub type Result<T> = std::result::Result<T, LearnError>;
