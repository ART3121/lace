//! A instalação de onde um `aurora` roda: a pasta que o instalador criou, com
//! `bin/aurora` (ou `bin\aurora.exe`), `toolchain/` e o que o instalador deixa
//! para desinstalar. É a base de `aurora install`, `aurora update` e `aurora
//! uninstall`, e do Studio, que chega à pasta pelo bundle dele.

use anyhow::Context;
use camino::{Utf8Path, Utf8PathBuf};

use crate::error::{Error, Result};
pub use crate::install::{RECEIPT_FILE, Receipt, UNINSTALL_SCRIPT};

/// A pasta da instalação de onde `exe` roda: a de cima de `bin/`, se é uma
/// instalação do instalador ([`check`]). Recusa um `aurora` que não foi
/// instalado pelo instalador (um build em `target/`, por exemplo), em vez de
/// mexer no que estiver em volta.
pub fn prefix_of(exe: &Utf8Path) -> Result<Utf8PathBuf> {
    let canonical = dunce::canonicalize(exe.as_std_path())
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        .unwrap_or_else(|| exe.to_owned());
    let prefix = canonical
        .parent()
        .filter(|bin| bin.file_name() == Some("bin"))
        .and_then(Utf8Path::parent)
        .map(Utf8Path::to_owned);
    match prefix {
        Some(prefix) if is_installed(&prefix) => Ok(prefix),
        _ => Err(Error::NoInstallation { exe: canonical }),
    }
}

/// `prefix` é uma instalação do instalador: no Windows tem o desinstalador
/// do Inno Setup; nos outros, o recibo e o `uninstall.sh`. O Studio chega
/// aqui pela pasta do bundle dele (`toolchain/..`).
pub fn check(prefix: &Utf8Path) -> Result<()> {
    if is_installed(prefix) {
        Ok(())
    } else {
        Err(Error::NoInstallation {
            exe: prefix.to_owned(),
        })
    }
}

fn is_installed(prefix: &Utf8Path) -> bool {
    if cfg!(windows) {
        uninstaller_exe(prefix).is_some()
    } else {
        prefix.join(RECEIPT_FILE).is_file() && prefix.join(UNINSTALL_SCRIPT).is_file()
    }
}

/// O recibo da instalação em `prefix`, com erro quando não há.
pub fn receipt(prefix: &Utf8Path) -> Result<Receipt> {
    let path = prefix.join(RECEIPT_FILE);
    let text = std::fs::read_to_string(&path).with_context(|| format!("Reading {path}"))?;
    Ok(serde_json::from_str(&text).with_context(|| format!("Invalid {path}"))?)
}

/// Abre o assistente do Inno Setup (o instalador do `aurora update`, o
/// desinstalador do `aurora uninstall`), que continua aberto depois que quem
/// chamou sai.
///
/// - Sem stdin, stdout nem stderr: quem lê a saída de quem chamou até o
///   fim recebe o fim quando ele termina, e não quando o assistente fecha.
/// - Fora do Job Object de quem chamou (`CREATE_BREAKAWAY_FROM_JOB`): o
///   Studio prende a CLI num job que encerra o que sobra quando ela
///   termina. Se o job não deixa sair, o sistema recusa, e o assistente abre
///   dentro dele, como antes.
pub fn open_wizard(command: &mut std::process::Command) -> std::io::Result<()> {
    use std::process::Stdio;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // `CREATE_BREAKAWAY_FROM_JOB`, de `WinBase.h`.
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        command.creation_flags(CREATE_BREAKAWAY_FROM_JOB);
        match command.spawn() {
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                command.creation_flags(0);
            }
            other => return other.map(|_| ()),
        }
    }
    command.spawn().map(|_| ())
}

/// A chave da AURORA na lista de programas instalados do Windows, que o Inno
/// Setup cria com o `AppId` do `installer/windows/aurora.iss`.
#[cfg(windows)]
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{E34F1510-EC32-4839-8048-14CA03F9EFBB}_is1";

/// Depois de uma atualização por componentes, a versão que a lista de
/// programas instalados do Windows mostra (o nome e a versão que o assistente
/// gravou ao instalar). Só mexe na chave cuja pasta é `prefix`: a do usuário
/// (`HKCU`) ou a de todos (`HKLM`, que pede administrador). `reg` é o
/// `reg.exe` (o do `System32`; `None`, o do `PATH`). `false` quando nenhuma
/// chave é desta instalação; nos outros sistemas, sempre.
#[cfg(windows)]
pub fn register_version(prefix: &Utf8Path, version: &str, reg: Option<&Utf8Path>) -> Result<bool> {
    let reg = reg.map_or_else(|| "reg.exe".to_owned(), ToString::to_string);
    let want = prefix.as_str().trim_end_matches('\\').to_lowercase();
    for root in ["HKCU", "HKLM"] {
        let key = format!("{root}\\{UNINSTALL_KEY}");
        let mut query = std::process::Command::new(&reg);
        query.args(["query", &key, "/v", "InstallLocation"]);
        aurora_core::hide_console(&mut query);
        let output = query.output().with_context(|| format!("Running {reg}"))?;
        if !output.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let location = text
            .lines()
            .find_map(|l| l.split_once("REG_SZ").map(|(_, v)| v.trim().to_owned()));
        if location.is_none_or(|l| l.trim_end_matches('\\').to_lowercase() != want) {
            continue;
        }
        for (name, value) in [
            ("DisplayVersion", version.to_owned()),
            ("DisplayName", format!("AURORA {version}")),
        ] {
            let mut add = std::process::Command::new(&reg);
            add.args(["add", &key, "/v", name, "/t", "REG_SZ", "/d", &value, "/f"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            aurora_core::hide_console(&mut add);
            let status = add.status().with_context(|| format!("Running {reg}"))?;
            if !status.success() {
                return Err(anyhow::anyhow!("could not write {name} in {key}").into());
            }
        }
        return Ok(true);
    }
    Ok(false)
}

/// Fora do Windows não há lista de programas: nada a fazer.
#[cfg(not(windows))]
pub fn register_version(
    _prefix: &Utf8Path,
    _version: &str,
    _reg: Option<&Utf8Path>,
) -> Result<bool> {
    Ok(false)
}

/// O desinstalador do Inno Setup na pasta: `unins000.exe` (o número sobe se
/// houver mais de um).
pub fn uninstaller_exe(prefix: &Utf8Path) -> Option<Utf8PathBuf> {
    let mut found: Vec<Utf8PathBuf> = prefix
        .read_dir_utf8()
        .ok()?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path().to_owned())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.starts_with("unins") && n.ends_with(".exe"))
        })
        .collect();
    found.sort();
    found.pop()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_in_target_is_not_an_installation() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8PathBuf::from_path_buf(dir.path().to_owned()).unwrap();
        let exe = dir.join("target").join("debug").join("aurora");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "").unwrap();
        let error = prefix_of(&exe).unwrap_err();
        assert_eq!(error.code(), "no_installation");
        assert!(error.to_string().contains("not installed by the installer"));
        assert!(check(&dir).is_err());
    }

    #[test]
    fn an_installed_lace_has_its_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = Utf8PathBuf::from_path_buf(dir.path().to_owned()).unwrap();
        let exe = prefix.join("bin").join("aurora");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "").unwrap();
        if cfg!(windows) {
            std::fs::write(prefix.join("unins000.exe"), "").unwrap();
            assert_eq!(
                uninstaller_exe(&prefix).unwrap().file_name(),
                Some("unins000.exe")
            );
        } else {
            std::fs::write(prefix.join(RECEIPT_FILE), "{}").unwrap();
            std::fs::write(prefix.join(UNINSTALL_SCRIPT), "").unwrap();
        }
        let found = prefix_of(&exe).unwrap();
        assert_eq!(
            dunce::canonicalize(&found).unwrap(),
            dunce::canonicalize(&prefix).unwrap()
        );
        assert!(check(&found).is_ok());
    }
}
