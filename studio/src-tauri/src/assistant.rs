//! O assistente: uma sessão do Claude Code com as ferramentas do `lace mcp`.
//! É o recurso experimental `assistant` do Lace: desligado, a vista nem
//! aparece, e `assistant_start` recusa com `experimental_disabled`.
//!
//! (`lace_agent`), aberta pela vista Assistente. Cada sessão é um processo
//! filho; os eventos dele chegam à interface por um `Channel`, e a sessão
//! vive até a interface pedir para fechar ou o Studio sair.
//!
//! O que o assistente pode fazer vem das preferências: só as ferramentas do
//! Lace e ler arquivos, ou também editar os arquivos do projeto.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use lace_agent::locate::Claude;
use lace_agent::{AgentEvent, Permission, Session, SessionOptions};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::error::{IpcError, IpcResult, codes};
use crate::settings::Settings;
use crate::state::{AppState, blocking};
use crate::toolchain;

/// Quanto esperar o Claude Code sair sozinho antes de matá-lo.
const STOP_WAIT: Duration = Duration::from_secs(3);

/// As sessões abertas, por identificador.
#[derive(Default)]
pub struct Assistants {
    sessions: Mutex<HashMap<u32, Session>>,
}

/// O que a vista precisa saber antes de abrir uma sessão.
#[derive(Debug, Clone, Serialize)]
pub struct AssistantInfo {
    /// O Claude Code achado (o caminho para mostrar), ou `null`.
    pub claude: Option<String>,
    /// A versão dele (`claude --version`), se respondeu.
    pub version: Option<String>,
    /// O `lace` que vai servir as ferramentas, ou `null` se não há.
    pub lace: Option<String>,
    /// O projeto padrão das ferramentas (o `.spf` aberto).
    pub project: Option<String>,
}

/// Onde o Claude Code e o `lace` estão, com as preferências de agora.
#[tauri::command]
pub async fn assistant_info(app: AppHandle) -> IpcResult<AssistantInfo> {
    blocking(app, |_, state| {
        let settings = state.settings.get();
        let claude = find_claude(&settings);
        Ok(AssistantInfo {
            version: claude.as_ref().and_then(lace_agent::version),
            claude: claude.map(|c| c.display()),
            lace: lace_for(&settings).ok().map(|p| p.to_string()),
            project: state.spf().ok().map(|p| p.to_string()),
        })
    })
    .await
}

/// Abre uma sessão e devolve o identificador. Os eventos chegam por
/// `channel`, começando por `ready` (ou `exited`, se o Claude Code não
/// abriu). Com `resume`, retoma a conversa daquele `session_id`.
#[tauri::command]
pub async fn assistant_start(
    app: AppHandle,
    resume: Option<String>,
    channel: Channel<AgentEvent>,
) -> IpcResult<u32> {
    blocking(app, move |app, state| {
        lace_core::experimental::require(lace_core::experimental::Feature::Assistant)?;
        let settings = state.settings.get();
        let claude = find_claude(&settings).ok_or_else(|| {
            IpcError::new(
                codes::ASSISTANT,
                "Claude Code was not found; install it or set its path in Preferences",
            )
        })?;
        let lace = lace_for(&settings)?;
        let spf = state.spf().ok();
        let cwd = spf
            .as_deref()
            .and_then(Utf8Path::parent)
            .map(Utf8Path::to_owned)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .and_then(|h| Utf8PathBuf::from_path_buf(h.into()).ok())
            })
            .unwrap_or_else(|| Utf8PathBuf::from("."));
        let options = SessionOptions {
            claude,
            cwd,
            lace,
            toolchain: toolchain_env(&settings),
            project: spf,
            permission: if settings.assistant_allow_edits {
                Permission::Edits
            } else {
                Permission::LaceOnly
            },
            allow_update: false,
            model: settings.assistant_model.clone().filter(|m| !m.is_empty()),
            resume,
            system_prompt: None,
        };
        let (session, events) = Session::start(&options).map_err(agent_error)?;
        let id = state.next_id();
        state
            .assistants()
            .sessions
            .lock()
            .expect("assistants lock")
            .insert(id, session);
        let app = app.clone();
        std::thread::Builder::new()
            .name(format!("assistant-{id}"))
            .spawn(move || {
                for event in events {
                    let last = matches!(event, AgentEvent::Exited { .. });
                    if channel.send(event).is_err() {
                        break;
                    }
                    if last {
                        break;
                    }
                }
                // O processo saiu: a sessão sai da lista.
                let state = app.state::<AppState>();
                state
                    .assistants()
                    .sessions
                    .lock()
                    .expect("assistants lock")
                    .remove(&id);
            })
            .map_err(|error| {
                IpcError::new(
                    codes::INTERNAL,
                    format!("Could not start the assistant thread: {error}"),
                )
            })?;
        Ok(id)
    })
    .await
}

/// Manda uma mensagem da pessoa à sessão `id`.
#[tauri::command]
pub fn assistant_send(app: AppHandle, id: u32, text: String) -> IpcResult<()> {
    with_session(&app, id, |session| session.send(&text).map_err(agent_error))
}

/// Interrompe o turno em andamento da sessão `id`.
#[tauri::command]
pub fn assistant_interrupt(app: AppHandle, id: u32) -> IpcResult<()> {
    with_session(&app, id, |session| session.interrupt().map_err(agent_error))
}

/// Encerra a sessão `id`. Sem ela, nada acontece.
#[tauri::command]
pub async fn assistant_stop(app: AppHandle, id: u32) -> IpcResult<()> {
    blocking(app, move |_, state| {
        let session = state
            .assistants()
            .sessions
            .lock()
            .expect("assistants lock")
            .remove(&id);
        if let Some(session) = session {
            session.stop(STOP_WAIT);
        }
        Ok(())
    })
    .await
}

/// Encerra todas as sessões: ao sair do Studio.
pub fn stop_all(app: &AppHandle) {
    let state = app.state::<AppState>();
    let sessions: Vec<Session> = state
        .assistants()
        .sessions
        .lock()
        .expect("assistants lock")
        .drain()
        .map(|(_, s)| s)
        .collect();
    for session in sessions {
        session.stop(STOP_WAIT);
    }
}

fn with_session<T>(
    app: &AppHandle,
    id: u32,
    f: impl FnOnce(&Session) -> IpcResult<T>,
) -> IpcResult<T> {
    let state = app.state::<AppState>();
    let sessions = state.assistants().sessions.lock().expect("assistants lock");
    match sessions.get(&id) {
        Some(session) => f(session),
        None => Err(IpcError::new(
            codes::ASSISTANT,
            "The assistant session has ended",
        )),
    }
}

fn agent_error(error: lace_agent::AgentError) -> IpcError {
    IpcError::new(error.code(), error.to_string())
}

/// O Claude Code: a preferência `assistant_claude`, senão a busca.
fn find_claude(settings: &Settings) -> Option<Claude> {
    let configured = settings
        .assistant_claude
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .map(Utf8Path::new);
    lace_agent::find_claude(configured)
}

/// O `lace` que serve as ferramentas: a preferência `assistant_lace`, senão
/// o da instalação do bundle.
fn lace_for(settings: &Settings) -> IpcResult<Utf8PathBuf> {
    if let Some(path) = settings
        .assistant_lace
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        let path = Utf8PathBuf::from(path);
        if !path.is_file() {
            return Err(IpcError::new(
                codes::ASSISTANT,
                format!("The lace executable for the assistant does not exist: {path}"),
            ));
        }
        return Ok(path);
    }
    let toolchain = toolchain::require(settings)?;
    toolchain::lace_cli(&toolchain).ok_or_else(|| {
        IpcError::new(
            codes::ASSISTANT,
            format!(
                "The lace command of the installation at {} was not found",
                toolchain.root()
            ),
        )
    })
}

/// O bundle que o `lace mcp` deve usar, quando não é o da instalação dele:
/// a preferência ou a variável de ambiente.
fn toolchain_env(settings: &Settings) -> Option<Utf8PathBuf> {
    settings
        .toolchain_dir
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(Utf8PathBuf::from)
        .or_else(|| {
            std::env::var("LACE_TOOLCHAIN")
                .ok()
                .filter(|d| !d.is_empty())
                .map(Utf8PathBuf::from)
        })
}
