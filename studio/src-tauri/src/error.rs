//! O erro que volta para a interface.
//!
//! Todo comando devolve `Result<T, IpcError>`. A interface recebe um objeto
//! `{ code, message }`: o `code` é estável e é por ele que a interface decide
//! a dica e a tradução; a `message` é o texto do Core (em inglês) ou do
//! próprio Studio, para quando não houver tradução.
//!
//! Os códigos do Core (`LaceError::code()`, ver `docs/API.md` da raiz, seção 8)
//! passam como estão. Os do Studio estão em [`codes`].

use serde::Serialize;

/// Os códigos de erro do próprio Studio. Os do Core vêm de
/// `LaceError::code()` e não se repetem aqui.
pub mod codes {
    /// Nenhum projeto aberto.
    pub const NO_PROJECT: &str = "no_project";
    /// Já há uma operação rodando.
    pub const BUSY: &str = "busy";
    /// Caminho fora da pasta do projeto numa operação que só vale dentro dela.
    pub const OUTSIDE_PROJECT: &str = "outside_project";
    /// O arquivo mudou no disco desde que foi aberto.
    pub const CONFLICT: &str = "conflict";
    /// O arquivo ou a pasta já existe.
    pub const EXISTS: &str = "exists";
    /// Argumento inválido vindo da interface.
    pub const INVALID_ARGUMENT: &str = "invalid_argument";
    /// A CLI `lace` não foi encontrada ou falhou.
    pub const CLI: &str = "cli";
    /// O terminal de shell não abriu ou não existe mais.
    pub const TERMINAL: &str = "terminal";
    /// Erro de leitura ou escrita do próprio Studio.
    pub const IO: &str = "io";
    /// O bundle não traz o cliente web do Surfer (a onda abre em janela).
    pub const SURFER_WEB_MISSING: &str = "surfer_web_missing";
    /// A onda é grande demais para uma aba (abre em janela).
    pub const WAVE_TOO_LARGE: &str = "wave_too_large";
    /// Falha interna (thread que morreu, JSON inválido).
    pub const INTERNAL: &str = "internal";
}

/// O erro de um comando, como a interface o recebe.
#[derive(Debug, Clone, Serialize)]
pub struct IpcError {
    /// Identificador estável: `LaceError::code()` ou um de [`codes`].
    pub code: String,
    /// O texto do erro, para mostrar quando a interface não tem tradução.
    pub message: String,
}

/// O resultado de todo comando.
pub type IpcResult<T> = Result<T, IpcError>;

impl IpcError {
    /// Um erro com código e mensagem.
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        IpcError {
            code: code.to_owned(),
            message: message.into(),
        }
    }

    /// Erro de E/S com o caminho que falhou.
    pub fn io(context: impl std::fmt::Display, error: std::io::Error) -> Self {
        IpcError::new(codes::IO, format!("{context}: {error}"))
    }
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for IpcError {}

impl From<lace_core::LaceError> for IpcError {
    fn from(error: lace_core::LaceError) -> Self {
        IpcError {
            code: error.code().to_owned(),
            message: error.to_string(),
        }
    }
}

impl From<lace_learn::LearnError> for IpcError {
    fn from(error: lace_learn::LearnError) -> Self {
        IpcError {
            code: error.code().to_owned(),
            message: error.to_string(),
        }
    }
}

impl From<std::io::Error> for IpcError {
    fn from(error: std::io::Error) -> Self {
        IpcError::new(codes::IO, error.to_string())
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(error: serde_json::Error) -> Self {
        IpcError::new(codes::INTERNAL, format!("JSON: {error}"))
    }
}

impl From<tauri::Error> for IpcError {
    fn from(error: tauri::Error) -> Self {
        IpcError::new(codes::INTERNAL, error.to_string())
    }
}
