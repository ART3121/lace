//! O assistente do Lace: uma sessão do Claude Code, rodando como processo
//! filho, com as ferramentas do `aurora mcp`. É o motor do chat do Lace
//! Studio e do `aurora chat` no terminal.
//!
//! O Claude Code já faz o laço do modelo com as ferramentas, a autenticação
//! (o login da conta, ou `ANTHROPIC_API_KEY` no ambiente) e a memória das
//! sessões. Esta biblioteca só o acha ([`locate`]), o abre no modo
//! `--print` com entrada e saída em JSON por linha (`stream-json`), com o
//! `aurora mcp` como servidor de ferramentas, e traduz o que ele escreve em
//! [`AgentEvent`]s para quem mostra. Nada aqui chama a API da Anthropic por
//! conta própria, nem guarda chave.
//!
//! Sem o Claude Code na máquina, [`locate::find_claude`] devolve `None`, e
//! quem mostra diz como instalar.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod locate;
pub mod session;

pub use locate::{find_claude, version};
pub use session::{AgentEvent, Permission, Session, SessionOptions};

/// Um erro do assistente.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AgentError {
    /// O Claude Code não está na máquina (ou a pasta declarada não o tem).
    #[error(
        "Claude Code was not found; install it (https://claude.com/claude-code) or set its path"
    )]
    ClaudeNotFound,
    /// O processo não abriu.
    #[error("Could not run {program}: {reason}")]
    Spawn {
        /// O executável.
        program: String,
        /// O erro do sistema.
        reason: String,
    },
    /// A sessão já terminou: o processo saiu.
    #[error("The assistant session has ended")]
    Ended,
    /// Escrever para o processo falhou.
    #[error("Could not write to Claude Code: {0}")]
    Io(#[from] std::io::Error),
    /// Qualquer outro erro, com a mensagem.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AgentError {
    /// O código estável, em `snake_case`.
    pub fn code(&self) -> &'static str {
        match self {
            AgentError::ClaudeNotFound => "claude_not_found",
            AgentError::Spawn { .. } => "spawn",
            AgentError::Ended => "ended",
            AgentError::Io(_) => "io",
            AgentError::Other(_) => "assistant",
        }
    }
}

/// O resultado do assistente.
pub type Result<T> = std::result::Result<T, AgentError>;
