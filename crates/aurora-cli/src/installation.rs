//! A instalação de onde este `aurora` roda, pela biblioteca do instalador
//! (`aurora_installer::installation`): a pasta que o instalador criou, com
//! `bin/aurora`, `toolchain/` e o que ele deixa para desinstalar.

use anyhow::Context;
use aurora_installer::installation::prefix_of;
use camino::Utf8PathBuf;

/// A pasta da instalação: a de cima de `bin/`, onde está este executável.
/// Erro para um `aurora` que não foi instalado pelo instalador (um build em
/// `target/`, por exemplo).
pub fn prefix() -> anyhow::Result<Utf8PathBuf> {
    prefix_result().map_err(lib_error)
}

/// [`prefix`] com o erro da biblioteca, para quem o repassa a ela.
pub fn prefix_result() -> aurora_installer::error::Result<Utf8PathBuf> {
    let exe = std::env::current_exe()
        .context("Could not find the aurora executable")
        .map_err(aurora_installer::Error::Other)?;
    let exe = Utf8PathBuf::from_path_buf(exe)
        .map_err(|p| anyhow::anyhow!("Path is not UTF-8: {}", p.display()))
        .map_err(aurora_installer::Error::Other)?;
    prefix_of(&exe)
}

/// Um erro da biblioteca do instalador na CLI: o cancelamento vira o da
/// CLI (a frase do Ctrl+C e a saída 130); os outros seguem com o código
/// deles na cadeia, para o JSON.
pub fn lib_error(error: aurora_installer::Error) -> anyhow::Error {
    if error.is_cancelled() {
        crate::cancel::Cancelled.into()
    } else {
        anyhow::Error::new(error)
    }
}
