//! Abertura de ondas no Surfer.
//!
//! Como a AURORA (`js/compilation/abrir_onda.ts`, `main/ipc/compile.ts`):
//! `surfer <onda> [-c <comandos.sucl> | -s <estado.surf.ron>]`, destacado,
//! com CWD na pasta da onda. O arquivo de layout é opcional; sem ele o Surfer
//! abre a onda sem sinais selecionados. O log do Surfer não fica ao lado da
//! onda: vai para a pasta temporária do projeto (`.lace/Temp/surfer/`).
//!
//! O layout dos processadores SAPHO (o `.surf.ron` e as tabelas de tradução
//! de opcode e de linha de fonte) sai de [`prepare_wave_layout`](crate::prepare_wave_layout),
//! em `wave_layout.rs`; com ele, o Surfer roda na pasta do layout
//! ([`ViewerOptions::working_dir`]).

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::error::{LaceError, Result};
use crate::paths;
use crate::process::{self, GUI_ENV, RunningProcess};
use crate::project::Project;
use crate::toolchain::{Tool, Toolchain};
use crate::wave_layout::PreparedLayout;

/// A pasta dos logs do Surfer, dentro da pasta temporária do projeto.
const LOG_DIR: &str = "surfer";

/// Como abrir a onda.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ViewerOptions {
    /// Arquivo de comandos (`.sucl`, passado com `-c`) ou de estado
    /// (`.surf.ron`, com `-s`).
    pub layout: Option<Utf8PathBuf>,
    /// A pasta onde o Surfer roda; é nela que ele procura `.surfer/`, com os
    /// tradutores de valor (`.surfer/mappings/`). `None`: a pasta da onda.
    #[serde(default)]
    pub working_dir: Option<Utf8PathBuf>,
}

impl ViewerOptions {
    /// Abre com um layout de [`prepare_wave_layout`](crate::prepare_wave_layout):
    /// o estado dele e a pasta dele, onde estão os tradutores.
    pub fn with_layout(layout: &PreparedLayout) -> Self {
        ViewerOptions {
            layout: Some(layout.state.clone()),
            working_dir: Some(layout.dir.clone()),
        }
    }
}

/// Abre `waveform` (VCD, FST ou GHW) no Surfer e retorna sem esperar. O processo continua
/// aberto depois que o [`RunningProcess`] sai de escopo; feche com
/// [`RunningProcess::kill`]. A saída do Surfer vai para `<onda>.log`
/// (`soma_tb.vcd.log`) em `.lace/Temp/surfer/`, no projeto que contém a
/// onda; com a onda fora de projeto, na pasta de cache do usuário
/// (`$XDG_CACHE_HOME/lace/surfer` ou `~/.cache/lace/surfer` no Linux,
/// `~/Library/Caches/lace/surfer` no macOS, `%TEMP%\lace\surfer` no
/// Windows). Cada abertura da mesma onda sobrescreve o log anterior.
///
/// O Surfer recebe as variáveis de ambiente de que uma aplicação gráfica
/// precisa (`DISPLAY`, `WAYLAND_DISPLAY`, `HOME`, `XDG_*` no Linux;
/// `USERPROFILE`, `APPDATA` no Windows) e nenhuma outra.
///
/// Sem display, o Surfer morre logo depois de abrir. A chamada retorna `Ok`
/// mesmo assim, porque o processo chegou a iniciar; para perceber, confira
/// com [`RunningProcess::ensure_started`], que vira erro com o fim do log:
///
/// ```no_run
/// use std::time::Duration;
/// use lace_core::{Toolchain, ViewerOptions, open_waveform};
/// # let toolchain = Toolchain::open("/opt/lace/toolchain")?;
///
/// let wave = camino::Utf8Path::new("/p/.lace/Temp/soma/soma_tb.vcd");
/// let mut surfer = open_waveform(&toolchain, wave, &ViewerOptions::default())?;
/// surfer.ensure_started(Duration::from_millis(1500))?;
/// # Ok::<(), lace_core::LaceError>(())
/// ```
///
/// # Erros
///
/// - [`LaceError::InvalidProject`] se a onda ou o layout não existirem;
/// - [`LaceError::ComponentMissing`] / [`LaceError::ToolchainIncomplete`]
///   se o Surfer não estiver na toolchain;
/// - [`LaceError::Io`] se a pasta do log não puder ser criada;
/// - [`LaceError::Spawn`] se o Surfer não puder ser executado.
pub fn open_waveform(
    toolchain: &Toolchain,
    waveform: &Utf8Path,
    options: &ViewerOptions,
) -> Result<RunningProcess> {
    if !waveform.is_file() {
        return Err(LaceError::InvalidProject {
            path: waveform.to_owned(),
            reason: "The waveform does not exist; simulate first".into(),
        });
    }
    let dir = match &options.working_dir {
        Some(dir) => dir.clone(),
        None => waveform
            .parent()
            .expect("a file has a parent folder")
            .to_owned(),
    };
    let mut invocation = toolchain
        .invocation(Tool::Surfer, &dir)?
        .path_arg(waveform)
        .inherit(GUI_ENV);
    if let Some(layout) = &options.layout {
        if !layout.is_file() {
            return Err(LaceError::InvalidProject {
                path: layout.clone(),
                reason: "The layout file does not exist".into(),
            });
        }
        let flag = if layout
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("sucl"))
        {
            "-c"
        } else {
            "-s"
        };
        invocation = invocation.arg(flag).path_arg(layout);
    }
    let log = log_path(waveform)?;
    tracing::info!(%waveform, %log, "Opening in Surfer");
    process::spawn(&invocation, &log)
}

/// A pasta do Surfer para `waveform`, já criada: `.lace/Temp/surfer/` do
/// projeto que contém a onda, ou a de cache do usuário fora de projeto. Os
/// logs e os layouts gerados ficam nela.
pub(crate) fn work_dir(waveform: &Utf8Path) -> Result<Utf8PathBuf> {
    let dir = match Project::discover(waveform) {
        Ok(project) => project.temp_dir().join(LOG_DIR),
        Err(_) => user_log_dir()?,
    };
    std::fs::create_dir_all(&dir).map_err(LaceError::io("Creating log folder", &dir))?;
    Ok(dir)
}

/// Onde vai o log do Surfer para `waveform`, com a pasta já criada.
fn log_path(waveform: &Utf8Path) -> Result<Utf8PathBuf> {
    Ok(work_dir(waveform)?.join(format!("{}.log", waveform.file_name().expect("tem nome"))))
}

/// A pasta dos logs do Surfer fora de projeto: a de cache do usuário, que
/// é só dele (o `/tmp` do Linux é de todos).
fn user_log_dir() -> Result<Utf8PathBuf> {
    let home = || std::env::var_os("HOME").filter(|h| !h.is_empty());
    let base = if cfg!(windows) {
        None
    } else if cfg!(target_os = "macos") {
        home().map(|h| std::path::PathBuf::from(h).join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home().map(|h| std::path::PathBuf::from(h).join(".cache")))
    };
    let base = base.unwrap_or_else(std::env::temp_dir);
    paths::to_utf8(base.join("lace").join(LOG_DIR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_goes_to_the_hidden_temp_folder_of_the_project() {
        let guard = tempfile::tempdir().unwrap();
        let dir = paths::canonicalize(Utf8Path::from_path(guard.path()).unwrap()).unwrap();
        let project = Project::create(&dir, "p").unwrap();
        // Uma onda numa pasta do usuário e a de um processador.
        let waves = [
            project.root().join("rtl").join("a_tb.vcd"),
            project.temp_dir().join("soma").join("soma_tb.vcd"),
        ];
        for wave in &waves {
            std::fs::create_dir_all(wave.parent().unwrap()).unwrap();
            std::fs::write(wave, "").unwrap();
            let log = log_path(wave).unwrap();
            assert_eq!(
                log.parent(),
                Some(project.temp_dir().join(LOG_DIR).as_path())
            );
            assert!(log.parent().unwrap().is_dir());
            assert_eq!(
                log.file_name(),
                Some(format!("{}.log", wave.file_name().unwrap()).as_str())
            );
        }
    }

    #[test]
    fn outside_a_project_the_log_goes_to_the_user_cache() {
        let dir = user_log_dir().unwrap();
        assert!(dir.is_absolute(), "{dir}");
        assert!(dir.ends_with(Utf8Path::new("lace").join(LOG_DIR)), "{dir}");
    }
}
