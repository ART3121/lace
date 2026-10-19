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
//! O servidor encerra quando a entrada fecha, quando recebe `shutdown` ou um
//! sinal, e depois de um `update.apply` que trocou os arquivos (o binário
//! dele é outro): cancela o que está rodando, espera até 3 s e sai, com a
//! notificação `shutdown` antes.
//!
//! Códigos JSON-RPC: `-32700` JSON inválido, `-32600` pedido malformado,
//! `-32601` método desconhecido, `-32602` parâmetros inválidos, `-32000` um
//! erro do Lace (o `ErrorInfo` em `error.data`, com `code` e `hint`),
//! `-32001` cancelado antes de começar, `-32002` sessão não inicializada,
//! `-32003` servidor encerrando.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, Read, Write};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::delivery::{Dispatcher, Limits, Notification};
use lace_core::flows::{self, FlowOptions};
use lace_core::fpga::{self, FpgaConfig};
use lace_core::history;
use lace_core::locate;
use lace_core::protocol::{
    CleanOutcome, ExperimentalInfo, METHODS, NameCheck, PROTOCOL_VERSION, ProcessorRemoval,
    RemoveOutcome, ReportView, Request, Response, ServerInfo, ShutdownReason, TopOutcome,
};
use lace_core::{CancelToken, Control, ErrorInfo, LaceError, Project, Toolchain};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::settings::ToolchainArgs;

/// Quanto esperar os pedidos em andamento depois de cancelá-los, ao
/// encerrar.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);

/// Os códigos de erro do JSON-RPC que o servidor usa.
mod rpc {
    pub const PARSE: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const LACE: i64 = -32000;
    pub const CANCELLED: i64 = -32001;
    pub const NOT_INITIALIZED: i64 = -32002;
    pub const SHUTTING_DOWN: i64 = -32003;
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
    let reason = server.serve(stdin.lock());
    server.finish(reason);
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

/// O escritor único do stdout: uma thread, uma linha por mensagem, `flush`
/// a cada uma. Quem manda só espera a fila, nunca o cliente.
#[derive(Clone)]
struct Writer {
    tx: SyncSender<String>,
}

impl Writer {
    fn stdout() -> Self {
        let (tx, rx) = mpsc::sync_channel::<String>(1024);
        thread::Builder::new()
            .name("lace-serve-stdout".into())
            .spawn(move || {
                let stdout = std::io::stdout();
                for line in rx {
                    let mut out = stdout.lock();
                    if out.write_all(line.as_bytes()).is_err()
                        || out.write_all(b"\n").is_err()
                        || out.flush().is_err()
                    {
                        // O cliente fechou a saída: não há mais a quem falar.
                        break;
                    }
                }
            })
            .expect("spawn the stdout thread");
        Writer { tx }
    }

    fn send(&self, value: &Value) {
        if let Ok(line) = serde_json::to_string(value) {
            let _ = self.tx.send(line);
        }
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
}

type Job = Box<dyn FnOnce() + Send>;

/// Por que um pedido não deu certo: um erro do core (que ganha a dica da
/// CLI ao sair) ou um já em forma de [`ErrorInfo`].
enum Failure {
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

impl Failure {
    fn into_info(self) -> ErrorInfo {
        match self {
            Failure::Lace(error) => crate::output::error_info(&error),
            Failure::Info(info) => *info,
        }
    }
}

type Handled = Result<Response, Failure>;

/// O servidor: o estado da sessão e as filas.
struct Server {
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
}

impl Server {
    fn new(toolchain: ToolchainArgs, writer: Writer) -> Self {
        Server {
            toolchain,
            writer,
            initialized: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            exit_when_done: AtomicBool::new(false),
            pending: Mutex::new(HashMap::new()),
            workers: Mutex::new(HashMap::new()),
        }
    }

    /// Lê a entrada até ela fechar ou até o `shutdown`. Devolve por quê.
    fn serve(self: &Arc<Self>, input: impl BufRead) -> ShutdownReason {
        for line in input.lines() {
            let Ok(line) = line else {
                break;
            };
            if line.trim().is_empty() {
                continue;
            }
            if let Some(reason) = self.handle_line(&line) {
                return reason;
            }
        }
        ShutdownReason::Requested
    }

    /// Uma linha da entrada. `Some` quando a sessão termina por causa dela.
    fn handle_line(self: &Arc<Self>, line: &str) -> Option<ShutdownReason> {
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

    /// Encerra: avisa, cancela tudo, espera até [`SHUTDOWN_WAIT`] e dá à
    /// thread do stdout um instante para esvaziar a fila. Só uma vez.
    fn finish(&self, reason: ShutdownReason) {
        self.shutting_down.store(true, Ordering::SeqCst);
        if self.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        let reason = serde_json::to_value(reason).unwrap_or(Value::Null);
        self.writer.notify("shutdown", json!({ "reason": reason }));
        for entry in self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
        {
            entry.cancel.cancel();
        }
        let until = Instant::now() + SHUTDOWN_WAIT;
        while Instant::now() < until {
            if self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
            {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        thread::sleep(Duration::from_millis(50));
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
    fn handle(&self, id: &Value, request: Request, cancel: CancelToken) -> Handled {
        Ok(match request {
            Request::Initialize { .. } | Request::Shutdown => Response::Nothing(()),
            Request::ProjectCreate { parent, name } => {
                let project = Project::create(&parent, &name)?;
                Response::Status(Box::new(project.status()))
            }
            Request::ProjectStatus { project } => {
                Response::Status(Box::new(Project::open(&project)?.status()))
            }
            Request::ProjectAdd {
                project,
                files,
                testbench,
            } => {
                let mut project = Project::open(&project)?;
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
                let mut project = Project::open(&project)?;
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
                let mut project = Project::open(&project)?;
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
                let mut project = Project::open(&project)?;
                project.set_testbench(&path)?;
                Response::Status(Box::new(project.status()))
            }
            Request::ProjectOrder {
                project,
                file,
                position,
            } => {
                let mut project = Project::open(&project)?;
                Response::Files(project.reorder_file(&file, &position)?)
            }
            Request::ProjectMove {
                project,
                from,
                to,
                after,
            } => {
                let mut project = Project::open(&project)?;
                let moved = if after {
                    project.record_move(&from, &to)?
                } else {
                    project.move_path(&from, &to)?
                };
                Response::Moved(moved)
            }
            Request::ProjectConvert { project } => {
                let mut project = Project::open(&project)?;
                Response::Converted(project.convert_layout()?)
            }
            Request::ProjectCheckName { name } => {
                Response::Name(name_check(lace_core::validate_project_name(&name)))
            }
            Request::ProcessorCheckName { name } => {
                Response::Name(name_check(lace_core::validate_processor_name(&name)))
            }
            Request::ProcessorAdd { project, spec } => {
                let mut project = Project::open(&project)?;
                let processor = project.add_processor(&spec)?.clone();
                Response::Processor(Box::new(processor))
            }
            Request::ProcessorSet {
                project,
                name,
                config,
            } => {
                let mut project = Project::open(&project)?;
                project.configure_processor(&name, &config)?;
                let processor = project.require_processor(&name)?.clone();
                Response::Processor(Box::new(processor))
            }
            Request::ProcessorRemove {
                project,
                name,
                to_trash,
            } => {
                let mut project = Project::open(&project)?;
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
                let project = Project::open(&project)?;
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
                let mut project = Project::open(&project)?;
                let options =
                    options.unwrap_or_else(|| FlowOptions::new(request.command_line("lace")));
                let writer = self.writer.clone();
                let id = id.clone();
                let dispatcher = Dispatcher::start(Limits::default(), move |notification| {
                    writer.notify_for(&id, &notification);
                });
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
                let project = project.map(|p| Project::open(&p)).transpose()?;
                Response::Wave(flows::open_wave(&toolchain, project.as_ref(), &request)?)
            }
            Request::WaveSignals { project } => {
                let toolchain = self.toolchain()?;
                let project = Project::open(&project)?;
                let control = Control::new().with_cancel(cancel);
                Response::Signals(Box::new(lace_core::wave_signals(
                    &toolchain, &project, &control,
                )?))
            }
            Request::WaveChoose { project, choice } => {
                let toolchain = self.toolchain()?;
                let project = Project::open(&project)?;
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
                let project = Project::open(&project)?;
                let mut reports = history::list(&project)?;
                if let Some(limit) = limit {
                    reports.truncate(limit);
                }
                Response::Reports(reports)
            }
            Request::HistoryShow { project, id } => {
                let project = Project::open(&project)?;
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
                let project = Project::open(&project)?;
                Response::Comparison(Box::new(history::compare_reports(
                    &project,
                    id.as_deref(),
                    against.as_deref(),
                )?))
            }
            Request::HistoryPlanCleanup { project, cleanup } => {
                let project = Project::open(&project)?;
                Response::Ids(history::plan_cleanup(&project, &cleanup)?)
            }
            Request::HistoryClean { project, ids } => {
                let project = Project::open(&project)?;
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
            Request::FpgaBoards { board } => Response::Boards(match board {
                Some(id) => vec![fpga::board(&id)?],
                None => fpga::boards()?,
            }),
            Request::FpgaConfigGet { project } => {
                let project = Project::open(&project)?;
                let path = FpgaConfig::path(project.root());
                Response::FpgaConfig(if path.is_file() {
                    Some(FpgaConfig::read(project.root())?)
                } else {
                    None
                })
            }
            Request::FpgaConfigSet { project, config } => {
                let project = Project::open(&project)?;
                config.write(project.root())?;
                Response::Nothing(())
            }
            Request::FpgaCheck { project } => {
                let project = Project::open(&project)?;
                let toolchain = self.toolchain().ok();
                Response::Prepared(Box::new(fpga::prepare(toolchain.as_ref(), &project)?))
            }
            Request::FpgaModules { project } => {
                let project = Project::open(&project)?;
                let toolchain = self.toolchain().ok();
                Response::Modules(fpga::modules(toolchain.as_ref(), &project)?)
            }
            Request::FpgaStatus { project } => {
                let toolchain = self.toolchain()?;
                let project = Project::open(&project)?;
                Response::Bitstream(fpga::bitstream_state(&toolchain, &project)?)
            }
            Request::FpgaCables => {
                let toolchain = self.toolchain()?;
                Response::Cables(fpga::cables(&toolchain)?)
            }
            Request::UpdateCheck => {
                Response::Other(self.run_cli(id, &["update", "--check", "--json"], &cancel)?)
            }
            Request::UpdateApply { full, from } => {
                let mut args = vec!["update".to_owned(), "--yes".to_owned(), "--json".to_owned()];
                if full {
                    args.push("--full".to_owned());
                }
                if let Some(from) = from {
                    args.push("--from".to_owned());
                    args.push(from.to_string());
                }
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let result = self.run_cli(id, &args, &cancel)?;
                let action = result.get("action").and_then(Value::as_str).unwrap_or("");
                if matches!(action, "updated" | "wizard_opened") {
                    // O binário deste servidor foi (ou vai ser) trocado: a
                    // sessão termina depois desta resposta.
                    self.shutting_down.store(true, Ordering::SeqCst);
                    self.exit_when_done.store(true, Ordering::SeqCst);
                }
                Response::Other(result)
            }
            Request::InstallAdd { components } => {
                let mut args = vec!["install".to_owned()];
                args.extend(components);
                args.push("--json".to_owned());
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                Response::Other(self.run_cli(id, &args, &cancel)?)
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

    /// Roda o próprio `lace` com `args` (instalar, atualizar), repassando
    /// cada linha do stderr dele como notificação `install` e devolvendo o
    /// JSON que ele escreveu. Cancelar encerra o processo e o que ele
    /// iniciou; o instalador do Windows, que sai do job de propósito,
    /// continua, e o diário dele desfaz o que ficou pela metade.
    fn run_cli(&self, id: &Value, args: &[&str], cancel: &CancelToken) -> Result<Value, Failure> {
        let exe = std::env::current_exe().map_err(|e| {
            ErrorInfo::new("cli", format!("Could not find the lace executable: {e}"))
        })?;
        let mut command = std::process::Command::new(&exe);
        command
            .args(args)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &self.toolchain.toolchain {
            command.env("LACE_TOOLCHAIN", dir);
        }
        lace_core::hide_console(&mut command);
        let mut job = lace_core::ProcessJob::new(true);
        let mut child = command
            .spawn()
            .map_err(|e| ErrorInfo::new("cli", format!("Could not run {}: {e}", exe.display())))?;
        job.adopt(&child);
        let stderr = child.stderr.take().expect("piped stderr");
        let writer = self.writer.clone();
        let id_for_lines = id.clone();
        let lines = thread::spawn(move || {
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                writer.notify(
                    "install",
                    json!({
                        "id": id_for_lines,
                        "event": { "event": "cli_output", "stream": "stderr", "line": line },
                    }),
                );
            }
        });
        let mut stdout = child.stdout.take().expect("piped stdout");
        let reader = thread::spawn(move || {
            let mut all = String::new();
            let _ = stdout.read_to_string(&mut all);
            all
        });
        let mut cancelled = false;
        let status = loop {
            if let Ok(Some(status)) = child.try_wait() {
                break Some(status);
            }
            if cancel.is_cancelled() {
                cancelled = true;
                let _ = child.kill();
                job.kill();
                break child.wait().ok();
            }
            thread::sleep(Duration::from_millis(50));
        };
        job.kill();
        let _ = lines.join();
        let stdout = reader.join().unwrap_or_default();
        if cancelled {
            return Err(ErrorInfo::new("cancelled", "Cancelled").into());
        }
        let value: Value = serde_json::from_str(stdout.trim()).map_err(|_| {
            ErrorInfo::new(
                "cli",
                format!(
                    "lace {} exited with {:?} without JSON",
                    args.join(" "),
                    status.and_then(|s| s.code())
                ),
            )
        })?;
        if let Some(error) = value.get("error") {
            let info: ErrorInfo = serde_json::from_value(error.clone())
                .unwrap_or_else(|_| ErrorInfo::new("cli", error.to_string()));
            return Err(info.into());
        }
        Ok(value)
    }
}

/// O [`Request`] de um método e seus parâmetros. Sem `params`, um método
/// cujos parâmetros têm todos padrão (`initialize`) aceita `{}`.
fn parse_request(method: &str, params: Option<Value>) -> Result<Request, serde_json::Error> {
    let mut envelope = json!({ "method": method });
    let retry = params.is_none();
    if let Some(params) = params {
        envelope["params"] = params;
    }
    match serde_json::from_value(envelope) {
        Ok(request) => Ok(request),
        Err(error) if retry => {
            serde_json::from_value(json!({ "method": method, "params": {} })).map_err(|_| error)
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

    fn capture() -> (Writer, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::sync_channel::<String>(1024);
        (Writer { tx }, rx)
    }

    fn server() -> (Arc<Server>, mpsc::Receiver<String>) {
        let (writer, rx) = capture();
        let args = ToolchainArgs {
            toolchain: None,
            compiler: None,
            quartus: None,
        };
        (Arc::new(Server::new(args, writer)), rx)
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
        let (writer, rx) = capture();
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
        assert!(parse_request("project.status", None).is_err());
        assert!(parse_request("nothing.here", None).is_err());
    }

    #[test]
    fn project_keys_ignore_the_spf_file_name() {
        let dir = Utf8PathBuf::from_path_buf(std::env::temp_dir()).unwrap();
        let spf = dir.join("x.spf");
        assert_eq!(project_key(&spf), project_key(&dir));
    }
}
