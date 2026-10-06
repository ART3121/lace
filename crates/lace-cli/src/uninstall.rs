//! `lace uninstall`: remove a instalação de onde este `lace` roda, com o
//! bundle.
//!
//! Quem sabe remover é o desinstalador que o instalador deixou na pasta, e
//! este comando só o chama:
//!
//! - Linux e macOS: `<instalação>/uninstall.sh`, gerado por
//!   `lace-installer` (`write_uninstaller`, em
//!   `crates/lace-installer/src/install.rs`). Ele apaga `toolchain/`,
//!   `bin/lace`, o `install.json`, o atalho e a pasta, se ficar vazia.
//! - Windows: o `unins000.exe` do Inno Setup, que também tira a pasta do
//!   `PATH`. Ele roda depois que o `lace` sai, porque o Windows não apaga um
//!   executável em uso.
//!
//! Também sai o arquivo de configuração da versão 0.1.0, que nenhuma versão
//! nova lê. Ficam os projetos (e o `.lace/` de cada um) e o que o
//! surfer-aurora guarda no perfil do usuário, que o Surfer e a AURORA
//! compartilham.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::{Context, bail};
use camino::{Utf8Path, Utf8PathBuf};

use crate::installation;
use crate::output::Output;
use crate::report::UninstallReport;

pub fn run(out: &Output, yes: bool) -> anyhow::Result<()> {
    let prefix = installation::prefix()?;
    if !yes {
        confirm(&prefix)?;
    }
    let removed = remove(&prefix, yes, out.is_text())?;
    remove_legacy_config();
    out.json(&UninstallReport {
        prefix: prefix.clone(),
        removed,
    })?;
    if out.is_text() && !removed {
        println!("Opened the Windows uninstaller for {prefix}");
    }
    Ok(())
}

/// Pergunta no terminal. Sem terminal (um script, uma extensão), exige
/// `--yes`: apagar não acontece por engano.
fn confirm(prefix: &Utf8Path) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("No terminal to confirm: use lace uninstall --yes");
    }
    eprint!("Remove Lace and its bundle from {prefix}? [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        bail!("Nothing was removed")
    }
}

/// Chama o desinstalador. `true` se a instalação já saiu; `false` no
/// Windows, onde o desinstalador termina depois que o `lace` sai.
///
/// Com `--json`, o que o script escreve vai para o stderr: o stdout é só do
/// objeto JSON.
#[cfg(not(windows))]
fn remove(prefix: &Utf8Path, _yes: bool, text: bool) -> anyhow::Result<bool> {
    let script = prefix.join(installation::UNINSTALL_SCRIPT);
    let stdout = if text {
        std::process::Stdio::inherit()
    } else {
        std::process::Stdio::from(std::io::stderr())
    };
    let status = std::process::Command::new("/bin/sh")
        .arg(script.as_std_path())
        .stdout(stdout)
        .status()
        .with_context(|| format!("Running {script}"))?;
    if !status.success() {
        bail!("{script} failed; for a system installation (/opt/lace), run: sudo lace uninstall");
    }
    Ok(true)
}

#[cfg(windows)]
fn remove(prefix: &Utf8Path, yes: bool, _text: bool) -> anyhow::Result<bool> {
    let exe = installation::uninstaller_exe(prefix)
        .context("The Lace uninstaller is not in the folder")?;
    let mut command = std::process::Command::new(exe.as_std_path());
    if yes {
        command.arg("/SILENT");
    }
    installation::open_wizard(&mut command).with_context(|| format!("Opening {exe}"))?;
    Ok(false)
}

/// O arquivo de configuração de `lace config`, que saiu na 0.2.0. Se ficou
/// de uma instalação antiga, sai também, com a pasta se ela ficar vazia.
fn remove_legacy_config() {
    let base = if cfg!(windows) {
        std::env::var("APPDATA").ok().map(Utf8PathBuf::from)
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|v| !v.is_empty())
            .map(Utf8PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| Utf8PathBuf::from(h).join(".config"))
            })
    };
    let Some(dir) = base.map(|b| b.join("lace")) else {
        return;
    };
    let _ = std::fs::remove_file(dir.join("config.json"));
    let _ = std::fs::remove_dir(&dir);
}
