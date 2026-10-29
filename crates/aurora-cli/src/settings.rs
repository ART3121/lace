//! De onde vem o bundle e o compilador do sistema.
//!
//! O bundle é o instalado ao lado do executável (`<instalação>/toolchain`).
//! `--toolchain <DIR>` (ou `AURORA_TOOLCHAIN`) aponta para outro bundle, para
//! desenvolvimento e testes; continua precisando ser um bundle, com
//! `bundle.json`.
//!
//! O compilador que o Verilator usa vem do bundle no Windows (com o
//! componente verilator) e, no Linux e no macOS, do sistema, nos locais
//! padrão que o Core procura (`SystemCompiler::detect`). Fora deles,
//! `--compiler <DIR>` (ou `AURORA_COMPILER`) diz onde ele está: no Linux e no
//! macOS, o diretório com `perl`, `make` e `g++` ou `clang++`. No Windows a
//! opção troca o do bundle pelo de um MSYS2 (a raiz dele), para
//! desenvolvimento. Quem precisa sempre põe a variável no perfil do shell.
//! Não há arquivo de configuração.
//!
//! O Quartus Prime, que compila para as placas Intel, também vem do sistema:
//! `--quartus <DIR>` (ou `AURORA_QUARTUS`), senão a `QUARTUS_ROOTDIR` que o
//! instalador da Intel cria, senão as pastas padrão do instalador
//! (`Quartus::detect`).

use anyhow::Context;
use camino::{Utf8Path, Utf8PathBuf};
use aurora_core::Toolchain;
use aurora_core::locate::{self, Found, Lookup};

/// Opções globais de bundle.
#[derive(clap::Args, Debug, Clone)]
pub struct ToolchainArgs {
    /// Outro bundle no lugar do instalado, para desenvolvimento; fora da ajuda.
    #[arg(
        long,
        global = true,
        hide = true,
        env = "AURORA_TOOLCHAIN",
        value_name = "DIR"
    )]
    pub toolchain: Option<Utf8PathBuf>,
    /// Dir with perl, make and g++/clang++ for Verilator (MSYS2 root on Windows) [env: AURORA_COMPILER]
    #[arg(
        long,
        global = true,
        help_heading = "Global options",
        env = "AURORA_COMPILER",
        hide_env = true,
        value_name = "DIR"
    )]
    pub compiler: Option<Utf8PathBuf>,
    /// Quartus Prime install for Intel FPGA boards (default: QUARTUS_ROOTDIR, then the installer folders) [env: AURORA_QUARTUS]
    #[arg(
        long,
        global = true,
        help_heading = "Global options",
        env = "AURORA_QUARTUS",
        hide_env = true,
        value_name = "DIR"
    )]
    pub quartus: Option<Utf8PathBuf>,
}

/// A variável que o instalador do Quartus cria com a pasta `quartus` dele.
const QUARTUS_ROOTDIR: &str = "QUARTUS_ROOTDIR";

impl ToolchainArgs {
    /// O bundle, com o compilador do sistema declarado em `--compiler`, se
    /// houver. Um compilador declarado que não está lá é erro, e não volta
    /// em silêncio aos locais padrão.
    pub fn resolve(&self) -> anyhow::Result<Toolchain> {
        Ok(self.found()?.toolchain)
    }

    /// O bundle achado, com a origem, como o [`resolve`](Self::resolve) o
    /// acha: o que o `aurora tools` mostra ([`locate::describe`]).
    pub fn found(&self) -> anyhow::Result<Found> {
        let lookup = self.lookup()?;
        let found = locate::resolve(&lookup)?;
        // Na CLI, um compilador ou um Quartus declarado que não está lá é
        // erro, e não volta em silêncio aos locais padrão.
        if found.compiler_error.is_some() {
            let dir = lookup.compiler.unwrap_or_default();
            anyhow::bail!(
                "The compiler at {dir} (from --compiler or AURORA_COMPILER) is missing perl, make or a C++ compiler{}",
                if cfg!(windows) {
                    " (expected: an MSYS2 root with usr/bin and ucrt64/bin or mingw64/bin)"
                } else {
                    ""
                }
            );
        }
        if found.quartus_error.is_some() {
            let dir = lookup.quartus.unwrap_or_default();
            anyhow::bail!(
                "No Quartus Prime at {dir} (from --quartus or AURORA_QUARTUS): quartus_sh is not there"
            );
        }
        Ok(found)
    }

    /// Onde procurar, do jeito da CLI: o `--toolchain` (ou `AURORA_TOOLCHAIN`),
    /// senão o bundle ao lado do executável; o compilador e o Quartus das
    /// opções, a `QUARTUS_ROOTDIR` e a `HOME` do ambiente.
    pub fn lookup(&self) -> anyhow::Result<Lookup> {
        let exe = std::env::current_exe().context("Could not find the aurora executable")?;
        let exe = Utf8PathBuf::from_path_buf(exe)
            .map_err(|p| anyhow::anyhow!("Path is not UTF-8: {}", p.display()))?;
        let compiler = self.compiler.as_deref().map(absolute).transpose()?;
        let quartus = self.quartus.as_deref().map(absolute).transpose()?;
        Ok(Lookup {
            settings: self.toolchain.clone(),
            exe: Some(exe),
            compiler,
            quartus,
            quartus_rootdir: std::env::var_os(QUARTUS_ROOTDIR)
                .filter(|d| !d.is_empty())
                .and_then(|d| Utf8PathBuf::from_path_buf(d.into()).ok()),
            home: std::env::var_os("HOME")
                .filter(|h| !h.is_empty())
                .and_then(|h| Utf8PathBuf::from_path_buf(h.into()).ok()),
            ..Lookup::default()
        })
    }
}

/// Caminho absoluto, a partir do diretório atual, sem exigir que exista.
pub fn absolute(path: &Utf8Path) -> anyhow::Result<Utf8PathBuf> {
    let absolute = std::path::absolute(path).with_context(|| format!("Resolving {path}"))?;
    Utf8PathBuf::from_path_buf(absolute)
        .map_err(|p| anyhow::anyhow!("Path is not UTF-8: {}", p.display()))
}
