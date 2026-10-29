//! `aurora uninstall`: remove a instalação de onde este `aurora` roda, com o
//! bundle, pelo desinstalador que o instalador deixou na pasta
//! (`aurora_installer::uninstall`). Aqui fica a pergunta, o JSON e a linha
//! do Windows, onde o desinstalador termina depois que o `aurora` sai.
//!
//! Também sai o arquivo de configuração da versão 0.1.0, que nenhuma versão
//! nova lê. Ficam os projetos (e o `.aurora8/` de cada um) e o que o
//! surfer-aurora guarda no perfil do usuário, que o Surfer e a AURORA
//! compartilham.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::bail;
use camino::{Utf8Path, Utf8PathBuf};
use aurora_installer::uninstall::{self, UninstallOptions};
use aurora_installer::update::ChildOutput;

use crate::installation;
use crate::output::Output;

pub fn run(out: &Output, yes: bool) -> anyhow::Result<()> {
    let prefix = installation::prefix()?;
    if !yes {
        confirm(&prefix)?;
    }
    // Um Ctrl+C na pergunta não deixa a remoção começar.
    crate::cancel::check()?;
    let report = uninstall::run(
        &prefix,
        &UninstallOptions {
            silent: yes,
            // Com `--json`, o que o script escreve vai para o stderr: o
            // stdout é só do objeto JSON.
            script_output: if out.is_text() {
                ChildOutput::Inherit
            } else {
                ChildOutput::ToStderr
            },
        },
    )
    .map_err(installation::lib_error)?;
    if let Some(dir) = legacy_config_dir() {
        uninstall::remove_legacy_config(&dir);
    }
    let removed = report.removed;
    out.json(&report)?;
    if out.is_text() && !removed {
        println!("Opened the Windows uninstaller for {prefix}");
    }
    Ok(())
}

/// Pergunta no terminal. Sem terminal (um script, uma extensão), exige
/// `--yes`: apagar não acontece por engano.
fn confirm(prefix: &Utf8Path) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("No terminal to confirm: use aurora uninstall --yes");
    }
    eprint!("Remove AURORA and its bundle from {prefix}? [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    crate::cancel::check()?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        bail!("Nothing was removed")
    }
}

/// A pasta `aurora` da configuração do usuário, onde a 0.1.0 gravava o
/// `config.json`: `APPDATA` no Windows, `XDG_CONFIG_HOME` ou `~/.config` nos
/// outros.
fn legacy_config_dir() -> Option<Utf8PathBuf> {
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
    base.map(|b| b.join("aurora"))
}
