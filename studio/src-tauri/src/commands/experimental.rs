//! Os recursos experimentais do Lace (`lace_core::experimental`): quais
//! existem, quais estão ligados, ligar e desligar. É a página Experimental
//! das Preferências. O que fica ligado vale também para a CLI: os dois leem
//! o mesmo `experimental.json`, na pasta de configuração do Lace, e não no
//! `settings.json` do Studio.

use camino::Utf8PathBuf;
use lace_core::experimental::{self, Feature, FeatureState};
use serde::Serialize;
use tauri::AppHandle;

use crate::error::IpcResult;
use crate::state::blocking;

/// Os recursos e onde fica o que está ligado, como o `lace experimental
/// --json`.
#[derive(Debug, Clone, Serialize)]
pub struct ExperimentalInfo {
    /// O `experimental.json`; `None` se o sistema não diz onde fica a pasta
    /// de configuração do usuário.
    pub path: Option<Utf8PathBuf>,
    /// Cada recurso e se está ligado.
    pub components: Vec<FeatureState>,
}

fn info() -> ExperimentalInfo {
    ExperimentalInfo {
        path: experimental::settings_path(),
        components: experimental::states(),
    }
}

/// Os recursos experimentais e quais estão ligados, lidos do disco a cada
/// chamada: a CLI pode ter mudado o arquivo com o Studio aberto.
#[tauri::command]
pub async fn experimental_list(app: AppHandle) -> IpcResult<ExperimentalInfo> {
    blocking(app, |_, _| Ok(info())).await
}

/// Liga ou desliga um recurso e devolve como ficaram todos.
#[tauri::command]
pub async fn experimental_set(
    app: AppHandle,
    feature: Feature,
    enabled: bool,
) -> IpcResult<ExperimentalInfo> {
    blocking(app, move |_, _| {
        experimental::set_enabled(feature, enabled)?;
        Ok(info())
    })
    .await
}
