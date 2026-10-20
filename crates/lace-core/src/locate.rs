//! Onde está o Lace: a busca do bundle e a descrição dele, para qualquer
//! programa que não seja a CLI instalada ao lado do bundle.
//!
//! A CLI acha o bundle ao lado do próprio executável ([`Toolchain::locate`]).
//! O Studio, e qualquer outro cliente, procura a instalação do Lace numa
//! ordem fixa e fica com a primeira que abrir:
//!
//! 1. A pasta declarada numa preferência ([`Lookup::settings`]). Declarada e
//!    inválida é erro: não cai em silêncio para as outras.
//! 2. A pasta declarada no ambiente ([`Lookup::environment`], o valor de
//!    `LACE_TOOLCHAIN`), com a mesma regra.
//! 3. O bundle em que o programa está instalado: a primeira pasta acima do
//!    executável ([`Lookup::exe`]) com um `bundle.json`. O instalador põe o
//!    Studio dentro do bundle (`toolchain/studio/`); vem antes das pastas
//!    padrão para uma instalação em outra pasta não abrir o bundle de outra.
//! 4. As pastas padrão do instalador ([`Lookup::installation_dirs`]).
//! 5. O `lace` das pastas do `PATH` ([`Lookup::path_dirs`]), com
//!    [`Toolchain::locate`], que resolve o atalho até a instalação.
//! 6. Ao lado do executável, como a CLI ([`Toolchain::locate`] no `exe`).
//!
//! O core não lê o ambiente: quem chama monta o [`Lookup`] com o que leu. O
//! `PATH` só serve para achar a instalação; as ferramentas continuam saindo
//! do bundle.
//!
//! O compilador do Verilator e o Quartus Prime seguem a regra da CLI: o
//! declarado ([`Lookup::compiler`], [`Lookup::quartus`]) vale, e um declarado
//! que não serve vira [`Found::compiler_error`] ou [`Found::quartus_error`],
//! com o bundle aberto sem ele; quem chama decide se isso é erro (a CLI) ou
//! aviso (o Studio). Sem Quartus declarado, vale o de
//! [`Lookup::quartus_rootdir`] (a variável que o instalador da Intel cria),
//! senão o das pastas padrão ([`Quartus::detect`]).

use camino::{Utf8Path, Utf8PathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::fpga::Quartus;
use crate::toolchain::{BundleComponent, SystemCompiler, Tool, Toolchain, component};
use crate::{ErrorInfo, LaceError, MANIFEST_FILE, Result};

/// De onde o Lace pode estar, na ordem de busca do módulo. Tudo vem de quem
/// chama; o core não lê o ambiente.
#[derive(Debug, Clone, Default)]
pub struct Lookup {
    /// A pasta do bundle declarada numa preferência (o `toolchain_dir` do
    /// Studio). Inválida é erro.
    pub settings: Option<Utf8PathBuf>,
    /// A pasta do bundle declarada no ambiente (`LACE_TOOLCHAIN`). Inválida
    /// é erro.
    pub environment: Option<Utf8PathBuf>,
    /// O executável de quem chama (`current_exe`): o bundle que o contém, e
    /// o bundle ao lado dele.
    pub exe: Option<Utf8PathBuf>,
    /// As pastas padrão do instalador, por plataforma
    /// ([`Lookup::installation_dirs`]).
    pub installation_dirs: Vec<Utf8PathBuf>,
    /// As pastas do `PATH` (`std::env::split_paths`), para achar o `lace` e,
    /// dele, a instalação.
    pub path_dirs: Vec<Utf8PathBuf>,
    /// O compilador do Verilator declarado (`--compiler`, `LACE_COMPILER`, a
    /// preferência): a raiz de um MSYS2 no Windows, a pasta com `perl`,
    /// `make` e um compilador C++ nos outros sistemas.
    pub compiler: Option<Utf8PathBuf>,
    /// O Quartus Prime declarado (`--quartus`, `LACE_QUARTUS`, a
    /// preferência).
    pub quartus: Option<Utf8PathBuf>,
    /// O valor de `QUARTUS_ROOTDIR`, que o instalador da Intel cria.
    pub quartus_rootdir: Option<Utf8PathBuf>,
    /// A pasta do usuário, para [`Quartus::detect`].
    pub home: Option<Utf8PathBuf>,
}

impl Lookup {
    /// As pastas onde o instalador do Lace põe o bundle, por plataforma:
    /// `%LOCALAPPDATA%\Programs\Lace\toolchain` e
    /// `%ProgramFiles%\Lace\toolchain` no Windows;
    /// `$XDG_DATA_HOME/lace/toolchain` e `~/.local/share/lace/toolchain` nos
    /// outros. `var` lê uma variável de ambiente (`None` quando ausente ou
    /// vazia): o core não a lê sozinho.
    pub fn installation_dirs(var: impl Fn(&str) -> Option<Utf8PathBuf>) -> Vec<Utf8PathBuf> {
        let mut dirs = Vec::new();
        if cfg!(windows) {
            for (name, sub) in [
                ("LOCALAPPDATA", "Programs/Lace/toolchain"),
                ("ProgramFiles", "Lace/toolchain"),
            ] {
                if let Some(base) = var(name) {
                    dirs.push(base.join(sub));
                }
            }
        } else {
            if let Some(data) = var("XDG_DATA_HOME") {
                dirs.push(data.join("lace/toolchain"));
            }
            if let Some(home) = var("HOME") {
                dirs.push(home.join(".local/share/lace/toolchain"));
            }
            dirs.dedup();
        }
        dirs
    }
}

/// Como o bundle foi achado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// A pasta declarada numa preferência.
    Settings,
    /// A pasta declarada no ambiente (`LACE_TOOLCHAIN`).
    Environment,
    /// O bundle em que o programa está instalado.
    Bundled,
    /// Uma pasta padrão do instalador.
    Installation,
    /// O `lace` do `PATH`.
    Path,
    /// Ao lado do executável.
    Beside,
}

/// O bundle aberto e como foi achado.
#[derive(Debug)]
pub struct Found {
    /// O bundle, já com o compilador do Verilator e o Quartus decididos.
    pub toolchain: Toolchain,
    /// Como foi achado.
    pub origin: Origin,
    /// O compilador declarado não tem os três programas (código
    /// `compiler_invalid`, com a pasta em `data.dir`); o bundle abriu sem
    /// ele.
    pub compiler_error: Option<ErrorInfo>,
    /// A pasta declarada do Quartus não tem o `quartus_sh` (código
    /// `quartus_invalid`, com a pasta em `data.dir`); o bundle abriu sem
    /// ele.
    pub quartus_error: Option<ErrorInfo>,
}

/// Abre o bundle pela ordem descrita no módulo. `Err` só quando nenhuma
/// origem abre ([`LaceError::BundleNotFound`], com onde procurou) ou quando
/// uma pasta declarada é inválida (o erro de [`Toolchain::open`]).
pub fn resolve(lookup: &Lookup) -> Result<Found> {
    let (toolchain, origin) = open_bundle(lookup)?;
    let (toolchain, compiler_error) = apply_compiler(toolchain, lookup.compiler.as_deref());
    let (toolchain, quartus_error) = apply_quartus(toolchain, lookup);
    Ok(Found {
        toolchain,
        origin,
        compiler_error,
        quartus_error,
    })
}

fn open_bundle(lookup: &Lookup) -> Result<(Toolchain, Origin)> {
    if let Some(dir) = lookup
        .settings
        .as_deref()
        .filter(|d| !d.as_str().is_empty())
    {
        return Ok((Toolchain::open(dir)?, Origin::Settings));
    }
    if let Some(dir) = lookup
        .environment
        .as_deref()
        .filter(|d| !d.as_str().is_empty())
    {
        return Ok((Toolchain::open(dir)?, Origin::Environment));
    }
    let mut searched = Vec::new();
    if let Some(dir) = lookup.exe.as_deref().and_then(containing_bundle) {
        match Toolchain::open(&dir) {
            Ok(toolchain) => return Ok((toolchain, Origin::Bundled)),
            Err(error) => tracing::warn!("Bundle at {dir} did not open: {error}"),
        }
        searched.push(dir);
    }
    for dir in &lookup.installation_dirs {
        if dir.join(MANIFEST_FILE).is_file() {
            match Toolchain::open(dir) {
                Ok(toolchain) => return Ok((toolchain, Origin::Installation)),
                Err(error) => tracing::warn!("Bundle at {dir} did not open: {error}"),
            }
        }
        searched.push(dir.clone());
    }
    if let Some(lace) = lace_on_path(&lookup.path_dirs) {
        match Toolchain::locate(&lace) {
            Ok(toolchain) => return Ok((toolchain, Origin::Path)),
            Err(error) => tracing::warn!("The lace at {lace} has no usable bundle: {error}"),
        }
        searched.push(lace);
    }
    if let Some(exe) = &lookup.exe {
        match Toolchain::locate(exe) {
            Ok(toolchain) => return Ok((toolchain, Origin::Beside)),
            // Só o executável foi dado (a CLI): o erro é o de `locate`, com
            // o texto que a CLI sempre deu.
            Err(error) if searched.is_empty() => return Err(error),
            Err(LaceError::BundleNotFound { searched: beside }) => searched.extend(beside),
            Err(error) => return Err(error),
        }
    }
    Err(LaceError::BundleNotFound { searched })
}

/// O bundle que contém o executável: a primeira pasta acima dele com um
/// `bundle.json`, com os symlinks resolvidos. Cinco níveis cobrem o `.app`
/// do macOS (`toolchain/studio/Lace Studio.app/Contents/MacOS`).
fn containing_bundle(exe: &Utf8Path) -> Option<Utf8PathBuf> {
    let exe = dunce::canonicalize(exe)
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())?;
    exe.ancestors()
        .skip(1)
        .take(5)
        .find(|dir| dir.join(MANIFEST_FILE).is_file())
        .map(Utf8Path::to_owned)
}

/// O `lace` (ou `lace.exe`) nas pastas do `PATH`.
fn lace_on_path(dirs: &[Utf8PathBuf]) -> Option<Utf8PathBuf> {
    let name = if cfg!(windows) { "lace.exe" } else { "lace" };
    dirs.iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// O compilador do Verilator num diretório declarado: a raiz do MSYS2 no
/// Windows, o diretório com os três programas nos outros sistemas.
pub fn compiler_in(dir: &Utf8Path) -> Option<SystemCompiler> {
    if cfg!(windows) {
        SystemCompiler::in_msys2(dir)
    } else {
        SystemCompiler::in_dir(dir.to_owned())
    }
}

fn apply_compiler(
    toolchain: Toolchain,
    declared: Option<&Utf8Path>,
) -> (Toolchain, Option<ErrorInfo>) {
    let Some(dir) = declared.filter(|d| !d.as_str().is_empty()) else {
        return (toolchain, None);
    };
    match compiler_in(dir) {
        Some(compiler) => (toolchain.with_system_compiler(Some(compiler)), None),
        None => {
            let mut error = ErrorInfo::new(
                "compiler_invalid",
                format!("The compiler directory {dir} is missing perl, make or a C++ compiler"),
            );
            error.data = Some(serde_json::json!({ "dir": dir }));
            (toolchain.with_system_compiler(None), Some(error))
        }
    }
}

fn apply_quartus(toolchain: Toolchain, lookup: &Lookup) -> (Toolchain, Option<ErrorInfo>) {
    if let Some(dir) = lookup.quartus.as_deref().filter(|d| !d.as_str().is_empty()) {
        return match Quartus::in_dir(dir) {
            Some(quartus) => (toolchain.with_quartus(Some(quartus)), None),
            None => {
                let mut error = ErrorInfo::new(
                    "quartus_invalid",
                    format!("No Quartus Prime at {dir}: quartus_sh is not there"),
                );
                error.data = Some(serde_json::json!({ "dir": dir }));
                (toolchain.with_quartus(None), Some(error))
            }
        };
    }
    if let Some(found) = lookup
        .quartus_rootdir
        .as_deref()
        .filter(|d| !d.as_str().is_empty())
        .and_then(Quartus::in_dir)
    {
        return (toolchain.with_quartus(Some(found)), None);
    }
    (
        toolchain.with_quartus(Quartus::detect(lookup.home.as_deref())),
        None,
    )
}

/// O `lace` da instalação do bundle: `<instalação>/bin/lace`, se existe.
pub fn lace_cli(toolchain: &Toolchain) -> Option<Utf8PathBuf> {
    let name = if cfg!(windows) { "lace.exe" } else { "lace" };
    let candidate = toolchain.root().parent()?.join("bin").join(name);
    candidate.is_file().then_some(candidate)
}

/// O que uma tela de ferramentas mostra: o `lace tools` em dados. Nunca
/// falha: um bundle que não abriu vira `found: false` com o erro
/// ([`describe_error`]).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ToolchainInfo {
    /// O bundle abriu.
    pub found: bool,
    /// Por que não abriu.
    pub error: Option<ErrorInfo>,
    /// Como foi achado.
    pub origin: Option<Origin>,
    /// A pasta do bundle.
    #[schemars(with = "Option<String>")]
    pub root: Option<Utf8PathBuf>,
    /// A versão do bundle.
    pub bundle: Option<String>,
    /// A plataforma do bundle (`linux-x64`, `darwin-arm64`, `windows-x64`).
    pub platform: Option<String>,
    /// Os componentes instalados.
    pub components: Vec<BundleComponent>,
    /// Os componentes que o Lace conhece e que não estão instalados.
    pub not_installed: Vec<String>,
    /// Cada ferramenta que o Lace roda.
    pub tools: Vec<ToolStatus>,
    /// O compilador do Verilator: do sistema no Linux e no macOS, do bundle
    /// no Windows (`bundled`).
    pub system_compiler: Option<SystemCompiler>,
    /// O compilador declarado não serve.
    pub compiler_error: Option<ErrorInfo>,
    /// O Quartus Prime do sistema, se encontrado.
    pub quartus: Option<Quartus>,
    /// A pasta declarada do Quartus não serve.
    pub quartus_error: Option<ErrorInfo>,
    /// A CLI `lace` da instalação.
    #[schemars(with = "Option<String>")]
    pub lace_cli: Option<Utf8PathBuf>,
    /// O cliente web do Surfer (`surfer-aurora/web`), para a onda numa aba;
    /// `None` num bundle que não o traz.
    #[schemars(with = "Option<String>")]
    pub surfer_web: Option<Utf8PathBuf>,
}

/// Uma ferramenta do bundle: onde está, ou por que não está.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ToolStatus {
    /// O nome do executável (`iverilog`, `cmmcomp`).
    pub name: String,
    /// O componente que a traz; `None` para o Perl do sistema e o Quartus.
    pub component: Option<String>,
    /// O executável, quando disponível.
    #[schemars(with = "Option<String>")]
    pub path: Option<Utf8PathBuf>,
    /// Vem do sistema, não do bundle: o Perl do Verilator no Linux e no
    /// macOS (no Windows ele vem no bundle) e o Quartus.
    pub system: bool,
    /// Por que não está disponível.
    pub error: Option<ErrorInfo>,
}

/// O [`ToolchainInfo`] de um bundle aberto.
pub fn describe(found: &Found) -> ToolchainInfo {
    let toolchain = &found.toolchain;
    let manifest = toolchain.manifest();
    // O Perl do Verilator vem do sistema no Linux e no macOS, e do bundle no
    // Windows; o Quartus, sempre do sistema.
    let bundled = toolchain.system_compiler().is_some_and(|c| c.bundled);
    let tools = toolchain
        .tools()
        .into_iter()
        .map(|tool| {
            let (path, error) = match toolchain.tool(tool) {
                Ok(path) => (Some(path), None),
                Err(error) => (None, Some(ErrorInfo::from(error))),
            };
            ToolStatus {
                name: tool.binary_name().to_owned(),
                component: tool.component().map(str::to_owned),
                path,
                system: tool.is_system() && !(tool == Tool::Perl && bundled),
                error,
            }
        })
        .collect();
    ToolchainInfo {
        found: true,
        error: None,
        origin: Some(found.origin),
        root: Some(toolchain.root().to_owned()),
        bundle: Some(manifest.bundle.clone()),
        platform: Some(manifest.platform.clone()),
        components: manifest.components.clone(),
        not_installed: component::ALL
            .iter()
            .filter(|c| toolchain.component(c).is_none())
            .map(|c| (*c).to_owned())
            .collect(),
        tools,
        system_compiler: toolchain.system_compiler().cloned(),
        compiler_error: found.compiler_error.clone(),
        quartus: toolchain.quartus().cloned(),
        quartus_error: found.quartus_error.clone(),
        lace_cli: lace_cli(toolchain),
        surfer_web: toolchain.surfer_web_dir().ok(),
    }
}

/// O [`ToolchainInfo`] de um bundle que não abriu: `found: false`, o erro, e
/// todos os componentes como não instalados.
pub fn describe_error(error: &LaceError) -> ToolchainInfo {
    ToolchainInfo {
        found: false,
        error: Some(ErrorInfo::from(error)),
        origin: None,
        root: None,
        bundle: None,
        platform: None,
        components: Vec::new(),
        not_installed: component::ALL.iter().map(|c| (*c).to_owned()).collect(),
        tools: Vec::new(),
        system_compiler: None,
        compiler_error: None,
        quartus: None,
        quartus_error: None,
        lace_cli: None,
        surfer_web: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um bundle vazio mas válido: `bundle.json` da plataforma atual, sem
    /// componentes.
    fn fake_bundle(dir: &Utf8Path) {
        std::fs::create_dir_all(dir.join(crate::COMPONENTS_DIR)).unwrap();
        let platform = crate::Platform::current().expect("plataforma suportada");
        std::fs::write(
            dir.join(MANIFEST_FILE),
            format!(
                r#"{{"schema": {}, "bundle": "teste", "platform": "{}"}}"#,
                crate::BUNDLE_SCHEMA,
                platform.as_str()
            ),
        )
        .unwrap();
    }

    fn tempdir() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dunce::canonicalize(dir.path()).unwrap()).unwrap();
        (dir, path)
    }

    #[test]
    fn a_declared_folder_wins_and_an_invalid_one_is_an_error() {
        let (_guard, root) = tempdir();
        let declared = root.join("declared");
        fake_bundle(&declared);
        let found = resolve(&Lookup {
            settings: Some(declared.clone()),
            installation_dirs: vec![root.join("nao-existe")],
            ..Lookup::default()
        })
        .unwrap();
        assert_eq!(found.origin, Origin::Settings);
        assert_eq!(found.toolchain.root(), declared);

        let error = resolve(&Lookup {
            settings: Some(root.join("vazia")),
            ..Lookup::default()
        })
        .unwrap_err();
        assert_ne!(error.code(), "bundle_not_found", "{error}");
    }

    #[test]
    fn installation_dirs_and_path_are_searched_in_order() {
        let (_guard, root) = tempdir();
        let installed = root.join("inst/toolchain");
        fake_bundle(&installed);
        let found = resolve(&Lookup {
            installation_dirs: vec![root.join("outra"), installed.clone()],
            ..Lookup::default()
        })
        .unwrap();
        assert_eq!(found.origin, Origin::Installation);

        // Pelo PATH: um `lace` em `<inst>/bin`, e o bundle ao lado.
        let bin = root.join("inst/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let lace = bin.join(if cfg!(windows) { "lace.exe" } else { "lace" });
        std::fs::write(&lace, "").unwrap();
        let found = resolve(&Lookup {
            path_dirs: vec![root.join("nada"), bin.clone()],
            ..Lookup::default()
        })
        .unwrap();
        assert_eq!(found.origin, Origin::Path);
        assert_eq!(found.toolchain.root(), installed);

        // Nada: o erro lista onde procurou.
        let error = resolve(&Lookup {
            installation_dirs: vec![root.join("x")],
            path_dirs: vec![root.join("y")],
            ..Lookup::default()
        })
        .unwrap_err();
        assert_eq!(error.code(), "bundle_not_found");
        assert!(error.to_string().contains("x"), "{error}");
    }

    #[test]
    fn the_bundle_containing_the_executable_comes_first() {
        let (_guard, root) = tempdir();
        let bundle = root.join("toolchain");
        fake_bundle(&bundle);
        let exe_dir = bundle.join("studio");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let exe = exe_dir.join("lace-studio.exe");
        std::fs::write(&exe, "").unwrap();
        let other = root.join("outra/toolchain");
        fake_bundle(&other);
        let found = resolve(&Lookup {
            exe: Some(exe),
            installation_dirs: vec![other],
            ..Lookup::default()
        })
        .unwrap();
        assert_eq!(found.origin, Origin::Bundled);
        assert_eq!(found.toolchain.root(), bundle);
    }

    #[test]
    fn a_declared_compiler_or_quartus_that_is_not_there_is_reported_not_fatal() {
        let (_guard, root) = tempdir();
        let bundle = root.join("toolchain");
        fake_bundle(&bundle);
        let found = resolve(&Lookup {
            settings: Some(bundle),
            compiler: Some(root.join("sem-compilador")),
            quartus: Some(root.join("sem-quartus")),
            ..Lookup::default()
        })
        .unwrap();
        let compiler = found.compiler_error.as_ref().expect("erro do compilador");
        assert_eq!(compiler.code, "compiler_invalid");
        assert!(compiler.data.as_ref().unwrap()["dir"].is_string());
        assert_eq!(
            found.quartus_error.as_ref().unwrap().code,
            "quartus_invalid"
        );
        assert!(found.toolchain.quartus().is_none());
        let info = describe(&found);
        assert!(info.found);
        assert_eq!(info.origin, Some(Origin::Settings));
        assert_eq!(info.not_installed.len(), component::ALL.len());
        assert!(info.compiler_error.is_some());
    }

    #[test]
    fn describe_error_has_everything_missing() {
        let info = describe_error(&LaceError::BundleNotFound {
            searched: vec!["/x".into()],
        });
        assert!(!info.found);
        assert_eq!(info.error.as_ref().unwrap().code, "bundle_not_found");
        assert!(info.tools.is_empty());
        assert_eq!(info.not_installed.len(), component::ALL.len());
    }

    #[test]
    fn installation_dirs_follow_the_platform() {
        let dirs = Lookup::installation_dirs(|var| match var {
            "LOCALAPPDATA" => Some("C:/u/AppData/Local".into()),
            "ProgramFiles" => Some("C:/Program Files".into()),
            "XDG_DATA_HOME" => Some("/x/data".into()),
            "HOME" => Some("/home/eu".into()),
            _ => None,
        });
        if cfg!(windows) {
            assert_eq!(
                dirs,
                [
                    Utf8PathBuf::from("C:/u/AppData/Local/Programs/Lace/toolchain"),
                    Utf8PathBuf::from("C:/Program Files/Lace/toolchain")
                ]
            );
        } else {
            assert_eq!(
                dirs,
                [
                    Utf8PathBuf::from("/x/data/lace/toolchain"),
                    Utf8PathBuf::from("/home/eu/.local/share/lace/toolchain")
                ]
            );
        }
        assert!(Lookup::installation_dirs(|_| None).is_empty());
    }
}
