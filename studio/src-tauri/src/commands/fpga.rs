//! O módulo que vai para a placa FPGA e as portas dele. As placas, o
//! `fpga.json` e a conferência das ligações são métodos do protocolo
//! (`fpga.*`), que a interface chama pelo servidor do Lace
//! ([`crate::server`]); a compilação para a placa é um fluxo (`fpga_build`).
//!
//! A placa FPGA é um recurso experimental do Lace: com ela desligada, o
//! comando daqui, os métodos `fpga.*` e os fluxos dela respondem
//! `experimental_disabled`, como o `lace fpga` da CLI.

use lace_core::experimental::{self, Feature};
use lace_core::fpga;
use lace_core::verilog::ModuleInterface;
use tauri::AppHandle;

use crate::error::IpcResult;
use crate::state::blocking;
use crate::toolchain;

/// A placa FPGA está ligada (Preferências > Experimental, ou `lace
/// experimental enable fpga`).
fn require() -> IpcResult<()> {
    Ok(experimental::require(Feature::Fpga)?)
}

/// O módulo que vai para a placa e as portas dele (o `top` do `fpga.json`,
/// senão o do projeto), para montar as ligações antes de estarem certas.
#[tauri::command]
pub async fn fpga_top(app: AppHandle, top: Option<String>) -> IpcResult<ModuleInterface> {
    blocking(app, move |_, state| {
        require()?;
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
