//! Abertura de ondas no Surfer.
//!
//! Como a AURORA (`js/compilation/abrir_onda.ts`, `main/ipc/compile.ts`):
//! `surfer <onda> [-c <comandos.sucl> | -s <estado.surf.ron>]`, destacado,
//! com CWD na pasta da onda. O arquivo de layout é opcional; sem ele o Surfer
//! abre a onda sem sinais selecionados.
//!
//! A AURORA também gera o layout (`.surf.ron`) e tabelas de tradução de
//! opcode e de linha de fonte para o Surfer. Isso ainda não está no Solar.

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::error::{Result, SolarError};
use crate::process::{self, GUI_ENV, RunningProcess};
use crate::toolchain::{Tool, Toolchain};

/// Como abrir a onda.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ViewerOptions {
    /// Arquivo de comandos (`.sucl`, passado com `-c`) ou de estado
    /// (`.surf.ron`, com `-s`).
    pub layout: Option<Utf8PathBuf>,
}

/// Abre `waveform` (VCD, FST ou GHW) no Surfer e retorna sem esperar. O processo continua
/// aberto depois que o [`RunningProcess`] sai de escopo; feche com
/// [`RunningProcess::kill`]. A saída do Surfer vai para
/// `<onda>.surfer.log`, ao lado da onda.
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
/// use solar_core::{Toolchain, ViewerOptions, open_waveform};
/// # let toolchain = Toolchain::open("/opt/solar/toolchain")?;
///
/// let wave = camino::Utf8Path::new("/p/.solar/Temp/soma/soma_tb.vcd");
/// let mut surfer = open_waveform(&toolchain, wave, &ViewerOptions::default())?;
/// surfer.ensure_started(Duration::from_millis(1500))?;
/// # Ok::<(), solar_core::SolarError>(())
/// ```
///
/// # Erros
///
/// - [`SolarError::InvalidProject`] se a onda ou o layout não existirem;
/// - [`SolarError::ComponentMissing`] / [`SolarError::ToolchainIncomplete`]
///   se o Surfer não estiver na toolchain;
/// - [`SolarError::Spawn`] se o Surfer não puder ser executado.
pub fn open_waveform(
    toolchain: &Toolchain,
    waveform: &Utf8Path,
    options: &ViewerOptions,
) -> Result<RunningProcess> {
    if !waveform.is_file() {
        return Err(SolarError::InvalidProject {
            path: waveform.to_owned(),
            reason: "a onda não existe; simule antes".into(),
        });
    }
    let dir = waveform.parent().expect("arquivo tem pasta").to_owned();
    let mut invocation = toolchain
        .invocation(Tool::Surfer, &dir)?
        .path_arg(waveform)
        .inherit(GUI_ENV);
    if let Some(layout) = &options.layout {
        if !layout.is_file() {
            return Err(SolarError::InvalidProject {
                path: layout.clone(),
                reason: "o arquivo de layout não existe".into(),
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
    let log = dir.join(format!(
        "{}.surfer.log",
        waveform.file_name().expect("tem nome")
    ));
    tracing::info!(%waveform, "abrindo no Surfer");
    process::spawn(&invocation, &log)
}
