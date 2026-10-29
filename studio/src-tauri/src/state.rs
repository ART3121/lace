//! O estado do aplicativo, guardado pelo Tauri (`app.manage`) e lido pelos
//! comandos com `app.state::<AppState>()`.
//!
//! O Studio guarda pouco: o caminho do `.spf` aberto, as preferências, a
//! operação que está rodando, os terminais, o vigia de arquivos e o servidor
//! do Lace. O projeto em si é do servidor, que o reabre do disco a cada
//! pedido, como a CLI faz a cada execução: o `.spf` pode mudar por fora (a
//! CLI no terminal, um editor de texto).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use camino::Utf8PathBuf;
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
    /// mesmo projeto dividiriam o `.aurora8/Temp`.
    pub job: Mutex<Option<ActiveJob>>,
    /// Os terminais de shell abertos, por identificador.
    pub terminals: Mutex<HashMap<u32, Terminal>>,
    /// O vigia da pasta do projeto.
    pub watcher: Mutex<Option<RecommendedWatcher>>,
    /// As sessões do assistente abertas.
    assistants: crate::assistant::Assistants,
    /// O servidor do Lace (`aurora serve`) que atende os métodos do protocolo.
    pub server: crate::server::AuroraServer,
    next_id: AtomicU32,
}

/// A operação que está rodando.
pub struct ActiveJob {
    /// O identificador que a interface usa para cancelar.
    pub id: u32,
    /// O pedido ao servidor do Lace que a operação fez, depois que ele sai:
    /// é para ele que vai o `$/cancel`.
    pub request: Option<u64>,
    /// Cancelar foi pedido. Se o pedido ainda não saiu, ele é cancelado
    /// assim que sair.
    pub cancelled: bool,
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
            assistants: crate::assistant::Assistants::default(),
            server: crate::server::AuroraServer::default(),
            next_id: AtomicU32::new(1),
        }
    }

    /// As sessões do assistente.
    pub fn assistants(&self) -> &crate::assistant::Assistants {
        &self.assistants
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
