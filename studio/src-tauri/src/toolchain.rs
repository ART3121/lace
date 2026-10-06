//! De onde vem o bundle do Lace.
//!
//! A CLI acha o bundle ao lado do próprio executável
//! (`Toolchain::locate`). O Studio procura a instalação do Lace nesta ordem
//! e usa a primeira que abrir:
//!
//! 1. `toolchain_dir` das preferências (o `--toolchain` da CLI). Declarado e
//!    inválido é erro: não cai em silêncio para os outros.
//! 2. A variável `LACE_TOOLCHAIN`, com a mesma regra.
//! 3. O bundle em que o Studio está instalado. O instalador do Lace põe o
//!    Studio no bundle, como o componente `studio`
//!    (`toolchain/studio/lace-studio`; no macOS,
//!    `toolchain/studio/Lace Studio.app`): é a primeira pasta acima do
//!    executável com um `bundle.json`. Vem antes da pasta padrão para uma
//!    instalação em outra pasta não abrir o bundle de outra instalação.
//! 4. A pasta padrão do instalador: `~/.local/share/lace/toolchain` (ou
//!    `$XDG_DATA_HOME/lace/toolchain`) no Linux e no macOS;
//!    `%LOCALAPPDATA%\Programs\Lace\toolchain` e
//!    `%ProgramFiles%\Lace\toolchain` no Windows. Serve para um Studio
//!    de fora do bundle, como o do `npm run tauri dev`.
//! 5. O `lace` do `PATH`, com `Toolchain::locate`, que resolve o atalho
//!    `~/.local/bin/lace` até a instalação. Serve para quem instalou em
//!    outra pasta (`/DIR=` do assistente do Windows).
//! 6. Ao lado do executável do Studio (`<dir>/toolchain` ou
//!    `<dir>/../toolchain`), como a CLI.
//!
//! O `PATH` só serve para achar a instalação. As ferramentas continuam saindo
//! do bundle.
//!
//! O compilador do Verilator segue a regra da CLI (`settings.rs` dela):
//! `compiler_dir` das preferências ou `LACE_COMPILER`; sem eles, o que o Core
//! acha nos locais padrão. Um diretório declarado sem os três programas não
//! derruba o bundle: o Studio abre sem compilador, a operação com Verilator
//! falha com `system_compiler_missing`, e a tela de ferramentas mostra o
//! motivo.

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{BundleComponent, SystemCompiler, Tool, Toolchain, component};
use serde::Serialize;

use crate::error::{IpcError, IpcResult};
use crate::settings::Settings;

/// Como o bundle foi achado.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// `toolchain_dir` das preferências.
    Settings,
    /// A variável `LACE_TOOLCHAIN`.
    Environment,
    /// O bundle em que o Studio está instalado.
    Bundled,
    /// A pasta padrão do instalador.
    Installation,
    /// O `lace` do `PATH`.
    Path,
    /// Ao lado do executável do Studio.
    Beside,
}

/// O bundle aberto e como ele foi achado.
pub struct Located {
    /// O bundle, já com o compilador do Verilator decidido.
    pub toolchain: Toolchain,
    /// Como foi achado.
    pub origin: Origin,
    /// O compilador declarado não tem os três programas.
    pub compiler_error: Option<IpcError>,
}

/// Abre o bundle pela ordem descrita no módulo.
pub fn resolve(settings: &Settings) -> IpcResult<Located> {
    let (toolchain, origin) = open_bundle(settings)?;
    let (toolchain, compiler_error) = apply_compiler(toolchain, settings);
    Ok(Located {
        toolchain,
        origin,
        compiler_error,
    })
}

fn open_bundle(settings: &Settings) -> IpcResult<(Toolchain, Origin)> {
    if let Some(dir) = settings.toolchain_dir.as_deref().filter(|d| !d.is_empty()) {
        return Ok((Toolchain::open(dir)?, Origin::Settings));
    }
    if let Some(dir) = std::env::var("LACE_TOOLCHAIN")
        .ok()
        .filter(|d| !d.is_empty())
    {
        return Ok((Toolchain::open(&dir)?, Origin::Environment));
    }
    let mut searched = Vec::new();
    let exe = current_exe();
    if let Some(dir) = exe.as_deref().and_then(containing_bundle) {
        match Toolchain::open(&dir) {
            Ok(toolchain) => return Ok((toolchain, Origin::Bundled)),
            Err(error) => tracing::warn!("Bundle at {dir} did not open: {error}"),
        }
        searched.push(dir.to_string());
    }
    for dir in installation_dirs() {
        if dir.join(lace_core::MANIFEST_FILE).is_file() {
            match Toolchain::open(&dir) {
                Ok(toolchain) => return Ok((toolchain, Origin::Installation)),
                Err(error) => tracing::warn!("Bundle at {dir} did not open: {error}"),
            }
        }
        searched.push(dir.to_string());
    }
    if let Some(lace) = lace_on_path() {
        match Toolchain::locate(&lace) {
            Ok(toolchain) => return Ok((toolchain, Origin::Path)),
            Err(error) => tracing::warn!("The lace at {lace} has no usable bundle: {error}"),
        }
        searched.push(lace.to_string());
    }
    if let Some(exe) = exe {
        if let Ok(toolchain) = Toolchain::locate(&exe) {
            return Ok((toolchain, Origin::Beside));
        }
        searched.push(format!("{} (beside Lace Studio)", exe));
    }
    Err(IpcError::new(
        "bundle_not_found",
        format!(
            "Lace installation not found (searched: {})",
            searched.join(", ")
        ),
    ))
}

/// O bundle que contém o executável do Studio: a primeira pasta acima dele
/// com um `bundle.json`, com os symlinks resolvidos. Quatro níveis cobrem o
/// `.app` do macOS (`toolchain/studio/Lace Studio.app/Contents/MacOS`).
fn containing_bundle(exe: &Utf8Path) -> Option<Utf8PathBuf> {
    let exe = std::fs::canonicalize(exe)
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())?;
    exe.ancestors()
        .skip(1)
        .take(5)
        .find(|dir| dir.join(lace_core::MANIFEST_FILE).is_file())
        .map(Utf8Path::to_owned)
}

/// As pastas onde o instalador do Lace põe o bundle.
fn installation_dirs() -> Vec<Utf8PathBuf> {
    let mut dirs = Vec::new();
    if cfg!(windows) {
        for (var, sub) in [
            ("LOCALAPPDATA", "Programs/Lace/toolchain"),
            ("ProgramFiles", "Lace/toolchain"),
        ] {
            if let Some(base) = env_path(var) {
                dirs.push(base.join(sub));
            }
        }
    } else {
        if let Some(data) = env_path("XDG_DATA_HOME") {
            dirs.push(data.join("lace/toolchain"));
        }
        if let Some(home) = env_path("HOME") {
            dirs.push(home.join(".local/share/lace/toolchain"));
        }
        dirs.dedup();
    }
    dirs
}

fn env_path(var: &str) -> Option<Utf8PathBuf> {
    std::env::var(var)
        .ok()
        .filter(|v| !v.is_empty())
        .map(Utf8PathBuf::from)
}

/// O `lace` (ou `lace.exe`) do `PATH`.
fn lace_on_path() -> Option<Utf8PathBuf> {
    let name = if cfg!(windows) { "lace.exe" } else { "lace" };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter_map(|dir| Utf8PathBuf::from_path_buf(dir.join(name)).ok())
        .find(|candidate| candidate.is_file())
}

fn current_exe() -> Option<Utf8PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
}

/// O compilador do Verilator declarado nas preferências ou em
/// `LACE_COMPILER`, com a regra da CLI.
fn apply_compiler(toolchain: Toolchain, settings: &Settings) -> (Toolchain, Option<IpcError>) {
    let declared = settings
        .compiler_dir
        .clone()
        .filter(|d| !d.is_empty())
        .or_else(|| {
            std::env::var("LACE_COMPILER")
                .ok()
                .filter(|d| !d.is_empty())
        });
    let Some(dir) = declared else {
        return (toolchain, None);
    };
    let dir = Utf8PathBuf::from(dir);
    let found = if cfg!(windows) {
        SystemCompiler::in_msys2(&dir)
    } else {
        SystemCompiler::in_dir(dir.clone())
    };
    match found {
        Some(compiler) => (toolchain.with_system_compiler(Some(compiler)), None),
        None => (
            toolchain.with_system_compiler(None),
            Some(IpcError::new(
                "compiler_invalid",
                format!("The compiler directory {dir} is missing perl, make or a C++ compiler"),
            )),
        ),
    }
}

/// O `lace` da instalação do bundle: `<instalação>/bin/lace`. É ele que o
/// Studio chama para o que só a CLI faz (instalar componentes, procurar
/// atualização).
pub fn lace_cli(toolchain: &Toolchain) -> Option<Utf8PathBuf> {
    let name = if cfg!(windows) { "lace.exe" } else { "lace" };
    let candidate = toolchain.root().parent()?.join("bin").join(name);
    candidate.is_file().then_some(candidate)
}

/// O que a tela de ferramentas mostra: o equivalente do `lace tools`.
#[derive(Debug, Clone, Serialize)]
pub struct ToolchainInfo {
    /// O bundle abriu.
    pub found: bool,
    /// Por que não abriu.
    pub error: Option<IpcError>,
    /// Como foi achado.
    pub origin: Option<Origin>,
    /// A pasta do bundle.
    pub root: Option<Utf8PathBuf>,
    /// A versão do bundle.
    pub bundle: Option<String>,
    /// A plataforma do bundle.
    pub platform: Option<String>,
    /// Os componentes instalados.
    pub components: Vec<BundleComponent>,
    /// Os componentes que o Lace conhece e que não estão instalados.
    pub not_installed: Vec<String>,
    /// Cada ferramenta.
    pub tools: Vec<ToolStatus>,
    /// O compilador do Verilator.
    pub system_compiler: Option<SystemCompiler>,
    /// O compilador declarado não serve.
    pub compiler_error: Option<IpcError>,
    /// A CLI `lace` da instalação.
    pub lace_cli: Option<Utf8PathBuf>,
    /// O cliente web do Surfer (`surfer-aurora/web`), para a onda numa aba;
    /// `None` num bundle que não o traz.
    pub surfer_web: Option<Utf8PathBuf>,
}

/// Uma ferramenta do bundle.
#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    /// O nome do executável (`iverilog`, `cmmcomp`).
    pub name: String,
    /// O componente que a traz; `None` para o Perl do sistema.
    pub component: Option<String>,
    /// O executável, quando disponível.
    pub path: Option<Utf8PathBuf>,
    /// Vem do sistema, não do bundle.
    pub system: bool,
    /// Por que não está disponível.
    pub error: Option<IpcError>,
}

/// Monta o [`ToolchainInfo`] sem falhar: um bundle ausente vira `found:
/// false` com o erro.
pub fn info(settings: &Settings) -> ToolchainInfo {
    let located = match resolve(settings) {
        Ok(located) => located,
        Err(error) => {
            return ToolchainInfo {
                found: false,
                error: Some(error),
                origin: None,
                root: None,
                bundle: None,
                platform: None,
                components: Vec::new(),
                not_installed: component::ALL.iter().map(|c| (*c).to_owned()).collect(),
                tools: Vec::new(),
                system_compiler: None,
                compiler_error: None,
                lace_cli: None,
                surfer_web: None,
            };
        }
    };
    let toolchain = &located.toolchain;
    let manifest = toolchain.manifest();
    let bundled = toolchain.system_compiler().is_some_and(|c| c.bundled);
    let tools = Tool::all()
        .iter()
        .map(|&tool| {
            let (path, error) = match toolchain.tool(tool) {
                Ok(path) => (Some(path), None),
                Err(error) => (None, Some(IpcError::from(error))),
            };
            ToolStatus {
                name: tool.binary_name().to_owned(),
                component: tool.component().map(str::to_owned),
                path,
                system: tool.is_system() && !bundled,
                error,
            }
        })
        .collect();
    ToolchainInfo {
        found: true,
        error: None,
        origin: Some(located.origin),
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
        compiler_error: located.compiler_error.clone(),
        lace_cli: lace_cli(toolchain),
        surfer_web: toolchain.surfer_web_dir().ok(),
    }
}

/// O bundle, para as operações: o erro de não achar vira erro do comando.
pub fn require(settings: &Settings) -> IpcResult<Toolchain> {
    Ok(resolve(settings)?.toolchain)
}

/// A pasta `bin/` da instalação, para pôr no `PATH` do terminal de shell e
/// o usuário poder digitar `lace` nele.
pub fn install_bin_dir(settings: &Settings) -> Option<Utf8PathBuf> {
    let located = resolve(settings).ok()?;
    lace_cli(&located.toolchain).and_then(|cli| cli.parent().map(Utf8Path::to_owned))
}
