//! Comandos do aplicativo: versão, preferências, recentes e abrir a onda.

use camino::Utf8PathBuf;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::error::{IpcError, IpcResult};
use crate::flows::{self, WaveOpened};
use crate::settings::{RecentProject, Settings};
use crate::state::{AppState, blocking};
use crate::toolchain;

/// O que a tela "Sobre" mostra.
#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    /// `Lace Studio`.
    pub name: String,
    /// A versão do Studio.
    pub version: String,
    /// O sistema e a arquitetura (`linux-x86_64`).
    pub os: String,
    /// A plataforma do Lace (`linux-x64`), se o Lace tem bundle para ela.
    pub lace_platform: Option<&'static str>,
    /// Onde ficam as preferências.
    pub config_dir: Option<Utf8PathBuf>,
    /// A versão do Tauri com que o Studio foi compilado.
    pub tauri_version: &'static str,
    /// A versão do motor de páginas do sistema (WebKitGTK, WebView2,
    /// WKWebView), se ele informar.
    pub webview_version: Option<String>,
    /// Build de desenvolvimento (`npm run tauri dev`), não de distribuição.
    pub debug: bool,
}

/// A versão e a plataforma.
#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        name: app.package_info().name.clone(),
        version: app.package_info().version.to_string(),
        os: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        lace_platform: lace_core::Platform::current().map(|p| p.as_str()),
        config_dir: app
            .path()
            .app_config_dir()
            .ok()
            .and_then(|d| Utf8PathBuf::from_path_buf(d).ok()),
        tauri_version: tauri::VERSION,
        webview_version: tauri::webview_version().ok(),
        debug: cfg!(debug_assertions),
    }
}

/// As preferências.
#[tauri::command]
pub fn settings_get(app: AppHandle) -> Settings {
    app.state::<AppState>().settings.get()
}

/// Troca as preferências e devolve como ficaram. A lista de recentes que
/// vier junto é ignorada.
#[tauri::command]
pub async fn settings_set(app: AppHandle, settings: Settings) -> IpcResult<Settings> {
    blocking(app, move |_, state| state.settings.set(settings)).await
}

/// Um projeto recente e se ele ainda existe no disco.
#[derive(Debug, Clone, Serialize)]
pub struct RecentStatus {
    /// O projeto.
    #[serde(flatten)]
    pub project: RecentProject,
    /// O `.spf` ainda existe.
    pub exists: bool,
}

/// Os projetos recentes, do mais novo para o mais antigo.
#[tauri::command]
pub async fn recent_projects(app: AppHandle) -> IpcResult<Vec<RecentStatus>> {
    blocking(app, |_, state| {
        Ok(state
            .settings
            .get()
            .recent_projects
            .into_iter()
            .map(|project| RecentStatus {
                exists: std::path::Path::new(&project.spf).is_file(),
                project,
            })
            .collect())
    })
    .await
}

/// Tira um projeto da lista de recentes (não mexe no disco).
#[tauri::command]
pub async fn recent_forget(app: AppHandle, spf: String) -> IpcResult<()> {
    blocking(app, move |_, state| state.settings.forget_recent(&spf)).await
}

/// Abre uma onda no surfer-aurora (`lace wave`). Com `path`, aquele arquivo;
/// sem, a da simulação do processador ou, sem processador, a do projeto.
#[tauri::command]
pub async fn wave_open(
    app: AppHandle,
    processor: Option<String>,
    path: Option<String>,
) -> IpcResult<WaveOpened> {
    blocking(app, move |_, state| {
        let settings = state.settings.get();
        let waveform = match path {
            Some(path) => Utf8PathBuf::from(path),
            None => {
                let project = state.project()?;
                let processor = processor
                    .as_deref()
                    .map(|name| project.require_processor(name))
                    .transpose()?;
                lace_core::waveform_path(&project, processor)?
            }
        };
        if !waveform.is_file() {
            return Err(IpcError::new(
                "no_waveform",
                format!("Waveform {waveform} does not exist yet; simulate first"),
            ));
        }
        flows::open_wave(&toolchain::require(&settings)?, &waveform)
    })
    .await
}

/// Escreve no log do backend uma mensagem da interface: os erros de
/// JavaScript e os `console.error` aparecem no mesmo terminal que o log do
/// Rust (`RUST_LOG`), sem precisar do inspetor da WebView.
#[tauri::command]
pub fn log_frontend(level: String, message: String) {
    match level.as_str() {
        "error" => tracing::error!(target: "lace_studio::ui", "{message}"),
        "warn" => tracing::warn!(target: "lace_studio::ui", "{message}"),
        _ => tracing::info!(target: "lace_studio::ui", "{message}"),
    }
}

/// O roteiro de fumaça da variável `LACE_STUDIO_SMOKE`, só na build de
/// desenvolvimento (`src/dev/smoke.ts`, docs/DEVELOPMENT.md). Na de release,
/// sempre `None`.
#[tauri::command]
pub fn dev_smoke_script() -> Option<String> {
    if cfg!(debug_assertions) {
        std::env::var("LACE_STUDIO_SMOKE")
            .ok()
            .filter(|s| !s.is_empty())
    } else {
        None
    }
}
