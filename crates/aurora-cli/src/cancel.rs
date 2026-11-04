//! O Ctrl+C nas perguntas dos comandos que mexem na instalação (atualizar,
//! desinstalar): o pedido de cancelamento de `main` fica guardado aqui, e
//! as perguntas conferem antes de seguir. Dentro da biblioteca do
//! instalador o mesmo token vai por parâmetro, e o cancelamento dela volta
//! como [`Cancelled`] (`installation::lib_error`): a frase do Ctrl+C e a
//! saída 130.

use std::sync::OnceLock;

use aurora_core::CancelToken;

static TOKEN: OnceLock<CancelToken> = OnceLock::new();

/// Guarda o pedido de cancelamento do processo, uma vez, em `main`.
pub fn set(token: CancelToken) {
    let _ = TOKEN.set(token);
}

/// O Ctrl+C já veio.
fn requested() -> bool {
    TOKEN.get().is_some_and(CancelToken::is_cancelled)
}

/// O comando parou no Ctrl+C, antes de mudar a instalação.
#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cancelled with Ctrl+C; nothing was changed")
    }
}

impl std::error::Error for Cancelled {}

/// `Err(Cancelled)` se o Ctrl+C já veio.
pub fn check() -> anyhow::Result<()> {
    if requested() {
        Err(Cancelled.into())
    } else {
        Ok(())
    }
}

/// O erro é o cancelamento.
pub fn is(error: &anyhow::Error) -> bool {
    error.chain().any(|e| e.is::<Cancelled>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cancellation_is_found_under_a_context() {
        let error = anyhow::Error::new(Cancelled).context("Downloading the index");
        assert!(is(&error));
        assert!(!is(&anyhow::anyhow!("Could not download")));
    }
}
