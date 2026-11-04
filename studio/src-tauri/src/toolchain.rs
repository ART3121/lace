//! Onde está a AURORA para o Studio: o programa `aurora`, que o Studio abre
//! como servidor ([`crate::server`]), e o bundle que o servidor recebe em
//! `--toolchain`. Achar o servidor é do cliente, como a extensão do VS Code
//! faz com o `aurora` dela; o resto (as ferramentas, os componentes, o
//! compilador do Verilator, o Quartus, o cliente web do Surfer) é o
//! `toolchain.info` do servidor.
//!
//! O `aurora` é, nesta ordem: o da variável `AURORA_IDE_CLI` (para
//! desenvolvimento), o da preferência `aurora_path`, o da instalação do
//! bundle das preferências (`<instalação>/bin/aurora`), o da instalação em
//! que o Studio está (`<instalação>/toolchain/studio/`) e o do `PATH`. O
//! bundle é o da preferência `toolchain_dir`, o de `AURORA_TOOLCHAIN` ou o da
//! instalação achada; sem nenhum, o servidor procura o dele.

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::{Value, json};

use crate::error::{IpcError, IpcResult, codes};
use crate::server::AuroraServer;
use crate::settings::Settings;

/// O nome do executável da AURORA neste sistema.
const AURORA_EXE: &str = if cfg!(windows) {
    "aurora.exe"
} else {
    "aurora"
};

/// Quantas pastas acima do Studio procurar a instalação: o Studio fica em
/// `<instalação>/toolchain/studio/` (no macOS, dentro de um `.app`).
const INSTALLATION_DEPTH: usize = 6;

/// Uma variável de ambiente como caminho, ou `None` quando ausente ou vazia.
fn env_path(name: &str) -> Option<Utf8PathBuf> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(Utf8PathBuf::from)
}

/// Uma preferência como caminho, ou `None` quando ausente ou vazia.
fn setting_path(value: &Option<String>) -> Option<Utf8PathBuf> {
    value
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .map(Utf8PathBuf::from)
}

/// O `aurora` de uma instalação (`<instalação>/bin/aurora`), se existe.
fn aurora_in(prefix: &Utf8Path) -> Option<Utf8PathBuf> {
    let aurora = prefix.join("bin").join(AURORA_EXE);
    aurora.is_file().then_some(aurora)
}

/// A instalação em que o Studio está: a primeira pasta acima dele com
/// `bin/aurora` e `toolchain/`.
fn installation_of_studio() -> Option<Utf8PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = Utf8PathBuf::from_path_buf(exe).ok()?;
    exe.ancestors()
        .skip(1)
        .take(INSTALLATION_DEPTH)
        .find(|dir| aurora_in(dir).is_some() && dir.join("toolchain").is_dir())
        .map(Utf8Path::to_owned)
}

/// O `aurora` do `PATH`.
fn aurora_on_path() -> Option<Utf8PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter_map(|dir| Utf8PathBuf::from_path_buf(dir).ok())
        .map(|dir| dir.join(AURORA_EXE))
        .find(|aurora| aurora.is_file())
}

/// A instalação de um `aurora` em `<instalação>/bin/`, com o bundle ao lado.
fn installation_of(aurora: &Utf8Path) -> Option<Utf8PathBuf> {
    let bin = aurora.parent()?;
    let prefix = bin.parent()?;
    (bin.file_name() == Some("bin") && prefix.join("toolchain").is_dir()).then(|| prefix.to_owned())
}

/// A instalação da AURORA: a do bundle das preferências, a do Studio, ou a
/// do `aurora` do `PATH`.
fn installation(settings: &Settings) -> Option<Utf8PathBuf> {
    setting_path(&settings.toolchain_dir)
        .and_then(|dir| {
            dir.parent()
                .filter(|p| aurora_in(p).is_some())
                .map(Utf8Path::to_owned)
        })
        .or_else(installation_of_studio)
        .or_else(|| aurora_on_path().as_deref().and_then(installation_of))
}

/// O `aurora` que o Studio usa como servidor.
pub fn aurora_executable(settings: &Settings) -> IpcResult<Utf8PathBuf> {
    let declared = env_path("AURORA_IDE_CLI").or_else(|| setting_path(&settings.aurora_path));
    if let Some(path) = declared {
        return if path.is_file() {
            Ok(path)
        } else {
            Err(IpcError::new(
                codes::AURORA_MISSING,
                format!("The aurora program {path} does not exist"),
            ))
        };
    }
    installation(settings)
        .and_then(|prefix| aurora_in(&prefix))
        .or_else(aurora_on_path)
        .ok_or_else(|| {
            IpcError::new(
                codes::AURORA_MISSING,
                "The aurora program was not found next to the Studio or in the PATH; set it in Preferences > AURORA bundle",
            )
        })
}

/// O bundle que o servidor recebe em `--toolchain`: o das preferências, o
/// de `AURORA_TOOLCHAIN` ou o da instalação; `None` deixa o servidor achar o
/// dele.
pub fn bundle_dir(settings: &Settings) -> Option<Utf8PathBuf> {
    setting_path(&settings.toolchain_dir)
        .or_else(|| env_path("AURORA_TOOLCHAIN"))
        .or_else(|| installation(settings).map(|prefix| prefix.join("toolchain")))
}

/// A pasta `bin/` da instalação, para pôr no `PATH` do terminal de shell e
/// o usuário poder digitar `aurora` nele.
pub fn install_bin_dir(settings: &Settings) -> Option<Utf8PathBuf> {
    installation(settings).map(|prefix| prefix.join("bin"))
}

/// O que a tela de ferramentas mostra: o `toolchain.info` do servidor, o
/// equivalente do `aurora tools`. Nunca falha: sem servidor, vem
/// `found: false` com o erro, como um bundle que não abriu.
pub fn info(server: &AuroraServer, settings: &Settings) -> Value {
    match server.call(settings, "toolchain.info", Value::Null) {
        Ok(info) => info,
        Err(error) => json!({
            "found": false,
            "error": { "code": error.code, "message": error.message },
            "origin": null,
            "root": null,
            "bundle": null,
            "platform": null,
            "components": [],
            "not_installed": [],
            "tools": [],
            "system_compiler": null,
            "compiler_error": null,
            "quartus": null,
            "quartus_error": null,
            "aurora_cli": null,
            "surfer_web": null,
        }),
    }
}

/// O cliente web do Surfer do bundle, para a onda numa aba; `None` sem ele
/// ou sem servidor.
pub fn surfer_web(server: &AuroraServer, settings: &Settings) -> Option<Utf8PathBuf> {
    info(server, settings)["surfer_web"]
        .as_str()
        .map(Utf8PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_installation_has_the_aurora_in_bin_and_the_bundle_beside() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = Utf8PathBuf::from_path_buf(dir.path().to_owned()).unwrap();
        std::fs::create_dir_all(prefix.join("bin")).unwrap();
        std::fs::create_dir_all(prefix.join("toolchain")).unwrap();
        let aurora = prefix.join("bin").join(AURORA_EXE);
        std::fs::write(&aurora, "").unwrap();
        assert_eq!(aurora_in(&prefix), Some(aurora.clone()));
        assert_eq!(installation_of(&aurora), Some(prefix.clone()));
        // Um aurora fora de `bin/` não diz qual é a instalação.
        let loose = prefix.join(AURORA_EXE);
        std::fs::write(&loose, "").unwrap();
        assert_eq!(installation_of(&loose), None);
        // O bundle das preferências leva à instalação dele.
        let settings = Settings {
            toolchain_dir: Some(prefix.join("toolchain").to_string()),
            ..Settings::default()
        };
        assert_eq!(installation(&settings), Some(prefix.clone()));
        assert_eq!(bundle_dir(&settings), Some(prefix.join("toolchain")));
        assert_eq!(install_bin_dir(&settings), Some(prefix.join("bin")));
    }
}
