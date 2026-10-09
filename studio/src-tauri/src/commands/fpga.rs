//! Comandos da placa FPGA: as placas conhecidas, o `fpga.json` do projeto e
//! a conferência das ligações (`lace fpga boards` e `lace fpga check`). A
//! compilação para a placa é um fluxo (`flows.rs`, `fpga_build`), que roda
//! as ferramentas com saída ao vivo e cancelamento.

use lace_core::fpga::{self, BitstreamState, Board, FpgaConfig, Prepared};
use lace_core::verilog::ModuleInterface;
use tauri::AppHandle;

use crate::error::IpcResult;
use crate::state::blocking;
use crate::toolchain;

/// As placas conhecidas, com os sinais e os pinos.
#[tauri::command]
pub fn fpga_boards() -> IpcResult<Vec<Board>> {
    Ok(fpga::boards()?)
}

/// O `fpga.json` do projeto aberto; `null` se ele ainda não existe.
#[tauri::command]
pub async fn fpga_config(app: AppHandle) -> IpcResult<Option<FpgaConfig>> {
    blocking(app, |_, state| {
        let project = state.project()?;
        if !FpgaConfig::path(project.root()).is_file() {
            return Ok(None);
        }
        Ok(Some(FpgaConfig::read(project.root())?))
    })
    .await
}

/// Grava o `fpga.json` do projeto aberto.
#[tauri::command]
pub async fn fpga_config_set(app: AppHandle, config: FpgaConfig) -> IpcResult<()> {
    blocking(app, move |_, state| {
        let project = state.project()?;
        Ok(config.write(project.root())?)
    })
    .await
}

/// O módulo que vai para a placa e as portas dele (o `top` do `fpga.json`,
/// senão o do projeto), para montar as ligações antes de estarem certas.
#[tauri::command]
pub async fn fpga_top(app: AppHandle, top: Option<String>) -> IpcResult<ModuleInterface> {
    blocking(app, move |_, state| {
        let project = state.project()?;
        let toolchain = toolchain::require(&state.settings.get()).ok();
        Ok(fpga::top_interface(
            toolchain.as_ref(),
            &project,
            top.as_deref(),
        )?)
    })
    .await
}

/// Confere o `fpga.json` contra a placa e o topo (`lace fpga check`): as
/// ligações bit a bit, as notas e o topo da placa.
#[tauri::command]
pub async fn fpga_check(app: AppHandle) -> IpcResult<Prepared> {
    blocking(app, |_, state| {
        let project = state.project()?;
        let toolchain = toolchain::require(&state.settings.get()).ok();
        Ok(fpga::prepare(toolchain.as_ref(), &project)?)
    })
    .await
}

/// Os módulos do projeto que podem ir para a placa.
#[tauri::command]
pub async fn fpga_modules(app: AppHandle) -> IpcResult<Vec<String>> {
    blocking(app, |_, state| {
        let project = state.project()?;
        let toolchain = toolchain::require(&state.settings.get()).ok();
        Ok(fpga::modules(toolchain.as_ref(), &project)?)
    })
    .await
}

/// O arquivo de gravação da placa do `fpga.json`: se existe e por que não
/// serve para gravar (`lace fpga program` confere o mesmo antes de gravar).
#[tauri::command]
pub async fn fpga_status(app: AppHandle) -> IpcResult<BitstreamState> {
    blocking(app, |_, state| {
        let project = state.project()?;
        let toolchain = toolchain::require(&state.settings.get())?;
        Ok(fpga::bitstream_state(&toolchain, &project)?)
    })
    .await
}

/// Os cabos de gravação ligados (`lace fpga program --list`). Roda o
/// `quartus_pgm -l`, que leva cerca de um segundo.
#[tauri::command]
pub async fn fpga_cables(app: AppHandle) -> IpcResult<Vec<String>> {
    blocking(app, |_, state| {
        let toolchain = toolchain::require(&state.settings.get())?;
        Ok(fpga::cables(&toolchain)?)
    })
    .await
}
