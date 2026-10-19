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

use camino::{Utf8Path, Utf8PathBuf};
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
    /// As sessões do assistente abertas.
    assistants: crate::assistant::Assistants,
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
            assistants: crate::assistant::Assistants::default(),
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

    /// O projeto aberto, relido do disco ([`open_project`]).
    pub fn project(&self) -> IpcResult<Project> {
        open_project(&self.spf()?)
    }
}

/// Abre o projeto de `spf`. Se ele não abre porque o `.spf` não existe mais
/// (apagado depois de aberto), o erro é `spf_missing`, e não o
/// `invalid_project` do Core: a interface avisa que sem ele não há operação
/// de projeto. A conferência vem depois da tentativa do Core, que já repete
/// a abertura enquanto o arquivo some por um instante numa gravação.
pub fn open_project(spf: &Utf8Path) -> IpcResult<Project> {
    Project::open(spf).map_err(|error| {
        if spf.exists() {
            IpcError::from(error)
        } else {
            IpcError::new(
                codes::SPF_MISSING,
                format!(
                    "The project file {spf} does not exist anymore: without it Lace cannot build, simulate or change the project"
                ),
            )
        }
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deleted_spf_is_spf_missing() {
        let dir = tempfile::tempdir().unwrap();
        let parent = Utf8PathBuf::from_path_buf(dir.path().to_owned()).unwrap();
        let spf = Project::create(&parent, "p").unwrap().spf_path().to_owned();
        assert!(open_project(&spf).is_ok());
        std::fs::remove_file(&spf).unwrap();
        assert_eq!(open_project(&spf).unwrap_err().code, codes::SPF_MISSING);
    }

    #[test]
    fn an_unreadable_spf_keeps_the_core_error() {
        let dir = tempfile::tempdir().unwrap();
        let parent = Utf8PathBuf::from_path_buf(dir.path().to_owned()).unwrap();
        let spf = Project::create(&parent, "p").unwrap().spf_path().to_owned();
        std::fs::write(&spf, "{ isto não é um .spf").unwrap();
        assert_ne!(open_project(&spf).unwrap_err().code, codes::SPF_MISSING);
    }
}
