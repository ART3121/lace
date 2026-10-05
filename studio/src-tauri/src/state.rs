//! O estado do aplicativo, guardado pelo Tauri (`app.manage`) e lido pelos
//! comandos com `app.state::<AppState>()`.
//!
//! O Studio guarda pouco: o caminho do `.spf` aberto, as preferências, a
//! operação que está rodando, os terminais e o vigia de arquivos. O projeto
//! em si é reaberto do disco a cada comando (`Project::open`), como a CLI
//! faz a cada execução: o `.spf` pode mudar por fora (a CLI no terminal, um
//! editor de texto), e um `Project` aberto não percebe isso (API.md, 3.2).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use camino::Utf8PathBuf;
use lace_core::{CancelToken, Project};
use notify::RecommendedWatcher;
use tauri::{AppHandle, Manager};

use crate::error::{IpcError, IpcResult, codes};
use crate::settings::SettingsStore;
use crate::terminal::Terminal;

/// O estado do aplicativo.
pub struct AppState {
    /// Preferências e recentes.
    pub settings: SettingsStore,
    /// O `.spf` do projeto aberto.
    pub project: Mutex<Option<Utf8PathBuf>>,
    /// A operação rodando, se houver. Só uma por vez: duas operações no
    /// mesmo projeto dividiriam o `.lace/Temp`.
    pub job: Mutex<Option<ActiveJob>>,
    /// Os terminais de shell abertos, por identificador.
    pub terminals: Mutex<HashMap<u32, Terminal>>,
    /// O vigia da pasta do projeto.
    pub watcher: Mutex<Option<RecommendedWatcher>>,
    next_id: AtomicU32,
}

/// A operação que está rodando.
pub struct ActiveJob {
    /// O identificador que a interface usa para cancelar.
    pub id: u32,
    /// O pedido de cancelamento.
    pub cancel: CancelToken,
}

impl AppState {
    /// O estado inicial, com as preferências lidas de `settings`.
    pub fn new(settings: SettingsStore) -> Self {
        AppState {
            settings,
            project: Mutex::new(None),
            job: Mutex::new(None),
            terminals: Mutex::new(HashMap::new()),
            watcher: Mutex::new(None),
            next_id: AtomicU32::new(1),
        }
    }

    /// Um identificador novo, para operações e terminais.
    pub fn next_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// O `.spf` aberto, ou o erro `no_project`.
    pub fn spf(&self) -> IpcResult<Utf8PathBuf> {
        self.project
            .lock()
            .expect("project lock")
            .clone()
            .ok_or_else(|| IpcError::new(codes::NO_PROJECT, "No project is open"))
    }

    /// O projeto aberto, relido do disco.
    pub fn project(&self) -> IpcResult<Project> {
        Ok(Project::open(self.spf()?)?)
    }
}

/// Roda `f` numa thread de bloqueio, fora da thread da interface e do
/// executor assíncrono. Todo comando que toca o disco ou roda ferramenta
/// passa por aqui.
pub async fn blocking<T, F>(app: AppHandle, f: F) -> IpcResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppHandle, &AppState) -> IpcResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        f(&app, &state)
    })
    .await
    .map_err(|e| IpcError::new(codes::INTERNAL, format!("Worker thread failed: {e}")))?
}
