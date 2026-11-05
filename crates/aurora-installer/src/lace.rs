//! A passagem do Lace para a AURORA.
//!
//! O Lace (da 0.4 à 0.9) se atualiza pelo `lace update`, que lê a última
//! release deste repositório com o código da versão dele. Ele procura o
//! índice `lace-<versão>-<plataforma>-index.json`, que a AURORA não publica:
//! a atualização por componentes não prepara, e ele cai para o instalador
//! inteiro da versão nova, `lace-<versão>-<plataforma>.tar.gz` no Linux e no
//! macOS e `lace-<versão>-windows-x64-setup.exe` no Windows, conferido pelo
//! `SHA256SUMS` da release. O `release.yml` publica os instaladores da AURORA
//! também com esses nomes, e o `lace update` (ou a atualização do Lace
//! Studio, que roda o mesmo código) instala a AURORA no lugar do Lace.
//!
//! No Linux e no macOS, o `lace update` extrai o `.tar.gz`, acha a pasta que
//! tem o `install` e o roda com `--yes --components <os do Lace>
//! --prefix <a pasta do Lace>` e `--link <o atalho lace>` ou `--no-link`.
//! O `install` reconhece a pasta do Lace ([`detect`]) e, em vez de instalar
//! nela, faz a troca ([`replace`]): a AURORA vai para a pasta dela
//! ([`target`]), com os mesmos aplicativos ([`components`]) e o atalho
//! `aurora` onde estava o `lace`, e o Lace sai pelo `uninstall.sh` dele
//! ([`uninstall`]).
//!
//! No Windows, quem faz o mesmo é o assistente
//! (`installer/windows/aurora.iss`): ele acha o Lace pela entrada dele na
//! lista de programas, marca os aplicativos da pasta dele (os nomes antigos
//! de [`RENAMED_COMPONENTS`] entram no `SelectInstalledComponents` que o
//! `aurora-pack` gera) e, no fim, roda o desinstalador dele.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

use crate::install::{self, Event, Report, Target};
use crate::payload::Index;
use crate::plan::{self, Selection};

/// Os componentes que mudaram de nome do Lace para a AURORA: `(Lace, AURORA)`.
pub const RENAMED_COMPONENTS: &[(&str, &str)] = &[("lace-learn", "aurora-learn")];

/// O recibo que o instalador do Lace grava na pasta, o mesmo nome do da
/// AURORA ([`install::RECEIPT_FILE`]).
const RECEIPT_FILE: &str = "install.json";

/// O desinstalador que o instalador do Lace grava na pasta.
const UNINSTALL_SCRIPT: &str = "uninstall.sh";

/// O nome da pasta padrão do Lace (`~/.local/share/lace`, `/opt/lace`) e o
/// do executável dele.
const LACE: &str = "lace";

/// O executável e a pasta padrão da AURORA, no lugar dos do Lace.
const AURORA: &str = "aurora";

/// Uma instalação do Lace, como o instalador dele a deixou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lace {
    /// A pasta da instalação.
    pub prefix: PathBuf,
    /// A versão do Lace, do recibo.
    pub version: String,
    /// Os componentes instalados, com os nomes do Lace.
    pub components: Vec<String>,
    /// O atalho `lace` que o instalador criou, se criou.
    pub link: Option<PathBuf>,
}

/// O que interessa do recibo do Lace: a AURORA grava `aurora_version` no
/// lugar de `lace_version`, e é por ele que as duas se distinguem.
#[derive(Deserialize)]
struct LaceReceipt {
    lace_version: String,
    #[serde(default)]
    components: Vec<String>,
    #[serde(default)]
    link: Option<PathBuf>,
}

/// O Lace instalado em `prefix`: o recibo dele, com `lace_version`, o
/// `bin/lace` e o `uninstall.sh`. `None` para qualquer outra pasta, também a
/// de uma instalação da AURORA.
pub fn detect(prefix: &Path) -> Option<Lace> {
    if !prefix.join("bin").join(LACE).is_file() || !prefix.join(UNINSTALL_SCRIPT).is_file() {
        return None;
    }
    let text = std::fs::read_to_string(prefix.join(RECEIPT_FILE)).ok()?;
    let receipt: LaceReceipt = serde_json::from_str(&text).ok()?;
    Some(Lace {
        prefix: prefix.to_owned(),
        version: receipt.lace_version,
        components: receipt.components,
        link: receipt.link,
    })
}

/// O nome na AURORA de um componente do Lace.
pub fn aurora_component(name: &str) -> &str {
    RENAMED_COMPONENTS
        .iter()
        .find(|(lace, _)| *lace == name)
        .map_or(name, |(_, aurora)| aurora)
}

/// Os componentes do Lace com os nomes da AURORA, sem repetir, e à parte os
/// que o índice da AURORA não tem. O `lace` (a linha de comando, que nunca é
/// um componente opcional) e os nomes vazios (um `--components ""`) ficam de
/// fora.
pub fn components(names: &[String], index: &Index) -> (Vec<String>, Vec<String>) {
    let mut known: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for name in names.iter().map(|n| n.trim()) {
        if name.is_empty() || name == LACE {
            continue;
        }
        let name = aurora_component(name);
        let list = if index.components.iter().any(|c| c.name == name) {
            &mut known
        } else {
            &mut missing
        };
        if !list.iter().any(|n| n == name) {
            list.push(name.to_owned());
        }
    }
    (known, missing)
}

/// Onde a AURORA fica para quem tinha o Lace em `lace_prefix`: a pasta
/// `aurora` ao lado da do Lace, quando ela se chama `lace` (as padrão,
/// `~/.local/share/lace` e `/opt/lace`, viram `~/.local/share/aurora` e
/// `/opt/aurora`); senão, a pasta padrão da AURORA, de `default`. O atalho
/// `aurora` fica onde estava o `link` do Lace; sem ele (`--no-link`), sem
/// atalho.
pub fn target(lace_prefix: &Path, link: Option<&Path>, default: &Target) -> Target {
    let prefix = match (lace_prefix.file_name(), lace_prefix.parent()) {
        (Some(name), Some(parent)) if name == LACE => parent.join(AURORA),
        _ => default.prefix.clone(),
    };
    Target {
        prefix,
        link: link.map(|l| l.with_file_name(AURORA)),
    }
}

/// Tira o Lace pelo `uninstall.sh` dele, que apaga o atalho `lace`, o do
/// Lace Studio no menu de aplicativos, o `toolchain/`, o `bin/lace`, o
/// recibo e a pasta, se ela ficar vazia. O `lace update` que rodou o
/// `install` segue até o fim: no Linux e no macOS, apagar o executável de um
/// processo não o interrompe.
pub fn uninstall(lace: &Lace) -> anyhow::Result<()> {
    let script = lace.prefix.join(UNINSTALL_SCRIPT);
    let status = std::process::Command::new("sh")
        .arg(&script)
        .status()
        .with_context(|| format!("Running {}", script.display()))?;
    anyhow::ensure!(
        status.success(),
        "{} exited with code {}",
        script.display(),
        status
            .code()
            .map_or_else(|| "?".to_owned(), |c| c.to_string())
    );
    Ok(())
}

/// O que a troca fez.
#[derive(Debug)]
pub struct Replaced {
    /// A instalação da AURORA.
    pub report: Report,
    /// Onde ela ficou.
    pub target: Target,
    /// Os componentes do Lace que a AURORA não tem.
    pub missing: Vec<String>,
    /// As dependências que a seleção acrescentou.
    pub added: Vec<String>,
    /// Como foi a remoção do Lace: `Err` com o motivo, e então ele ficou.
    pub removed: Result<(), String>,
}

/// De onde e como instalar a AURORA na troca ([`replace`]).
#[derive(Debug, Clone, Copy)]
pub struct Replacement<'a> {
    /// O `payload/` do instalador.
    pub payload: &'a Path,
    /// O índice dele.
    pub index: &'a Index,
    /// Os componentes do `--components` do `lace update`, com os nomes do
    /// Lace; sem eles, os do recibo do Lace.
    pub names: Option<&'a [String]>,
    /// O atalho `lace` do `--link`; `None` com `--no-link`.
    pub link: Option<&'a Path>,
    /// O destino padrão da AURORA, para quando a pasta do Lace não se chama
    /// `lace` ([`target`]).
    pub default: &'a Target,
}

/// Troca o Lace pela AURORA: instala a AURORA onde [`target`] diz, com os
/// componentes do Lace nos nomes da AURORA ([`components`]), e então tira o
/// Lace com `remove` ([`uninstall`], fora dos testes). Se a instalação
/// falha, o Lace fica como estava; se só a remoção falha, a AURORA fica
/// instalada e [`Replaced::removed`] diz por que o Lace ficou.
pub fn replace(
    lace: &Lace,
    with: &Replacement<'_>,
    remove: impl FnOnce(&Lace) -> anyhow::Result<()>,
    on: impl FnMut(Event),
) -> anyhow::Result<Replaced> {
    let index = with.index;
    let target = target(&lace.prefix, with.link, with.default);
    let (names, missing) = components(with.names.unwrap_or(&lace.components), index);
    let (selection, added): (Selection, Vec<String>) = if names.is_empty() {
        (
            plan::profile(index, &plan::default_profile(index))?,
            Vec::new(),
        )
    } else {
        plan::from_names(index, &names)?
    };
    let report = install::install(with.payload, index, &selection, &target, on)?;
    let removed = remove(lace).map_err(|e| format!("{e:#}"));
    Ok(Replaced {
        report,
        target,
        missing,
        added,
        removed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lace_folder(dir: &Path, receipt: &str) -> PathBuf {
        let prefix = dir.join("share").join(LACE);
        std::fs::create_dir_all(prefix.join("bin")).unwrap();
        std::fs::write(prefix.join("bin").join(LACE), "lace").unwrap();
        std::fs::write(prefix.join(UNINSTALL_SCRIPT), "#!/bin/sh\n").unwrap();
        std::fs::write(prefix.join(RECEIPT_FILE), receipt).unwrap();
        prefix
    }

    #[test]
    fn detects_the_lace_by_its_receipt_and_not_aurora() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = lace_folder(
            dir.path(),
            r#"{"lace_version": "0.9.0", "bundle": "2026.10.13", "platform": "linux-x64",
                "components": ["yanc", "lace-learn"], "link": "/home/u/.local/bin/lace"}"#,
        );
        let lace = detect(&prefix).unwrap();
        assert_eq!(lace.version, "0.9.0");
        assert_eq!(lace.components, ["yanc", "lace-learn"]);
        assert_eq!(
            lace.link.as_deref(),
            Some(Path::new("/home/u/.local/bin/lace"))
        );

        // A AURORA grava aurora_version, e a pasta dela tem bin/aurora.
        let aurora = lace_folder(
            &dir.path().join("outra"),
            r#"{"aurora_version": "1.0.0", "bundle": "b", "platform": "linux-x64", "components": []}"#,
        );
        assert_eq!(detect(&aurora), None);
        std::fs::remove_file(prefix.join(UNINSTALL_SCRIPT)).unwrap();
        assert_eq!(
            detect(&prefix),
            None,
            "sem o uninstall.sh não é a pasta do instalador"
        );
        assert_eq!(detect(&dir.path().join("nada")), None);
    }

    #[test]
    fn the_aurora_goes_next_to_the_lace_with_the_link_beside_it() {
        let default = Target {
            prefix: "/home/u/.local/share/aurora".into(),
            link: Some("/home/u/.local/bin/aurora".into()),
        };
        let user = target(
            Path::new("/home/u/.local/share/lace"),
            Some(Path::new("/home/u/.local/bin/lace")),
            &default,
        );
        assert_eq!(user.prefix, Path::new("/home/u/.local/share/aurora"));
        assert_eq!(
            user.link.as_deref(),
            Some(Path::new("/home/u/.local/bin/aurora"))
        );

        let root = target(
            Path::new("/opt/lace"),
            Some(Path::new("/usr/local/bin/lace")),
            &default,
        );
        assert_eq!(root.prefix, Path::new("/opt/aurora"));
        assert_eq!(
            root.link.as_deref(),
            Some(Path::new("/usr/local/bin/aurora"))
        );

        // Uma pasta com outro nome: a padrão da AURORA. Sem atalho, sem atalho.
        let custom = target(Path::new("/srv/eda"), None, &default);
        assert_eq!(custom.prefix, default.prefix);
        assert_eq!(custom.link, None);
    }

    #[test]
    fn lace_components_get_the_aurora_names() {
        assert_eq!(aurora_component("lace-learn"), "aurora-learn");
        assert_eq!(aurora_component("yanc"), "yanc");
    }
}
