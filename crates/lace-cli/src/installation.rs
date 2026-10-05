//! A instalação de onde este `lace` roda: a pasta que o instalador criou,
//! com `bin/lace` (ou `bin\lace.exe`), `toolchain/` e o que o instalador
//! deixa para desinstalar. É a base de `lace install` e `lace uninstall`.

use anyhow::{Context, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;

/// O recibo que o instalador de Linux e macOS grava na pasta
/// (`RECEIPT_FILE` em `crates/lace-installer/src/install.rs`).
pub const RECEIPT_FILE: &str = "install.json";
/// O desinstalador de Linux e macOS (`UNINSTALL_SCRIPT` no mesmo arquivo).
pub const UNINSTALL_SCRIPT: &str = "uninstall.sh";

/// O que o recibo diz da instalação (`Receipt` em `lace-installer`).
#[derive(Debug, Deserialize)]
pub struct Receipt {
    /// A plataforma (`linux-x64`, `darwin-arm64`).
    pub platform: String,
    /// Os componentes instalados.
    pub components: Vec<String>,
    /// O atalho `lace` que o instalador criou, se criou.
    #[serde(default)]
    pub link: Option<Utf8PathBuf>,
}

impl Receipt {
    /// O recibo da instalação em `prefix`.
    pub fn load(prefix: &Utf8Path) -> anyhow::Result<Receipt> {
        let path = prefix.join(RECEIPT_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| format!("Reading {path}"))?;
        serde_json::from_str(&text).with_context(|| format!("Invalid {path}"))
    }
}

/// A pasta da instalação: a de cima de `bin/`, onde está este executável.
/// Recusa um `lace` que não foi instalado pelo instalador (um build em
/// `target/`, por exemplo), em vez de mexer no que estiver em volta.
pub fn prefix() -> anyhow::Result<Utf8PathBuf> {
    let exe = std::env::current_exe().context("Could not find the lace executable")?;
    let exe = dunce::canonicalize(&exe).unwrap_or(exe);
    let exe = Utf8PathBuf::from_path_buf(exe)
        .map_err(|p| anyhow::anyhow!("Path is not UTF-8: {}", p.display()))?;
    let prefix = exe
        .parent()
        .filter(|bin| bin.file_name() == Some("bin"))
        .and_then(Utf8Path::parent)
        .map(Utf8Path::to_owned);
    let installed = prefix.as_ref().is_some_and(|p| {
        if cfg!(windows) {
            uninstaller_exe(p).is_some()
        } else {
            p.join(RECEIPT_FILE).is_file() && p.join(UNINSTALL_SCRIPT).is_file()
        }
    });
    match prefix {
        Some(prefix) if installed => Ok(prefix),
        _ => bail!(
            "This lace ({exe}) was not installed by the installer, so there is no installation here"
        ),
    }
}

/// O desinstalador do Inno Setup na pasta: `unins000.exe` (o número sobe se
/// houver mais de um).
pub fn uninstaller_exe(prefix: &Utf8Path) -> Option<Utf8PathBuf> {
    let mut found: Vec<Utf8PathBuf> = prefix
        .read_dir_utf8()
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path().to_owned())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.starts_with("unins") && n.ends_with(".exe"))
        })
        .collect();
    found.sort();
    found.pop()
}
