//! A escolha dos sinais da onda do projeto e o layout salvo
//! (`lace wave signals`, `lace wave select`, `lace wave --reset-layout`). A
//! onda grava só a escolha na próxima simulação; o layout salvo é o
//! `wave/<testbench>.surf.ron`, onde o Surfer salva com Ctrl+S
//! ([`crate::wave_tab`]).

use lace_core::{Control, WaveSignals};
use tauri::AppHandle;

use crate::error::IpcResult;
use crate::state::blocking;
use crate::toolchain;

/// A árvore de sinais do testbench do projeto, com a escolha gravada
/// (`lace wave signals --json`). Elabora o testbench com o Icarus; uma
/// elaboração que falha vem sem árvore, com os erros.
#[tauri::command]
pub async fn wave_signals(app: AppHandle) -> IpcResult<WaveSignals> {
    blocking(app, |_, state| {
        let project = state.project()?;
        let toolchain = toolchain::require(&state.settings.get())?;
        Ok(lace_core::wave_signals(
            &toolchain,
            &project,
            &Control::default(),
        )?)
    })
    .await
}

/// Grava a escolha de sinais do testbench do projeto (`wave/<testbench>.json`)
/// e devolve como ela ficou. Uma lista vazia apaga a escolha: a onda volta a
/// gravar todos os sinais.
#[tauri::command]
pub async fn wave_selection_set(app: AppHandle, signals: Vec<String>) -> IpcResult<Vec<String>> {
    blocking(app, move |_, state| {
        let project = state.project()?;
        let module = lace_core::wave_testbench(&project)?;
        lace_core::write_selection(&project, &module, &signals)?;
        Ok(lace_core::read_selection(&project, &module)?)
    })
    .await
}

/// Apaga o layout salvo da onda `waveform` (`wave/<testbench>.surf.ron`): na
/// próxima vez ela abre com o layout gerado. `false` se não havia.
#[tauri::command]
pub async fn wave_layout_reset(app: AppHandle, waveform: String) -> IpcResult<bool> {
    blocking(app, move |_, _| {
        let waveform = camino::Utf8PathBuf::from(waveform);
        let Ok(project) = lace_core::Project::discover(&waveform) else {
            return Ok(false);
        };
        let Some(testbench) = lace_core::wave_testbench_of(&waveform)? else {
            return Ok(false);
        };
        Ok(lace_core::reset_saved_layout(&project, &testbench)?)
    })
    .await
}
