//! `lace serve`: a API do orquestrador para outro programa, por stdin e
//! stdout, em JSON-RPC 2.0 com uma mensagem por linha.
//!
//! É o transporte do contrato de `lace_core::protocol` para quem não é Rust:
//! a extensão do VS Code, um agente, um script. Cada linha de entrada é um
//! pedido (`{"jsonrpc":"2.0","id":1,"method":"project.status","params":
//! {...}}`) ou a notificação `$/cancel`; cada linha de saída é uma resposta
//! (`result` ou `error`) ou uma notificação do que acontece enquanto um
//! pedido roda (`phase`, `events`, `output_omitted`, `build`, `install`,
//! `shutdown`), com o `id` do pedido em `params.id`. O stdout é só do
//! protocolo; o log vai para o stderr.
//!
//! Não há "projeto atual": cada pedido traz o caminho do projeto, e o
//! projeto e o bundle são abertos a cada pedido, como a CLI e o Studio
//! fazem. Os pedidos do mesmo projeto rodam um de cada vez, na ordem, num
//! worker daquele projeto; os que só leem (`project.status`,
//! `history.list`) rodam em paralelo, numa thread própria; os que mexem na
//! instalação (`update.*`, `install.add`) num worker só. Cancelar é mandar
//! `$/cancel` com o `id`: um pedido que ainda não começou responde
//! `-32001`; um fluxo em andamento para e devolve o resultado com `status:
//! cancelled`, como a CLI faz com o Ctrl+C.
//!
//! Quando a entrada fecha, o que já foi pedido roda até o fim e é
//! respondido, e o servidor sai depois, com a notificação `shutdown`; se o
//! cliente fechou também a saída, o que roda é cancelado. Com `shutdown`,
//! com um sinal e depois de um `update.apply` que trocou os arquivos (o
//! binário dele é outro), o servidor cancela o que está rodando, espera até
//! 3 s e sai, com a notificação `shutdown` antes.
//!
//! Um parâmetro que o método não tem é recusado (`-32602`), com o nome mais
//! parecido, em vez de ignorado; no `flow.run`, também um campo que o fluxo
//! pedido não tem. Um BOM no começo da linha é ignorado: o PowerShell o põe
//! na primeira coisa que escreve para um programa. Os métodos `fpga.*`
//! exigem o recurso experimental `fpga` ligado, como na CLI e no Studio, e
//! um projeto cujo caminho não existe responde `spf_missing`.
//!
//! `update.*` e `install.add` rodam a biblioteca do instalador em processo,
//! como a CLI e o Studio; o progresso sai como notificação `install`, com o
//! evento do instalador em `params.event`.
//!
//! Códigos JSON-RPC: `-32700` JSON inválido, `-32600` pedido malformado,
//! `-32601` método desconhecido, `-32602` parâmetros inválidos, `-32000` um
//! erro do Lace (o `ErrorInfo` em `error.data`, com `code` e `hint`),
//! `-32001` cancelado antes de começar, `-32002` sessão não inicializada,
//! `-32003` servidor encerrando.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::delivery::{Dispatcher, Limits, Notification};
use lace_core::experimental::Feature;
use lace_core::flows::{self, FlowOptions};
use lace_core::fpga::{self, FpgaConfig};
use lace_core::history;
use lace_core::locate;
use lace_core::protocol::{
    CleanOutcome, ExperimentalInfo, METHODS, NameCheck, PROTOCOL_VERSION, ProcessorRemoval,
    RemoveOutcome, ReportView, Request, Response, ServerInfo, ShutdownReason, TopOutcome,
};
use lace_core::{CancelToken, Control, ErrorInfo, LaceError, Project, Toolchain};
use lace_installer::add::{self as installer_add, Payload};
use lace_installer::install::Event as InstallEvent;
use lace_installer::release::{Bar, Remote};
use lace_installer::update::{ChildOutput, RunOptions, Silent, UpdateAction};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::settings::ToolchainArgs;

/// Quanto esperar os pedidos em andamento depois de cancelá-los, ao
/// encerrar.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);

/// Os códigos de erro do JSON-RPC que o servidor usa.
pub(crate) mod rpc {
    pub const PARSE: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const LACE: i64 = -32000;
    pub const CANCELLED: i64 = -32001;
    pub const NOT_INITIALIZED: i64 = -32002;
    pub const SHUTTING_DOWN: i64 = -32003;

    /// Cada código e quando sai, para o docs/PROTOCOL.md.
    #[cfg(test)]
    pub const ALL: &[(i64, &str)] = &[
        (PARSE, "a linha não é JSON"),
        (INVALID_REQUEST, "o JSON não é um pedido JSON-RPC 2.0"),
        (METHOD_NOT_FOUND, "método desconhecido"),
        (INVALID_PARAMS, "parâmetros que o método não aceita"),
        (
            LACE,
            "o Lace não conseguiu atender: o `ErrorInfo` vai em `error.data`",
        ),
        (CANCELLED, "cancelado com `$/cancel` antes de começar"),
        (NOT_INITIALIZED, "pedido antes do `initialize`"),
        (SHUTTING_DOWN, "pedido depois do `shutdown`"),
    ];
}

/// As opções de `lace serve`.
#[derive(clap::Args, Debug, Clone)]
pub struct ServeArgs {
    /// Print the JSON Schema of the protocol (requests and responses) and exit
    #[arg(long)]
    pub schema: bool,
}

/// `lace serve`. `cancel` é o pedido dos sinais, que aqui encerra o
/// servidor do jeito certo: cancela o que roda, espera e sai.
pub fn run(
    toolchain: &ToolchainArgs,
    args: &ServeArgs,
    cancel: &CancelToken,
) -> anyhow::Result<bool> {
    if args.schema {
        let schema = json!({
            "request": lace_core::protocol::request_schema(),
            "response": lace_core::protocol::response_schema(),
        });
        println!("{}", serde_json::to_string_pretty(&schema)?);
        return Ok(true);
    }
    let server = Arc::new(Server::new(toolchain.clone(), Writer::stdout()));
    let signalled = cancel.clone();
    let watcher = Arc::clone(&server);
    thread::Builder::new()
        .name("lace-serve-signals".into())
        .spawn(move || {
            while !signalled.is_cancelled() {
                thread::sleep(Duration::from_millis(50));
            }
            watcher.finish(ShutdownReason::Signal);
            std::process::exit(0);
        })?;
    let stdin = std::io::stdin();
    match server.serve(stdin.lock()) {
        Some(reason) => server.finish(reason),
        // A entrada fechou: o que já foi pedido roda até o fim e é
        // respondido.
        None => server.finish_after_input(),
    }
    Ok(true)
}

/// Um pedido como chega, antes de virar um [`Request`].
#[derive(Deserialize)]
struct Incoming {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
}

/// O que vai para a thread do stdout: uma linha, ou uma marca que ela
/// confirma quando chega nela, com tudo o que veio antes já escrito.
enum Out {
    Line(String),
    Barrier(mpsc::Sender<()>),
}

/// O escritor único do stdout: uma thread, uma linha por mensagem, `flush`
/// a cada uma. Quem manda só espera a fila, nunca o cliente.
#[derive(Clone)]
pub(crate) struct Writer {
    tx: SyncSender<Out>,
    /// Sem notificações: o `lace mcp` usa o despacho do servidor, mas o
    /// stdout dele é do MCP, que não entende as notificações do Lace.
    quiet: bool,
    /// O cliente fechou a saída: não há mais a quem responder.
    closed: Arc<AtomicBool>,
}

impl Writer {
    /// Um escritor cujas linhas vão para um canal: os testes leem dali.
    #[cfg(test)]
    pub(crate) fn channel() -> (Self, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::sync_channel::<Out>(1024);
        let (lines, received) = mpsc::sync_channel::<String>(1024);
        thread::spawn(move || {
            for out in rx {
                match out {
                    Out::Line(line) => {
                        if lines.send(line).is_err() {
                            break;
                        }
                    }
                    Out::Barrier(ack) => {
                        let _ = ack.send(());
                    }
                }
            }
        });
        let writer = Writer {
            tx,
            quiet: false,
            closed: Arc::default(),
        };
        (writer, received)
    }

    /// Um escritor que não escreve nada: para quem só usa [`Server::handle`].
    pub(crate) fn silent() -> Self {
        let (tx, _rx) = mpsc::sync_channel::<Out>(1);
        Writer {
            tx,
            quiet: true,
            closed: Arc::default(),
        }
    }

    pub(crate) fn stdout() -> Self {
        let (tx, rx) = mpsc::sync_channel::<Out>(1024);
        let closed = Arc::new(AtomicBool::new(false));
        let gone = Arc::clone(&closed);
        thread::Builder::new()
            .name("lace-serve-stdout".into())
            .spawn(move || {
                let stdout = std::io::stdout();
                for out in rx {
                    match out {
                        Out::Line(line) => {
                            let mut handle = stdout.lock();
                            if handle.write_all(line.as_bytes()).is_err()
                                || handle.write_all(b"\n").is_err()
                                || handle.flush().is_err()
                            {
                                // O cliente fechou a saída: não há mais a
                                // quem falar.
                                gone.store(true, Ordering::SeqCst);
                                break;
                            }
                        }
                        Out::Barrier(ack) => {
                            let _ = ack.send(());
                        }
                    }
                }
            })
            .expect("spawn the stdout thread");
        Writer {
            tx,
            quiet: false,
            closed,
        }
    }

    pub(crate) fn send(&self, value: &Value) {
        if let Ok(line) = serde_json::to_string(value) {
            let _ = self.tx.send(Out::Line(line));
        }
    }

    /// Espera a thread do stdout escrever o que já está na fila, até
    /// `wait`. Sem isso, um processo que sai logo depois da última resposta
    /// pode sair antes de ela chegar ao cliente.
    pub(crate) fn drain(&self, wait: Duration) {
        let (ack, done) = mpsc::channel();
        if self.tx.send(Out::Barrier(ack)).is_ok() {
            let _ = done.recv_timeout(wait);
        }
    }

    /// O cliente fechou a saída.
    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn respond(&self, id: &Value, result: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    fn fail(&self, id: Option<&Value>, code: i64, message: &str, data: Option<Value>) {
        let mut error = json!({ "code": code, "message": message });
        if let Some(data) = data {
            error["data"] = data;
        }
        let id = id.cloned().unwrap_or(Value::Null);
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "error": error }));
    }

    fn notify(&self, method: &str, params: Value) {
        if self.quiet {
            return;
        }
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// Uma notificação do core sobre o pedido `id`: o `type` vira o método,
    /// e o resto vai em `params` com o `id`.
    fn notify_for(&self, id: &Value, notification: &Notification) {
        let Ok(Value::Object(mut fields)) = serde_json::to_value(notification) else {
            return;
        };
        let method = fields
            .remove("type")
            .and_then(|t| t.as_str().map(str::to_owned))
            .unwrap_or_else(|| "notification".to_owned());
        fields.insert("id".to_owned(), id.clone());
        self.notify(&method, Value::Object(fields));
    }
}

/// Um pedido aceito e ainda não respondido.
struct Pending {
    cancel: CancelToken,
    /// Já começou a rodar: cancelar é pelo token; antes disso, o worker o
    /// pula e responde `-32001`.
    started: Arc<AtomicBool>,
    /// É um `update.check`, que só consulta.
    check_only: bool,
}

type Job = Box<dyn FnOnce() + Send>;

/// Por que um pedido não deu certo: um erro do core (que ganha a dica da
/// CLI ao sair) ou um já em forma de [`ErrorInfo`].
pub(crate) enum Failure {
    Lace(Box<LaceError>),
    Info(Box<ErrorInfo>),
}

impl From<LaceError> for Failure {
    fn from(error: LaceError) -> Self {
        Failure::Lace(Box::new(error))
    }
}

impl From<ErrorInfo> for Failure {
    fn from(info: ErrorInfo) -> Self {
        Failure::Info(Box::new(info))
    }
}

impl From<lace_installer::Error> for Failure {
    fn from(error: lace_installer::Error) -> Self {
        Failure::Info(Box::new(ErrorInfo::new(error.code(), error.to_string())))
    }
}

impl Failure {
    pub(crate) fn into_info(self) -> ErrorInfo {
        match self {
            Failure::Lace(error) => crate::output::error_info(&error),
            Failure::Info(info) => *info,
        }
    }
}

pub(crate) type Handled = Result<Response, Failure>;

/// Para onde vão as notificações de um pedido enquanto ele roda: no `serve`,
/// o stdout com o `id` do pedido; no `lace mcp`, o progresso do MCP.
pub(crate) type Emit = Arc<dyn Fn(Notification) + Send + Sync>;

/// O servidor: o estado da sessão e as filas.
pub(crate) struct Server {
    toolchain: ToolchainArgs,
    writer: Writer,
    initialized: AtomicBool,
    /// Pedidos novos são recusados (`-32003`).
    shutting_down: AtomicBool,
    /// O encerramento já foi feito (a notificação, o cancelamento, a
    /// espera): só uma vez.
    finished: AtomicBool,
    /// Sair quando o pedido que está rodando terminar: o `update.apply`
    /// trocou o binário deste servidor.
    exit_when_done: AtomicBool,
    pending: Mutex<HashMap<String, Pending>>,
    workers: Mutex<HashMap<String, Sender<Job>>>,
    /// Se um recurso experimental está ligado: o `experimental.json` do
    /// usuário, ou o que o teste disser.
    features: fn(Feature) -> bool,
}

impl Server {
    pub(crate) fn new(toolchain: ToolchainArgs, writer: Writer) -> Self {
        Server {
            toolchain,
            writer,
            initialized: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            exit_when_done: AtomicBool::new(false),
            pending: Mutex::new(HashMap::new()),
            workers: Mutex::new(HashMap::new()),
            features: lace_core::experimental::is_enabled,
        }
    }

    /// O mesmo servidor, com outra regra para os recursos experimentais.
    #[cfg(test)]
    pub(crate) fn with_features(mut self, features: fn(Feature) -> bool) -> Self {
        self.features = features;
        self
    }

    /// Confere um recurso experimental antes de atender um método dele,
    /// como a CLI e o Studio fazem.
    fn require(&self, feature: Feature) -> Result<(), Failure> {
        if (self.features)(feature) {
            Ok(())
        } else {
            Err(LaceError::ExperimentalDisabled { feature }.into())
        }
    }

    /// Lê a entrada até ela fechar ou até um pedido encerrar a sessão.
    /// `Some` com o motivo, no segundo caso; `None` quando a entrada fechou.
    fn serve(self: &Arc<Self>, input: impl BufRead) -> Option<ShutdownReason> {
        for line in input.lines() {
            let Ok(line) = line else {
                break;
            };
            if line.trim().is_empty() {
                continue;
            }
            if let Some(reason) = self.handle_line(&line) {
                return Some(reason);
            }
        }
        None
    }

    /// Uma linha da entrada. `Some` quando a sessão termina por causa dela.
    fn handle_line(self: &Arc<Self>, line: &str) -> Option<ShutdownReason> {
        // O PowerShell põe um BOM na primeira coisa que escreve para um
        // programa, e o JSON não tem BOM.
        let line = line.strip_prefix('\u{feff}').unwrap_or(line);
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                self.writer
                    .fail(None, rpc::PARSE, &format!("Invalid JSON: {error}"), None);
                return None;
            }
        };
        if !value.is_object() {
            self.writer.fail(
                None,
                rpc::INVALID_REQUEST,
                "A request is a JSON object (batches are not supported)",
                None,
            );
            return None;
        }
        let incoming: Incoming = match serde_json::from_value(value) {
            Ok(incoming) => incoming,
            Err(error) => {
                self.writer
                    .fail(None, rpc::INVALID_REQUEST, &error.to_string(), None);
                return None;
            }
        };
        let Some(method) = incoming.method else {
            self.writer.fail(
                incoming.id.as_ref(),
                rpc::INVALID_REQUEST,
                "The request has no method",
                None,
            );
            return None;
        };
        if method == "$/cancel" {
            if let Some(id) = incoming.params.as_ref().and_then(|p| p.get("id")) {
                self.cancel(id);
            }
            return None;
        }
        let Some(id) = incoming.id else {
            // Uma notificação que o servidor não conhece: ignorada, como o
            // JSON-RPC manda.
            tracing::debug!("Ignoring the notification {method}");
            return None;
        };
        if self.shutting_down.load(Ordering::SeqCst) {
            self.writer.fail(
                Some(&id),
                rpc::SHUTTING_DOWN,
                "The server is shutting down",
                None,
            );
            return None;
        }
        if method != "initialize" && !self.initialized.load(Ordering::SeqCst) {
            self.writer.fail(
                Some(&id),
                rpc::NOT_INITIALIZED,
                "Send initialize first",
                None,
            );
            return None;
        }
        if let Some(message) = unknown_params(&method, incoming.params.as_ref()) {
            self.writer
                .fail(Some(&id), rpc::INVALID_PARAMS, &message, None);
            return None;
        }
        let request: Request = match parse_request(&method, incoming.params) {
            Ok(request) => request,
            Err(error) => {
                let (code, message) = if METHODS.contains(&method.as_str()) {
                    (
                        rpc::INVALID_PARAMS,
                        format!("Invalid params for {method}: {error}"),
                    )
                } else {
                    (rpc::METHOD_NOT_FOUND, format!("Unknown method {method}"))
                };
                self.writer.fail(Some(&id), code, &message, None);
                return None;
            }
        };
        match request {
            Request::Initialize { client } => {
                self.initialized.store(true, Ordering::SeqCst);
                tracing::info!(client = client.as_deref().unwrap_or("?"), "Session started");
                let info = serde_json::to_value(self.info()).unwrap_or(Value::Null);
                self.writer.respond(&id, info);
                None
            }
            Request::Shutdown => {
                self.shutting_down.store(true, Ordering::SeqCst);
                self.writer.respond(&id, Value::Null);
                Some(ShutdownReason::Requested)
            }
            request => {
                self.dispatch(id, request);
                None
            }
        }
    }

    /// Põe o pedido na fila certa.
    fn dispatch(self: &Arc<Self>, id: Value, request: Request) {
        let cancel = CancelToken::new();
        let started = Arc::new(AtomicBool::new(false));
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                id.to_string(),
                Pending {
                    cancel: cancel.clone(),
                    started: Arc::clone(&started),
                    check_only: matches!(request, Request::UpdateCheck),
                },
            );
        let lane = lane_of(&request);
        let server = Arc::clone(self);
        let job: Job = Box::new(move || {
            if cancel.is_cancelled() {
                server.writer.fail(
                    Some(&id),
                    rpc::CANCELLED,
                    "Cancelled before it started",
                    None,
                );
            } else {
                started.store(true, Ordering::SeqCst);
                match server.handle(&id, request, cancel) {
                    Ok(response) => {
                        let value = serde_json::to_value(&response).unwrap_or(Value::Null);
                        server.writer.respond(&id, value);
                    }
                    Err(failure) => {
                        let info = failure.into_info();
                        let data = serde_json::to_value(&info).ok();
                        server
                            .writer
                            .fail(Some(&id), rpc::LACE, &info.message, data);
                    }
                }
            }
            server
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&id.to_string());
            if server.exit_when_done.load(Ordering::SeqCst) {
                server.finish(ShutdownReason::Updated);
                std::process::exit(0);
            }
        });
        match lane {
            Lane::Thread => {
                if let Err(error) = thread::Builder::new()
                    .name("lace-serve-read".into())
                    .spawn(job)
                {
                    tracing::error!("Could not start a thread: {error}");
                }
            }
            Lane::Worker(key) => self.enqueue(&key, job),
        }
    }

    /// Entrega um trabalho ao worker `key`, criando-o se preciso.
    fn enqueue(&self, key: &str, job: Job) {
        let mut workers = self
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let sender = workers.entry(key.to_owned()).or_insert_with(|| {
            let (tx, rx) = mpsc::channel::<Job>();
            let tail: Vec<char> = key.chars().rev().take(20).collect();
            let short: String = tail.into_iter().rev().collect();
            thread::Builder::new()
                .name(format!("lace-serve-{short}"))
                .spawn(move || {
                    for job in rx {
                        job();
                    }
                })
                .expect("spawn a worker thread");
            tx
        });
        if sender.send(job).is_err() {
            tracing::error!("The worker {key} is gone");
        }
    }

    /// `$/cancel`: marca o token do pedido, rodando ou na fila.
    fn cancel(&self, id: &Value) {
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(entry) = pending.get(&id.to_string()) {
            entry.cancel.cancel();
            tracing::info!(id = %id, started = entry.started.load(Ordering::SeqCst), "Cancelled");
        }
    }

    /// Encerra por um pedido (`shutdown`), por um sinal ou depois de uma
    /// atualização: avisa, cancela o que roda, espera até [`SHUTDOWN_WAIT`]
    /// e espera a thread do stdout escrever o que falta. Só uma vez.
    fn finish(&self, reason: ShutdownReason) {
        self.shutting_down.store(true, Ordering::SeqCst);
        if self.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        let reason = serde_json::to_value(reason).unwrap_or(Value::Null);
        self.writer.notify("shutdown", json!({ "reason": reason }));
        self.cancel_all();
        self.wait_pending(SHUTDOWN_WAIT);
        self.writer.drain(SHUTDOWN_WAIT);
    }

    /// Encerra porque a entrada fechou: o que já foi pedido roda até o fim
    /// e é respondido, como num `printf ... | lace serve`. Se o cliente
    /// fechou também a saída, não há a quem responder, e o que roda é
    /// cancelado.
    fn finish_after_input(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        while !self.pending_is_empty() {
            if self.writer.is_closed() {
                self.cancel_all();
                self.wait_pending(SHUTDOWN_WAIT);
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        self.finish(ShutdownReason::Requested);
    }

    fn cancel_all(&self) {
        for entry in self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
        {
            entry.cancel.cancel();
        }
    }

    fn pending_is_empty(&self) -> bool {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    }

    fn wait_pending(&self, wait: Duration) {
        let until = Instant::now() + wait;
        while Instant::now() < until && !self.pending_is_empty() {
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn info(&self) -> ServerInfo {
        let experimental: BTreeMap<String, bool> = lace_core::experimental::states()
            .into_iter()
            .map(|s| (s.feature.id().to_owned(), s.enabled))
            .collect();
        ServerInfo {
            lace: env!("CARGO_PKG_VERSION").to_owned(),
            protocol: PROTOCOL_VERSION,
            prefix: crate::installation::prefix().ok(),
            experimental,
            methods: METHODS.iter().map(|m| (*m).to_owned()).collect(),
        }
    }

    fn toolchain(&self) -> Result<Toolchain, Failure> {
        self.toolchain.resolve().map_err(|e| to_info(&e).into())
    }

    /// Atende um pedido. `Err` é o Lace não conseguir rodar.
    pub(crate) fn handle(&self, id: &Value, request: Request, cancel: CancelToken) -> Handled {
        self.handle_with(id, request, cancel, None)
    }

    /// [`Server::handle`] com as notificações do pedido indo para `sink` em
    /// vez do stdout (o `lace mcp`).
    pub(crate) fn handle_with(
        &self,
        id: &Value,
        request: Request,
        cancel: CancelToken,
        sink: Option<Emit>,
    ) -> Handled {
        let emit: Emit = match sink {
            Some(sink) => sink,
            None => {
                let writer = self.writer.clone();
                let id = id.clone();
                Arc::new(move |notification| writer.notify_for(&id, &notification))
            }
        };
        Ok(match request {
            Request::Initialize { .. } | Request::Shutdown => Response::Nothing(()),
            Request::ProjectCreate { parent, name } => {
                let project = Project::create(&parent, &name)?;
                Response::Status(Box::new(project.status()))
            }
            Request::ProjectStatus { project } => {
                Response::Status(Box::new(open_project(&project)?.status()))
            }
            Request::ProjectAdd {
                project,
                files,
                testbench,
            } => {
                let mut project = open_project(&project)?;
                // Sem bundle o arquivo é criado do mesmo jeito, com o leitor
                // de portas embutido.
                let toolchain = self.toolchain().ok();
                for file in &files {
                    project.check_add_verilog(file)?;
                }
                let mut added = Vec::with_capacity(files.len());
                for file in files {
                    added.push(project.add_verilog(toolchain.as_ref(), file, testbench)?);
                }
                Response::Added(added)
            }
            Request::ProjectRemove { project, files } => {
                let mut project = open_project(&project)?;
                let mut removed = Vec::new();
                let mut not_registered = Vec::new();
                for file in files {
                    if project.remove_verilog(&file)? {
                        removed.push(file);
                    } else {
                        not_registered.push(file);
                    }
                }
                Response::Removed(RemoveOutcome {
                    removed,
                    not_registered,
                })
            }
            Request::ProjectTop { project, target } => {
                let mut project = open_project(&project)?;
                if let Some(target) = target {
                    project.set_top(&target)?;
                }
                let (module, module_error) = match project.top_module() {
                    Ok(module) => (module, None),
                    Err(error) => (None, Some(crate::output::error_info(&error))),
                };
                Response::Top(TopOutcome {
                    top_level: project.top_level(),
                    module,
                    module_error,
                })
            }
            Request::ProjectTestbench { project, path } => {
                let mut project = open_project(&project)?;
                project.set_testbench(&path)?;
                Response::Status(Box::new(project.status()))
            }
            Request::ProjectOrder {
                project,
                file,
                position,
            } => {
                let mut project = open_project(&project)?;
                Response::Files(project.reorder_file(&file, &position)?)
            }
            Request::ProjectMove {
                project,
                from,
                to,
                after,
            } => {
                let mut project = open_project(&project)?;
                let moved = if after {
                    project.record_move(&from, &to)?
                } else {
                    project.move_path(&from, &to)?
                };
                Response::Moved(moved)
            }
            Request::ProjectConvert { project } => {
                let mut project = open_project(&project)?;
                Response::Converted(project.convert_layout()?)
            }
            Request::ProjectCheckName { name } => {
                Response::Name(name_check(lace_core::validate_project_name(&name)))
            }
            Request::ProcessorCheckName { name } => {
                // A mesma regra do processor.add: a do YANC e, fora dela,
                // os nomes das pastas do Lace.
                Response::Name(name_check(lace_core::validate_new_processor_name(&name)))
            }
            Request::ProcessorAdd { project, spec } => {
                let mut project = open_project(&project)?;
                let processor = project.add_processor(&spec)?.clone();
                Response::Processor(Box::new(processor))
            }
            Request::ProcessorSet {
                project,
                name,
                config,
            } => {
                let mut project = open_project(&project)?;
                project.configure_processor(&name, &config)?;
                let processor = project.require_processor(&name)?.clone();
                Response::Processor(Box::new(processor))
            }
            Request::ProcessorRemove {
                project,
                name,
                to_trash,
            } => {
                let mut project = open_project(&project)?;
                let removed = project.remove_processor(&name)?;
                let trashed = to_trash && removed.dir.exists();
                if trashed {
                    trash::delete(&removed.dir).map_err(|error| {
                        ErrorInfo::new(
                            "io",
                            format!(
                                "{name} is out of the project, but its folder did not go to the trash ({error}): {}",
                                removed.dir
                            ),
                        )
                    })?;
                }
                Response::ProcessorRemoved(ProcessorRemoval { removed, trashed })
            }
            Request::ProcessorOutputs {
                project,
                processor,
                port,
            } => {
                let project = open_project(&project)?;
                let processor = project.require_processor(&processor)?;
                Response::Values(processor.read_output_values(port)?)
            }
            Request::FlowRun {
                project,
                request,
                options,
                output,
            } => {
                let toolchain = self.toolchain()?;
                let mut project = open_project(&project)?;
                let options =
                    options.unwrap_or_else(|| FlowOptions::new(request.command_line("lace")));
                let deliver = Arc::clone(&emit);
                let dispatcher =
                    Dispatcher::start(Limits::default(), move |notification| deliver(notification));
                let control = if output {
                    dispatcher.control(cancel)
                } else {
                    Control::new().with_cancel(cancel)
                };
                let progress = dispatcher.progress();
                let outcome = flows::run(
                    &toolchain,
                    &mut project,
                    &request,
                    &options,
                    &progress,
                    &control,
                );
                // Tudo que o fluxo disse sai antes do resultado.
                dispatcher.finish();
                Response::Flow(Box::new(outcome?))
            }
            Request::WaveOpen { project, request } => {
                let toolchain = self.toolchain()?;
                let project = project.map(|p| open_project(&p)).transpose()?;
                Response::Wave(flows::open_wave(&toolchain, project.as_ref(), &request)?)
            }
            Request::WaveSignals { project } => {
                let toolchain = self.toolchain()?;
                let project = open_project(&project)?;
                let control = Control::new().with_cancel(cancel);
                Response::Signals(Box::new(lace_core::wave_signals(
                    &toolchain, &project, &control,
                )?))
            }
            Request::WaveChoose { project, choice } => {
                let toolchain = self.toolchain()?;
                let project = open_project(&project)?;
                let control = Control::new().with_cancel(cancel);
                Response::Choice(flows::choose_signals(
                    &toolchain, &project, &choice, &control,
                )?)
            }
            Request::WaveResetLayout { waveform } => {
                let Ok(project) = Project::discover(&waveform) else {
                    return Ok(Response::Flag(false));
                };
                let Some(testbench) = lace_core::wave_testbench_of(&waveform)? else {
                    return Ok(Response::Flag(false));
                };
                Response::Flag(lace_core::reset_saved_layout(&project, &testbench)?)
            }
            Request::HistoryList { project, limit } => {
                let project = open_project(&project)?;
                let mut reports = history::list(&project)?;
                if let Some(limit) = limit {
                    reports.truncate(limit);
                }
                Response::Reports(reports)
            }
            Request::HistoryShow { project, id } => {
                let project = open_project(&project)?;
                let record = match id {
                    Some(id) => history::load(&project, &id)?,
                    None => history::latest(&project)?,
                };
                let text = history::report_text(&project, &record.id)?;
                let path = history::report_path(&project, &record.id)?;
                Response::Report(Box::new(ReportView {
                    id: record.id.clone(),
                    path,
                    record,
                    text,
                }))
            }
            Request::HistoryCompare {
                project,
                id,
                against,
            } => {
                let project = open_project(&project)?;
                Response::Comparison(Box::new(history::compare_reports(
                    &project,
                    id.as_deref(),
                    against.as_deref(),
                )?))
            }
            Request::HistoryPlanCleanup { project, cleanup } => {
                let project = open_project(&project)?;
                Response::Ids(history::plan_cleanup(&project, &cleanup)?)
            }
            Request::HistoryClean { project, ids } => {
                let project = open_project(&project)?;
                let removed = history::remove(&project, &ids)?;
                let kept = history::list(&project)?.len();
                Response::Cleaned(CleanOutcome { removed, kept })
            }
            Request::ToolchainInfo => {
                let lookup = self.toolchain.lookup().map_err(|e| to_info(&e))?;
                let info = match locate::resolve(&lookup) {
                    Ok(found) => locate::describe(&found),
                    Err(error) => locate::describe_error(&error),
                };
                Response::Toolchain(Box::new(info))
            }
            Request::ToolchainVerify => Response::Mismatches(self.toolchain()?.verify()?),
            Request::ExperimentalList => Response::Experimental(experimental_info()),
            Request::ExperimentalSet { feature, enabled } => {
                let feature =
                    lace_core::experimental::Feature::from_id(&feature).ok_or_else(|| {
                        ErrorInfo::new(
                            "invalid_argument",
                            format!("Unknown experimental component: {feature}"),
                        )
                    })?;
                lace_core::experimental::set_enabled(feature, enabled)?;
                Response::Experimental(experimental_info())
            }
            Request::FpgaBoards { board } => {
                self.require(Feature::Fpga)?;
                Response::Boards(match board {
                    Some(id) => vec![fpga::board(&id)?],
                    None => fpga::boards()?,
                })
            }
            Request::FpgaConfigGet { project } => {
                self.require(Feature::Fpga)?;
                let project = open_project(&project)?;
                let path = FpgaConfig::path(project.root());
                Response::FpgaConfig(if path.is_file() {
                    Some(FpgaConfig::read(project.root())?)
                } else {
                    None
                })
            }
            Request::FpgaConfigSet { project, config } => {
                self.require(Feature::Fpga)?;
                let project = open_project(&project)?;
                config.write(project.root())?;
                Response::Nothing(())
            }
            Request::FpgaCheck { project } => {
                self.require(Feature::Fpga)?;
                let project = open_project(&project)?;
                let toolchain = self.toolchain().ok();
                Response::Prepared(Box::new(fpga::prepare(toolchain.as_ref(), &project)?))
            }
            Request::FpgaModules { project } => {
                self.require(Feature::Fpga)?;
                let project = open_project(&project)?;
                let toolchain = self.toolchain().ok();
                Response::Modules(fpga::modules(toolchain.as_ref(), &project)?)
            }
            Request::FpgaStatus { project } => {
                self.require(Feature::Fpga)?;
                let toolchain = self.toolchain()?;
                let project = open_project(&project)?;
                Response::Bitstream(fpga::bitstream_state(&toolchain, &project)?)
            }
            Request::FpgaCables => {
                self.require(Feature::Fpga)?;
                let toolchain = self.toolchain()?;
                Response::Cables(fpga::cables(&toolchain)?)
            }
            Request::UpdateCheck => Response::Other(self.update(id, &emit, false, None, &cancel)?),
            Request::UpdateApply { full, from } => {
                let result =
                    self.update(id, &emit, full, from.as_deref().map(|f| (f, true)), &cancel)?;
                let _ = full;
                Response::Other(result)
            }
            Request::InstallAdd { components } => {
                let network = crate::update::network().map_err(cli_error)?;
                let remote = std::sync::Arc::new(Remote(network.clone()));
                let prefix = crate::installation::prefix_result()?;
                lace_installer::update::recover_interrupted(&prefix)?;
                lace_installer::update::clean_leftovers(prefix.as_std_path());
                let prepared = installer_add::prepare(
                    remote,
                    &network,
                    &prefix,
                    Payload::Release {
                        version: env!("CARGO_PKG_VERSION"),
                    },
                    &cancel,
                )?;
                let report = installer_add::add_components(
                    &prepared,
                    &prefix,
                    &components,
                    &mut |event| emit(install_notification(&event)),
                    &cancel,
                )?;
                Response::Other(serde_json::to_value(report).unwrap_or(Value::Null))
            }
            Request::LearnCheck {
                root,
                exercise,
                all,
            } => {
                let toolchain = self.toolchain()?;
                let root = lace_learn::Workspace::discover(&root).map_err(|e| learn_info(&e))?;
                let tracks =
                    lace_learn::tracks_dir(Some(&toolchain)).map_err(|e| learn_info(&e))?;
                let mut workspace = lace_learn::Workspace::open(&root, &tracks, None)
                    .map_err(|e| learn_info(&e))?;
                let exercises: Vec<lace_learn::Exercise> = if all {
                    workspace.track().exercises().cloned().collect()
                } else {
                    let exercise = match exercise {
                        Some(name) => workspace
                            .track()
                            .require(&name)
                            .map_err(|e| learn_info(&e))?
                            .clone(),
                        None => workspace.current().clone(),
                    };
                    vec![exercise]
                };
                let control = Control::new().with_cancel(cancel);
                let outcome = lace_learn::flows::check(
                    &toolchain,
                    &mut workspace,
                    &exercises,
                    &control,
                    &mut |_| {},
                )
                .map_err(|e| learn_info(&e))?;
                Response::Other(json!({
                    "results": outcome.results,
                    "solved": outcome.solved,
                    "total": outcome.total,
                }))
            }
            // O enum cresce sem quebrar quem o usa; um servidor desta versão
            // não atende o que não conhece.
            other => {
                return Err(ErrorInfo::new(
                    "unsupported",
                    format!("This server does not serve {}", other.method()),
                )
                .into());
            }
        })
    }

    /// `update.check` e `update.apply` pela biblioteca do instalador, com o
    /// progresso como notificação `install`. Depois de uma atualização que
    /// trocou os arquivos, o servidor encerra assim que responder: o binário
    /// dele é outro.
    fn update(
        &self,
        id: &Value,
        emit: &Emit,
        full: bool,
        from: Option<(&Utf8Path, bool)>,
        cancel: &CancelToken,
    ) -> Result<Value, Failure> {
        let check = from.is_none() && !full && self.check_only(id);
        let network = crate::update::network().map_err(cli_error)?;
        let remote = std::sync::Arc::new(Remote(network));
        let toolchain = self.toolchain.resolve().ok();
        let reg = crate::update::reg_exe();
        let options = RunOptions {
            check,
            full,
            from: from.map(|(path, _)| path),
            reopen: None,
            bar: Bar::Quiet,
            child_output: ChildOutput::Capture,
            reg: reg.as_deref(),
        };
        let report = lace_installer::update::run(
            &remote,
            env!("CARGO_PKG_VERSION"),
            toolchain.as_ref().map(Toolchain::manifest),
            crate::installation::prefix_result(),
            &options,
            &mut Silent,
            &mut |event| emit(install_notification(&event)),
            cancel,
        )?;
        if matches!(
            report.action,
            UpdateAction::Updated | UpdateAction::WizardOpened
        ) {
            // O binário deste servidor foi (ou vai ser) trocado: a sessão
            // termina depois desta resposta.
            self.shutting_down.store(true, Ordering::SeqCst);
            self.exit_when_done.store(true, Ordering::SeqCst);
        }
        Ok(serde_json::to_value(report).unwrap_or(Value::Null))
    }

    /// O pedido em andamento com `id` é um `update.check` (e não um
    /// `update.apply`): a distinção vem do método, guardado ao despachar.
    fn check_only(&self, id: &Value) -> bool {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&id.to_string())
            .is_some_and(|p| p.check_only)
    }
}

/// A notificação `install` com o evento do instalador.
fn install_notification(event: &InstallEvent) -> Notification {
    Notification::Install {
        event: serde_json::to_value(event).unwrap_or(Value::Null),
    }
}

/// Um erro da CLI (`anyhow`) como [`Failure`], com o código da biblioteca
/// do instalador quando é dela.
fn cli_error(error: anyhow::Error) -> Failure {
    Failure::Info(Box::new(to_info(&error)))
}

/// O [`Request`] de um método e seus parâmetros. Um método sem parâmetros
/// aceita `params` ausente ou `{}` (um cliente MCP manda `{}`), e um cujos
/// parâmetros têm todos padrão (`initialize`) aceita `params` ausente.
pub(crate) fn parse_request(
    method: &str,
    params: Option<Value>,
) -> Result<Request, serde_json::Error> {
    let envelope = |params: Option<Value>| {
        let mut envelope = json!({ "method": method });
        if let Some(params) = params {
            envelope["params"] = params;
        }
        envelope
    };
    let empty = params
        .as_ref()
        .is_none_or(|p| p.as_object().is_some_and(|o| o.is_empty()));
    match serde_json::from_value(envelope(params.clone())) {
        Ok(request) => Ok(request),
        Err(error) if empty => {
            // A outra forma de "sem parâmetros".
            let other = if params.is_none() {
                Some(json!({}))
            } else {
                None
            };
            serde_json::from_value(envelope(other)).map_err(|_| error)
        }
        Err(error) => Err(error),
    }
}

/// Onde um pedido roda.
enum Lane {
    /// Uma thread só para ele: só lê.
    Thread,
    /// O worker com esta chave, um pedido de cada vez.
    Worker(String),
}

fn lane_of(request: &Request) -> Lane {
    if request.is_read_only() {
        return Lane::Thread;
    }
    if request.is_installation() {
        return Lane::Worker("install".to_owned());
    }
    match request.project() {
        Some(project) => Lane::Worker(project_key(project)),
        None => Lane::Worker("general".to_owned()),
    }
}

/// A chave do worker de um projeto: a raiz canônica dele.
fn project_key(project: &Utf8Path) -> String {
    let dir = if project.extension() == Some("spf") {
        project.parent().unwrap_or(project)
    } else {
        project
    };
    dunce::canonicalize(dir)
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        .unwrap_or_else(|| dir.to_owned())
        .to_string()
}

fn name_check(result: Result<(), LaceError>) -> NameCheck {
    match result {
        Ok(()) => NameCheck {
            ok: true,
            error: None,
        },
        Err(error) => NameCheck {
            ok: false,
            error: Some(crate::output::error_info(&error)),
        },
    }
}

/// Abre o projeto de um pedido. Um caminho que não existe é
/// `spf_missing`, o código que o Studio já usava para um `.spf` que sumiu
/// depois de aberto (apagado, movido por fora).
fn open_project(path: &Utf8Path) -> Result<Project, Failure> {
    Project::open(path).map_err(|error| {
        if path.exists() {
            Failure::from(error)
        } else {
            ErrorInfo::new(
                "spf_missing",
                format!(
                    "The project file {path} does not exist anymore: without it Lace cannot build, simulate or change the project"
                ),
            )
            .into()
        }
    })
}

/// Os campos de `params` de cada método e os de cada fluxo do `flow.run`,
/// tirados do schema dos pedidos.
struct Fields {
    methods: HashMap<String, Vec<String>>,
    flows: HashMap<String, Vec<String>>,
}

fn fields() -> &'static Fields {
    static FIELDS: OnceLock<Fields> = OnceLock::new();
    FIELDS.get_or_init(|| {
        let schema =
            serde_json::to_value(lace_core::protocol::request_schema()).unwrap_or(Value::Null);
        let names = |node: &Value| -> Vec<String> {
            node.as_object()
                .map(|p| p.keys().cloned().collect())
                .unwrap_or_default()
        };
        let mut methods = HashMap::new();
        for entry in schema["oneOf"].as_array().into_iter().flatten() {
            if let Some(method) = entry["properties"]["method"]["const"].as_str() {
                methods.insert(
                    method.to_owned(),
                    names(&entry["properties"]["params"]["properties"]),
                );
            }
        }
        let mut flows = HashMap::new();
        for entry in schema["$defs"]["FlowRequest"]["oneOf"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(flow) = entry["properties"]["flow"]["const"].as_str() {
                flows.insert(flow.to_owned(), names(&entry["properties"]));
            }
        }
        Fields { methods, flows }
    })
}

/// Os campos de `params` de um método; `None` se o método não existe.
pub(crate) fn method_fields(method: &str) -> Option<&'static [String]> {
    fields().methods.get(method).map(Vec::as_slice)
}

/// Os campos de um fluxo do `flow.run`, com o `flow`; `None` se o fluxo
/// não existe.
pub(crate) fn flow_fields(flow: &str) -> Option<&'static [String]> {
    fields().flows.get(flow).map(Vec::as_slice)
}

/// A recusa de um campo que não está em `known`, com o mais parecido. Um
/// campo escrito errado seria ignorado em silêncio, e o pedido rodaria com
/// o padrão no lugar do que o cliente quis.
pub(crate) fn unknown_field(given: &Map<String, Value>, known: &[String]) -> Option<String> {
    let unknown = given.keys().find(|key| !known.contains(key))?;
    let mut message = format!("unknown field `{unknown}`");
    if let Some(near) = closest(unknown, known) {
        message.push_str(&format!(" (did you mean `{near}`?)"));
    }
    if known.is_empty() {
        message.push_str("; this method takes no params");
    } else {
        message.push_str(&format!("; the fields are: {}", known.join(", ")));
    }
    Some(message)
}

/// A recusa dos parâmetros de um pedido com um campo que o método não
/// tem; no `flow.run`, também um campo que o fluxo pedido não tem.
pub(crate) fn unknown_params(method: &str, params: Option<&Value>) -> Option<String> {
    let given = params?.as_object()?;
    let known = method_fields(method)?;
    if let Some(problem) = unknown_field(given, known) {
        return Some(format!("Invalid params for {method}: {problem}"));
    }
    let request = given.get("request")?.as_object()?;
    let flow = request.get("flow")?.as_str()?;
    let known = flow_fields(flow)?;
    unknown_field(request, known)
        .map(|problem| format!("Invalid params for {method}, flow {flow}: {problem}"))
}

/// O nome de `candidates` mais parecido com `word`, se for parecido o
/// bastante para ser um erro de digitação.
fn closest<'a>(word: &str, candidates: &'a [String]) -> Option<&'a str> {
    candidates
        .iter()
        .map(|candidate| (distance(word, candidate), candidate))
        .filter(|(d, _)| *d <= 3 && *d < word.chars().count())
        .min_by_key(|(d, _)| *d)
        .map(|(_, candidate)| candidate.as_str())
}

/// A distância de edição entre `a` e `b`.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            let value = (row[j + 1] + 1).min(row[j] + 1).min(diagonal + cost);
            diagonal = row[j + 1];
            row[j + 1] = value;
        }
    }
    row[b.len()]
}

fn experimental_info() -> ExperimentalInfo {
    ExperimentalInfo {
        path: lace_core::experimental::settings_path(),
        components: lace_core::experimental::states(),
    }
}

/// Um erro da CLI (`anyhow`) como [`ErrorInfo`]: o do core que houver na
/// cadeia, com a dica, senão `cli` com o texto inteiro.
fn to_info(error: &anyhow::Error) -> ErrorInfo {
    if let Some(lace) = error.chain().find_map(|e| e.downcast_ref::<LaceError>()) {
        return crate::output::error_info(lace);
    }
    if let Some(installer) = error
        .chain()
        .find_map(|e| e.downcast_ref::<lace_installer::Error>())
    {
        return ErrorInfo::new(installer.code(), installer.to_string());
    }
    if error.chain().any(|e| e.is::<crate::cancel::Cancelled>()) {
        return ErrorInfo::new("cancelled", error.to_string());
    }
    if let Some(learn) = error
        .chain()
        .find_map(|e| e.downcast_ref::<lace_learn::LearnError>())
    {
        return learn_info(learn);
    }
    ErrorInfo::new("cli", format!("{error:#}"))
}

/// Um erro do lace-learn como [`ErrorInfo`], com o código e a dica da CLI.
fn learn_info(error: &lace_learn::LearnError) -> ErrorInfo {
    let (code, hint) = crate::learn::error_info(error);
    let mut info = ErrorInfo::new(code, error.to_string());
    info.hint = hint;
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> (Arc<Server>, mpsc::Receiver<String>) {
        // Os recursos experimentais ligados: os testes não dependem do
        // `experimental.json` de quem os roda.
        server_with(|_| true)
    }

    fn server_with(features: fn(Feature) -> bool) -> (Arc<Server>, mpsc::Receiver<String>) {
        let (writer, rx) = Writer::channel();
        let args = ToolchainArgs {
            toolchain: None,
            compiler: None,
            quartus: None,
        };
        (
            Arc::new(Server::new(args, writer).with_features(features)),
            rx,
        )
    }

    fn next(rx: &mpsc::Receiver<String>) -> Value {
        let line = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("uma mensagem");
        serde_json::from_str(&line).expect("JSON")
    }

    #[test]
    fn rejects_bad_json_malformed_requests_and_unknown_methods() {
        let (server, rx) = server();
        assert!(server.handle_line("{nope").is_none());
        assert_eq!(next(&rx)["error"]["code"], rpc::PARSE);
        assert!(server.handle_line("[1,2]").is_none());
        assert_eq!(next(&rx)["error"]["code"], rpc::INVALID_REQUEST);
        assert!(server.handle_line(r#"{"jsonrpc":"2.0","id":1}"#).is_none());
        let reply = next(&rx);
        assert_eq!(reply["error"]["code"], rpc::INVALID_REQUEST);
        assert_eq!(reply["id"], 1);
        // Antes do initialize, nada além dele.
        server.handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"toolchain.info"}"#);
        assert_eq!(next(&rx)["error"]["code"], rpc::NOT_INITIALIZED);
        server.handle_line(
            r#"{"jsonrpc":"2.0","id":3,"method":"initialize","params":{"client":"test"}}"#,
        );
        let reply = next(&rx);
        assert_eq!(reply["id"], 3);
        assert_eq!(reply["result"]["protocol"], PROTOCOL_VERSION);
        assert!(
            reply["result"]["methods"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m == "flow.run")
        );
        server.handle_line(r#"{"jsonrpc":"2.0","id":4,"method":"nothing.here"}"#);
        assert_eq!(next(&rx)["error"]["code"], rpc::METHOD_NOT_FOUND);
        server.handle_line(r#"{"jsonrpc":"2.0","id":5,"method":"project.status","params":{}}"#);
        let reply = next(&rx);
        assert_eq!(reply["error"]["code"], rpc::INVALID_PARAMS);
        assert!(
            reply["error"]["message"]
                .as_str()
                .unwrap()
                .contains("project.status")
        );
        // Uma notificação desconhecida é ignorada em silêncio.
        server.handle_line(r#"{"jsonrpc":"2.0","method":"whatever"}"#);
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        // shutdown encerra a sessão e responde null.
        assert_eq!(
            server.handle_line(r#"{"jsonrpc":"2.0","id":6,"method":"shutdown"}"#),
            Some(ShutdownReason::Requested)
        );
        assert!(next(&rx)["result"].is_null());
        server.handle_line(r#"{"jsonrpc":"2.0","id":7,"method":"toolchain.info"}"#);
        assert_eq!(next(&rx)["error"]["code"], rpc::SHUTTING_DOWN);
    }

    #[test]
    fn a_lace_error_carries_the_error_info_in_data() {
        let (server, rx) = server();
        server.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
        next(&rx);
        let missing = std::env::temp_dir().join("lace-serve-nao-existe");
        let missing = missing.to_str().unwrap().replace('\\', "/");
        server.handle_line(&format!(
            r#"{{"jsonrpc":"2.0","id":"a","method":"project.status","params":{{"project":"{missing}"}}}}"#
        ));
        let reply = next(&rx);
        assert_eq!(reply["id"], "a");
        assert_eq!(reply["error"]["code"], rpc::LACE);
        assert!(reply["error"]["data"]["code"].is_string(), "{reply}");
        assert_eq!(reply["error"]["message"], reply["error"]["data"]["message"]);
        // O pedido sai da lista dos pendentes depois de respondido.
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until && !server.pending.lock().unwrap().is_empty() {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(server.pending.lock().unwrap().is_empty());
    }

    /// Manda o pedido, espera a resposta e confere o `result` contra o tipo
    /// que `protocol::result_schema` declara para o método.
    fn call_conforming(
        server: &Arc<Server>,
        rx: &mpsc::Receiver<String>,
        schema: &Value,
        method: &str,
        params: Value,
    ) -> Value {
        let id = format!("{method} {params}");
        server.handle_line(
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string(),
        );
        let reply = loop {
            let line = next(rx);
            if line["id"] == id.as_str() {
                break line;
            }
        };
        let result = reply
            .get("result")
            .unwrap_or_else(|| panic!("{method}: {reply}"))
            .clone();
        let validator = jsonschema::validator_for(&json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$defs": schema["$defs"],
            "allOf": [schema["properties"][method]],
        }))
        .unwrap();
        let errors: Vec<String> = validator
            .iter_errors(&result)
            .map(|e| format!("{}: {e}", e.instance_path()))
            .collect();
        assert!(
            errors.is_empty(),
            "{method} não devolve o tipo declarado em protocol.rs:\n{}\n\n{result:#}",
            errors.join("\n")
        );
        result
    }

    /// O resultado de cada método é o tipo que `protocol::result_schema`
    /// declara, que é o que o docs/PROTOCOL.md mostra. Roda, num projeto
    /// temporário, os métodos que não precisam de simulação, de síntese, de
    /// relatório guardado, do Quartus nem da rede.
    #[test]
    fn results_conform_to_the_declared_schema() {
        let schema = serde_json::to_value(lace_core::protocol::result_schema()).unwrap();
        let (server, rx) = server();
        let call =
            |method: &str, params: Value| call_conforming(&server, &rx, &schema, method, params);
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dunce::canonicalize(dir.path()).unwrap()).unwrap();
        let project = root.join("demo");
        let (top, tb) = (project.join("top.v"), project.join("top_tb.v"));

        call("initialize", json!({"client": "test"}));
        call("project.check_name", json!({"name": "demo"}));
        call("processor.check_name", json!({"name": "x-y"}));
        call("project.create", json!({"parent": root, "name": "demo"}));
        call("project.status", json!({"project": project}));
        call(
            "project.add",
            json!({"project": project, "files": [top, tb]}),
        );
        call("project.top", json!({"project": project}));
        call("project.testbench", json!({"project": project, "path": tb}));
        call(
            "project.order",
            json!({"project": project, "file": top, "position": {"kind": "first"}}),
        );
        let moved = project.join("rtl/top.v");
        call(
            "project.move",
            json!({"project": project, "from": top, "to": moved}),
        );
        call(
            "project.remove",
            json!({"project": project, "files": [moved]}),
        );
        call("project.convert", json!({"project": project}));
        call(
            "processor.add",
            json!({"project": project, "spec": {"name": "soma", "language": "cmm"}}),
        );
        call(
            "processor.set",
            json!({"project": project, "name": "soma", "config": {"frequency_mhz": 50}}),
        );
        std::fs::create_dir_all(project.join("soma/soma")).unwrap();
        std::fs::write(project.join("soma/soma/output_0.txt"), "1\n-2\n").unwrap();
        let values = call(
            "processor.outputs",
            json!({"project": project, "processor": "soma", "port": 0}),
        );
        assert_eq!(values, json!([1, -2]));
        call("history.list", json!({"project": project}));
        call("experimental.list", json!({}));
        call("fpga.boards", json!({}));
        assert!(call("fpga.config", json!({"project": project})).is_null());
        call(
            "fpga.config_set",
            json!({"project": project, "config": {"board": "de10-nano"}}),
        );
        call("fpga.config", json!({"project": project}));
        call(
            "wave.reset_layout",
            json!({"waveform": project.join("top_tb.fst")}),
        );
        call(
            "processor.remove",
            json!({"project": project, "name": "soma"}),
        );
        call("shutdown", json!({}));
    }

    #[test]
    fn cancelling_a_queued_request_answers_before_it_starts() {
        let (server, rx) = server();
        server.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
        next(&rx);
        // Ocupa o worker "general" com um trabalho que espera o portão: a
        // fila é por chave, então o pedido seguinte espera atrás dele.
        let gate = Arc::new(AtomicBool::new(false));
        let g = Arc::clone(&gate);
        server.enqueue(
            "general",
            Box::new(move || {
                while !g.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_millis(5));
                }
            }),
        );
        server.handle_line(
            r#"{"jsonrpc":"2.0","id":2,"method":"project.check_name","params":{"name":"x"}}"#,
        );
        // check_name é só leitura: roda já, fora da fila.
        let reply = next(&rx);
        assert_eq!(reply["id"], 2);
        assert_eq!(reply["result"]["ok"], true);
        // experimental.set vai para o worker "general", que está ocupado.
        server.handle_line(
            r#"{"jsonrpc":"2.0","id":3,"method":"experimental.set","params":{"feature":"nope","enabled":true}}"#,
        );
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        server.handle_line(r#"{"jsonrpc":"2.0","method":"$/cancel","params":{"id":3}}"#);
        gate.store(true, Ordering::SeqCst);
        let reply = next(&rx);
        assert_eq!(reply["id"], 3);
        assert_eq!(reply["error"]["code"], rpc::CANCELLED);
    }

    #[test]
    fn notifications_carry_the_request_id_and_the_type_as_method() {
        let (writer, rx) = Writer::channel();
        writer.notify_for(
            &json!(7),
            &Notification::Phase {
                phase: flows::Phase::Build,
            },
        );
        let line = next(&rx);
        assert_eq!(line["method"], "phase");
        assert_eq!(line["params"]["id"], 7);
        assert_eq!(line["params"]["phase"], "build");
        assert!(line.get("id").is_none());
    }

    #[test]
    fn methods_without_params_parse_with_and_without_defaults() {
        assert!(matches!(
            parse_request("initialize", None),
            Ok(Request::Initialize { client: None })
        ));
        assert!(matches!(
            parse_request("toolchain.info", None),
            Ok(Request::ToolchainInfo)
        ));
        assert!(matches!(
            parse_request("fpga.boards", None),
            Ok(Request::FpgaBoards { board: None })
        ));
        // Um cliente MCP manda `{}` para uma ferramenta sem parâmetros.
        assert!(matches!(
            parse_request("toolchain.info", Some(json!({}))),
            Ok(Request::ToolchainInfo)
        ));
        assert!(matches!(
            parse_request("initialize", Some(json!({}))),
            Ok(Request::Initialize { client: None })
        ));
        assert!(parse_request("project.status", None).is_err());
        assert!(parse_request("nothing.here", None).is_err());
    }

    #[test]
    fn project_keys_ignore_the_spf_file_name() {
        let dir = Utf8PathBuf::from_path_buf(std::env::temp_dir()).unwrap();
        let spf = dir.join("x.spf");
        assert_eq!(project_key(&spf), project_key(&dir));
    }

    fn initialized() -> (Arc<Server>, mpsc::Receiver<String>) {
        let (server, rx) = server();
        server.handle_line(r#"{"jsonrpc":"2.0","id":0,"method":"initialize"}"#);
        next(&rx);
        (server, rx)
    }

    #[test]
    fn an_unknown_param_is_refused_with_the_closest_name() {
        let (server, rx) = initialized();
        server.handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"project.move","params":{"project":"/p","form":"a","to":"b"}}"#,
        );
        let reply = next(&rx);
        assert_eq!(reply["error"]["code"], rpc::INVALID_PARAMS, "{reply}");
        let message = reply["error"]["message"].as_str().unwrap();
        assert!(message.contains("`form`"), "{message}");
        assert!(message.contains("did you mean `from`"), "{message}");
        // Um campo de um fluxo, dentro do pedido do flow.run.
        server.handle_line(
            r#"{"jsonrpc":"2.0","id":2,"method":"flow.run","params":{"project":"/p","request":{"flow":"simulate","simulator":"icarus","proccessor":"x"}}}"#,
        );
        let reply = next(&rx);
        assert_eq!(reply["error"]["code"], rpc::INVALID_PARAMS, "{reply}");
        let message = reply["error"]["message"].as_str().unwrap();
        assert!(message.contains("did you mean `processor`"), "{message}");
        // Um método sem parâmetros aceita `{}`, que um cliente MCP manda.
        assert!(unknown_params("toolchain.info", Some(&json!({}))).is_none());
        assert!(unknown_params("toolchain.info", Some(&json!({ "x": 1 }))).is_some());
        // O que o cliente manda certo passa.
        assert!(
            unknown_params(
                "project.move",
                Some(&json!({ "project": "/p", "from": "a", "to": "b", "after": true }))
            )
            .is_none()
        );
    }

    #[test]
    fn a_byte_order_mark_before_a_request_is_ignored() {
        let (server, rx) = server();
        server.handle_line("\u{feff}{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}");
        let reply = next(&rx);
        assert_eq!(reply["result"]["protocol"], PROTOCOL_VERSION, "{reply}");
    }

    #[test]
    fn closing_the_input_answers_what_was_already_asked() {
        let (server, rx) = server();
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dunce::canonicalize(dir.path()).unwrap()).unwrap();
        let spf = Project::create(&root, "p").unwrap().spf_path().to_owned();
        let input = format!(
            "{}\n{}\n",
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
            json!({ "jsonrpc": "2.0", "id": 2, "method": "processor.add",
                    "params": { "project": spf, "spec": { "name": "soma", "language": "cmm" } } })
        );
        assert!(server.serve(std::io::Cursor::new(input)).is_none());
        server.finish_after_input();
        let lines: Vec<Value> = rx
            .try_iter()
            .map(|l| serde_json::from_str(&l).unwrap())
            .collect();
        assert!(
            lines
                .iter()
                .any(|l| l["id"] == 2 && l["result"]["name"] == "soma"),
            "{lines:?}"
        );
        assert_eq!(lines.last().unwrap()["method"], "shutdown", "{lines:?}");
    }

    #[test]
    fn a_processor_name_is_checked_like_processor_add() {
        let (server, rx) = initialized();
        // `wave` é a pasta das escolhas de sinais: o processor.add recusa.
        server.handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"processor.check_name","params":{"name":"wave"}}"#,
        );
        let reply = next(&rx);
        assert_eq!(reply["result"]["ok"], false, "{reply}");
        assert_eq!(reply["result"]["error"]["code"], "invalid_name", "{reply}");
    }

    #[test]
    fn fpga_methods_need_the_experimental_component() {
        let (server, rx) = server_with(|_| false);
        server.handle_line(r#"{"jsonrpc":"2.0","id":0,"method":"initialize"}"#);
        next(&rx);
        server.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"fpga.boards"}"#);
        let reply = next(&rx);
        assert_eq!(reply["error"]["code"], rpc::LACE, "{reply}");
        assert_eq!(
            reply["error"]["data"]["code"], "experimental_disabled",
            "{reply}"
        );
    }

    #[test]
    fn a_project_file_that_does_not_exist_is_spf_missing() {
        let (server, rx) = initialized();
        let dir = tempfile::tempdir().unwrap();
        let gone = Utf8PathBuf::from_path_buf(dir.path().join("x.spf")).unwrap();
        server.handle_line(
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "project.status",
                     "params": { "project": gone } })
            .to_string(),
        );
        let reply = next(&rx);
        assert_eq!(reply["error"]["data"]["code"], "spf_missing", "{reply}");
    }

    #[test]
    fn edit_distance() {
        assert_eq!(distance("form", "from"), 2);
        assert_eq!(distance("proccessor", "processor"), 1);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("same", "same"), 0);
    }
}
