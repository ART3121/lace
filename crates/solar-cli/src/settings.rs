//! De onde vem o bundle e o compilador do sistema.
//!
//! O bundle é o instalado ao lado do executável (`<instalação>/toolchain`).
//! `--toolchain <DIR>` (ou `SOLAR_TOOLCHAIN`) aponta para outro bundle, para
//! desenvolvimento e testes; continua precisando ser um bundle, com
//! `bundle.json`.
//!
//! O arquivo de configuração só guarda o que a exceção do Verilator deixa
//! declarar: onde está o compilador do sistema, quando ele não está no local
//! padrão.
//!
//! ```json
//! { "compiler_dir": "D:/msys64" }
//! ```

use anyhow::Context;
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use solar_core::{SystemCompiler, Toolchain};

/// Opções globais de bundle.
#[derive(clap::Args, Debug, Clone)]
pub struct ToolchainArgs {
    /// Outro bundle no lugar do instalado (desenvolvimento) [env: SOLAR_TOOLCHAIN]
    #[arg(
        long,
        global = true,
        help_heading = "Bundle",
        env = "SOLAR_TOOLCHAIN",
        hide_env = true,
        value_name = "DIR"
    )]
    pub toolchain: Option<Utf8PathBuf>,
    /// Arquivo de configuração [env: SOLAR_CONFIG; padrão: ~/.config/solar/config.json]
    #[arg(
        long,
        global = true,
        help_heading = "Bundle",
        env = "SOLAR_CONFIG",
        hide_env = true,
        value_name = "ARQUIVO"
    )]
    pub config: Option<Utf8PathBuf>,
}

/// O conteúdo do arquivo de configuração.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ConfigFile {
    /// Onde está o compilador do sistema para o Verilator: um diretório com
    /// `perl`, `make` e o compilador C++ (Linux e macOS) ou a raiz do MSYS2
    /// (Windows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler_dir: Option<Utf8PathBuf>,
}

impl ConfigFile {
    pub fn load(path: &Utf8Path) -> anyhow::Result<Option<Self>> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(serde_json::from_str(&text).with_context(|| {
                format!("arquivo de configuração inválido: {path}")
            })?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("lendo {path}")),
        }
    }

    pub fn save(&self, path: &Utf8Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("criando {dir}"))?;
        }
        let text = serde_json::to_string_pretty(self)? + "\n";
        std::fs::write(path, text).with_context(|| format!("gravando {path}"))
    }
}

/// O compilador num diretório declarado: raiz do MSYS2 no Windows, diretório
/// com os três programas nos outros sistemas.
pub fn compiler_in(dir: &Utf8Path) -> Option<SystemCompiler> {
    if cfg!(windows) {
        SystemCompiler::in_msys2(dir)
    } else {
        SystemCompiler::in_dir(dir.to_owned())
    }
}

/// Onde fica o arquivo de configuração do usuário.
pub fn default_config_path() -> Option<Utf8PathBuf> {
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
    base.map(|dir| dir.join("solar").join("config.json"))
}

impl ToolchainArgs {
    pub fn config_path(&self) -> anyhow::Result<Utf8PathBuf> {
        match &self.config {
            Some(path) => Ok(path.clone()),
            None => default_config_path()
                .context("não sei onde fica o arquivo de configuração: defina SOLAR_CONFIG"),
        }
    }

    /// O bundle, com o compilador do sistema declarado na configuração, se
    /// houver.
    pub fn resolve(&self) -> anyhow::Result<Toolchain> {
        let toolchain = match &self.toolchain {
            Some(dir) => Toolchain::open(dir)?,
            None => {
                let exe =
                    std::env::current_exe().context("não sei onde está o executável do solar")?;
                let exe = Utf8PathBuf::from_path_buf(exe)
                    .map_err(|p| anyhow::anyhow!("caminho não é UTF-8: {}", p.display()))?;
                Toolchain::locate(&exe)?
            }
        };
        let config = match self.config_path() {
            Ok(path) => ConfigFile::load(&path)?.unwrap_or_default(),
            Err(_) => ConfigFile::default(),
        };
        Ok(match &config.compiler_dir {
            Some(dir) => toolchain.with_system_compiler(compiler_in(dir)),
            None => toolchain,
        })
    }
}

/// Caminho absoluto sem exigir que exista (para gravar na configuração).
pub fn absolute(path: &Utf8Path) -> anyhow::Result<Utf8PathBuf> {
    let absolute = std::path::absolute(path).with_context(|| format!("resolvendo {path}"))?;
    Utf8PathBuf::from_path_buf(absolute)
        .map_err(|p| anyhow::anyhow!("caminho não é UTF-8: {}", p.display()))
}
