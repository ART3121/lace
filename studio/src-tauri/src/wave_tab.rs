//! A onda numa aba: o cliente web do Surfer (o `web/` do surfer-aurora do
//! bundle, o mesmo Surfer compilado para WebAssembly) num iframe.
//!
//! Um servidor HTTP do Studio, só em `127.0.0.1`, numa porta livre, serve de
//! uma origem só:
//!
//! | Caminho | O que é |
//! |---|---|
//! | `/web/<arquivo>` | o cliente web (`index.html`, `surfer.js`, `surfer_bg.wasm`...) |
//! | `/wave/<aba>/<nome>` | a onda da aba, o arquivo como está no disco |
//! | `/layout/<aba>` | o `.surf.ron` do layout dos processadores |
//! | `/doc/<aba>/<nome>` | um tradutor do layout, ou `startup.sucl`, os comandos de partida |
//!
//! A página abre com `load_url=<origem>/wave/<aba>/<nome>` e, havendo
//! layout, `startup_commands` com os comandos do fork
//! (`load_mapping_translator_from_url` para cada tradutor e
//! `load_state_from_url` para o estado), que rodam depois que a onda carrega.
//! A aba é identificada por um segredo aleatório: o servidor só entrega o
//! que foi registrado para uma aba aberta.
//!
//! A AURORA ligava o cliente a um `surfer-aurora server`, que lê a onda e
//! manda só os sinais pedidos. Com ele, o layout some de vez em quando: o
//! `load_state` do fork zera o arquivo escolhido do servidor
//! (`selected_server_file_index`, que nem vai no `.surf.ron`), e a consulta de
//! status seguinte recarrega a onda do zero. Lendo o arquivo direto, o
//! cliente não consulta status e o layout fica. O preço é ler a onda inteira
//! no WebAssembly; acima de [`MAX_TAB_WAVE`], a aba manda abrir em janela.

use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::sync::{Arc, Mutex};

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{WaveLayout, WaveProcessor};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tiny_http::{Header, Response, Server};

use crate::error::{IpcError, IpcResult, codes};
use crate::state::blocking;
use crate::toolchain;

/// A maior onda que abre numa aba. O cliente web lê o arquivo inteiro na
/// memória do WebAssembly, que é limitada e mais lenta que o Surfer nativo.
pub const MAX_TAB_WAVE: u64 = 256 * 1024 * 1024;

/// A política de conteúdo das páginas do cliente: o WASM precisa de
/// `wasm-unsafe-eval`, e o `index.html` do trunk tem script inline.
const SURFER_CSP: &str = "default-src 'self'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; \
     style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; worker-src 'self' blob:";

/// As abas de onda abertas e o servidor local.
#[derive(Default)]
pub struct WaveTabs {
    shared: Arc<Mutex<Shared>>,
}

#[derive(Default)]
struct Shared {
    /// A origem do servidor local, depois que ele sobe.
    origin: Option<String>,
    /// O cliente web do bundle em uso.
    web_dir: Option<Utf8PathBuf>,
    /// As abas, pelo segredo de cada uma.
    sessions: HashMap<String, Session>,
}

struct Session {
    waveform: Utf8PathBuf,
    layout: Option<WaveLayout>,
    startup: String,
}

/// O que a interface recebe ao abrir uma aba de onda.
#[derive(Debug, Clone, Serialize)]
pub struct WaveTab {
    /// O identificador da aba, para [`wave_tab_close`].
    pub id: String,
    /// A página do cliente web, para o iframe.
    pub url: String,
    /// Os processadores SAPHO do layout; vazio sem layout.
    pub processors: Vec<WaveProcessor>,
}

/// Prepara `path` para uma aba e devolve a página do cliente web, com o
/// layout dos processadores SAPHO quando houver ([`lace_core::wave_layout`]).
///
/// Erros: `surfer_web_missing` quando o bundle não traz o cliente web, e
/// `wave_too_large` acima de [`MAX_TAB_WAVE`]; nos dois, a interface oferece
/// a janela.
#[tauri::command]
pub async fn wave_tab_open(app: AppHandle, path: String) -> IpcResult<WaveTab> {
    blocking(app, move |app, state| {
        let waveform = Utf8PathBuf::from(path);
        let size = std::fs::metadata(&waveform).map(|m| m.len()).map_err(|_| {
            IpcError::new(
                "no_waveform",
                format!("Waveform {waveform} does not exist yet; simulate first"),
            )
        })?;
        if size > MAX_TAB_WAVE {
            return Err(IpcError::new(
                codes::WAVE_TOO_LARGE,
                format!(
                    "Waveform {waveform} has {} MB; a tab opens up to {} MB, open it in a window",
                    size / 1_000_000,
                    MAX_TAB_WAVE / 1_000_000
                ),
            ));
        }
        let toolchain = toolchain::require(&state.settings.get())?;
        let web_dir = toolchain
            .surfer_web_dir()
            .map_err(|e| IpcError::new(codes::SURFER_WEB_MISSING, e.to_string()))?;
        let layout = match lace_core::wave_layout(&waveform) {
            Ok(layout) => layout,
            Err(error) => {
                tracing::warn!("No processor layout for {waveform}: {error}");
                None
            }
        };
        let tabs = app.state::<WaveTabs>();
        let origin = tabs.ensure_server(web_dir)?;

        let id = new_id();
        let name = waveform.file_name().unwrap_or("wave.vcd");
        let startup = startup_commands(&origin, &id, layout.as_ref());
        let mut query = format!("load_url={}", encode(&format!("{origin}/wave/{id}/{name}")));
        if !startup.is_empty() {
            let command = format!("run_command_file_from_url {origin}/doc/{id}/startup.sucl");
            query.push_str(&format!("&startup_commands={}", encode(&command)));
        }
        // O `#dev` desliga o service worker do index.html, que guardaria o
        // cliente em cache entre versões do bundle.
        let url = format!("{origin}/web/index.html?{query}#dev");
        let processors = layout
            .as_ref()
            .map(|l| l.processors.clone())
            .unwrap_or_default();
        tracing::info!(%waveform, %id, "Wave tab opened");
        tabs.lock().sessions.insert(
            id.clone(),
            Session {
                waveform,
                layout,
                startup,
            },
        );
        Ok(WaveTab {
            id,
            url,
            processors,
        })
    })
    .await
}

/// Fecha a aba: o servidor esquece a onda dela.
#[tauri::command]
pub async fn wave_tab_close(app: AppHandle, id: String) -> IpcResult<()> {
    blocking(app, move |app, _| {
        app.state::<WaveTabs>().lock().sessions.remove(&id);
        Ok(())
    })
    .await
}

impl WaveTabs {
    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().expect("wave tabs lock")
    }

    /// Sobe o servidor local, uma vez, e devolve a origem dele.
    fn ensure_server(&self, web_dir: Utf8PathBuf) -> IpcResult<String> {
        let mut shared = self.lock();
        shared.web_dir = Some(web_dir);
        if let Some(origin) = &shared.origin {
            return Ok(origin.clone());
        }
        let server = Server::http("127.0.0.1:0").map_err(|e| {
            IpcError::new(codes::IO, format!("Could not start the wave server: {e}"))
        })?;
        let port = server
            .server_addr()
            .to_ip()
            .map(|a| a.port())
            .ok_or_else(|| IpcError::new(codes::INTERNAL, "The wave server has no port"))?;
        let origin = format!("http://127.0.0.1:{port}");
        shared.origin = Some(origin.clone());
        let state = self.shared.clone();
        std::thread::Builder::new()
            .name("wave-http".into())
            .spawn(move || {
                for request in server.incoming_requests() {
                    let state = state.clone();
                    std::thread::spawn(move || handle(&state, request));
                }
            })
            .map_err(|e| IpcError::new(codes::INTERNAL, e.to_string()))?;
        tracing::info!(%origin, "Wave server listening");
        Ok(origin)
    }
}

/// Um segredo de 32 dígitos hexadecimais. O `RandomState` traz chaves
/// aleatórias do sistema; o servidor só escuta em `127.0.0.1`, e o segredo
/// separa uma aba da outra.
fn new_id() -> String {
    let a = RandomState::new().hash_one(std::time::SystemTime::now());
    let b = RandomState::new().hash_one(a);
    format!("{a:016x}{b:016x}")
}

/// Os comandos de partida do cliente: carregar cada tradutor e depois o
/// estado, que os usa. Vazio sem layout.
fn startup_commands(origin: &str, id: &str, layout: Option<&WaveLayout>) -> String {
    let Some(layout) = layout else {
        return String::new();
    };
    let mut lines: Vec<String> = layout
        .mappings
        .iter()
        .map(|m| {
            format!(
                "load_mapping_translator_from_url {origin}/doc/{id}/{}",
                m.name
            )
        })
        .collect();
    lines.push(format!("load_state_from_url {origin}/layout/{id}"));
    lines.join("\n") + "\n"
}

type Reply = Response<Box<dyn std::io::Read + Send>>;

/// Atende um pedido do cliente web.
fn handle(state: &Mutex<Shared>, request: tiny_http::Request) {
    let url = request.url().to_owned();
    tracing::debug!(method = %request.method(), %url, "Wave request");
    let path = url.split_once('?').map_or(url.as_str(), |(p, _)| p);
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let shared = || state.lock().expect("wave tabs lock");
    let response = match parts.as_slice() {
        ["web", file] => web_file(&shared(), file),
        ["wave", id, _] => {
            let waveform = shared().sessions.get(*id).map(|s| s.waveform.clone());
            waveform.and_then(|w| wave_file(&w))
        }
        ["layout", id] => shared()
            .sessions
            .get(*id)
            .and_then(|s| s.layout.as_ref())
            .map(|l| text(l.state.clone())),
        ["doc", id, name] => shared().sessions.get(*id).and_then(|s| {
            if *name == "startup.sucl" {
                return Some(text(s.startup.clone()));
            }
            s.layout
                .as_ref()?
                .mappings
                .iter()
                .find(|m| m.name == *name)
                .map(|m| text(m.content.clone()))
        }),
        _ => None,
    };
    let response = response
        .unwrap_or_else(|| boxed(Response::from_data(b"Not found".to_vec()).with_status_code(404)));
    let _ = request.respond(response);
}

fn boxed<R: std::io::Read + Send + 'static>(response: Response<R>) -> Reply {
    response.boxed()
}

fn header(key: &str, value: &str) -> Header {
    Header::from_bytes(key.as_bytes(), value.as_bytes()).expect("valid header")
}

fn text(body: String) -> Reply {
    boxed(
        Response::from_data(body.into_bytes())
            .with_header(header("Content-Type", "text/plain; charset=utf-8"))
            .with_header(header("Cache-Control", "no-store")),
    )
}

/// A onda, lida do disco aos pedaços pelo `tiny_http`.
fn wave_file(waveform: &Utf8Path) -> Option<Reply> {
    let file = std::fs::File::open(waveform).ok()?;
    Some(boxed(
        Response::from_file(file)
            .with_header(header("Content-Type", "application/octet-stream"))
            .with_header(header("Cache-Control", "no-store")),
    ))
}

/// Um arquivo do cliente web. Só nomes simples: nada de subir de pasta.
fn web_file(shared: &Shared, file: &str) -> Option<Reply> {
    if file.is_empty() || file.starts_with('.') || file.contains(['\\', ':']) {
        return None;
    }
    let body = std::fs::read(shared.web_dir.as_ref()?.join(file)).ok()?;
    let body = if file == "index.html" {
        with_key_forwarding(body)
    } else {
        body
    };
    let mime = match Utf8Path::new(file).extension() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    };
    Some(boxed(
        Response::from_data(body)
            .with_header(header("Content-Type", mime))
            .with_header(header("Cache-Control", "no-store"))
            .with_header(header("Content-Security-Policy", SURFER_CSP)),
    ))
}

/// O script que vai no começo do `index.html` do cliente: com o foco dentro
/// da aba (o iframe é de outra origem), as teclas não chegam ao Studio, e os
/// atalhos dele parariam de funcionar. As teclas de função (menos a F11, a
/// tela cheia do Surfer) e as combinações com Ctrl, Alt ou Cmd vão para a
/// janela do Studio por `postMessage` e não chegam ao Surfer; as de edição
/// (copiar, colar, desfazer, buscar) e o Ctrl+K, que começa um atalho de
/// duas teclas, ficam com o Surfer. O resto (letras, setas, espaço) é do
/// Surfer, como antes.
const KEY_FORWARDING: &str = r#"<script>
(function () {
  var own = ['c', 'v', 'x', 'a', 'z', 'y', 'f', 'g', 'h', 'k'];
  function forwarded(e) {
    if (/^F([1-9]|1[0-2])$/.test(e.key)) return e.key !== 'F11';
    if (!(e.ctrlKey || e.altKey || e.metaKey)) return false;
    if (['Control', 'Alt', 'Meta', 'Shift'].indexOf(e.key) >= 0) return false;
    return own.indexOf(e.key.toLowerCase()) < 0;
  }
  window.addEventListener('keydown', function (e) {
    if (!forwarded(e)) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    window.parent.postMessage({ lace: 'key', key: e.key, code: e.code, ctrlKey: e.ctrlKey,
      shiftKey: e.shiftKey, altKey: e.altKey, metaKey: e.metaKey }, '*');
  }, true);
})();
</script>"#;

/// O `index.html` com [`KEY_FORWARDING`] logo depois do `<head>` (antes dos
/// scripts do cliente, para o ouvinte vir primeiro).
fn with_key_forwarding(body: Vec<u8>) -> Vec<u8> {
    let Ok(text) = String::from_utf8(body.clone()) else {
        return body;
    };
    let at = text
        .to_ascii_lowercase()
        .find("<head>")
        .map_or(0, |i| i + "<head>".len());
    let mut out = String::with_capacity(text.len() + KEY_FORWARDING.len());
    out.push_str(&text[..at]);
    out.push_str(KEY_FORWARDING);
    out.push_str(&text[at..]);
    out.into_bytes()
}

/// Codifica um valor para a query de uma URL.
fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_page_forwards_studio_keys() {
        let page = with_key_forwarding(
            b"<!DOCTYPE html><html><head><script src=\"surfer.js\"></script>".to_vec(),
        );
        let page = String::from_utf8(page).unwrap();
        let script = page.find("lace: 'key'").unwrap();
        assert!(
            script < page.find("surfer.js").unwrap(),
            "o ouvinte vem antes do cliente"
        );
        assert!(page.starts_with("<!DOCTYPE html><html><head><script>"));
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(
            encode("run_command_file_from_url http://127.0.0.1:1/doc/a/startup.sucl"),
            "run_command_file_from_url%20http%3A%2F%2F127.0.0.1%3A1%2Fdoc%2Fa%2Fstartup.sucl"
        );
    }

    #[test]
    fn tab_ids_are_long_and_distinct() {
        let (a, b) = (new_id(), new_id());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
