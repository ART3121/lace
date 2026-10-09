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
//!
//! O Quartus Prime, que compila para as placas Intel, também vem do sistema:
//! `--quartus <DIR>` (ou `LACE_QUARTUS`), senão a `QUARTUS_ROOTDIR` que o
//! instalador da Intel cria, senão as pastas padrão do instalador
//! (`Quartus::detect`).

use anyhow::Context;
use camino::{Utf8Path, Utf8PathBuf};
use lace_core::fpga::Quartus;
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
    /// Quartus Prime install for Intel FPGA boards (default: QUARTUS_ROOTDIR, then the installer folders) [env: LACE_QUARTUS]
    #[arg(
        long,
        global = true,
        help_heading = "Global options",
        env = "LACE_QUARTUS",
        hide_env = true,
        value_name = "DIR"
    )]
    pub quartus: Option<Utf8PathBuf>,
}

/// A variável que o instalador do Quartus cria com a pasta `quartus` dele.
const QUARTUS_ROOTDIR: &str = "QUARTUS_ROOTDIR";

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
        let toolchain = toolchain.with_quartus(self.quartus()?);
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

impl ToolchainArgs {
    /// O Quartus Prime: o de `--quartus` (ou `LACE_QUARTUS`), que é erro se
    /// não estiver lá; senão o de `QUARTUS_ROOTDIR`, que o instalador da Intel
    /// cria; senão o das pastas padrão do instalador. `None` se nenhum.
    pub fn quartus(&self) -> anyhow::Result<Option<Quartus>> {
        if let Some(dir) = &self.quartus {
            let dir = absolute(dir)?;
            let Some(found) = Quartus::in_dir(&dir) else {
                anyhow::bail!(
                    "No Quartus Prime at {dir} (from --quartus or LACE_QUARTUS): quartus_sh is not there"
                );
            };
            return Ok(Some(found));
        }
        if let Some(dir) = std::env::var_os(QUARTUS_ROOTDIR).filter(|d| !d.is_empty())
            && let Ok(dir) = Utf8PathBuf::from_path_buf(dir.into())
            && let Some(found) = Quartus::in_dir(&dir)
        {
            return Ok(Some(found));
        }
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .and_then(|h| Utf8PathBuf::from_path_buf(h.into()).ok());
        Ok(Quartus::detect(home.as_deref()))
    }
}

/// Caminho absoluto, a partir do diretório atual, sem exigir que exista.
pub fn absolute(path: &Utf8Path) -> anyhow::Result<Utf8PathBuf> {
    let absolute = std::path::absolute(path).with_context(|| format!("Resolving {path}"))?;
    Utf8PathBuf::from_path_buf(absolute)
        .map_err(|p| anyhow::anyhow!("Path is not UTF-8: {}", p.display()))
}
