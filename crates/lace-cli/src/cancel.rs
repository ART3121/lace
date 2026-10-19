//! O Ctrl+C nos comandos que não passam pelo Core: instalar, atualizar,
//! desinstalar e o `setup` do assistente. O pedido de cancelamento de `main`
//! fica guardado aqui, e esses comandos conferem entre um passo e outro,
//! antes de mudar a instalação. O download que o Ctrl+C interrompe (o `curl`
//! recebe o sinal junto) vira cancelamento, e não um erro que leva a outro
//! caminho, como o instalador inteiro no lugar da atualização por partes.

use std::sync::OnceLock;

use lace_core::CancelToken;

static TOKEN: OnceLock<CancelToken> = OnceLock::new();

/// Guarda o pedido de cancelamento do processo, uma vez, em `main`.
pub fn set(token: CancelToken) {
    let _ = TOKEN.set(token);
}

/// O Ctrl+C já veio.
pub fn requested() -> bool {
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

/// `error`, ou o cancelamento se o Ctrl+C veio antes: o que ele interrompeu
/// falhou por causa dele.
pub fn or_cancelled(error: anyhow::Error) -> anyhow::Error {
    if requested() { Cancelled.into() } else { error }
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
