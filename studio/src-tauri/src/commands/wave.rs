//! A escolha dos sinais da onda do projeto (`lace wave select`). A onda
//! grava só a escolha na próxima simulação. A árvore de sinais e o layout
//! salvo (`wave.signals`, `wave.reset_layout`) são métodos do protocolo, que
//! a interface chama pelo servidor do Lace ([`crate::server`]).

use tauri::AppHandle;

use crate::error::IpcResult;
use crate::state::blocking;

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
