//! De onde vem o bundle e o compilador do sistema.
//!
//! O bundle é o instalado ao lado do executável (`<instalação>/toolchain`).
//! `--toolchain <DIR>` (ou `LACE_TOOLCHAIN`) aponta para outro bundle, para
//! desenvolvimento e testes; continua precisando ser um bundle, com
//! `bundle.json`.
//!
//! O compilador que o Verilator usa vem do bundle no Windows (com o
//! componente verilator) e, no Linux e no macOS, do sistema, nos locais
//! padrão que o Core procura (`SystemCompiler::detect`). Fora deles,
//! `--compiler <DIR>` (ou `LACE_COMPILER`) diz onde ele está: no Linux e no
//! macOS, o diretório com `perl`, `make` e `g++` ou `clang++`. No Windows a
//! opção troca o do bundle pelo de um MSYS2 (a raiz dele), para
//! desenvolvimento. Quem precisa sempre põe a variável no perfil do shell.
//! Não há arquivo de configuração.

use anyhow::Context;
use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{SystemCompiler, Toolchain};

/// Opções globais de bundle.
#[derive(clap::Args, Debug, Clone)]
pub struct ToolchainArgs {
    /// Outro bundle no lugar do instalado, para desenvolvimento; fora da ajuda.
    #[arg(
        long,
        global = true,
        hide = true,
        env = "LACE_TOOLCHAIN",
        value_name = "DIR"
    )]
    pub toolchain: Option<Utf8PathBuf>,
    /// Dir with perl, make and g++/clang++ for Verilator (MSYS2 root on Windows) [env: LACE_COMPILER]
    #[arg(
        long,
        global = true,
        help_heading = "Global options",
        env = "LACE_COMPILER",
        hide_env = true,
        value_name = "DIR"
    )]
    pub compiler: Option<Utf8PathBuf>,
}

/// O compilador num diretório declarado: raiz do MSYS2 no Windows, diretório
/// com os três programas nos outros sistemas.
pub fn compiler_in(dir: &Utf8Path) -> Option<SystemCompiler> {
    if cfg!(windows) {
        SystemCompiler::in_msys2(dir)
    } else {
        SystemCompiler::in_dir(dir.to_owned())
    }
}

impl ToolchainArgs {
    /// O bundle, com o compilador do sistema declarado em `--compiler`, se
    /// houver. Um compilador declarado que não está lá é erro, e não volta
    /// em silêncio aos locais padrão.
    pub fn resolve(&self) -> anyhow::Result<Toolchain> {
        let toolchain = match &self.toolchain {
            Some(dir) => Toolchain::open(dir)?,
            None => {
                let exe = std::env::current_exe().context("Could not find the lace executable")?;
                let exe = Utf8PathBuf::from_path_buf(exe)
                    .map_err(|p| anyhow::anyhow!("Path is not UTF-8: {}", p.display()))?;
                Toolchain::locate(&exe)?
            }
        };
        let Some(dir) = &self.compiler else {
            return Ok(toolchain);
        };
        let dir = absolute(dir)?;
        let Some(found) = compiler_in(&dir) else {
            anyhow::bail!(
                "The compiler at {dir} (from --compiler or LACE_COMPILER) is missing perl, make or a C++ compiler{}",
                if cfg!(windows) {
                    " (expected: an MSYS2 root with usr/bin and ucrt64/bin or mingw64/bin)"
                } else {
                    ""
                }
            );
        };
        Ok(toolchain.with_system_compiler(Some(found)))
    }
}

/// Caminho absoluto, a partir do diretório atual, sem exigir que exista.
pub fn absolute(path: &Utf8Path) -> anyhow::Result<Utf8PathBuf> {
    let absolute = std::path::absolute(path).with_context(|| format!("Resolving {path}"))?;
    Utf8PathBuf::from_path_buf(absolute)
        .map_err(|p| anyhow::anyhow!("Path is not UTF-8: {}", p.display()))
}
