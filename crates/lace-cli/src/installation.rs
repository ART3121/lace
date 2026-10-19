//! A instalação de onde este `lace` roda, pela biblioteca do instalador
//! (`lace_installer::installation`): a pasta que o instalador criou, com
//! `bin/lace`, `toolchain/` e o que ele deixa para desinstalar.

use anyhow::Context;
use camino::Utf8PathBuf;
use lace_installer::installation::prefix_of;

/// A pasta da instalação: a de cima de `bin/`, onde está este executável.
/// Erro para um `lace` que não foi instalado pelo instalador (um build em
/// `target/`, por exemplo).
pub fn prefix() -> anyhow::Result<Utf8PathBuf> {
    prefix_result().map_err(lib_error)
}

/// [`prefix`] com o erro da biblioteca, para quem o repassa a ela.
pub fn prefix_result() -> lace_installer::error::Result<Utf8PathBuf> {
    let exe = std::env::current_exe()
        .context("Could not find the lace executable")
        .map_err(lace_installer::Error::Other)?;
    let exe = Utf8PathBuf::from_path_buf(exe)
        .map_err(|p| anyhow::anyhow!("Path is not UTF-8: {}", p.display()))
        .map_err(lace_installer::Error::Other)?;
    prefix_of(&exe)
}

/// Um erro da biblioteca do instalador na CLI: o cancelamento vira o da
/// CLI (a frase do Ctrl+C e a saída 130); os outros seguem com o código
/// deles na cadeia, para o JSON.
pub fn lib_error(error: lace_installer::Error) -> anyhow::Error {
    if error.is_cancelled() {
        crate::cancel::Cancelled.into()
    } else {
        anyhow::Error::new(error)
    }
}
