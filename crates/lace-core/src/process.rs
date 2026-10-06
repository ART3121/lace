//! O único módulo do Core que cria processos.
//!
//! Cada [`Invocation`] declara programa (caminho absoluto), argumentos,
//! diretório de trabalho e ambiente. Nada é herdado implicitamente: o CWD é
//! parte do contrato dos compiladores YANC (o Verilog gerado embute caminhos),
//! e o ambiente parte de vazio mais uma lista mínima de variáveis que o sistema
//! operacional exige.
//!
//! [`run`] lê stdout e stderr linha a linha enquanto o processo roda, para
//! avisar cada linha a quem acompanha, e encerra o processo quando a
//! operação é cancelada ou passa do prazo. Encerrar é encerrar tudo o que a
//! ferramenta iniciou: o `iverilog` roda o `ivlpp` e o `ivl`, o Verilator
//! roda o `make`, que roda o compilador C++. No Unix cada passo roda num
//! grupo de processos próprio, e o Lace sinaliza o grupo. No Windows cada
//! passo roda num Job Object ([`ProcessJob`]), que encerra a árvore inteira,
//! também quando o próprio Lace morre (fechado à força, o Studio derrubado):
//! o sistema fecha o handle do job, e o que estava nele termina junto.
//!
//! Nenhuma ferramenta abre janela de console ([`hide_console`]): a saída vai
//! pelos pipes, para o terminal ou para os consoles do Studio.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use schemars::JsonSchema;
use serde::Serialize;

use crate::control::{CancelToken, Stream};
use crate::error::{LaceError, Result};

/// De quanto em quanto tempo o Lace confere cancelamento e prazo.
const POLL: Duration = Duration::from_millis(20);
/// Quanto um processo tem para sair depois do pedido de término (SIGTERM),
/// antes de ser morto (SIGKILL).
const KILL_GRACE: Duration = Duration::from_secs(1);
/// Quanto esperar pelo fim da saída depois que o processo terminou. Só
/// passa disso se um processo que ele iniciou continua vivo segurando o
/// pipe; aí o Lace para de ler.
const DRAIN_LIMIT: Duration = Duration::from_secs(2);
/// Quantas linhas lidas podem esperar por quem as recebe.
const LINE_QUEUE: usize = 1024;
/// Quanto do começo e quanto do fim da saída de cada pipe o Lace guarda. Um
/// testbench que imprime sem parar (`always #1 $display`) escrevia 33 MB em
/// 5 s, e tudo ficava na memória até o fim; agora o meio sai, e o resultado
/// diz quantos bytes ficaram de fora. Os erros dos compiladores vêm no
/// começo; o `$finish` e o resumo da simulação, no fim.
const OUTPUT_KEPT: usize = 2 * 1024 * 1024;

/// Variáveis repassadas do ambiente do Lace ao filho. No Windows, sem
/// `SystemRoot` vários programas nem inicializam, e sem `ComSpec` o
/// `system()` da biblioteca C não acha o `cmd.exe` (o `iverilog` roda o
/// `ivlpp` e o `ivl` por ele); fora dele, os compiladores YANC não leem
/// variável nenhuma.
#[cfg(windows)]
const INHERITED_ENV: &[&str] = &["SystemRoot", "windir", "ComSpec", "TEMP", "TMP"];
#[cfg(not(windows))]
const INHERITED_ENV: &[&str] = &[];

/// O que uma aplicação gráfica precisa para abrir janela e achar a própria
/// configuração. Só entra nas invocações que pedem ([`Invocation::inherit`]).
#[cfg(windows)]
pub(crate) const GUI_ENV: &[&str] = &[
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "HOMEDRIVE",
    "HOMEPATH",
    "USERNAME",
];
#[cfg(not(windows))]
pub(crate) const GUI_ENV: &[&str] = &[
    "HOME",
    "USER",
    "LANG",
    // macOS: diretório temporário por usuário.
    "TMPDIR",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_RUNTIME_DIR",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_SESSION_TYPE",
    "DBUS_SESSION_BUS_ADDRESS",
];

/// Uma execução de programa, totalmente especificada: o registro exato do
/// que o Lace rodou, presente em todo [`StepReport`](crate::StepReport).
///
/// Só o Core monta invocações; para os clientes o tipo é somente leitura.
/// Reproduzir um passo à mão é rodar `program` com `args`, dentro de `cwd`,
/// com um ambiente vazio mais `env` e as variáveis de `inherit`.
///
/// Em JSON:
///
/// ```json
/// { "program": "/opt/yanc/bin/cmmcomp",
///   "args": ["-i", "soma.cmm", "-n", "soma", "-p", "/p/soma", "..."],
///   "cwd": "/p/soma",
///   "env": [],
///   "inherit": [] }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Invocation {
    /// Caminho absoluto do executável. Nunca é procurado no `PATH`.
    #[schemars(with = "String")]
    pub program: Utf8PathBuf,
    /// Argumentos, na ordem. Caminhos já estão no formato que a ferramenta
    /// aceita: o nativo do sistema, menos os do `iverilog` no Windows, que
    /// vão com `/`.
    pub args: Vec<String>,
    /// Diretório de trabalho. Faz parte do contrato de várias ferramentas (o
    /// `appcomp` e o `asmcomp` leem `app_log.txt` relativo a ele, o testbench
    /// grava a onda nele), por isso nunca é herdado.
    #[schemars(with = "String")]
    pub cwd: Utf8PathBuf,
    /// Variáveis definidas pelo Lace, somadas às de `INHERITED_ENV`.
    pub env: Vec<(String, String)>,
    /// Nomes de variáveis copiadas do ambiente do Lace, quando existirem.
    /// Só os nomes aparecem no relatório.
    pub inherit: Vec<String>,
}

impl Invocation {
    pub(crate) fn new(program: impl Into<Utf8PathBuf>, cwd: impl Into<Utf8PathBuf>) -> Self {
        Invocation {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            env: Vec::new(),
            inherit: Vec::new(),
        }
    }

    pub(crate) fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Acrescenta um argumento de caminho, no formato que o Windows aceita.
    pub(crate) fn path_arg(self, path: &Utf8Path) -> Self {
        let native = dunce::simplified(path.as_std_path()).to_string_lossy();
        let arg = native.into_owned();
        self.arg(arg)
    }

    /// Acrescenta um caminho para o `iverilog`, como [`icarus_path`].
    pub(crate) fn icarus_path_arg(self, path: &Utf8Path) -> Self {
        self.arg(icarus_path(path))
    }

    pub(crate) fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Define o `PATH` do filho com exatamente `dirs` (nada é herdado). Sem
    /// diretórios, o filho fica sem `PATH`.
    pub(crate) fn search_path(self, dirs: &[Utf8PathBuf]) -> Self {
        if dirs.is_empty() {
            return self;
        }
        let joined = std::env::join_paths(dirs.iter().map(|d| dunce::simplified(d.as_std_path())))
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.env("PATH", joined)
    }

    /// Acrescenta `dirs` ao fim do `PATH` que [`Invocation::search_path`]
    /// definiu; sem `PATH` ainda, é o mesmo que ela.
    pub(crate) fn append_search_path(mut self, dirs: &[Utf8PathBuf]) -> Self {
        let Some(at) = self.env.iter().position(|(key, _)| key == "PATH") else {
            return self.search_path(dirs);
        };
        let (_, current) = self.env.remove(at);
        let all: Vec<std::path::PathBuf> = std::env::split_paths(&current)
            .chain(
                dirs.iter()
                    .map(|d| dunce::simplified(d.as_std_path()).to_owned()),
            )
            .collect();
        let joined = std::env::join_paths(all)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or(current);
        self.env.insert(at, ("PATH".into(), joined));
        self
    }

    /// Copia do ambiente do Lace as variáveis `keys`, se existirem.
    pub(crate) fn inherit(mut self, keys: &[&str]) -> Self {
        self.inherit.extend(keys.iter().map(|k| (*k).to_owned()));
        self
    }

    /// Linha de comando legível, com aspas onde houver espaço. Serve para log
    /// e para mostrar ao usuário; não é garantido que um shell a interprete
    /// igual.
    pub fn display_command(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .map(quote_if_needed)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Um caminho como o `iverilog` o recebe: no Windows, com `/`.
///
/// O pré-processador do Icarus (`ivlpp`) só reconhece `/` ao montar a pasta
/// de quem faz o `include` (`-grelative-include`): com `\`, o
/// `` `include "defs.vh" `` de `rtl\cpu.v` não acha `rtl\defs.vh`. E o
/// `.vvp` guarda os nomes da tabela `:file_names` como vieram, sem escapar a
/// `\`, que na leitura sumiria. O Windows aceita a `/`; o que volta das
/// ferramentas retoma o separador nativo
/// ([`native_separators`](crate::paths::native_separators)).
pub(crate) fn icarus_path(path: &Utf8Path) -> String {
    let native = dunce::simplified(path.as_std_path())
        .to_string_lossy()
        .into_owned();
    if cfg!(windows) {
        native.replace('\\', "/")
    } else {
        native
    }
}

fn quote_if_needed(s: &str) -> String {
    if !s.is_empty() && !s.contains([' ', '\t', '"', '\'']) {
        s.to_owned()
    } else {
        format!("\"{}\"", s.replace('"', "\\\""))
    }
}

/// Como um processo terminou.
///
/// Em JSON: `{"kind": "exited", "value": 1}`, `{"kind": "signaled",
/// "value": 11}`, `{"kind": "exception", "value": 3221225477}`,
/// `{"kind": "cancelled"}`, `{"kind": "timed_out"}`, `{"kind": "unknown"}`.
///
/// Um código de saída e um sinal são coisas diferentes: `msg_internal` do
/// `cppcomp` chama `abort()`, e um `asmcomp` que perde um `fopen` morre por
/// segfault. Nenhum dos dois é um "erro de compilação" do usuário.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
#[non_exhaustive]
pub enum Termination {
    /// Terminou com um código de saída.
    Exited(i32),
    /// Morto por sinal (só Unix).
    Signaled(i32),
    /// Terminou com um código NTSTATUS de exceção (só Windows), por exemplo
    /// `0xC0000005`, violação de acesso.
    Exception(u32),
    /// O Lace encerrou o processo porque a operação foi cancelada
    /// ([`CancelToken::cancel`]).
    Cancelled,
    /// O Lace encerrou o processo porque ele passou do prazo (por exemplo,
    /// [`SimulationOptions::timeout`](crate::SimulationOptions::timeout)).
    TimedOut,
    /// O sistema não informou nem código nem sinal.
    Unknown,
}

impl Termination {
    /// Terminou com código 0.
    pub fn success(self) -> bool {
        self == Termination::Exited(0)
    }
}

/// O que um processo produziu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessOutput {
    pub termination: Termination,
    pub stdout: String,
    pub stderr: String,
    #[serde(skip)]
    pub duration: Duration,
}

/// O `Command` de `invocation`: programa, argumentos, CWD e ambiente, com
/// stdin vazio e sem janela de console.
fn command(invocation: &Invocation) -> Command {
    let mut command = Command::new(invocation.program.as_std_path());
    command
        .args(&invocation.args)
        .current_dir(invocation.cwd.as_std_path())
        .env_clear()
        .stdin(Stdio::null());
    hide_console(&mut command);

    let inherited = INHERITED_ENV
        .iter()
        .copied()
        .chain(invocation.inherit.iter().map(String::as_str));
    for key in inherited {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    for (key, value) in &invocation.env {
        command.env(key, value);
    }
    command
}

/// Quem recebe cada linha de [`run`]: de qual pipe veio, e a linha sem o fim
/// de linha.
pub(crate) type OnLine<'a> = &'a dyn Fn(Stream, &str);

/// Como acompanhar uma execução de [`run`]. `Watch::default()` roda até o
/// fim, sem prazo e sem avisar linhas.
#[derive(Default)]
pub(crate) struct Watch<'a> {
    /// Encerra o processo quando o cancelamento for pedido.
    pub cancel: Option<&'a CancelToken>,
    /// Encerra o processo depois deste tempo.
    pub timeout: Option<Duration>,
    /// Recebe cada linha, sem o fim de linha, enquanto o processo roda.
    pub on_line: Option<OnLine<'a>>,
}

/// Executa `invocation` até o fim e coleta stdout e stderr.
///
/// `Err` só quando o processo nem começou (binário ausente, sem permissão,
/// CWD inexistente). Um processo que roda e falha volta como `Ok` com a
/// [`Termination`] correspondente; um que o Lace encerrou, com
/// [`Termination::Cancelled`] ou [`Termination::TimedOut`], e com tudo o que
/// ele escreveu até ali.
pub(crate) fn run(invocation: &Invocation, watch: &Watch<'_>) -> Result<ProcessOutput> {
    let mut command = command(invocation);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    own_process_group(&mut command);

    tracing::debug!(command = %invocation.display_command(), cwd = %invocation.cwd, "Running");

    // O job existe antes do processo, para prendê-lo assim que ele nasce: o
    // que ele iniciar depois disso já nasce preso também.
    let mut job = ProcessJob::new(false);
    let started = Instant::now();
    let mut child = command.spawn().map_err(|source| LaceError::Spawn {
        program: invocation.program.clone(),
        source,
    })?;
    job.adopt(&child);

    // Uma thread por pipe: ler um depois do outro trava quando o pipe do
    // segundo enche. As linhas chegam aqui por um canal só, com limite: se
    // quem recebe as linhas atrasa, a ferramenta espera, em vez de a fila
    // crescer na memória.
    let (lines, received) = mpsc::sync_channel(LINE_QUEUE);
    let pipes = (child.stdout.take(), child.stderr.take());
    let (Some(stdout), Some(stderr)) = pipes else {
        unreachable!("stdout and stderr are pipes");
    };
    let readers = [
        read_lines(stdout, Stream::Stdout, lines.clone()),
        read_lines(stderr, Stream::Stderr, lines),
    ];

    let mut out = Capture::default();
    let mut err = Capture::default();
    let mut status = None;
    let mut reading = true;
    // Por que o Lace encerrou o processo, e quando mandou o SIGTERM.
    let mut stopped: Option<(Termination, Instant)> = None;
    let mut killed = false;
    let mut exited_at = None;
    let mut checked_at = started;
    loop {
        if reading {
            match received.recv_timeout(POLL) {
                Ok((stream, bytes)) => {
                    if let Some(on_line) = watch.on_line {
                        on_line(stream, trim_line_end(&String::from_utf8_lossy(&bytes)));
                    }
                    match stream {
                        Stream::Stdout => out.push(&bytes),
                        Stream::Stderr => err.push(&bytes),
                    }
                    // Um testbench preso num laço que imprime sem parar
                    // nunca deixa o canal vazio: cancelamento e prazo são
                    // conferidos pelo relógio, não pela falta de linhas.
                    if checked_at.elapsed() < POLL {
                        continue;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => reading = false,
            }
        } else if status.is_none() {
            // Os dois pipes fecharam e o processo continua: só esperar.
            std::thread::sleep(POLL);
        }
        checked_at = Instant::now();

        if status.is_none() {
            status = child
                .try_wait()
                .map_err(LaceError::io("Waiting for process", &invocation.program))?;
            if status.is_some() {
                exited_at = Some(Instant::now());
            }
        }
        if status.is_none() {
            match stopped {
                None => {
                    let reason = if watch.cancel.is_some_and(CancelToken::is_cancelled) {
                        Some(Termination::Cancelled)
                    } else if watch.timeout.is_some_and(|t| started.elapsed() >= t) {
                        Some(Termination::TimedOut)
                    } else {
                        None
                    };
                    if let Some(reason) = reason {
                        tracing::debug!(?reason, "Stopping");
                        terminate(&mut child, &mut job);
                        stopped = Some((reason, Instant::now()));
                    }
                }
                Some((_, since)) if !killed && since.elapsed() >= KILL_GRACE => {
                    kill(&mut child, &mut job);
                    killed = true;
                }
                Some(_) => {}
            }
        }
        if let Some(at) = exited_at {
            if !reading {
                break;
            }
            // Terminou, mas alguém que ele iniciou ainda segura o pipe: o
            // Lace o encerra e para de ler. No Unix o número do grupo
            // continua reservado enquanto esse processo existir, então o
            // sinal não atinge outro; no Windows quem encerra é o job.
            if at.elapsed() >= DRAIN_LIMIT {
                tracing::warn!(program = %invocation.program, "Output did not close after the process exited");
                kill(&mut child, &mut job);
                break;
            }
        }
    }
    if !reading {
        for reader in readers {
            let _ = reader.join();
        }
    }
    // O que a ferramenta deixou rodando termina com o job.
    job.kill();
    let duration = started.elapsed();
    let status = status.expect("The loop only exits after the process ends");

    let mut termination = termination(status);
    if let Some((reason, _)) = stopped {
        termination = reason;
    } else if !termination.success() && watch.cancel.is_some_and(CancelToken::is_cancelled) {
        // O filho morreu sozinho enquanto o cancelamento era pedido, antes
        // de o Lace encerrá-lo: no Windows, quando o Lace tem console, o
        // filho divide esse console (`hide_console`), e o Ctrl+C chega aos
        // dois.
        termination = Termination::Cancelled;
    }
    let result = ProcessOutput {
        termination,
        stdout: out.into_text(),
        stderr: err.into_text(),
        duration,
    };
    tracing::debug!(termination = ?result.termination, ?duration, "Finished");
    Ok(result)
}

/// O que um pipe escreveu, com teto: os primeiros e os últimos
/// [`OUTPUT_KEPT`] bytes inteiros, e quantos ficaram de fora no meio.
#[derive(Default)]
struct Capture {
    head: Vec<u8>,
    tail: std::collections::VecDeque<u8>,
    dropped: u64,
}

impl Capture {
    fn push(&mut self, bytes: &[u8]) {
        let room = OUTPUT_KEPT.saturating_sub(self.head.len()).min(bytes.len());
        let (head, rest) = bytes.split_at(room);
        self.head.extend_from_slice(head);
        self.tail.extend(rest);
        let excess = self.tail.len().saturating_sub(OUTPUT_KEPT);
        if excess > 0 {
            self.tail.drain(..excess);
            self.dropped += excess as u64;
        }
    }

    /// O texto guardado. Com bytes de fora, uma linha no lugar deles diz
    /// quantos, e o fim começa numa linha inteira.
    fn into_text(self) -> String {
        let mut tail: Vec<u8> = self.tail.into();
        let mut text = String::from_utf8_lossy(&self.head).into_owned();
        if self.dropped > 0 {
            let start = tail.iter().position(|&b| b == b'\n').map_or(0, |i| i + 1);
            let dropped = self.dropped + start as u64;
            tail.drain(..start);
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&format!(
                "[Lace: {dropped} bytes of output left out here]\n"
            ));
        }
        text.push_str(&String::from_utf8_lossy(&tail));
        text
    }
}

/// Lê `pipe` linha a linha numa thread e manda cada linha, com o fim de
/// linha, para `lines`. Termina quando o pipe fecha.
fn read_lines(
    pipe: impl Read + Send + 'static,
    stream: Stream,
    lines: mpsc::SyncSender<(Stream, Vec<u8>)>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        loop {
            let mut line = Vec::new();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if lines.send((stream, line)).is_err() {
                        break;
                    }
                }
            }
        }
    })
}

fn trim_line_end(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
}

/// Põe o filho num grupo de processos próprio, para encerrar depois tudo o
/// que ele iniciar. Fora do grupo do Lace, o Ctrl+C do terminal não chega ao
/// filho: quem o encerra é o Lace, ao ver o cancelamento.
#[cfg(unix)]
fn own_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn own_process_group(_command: &mut Command) {}

/// Faz `command` rodar sem janela de console no Windows.
///
/// Um programa de console iniciado por um programa gráfico, como o Lace
/// Studio, ganharia uma janela própria por ferramenta, e fechar essa janela
/// mataria a ferramenta. Sem console, então, o filho vai com
/// `CREATE_NO_WINDOW`: um console próprio, sem janela, que os processos que
/// ele iniciar herdam. Com console ([`has_console`]: o de um terminal, ou o
/// que quem criou o Lace lhe deu), o filho divide o do Lace, como sempre: não
/// abre janela nenhuma, e o `CREATE_NO_WINDOW` só custaria um console novo
/// (um `conhost.exe`) por ferramenta, uns 16 ms em cada uma. A saída vai
/// pelos pipes nos dois casos. Fora do Windows não faz nada.
pub fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    if !has_console() {
        use std::os::windows::process::CommandExt;
        // `CREATE_NO_WINDOW`, de `WinBase.h`.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// O Lace tem console, com janela ou sem, mesmo com os três fluxos padrão
/// redirecionados. O `CONOUT$`, a tela do console do processo, só abre
/// quando ele existe: é o jeito de saber sem código `unsafe`.
#[cfg(windows)]
fn has_console() -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(r"\\.\CONOUT$")
        .is_ok()
}

/// Um processo filho e tudo o que ele iniciar, presos juntos.
///
/// No Windows é um Job Object com `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`:
/// [`ProcessJob::kill`] encerra a árvore inteira, e soltar o valor também.
/// Como o sistema fecha os handles de um processo que morre, as ferramentas
/// terminam junto com o Lace mesmo quando ele é encerrado à força (o Studio
/// fechado pelo Gerenciador de Tarefas, o `child.kill()` de uma extensão de
/// editor) em vez de ficarem órfãs. Fora do Windows não faz nada: lá cada
/// passo roda num grupo de processos próprio.
///
/// Quem cria processo fora do Core, como o Studio ao chamar a CLI `lace`,
/// usa o mesmo mecanismo:
///
/// ```no_run
/// let mut command = std::process::Command::new("lace");
/// lace_core::hide_console(&mut command);
/// let mut job = lace_core::ProcessJob::new(false);
/// let mut child = command.spawn()?;
/// job.adopt(&child);
/// // Para cancelar: o processo e, pelo job, tudo o que ele iniciou.
/// let _ = child.kill();
/// job.kill();
/// # Ok::<(), std::io::Error>(())
/// ```
pub struct ProcessJob {
    #[cfg(windows)]
    job: Option<win32job::Job>,
}

impl ProcessJob {
    /// Um job vazio. Com `allow_breakaway`, um processo preso pode iniciar
    /// outro fora do job (`CREATE_BREAKAWAY_FROM_JOB`): o assistente de
    /// instalação que o `lace update` abre precisa continuar aberto depois
    /// que o `lace` termina.
    ///
    /// Se o sistema recusar o job, o valor fica inativo:
    /// [`ProcessJob::adopt`] devolve `false`, e quem chama encerra a árvore de
    /// outro jeito.
    pub fn new(allow_breakaway: bool) -> ProcessJob {
        #[cfg(windows)]
        {
            let mut limits = win32job::ExtendedLimitInfo::new();
            limits.limit_kill_on_job_close();
            if allow_breakaway {
                limits.limit_breakaway_ok();
            }
            let job = win32job::Job::create_with_limit_info(&limits)
                .inspect_err(|error| tracing::warn!(%error, "Creating the job object"))
                .ok();
            ProcessJob { job }
        }
        #[cfg(not(windows))]
        {
            let _ = allow_breakaway;
            ProcessJob {}
        }
    }

    /// Prende `child` ao job; o que ele iniciar daqui em diante nasce preso
    /// também. `false` se não deu, e sempre fora do Windows.
    pub fn adopt(&mut self, child: &Child) -> bool {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let Some(job) = &self.job else {
                return false;
            };
            // O job só usa o handle durante a chamada; ele continua de `child`.
            match job.assign_process(child.as_raw_handle() as isize) {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(%error, "Assigning the process to the job object");
                    self.job = None;
                    false
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = child;
            false
        }
    }

    /// Encerra todos os processos do job e o fecha. `false` se não havia job
    /// ativo: aí cabe a quem chama encerrar o processo.
    pub fn kill(&mut self) -> bool {
        // Fechar o último handle de um job com `KILL_ON_JOB_CLOSE` encerra
        // todos os processos dele.
        #[cfg(windows)]
        {
            self.job.take().is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    /// `true` enquanto há um job prendendo processos (só no Windows).
    pub fn is_active(&self) -> bool {
        #[cfg(windows)]
        {
            self.job.is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}

impl std::fmt::Debug for ProcessJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessJob")
            .field("active", &self.is_active())
            .finish()
    }
}

/// Pede ao processo e a tudo o que ele iniciou que terminem.
#[cfg(unix)]
fn terminate(child: &mut Child, _job: &mut ProcessJob) {
    signal_group(child, rustix::process::Signal::TERM);
}

/// Mata o processo e tudo o que ele iniciou. Chamado antes de o processo
/// ser colhido (`try_wait`), quando o número do grupo ainda é dele, ou
/// depois, quando um processo do grupo continua vivo segurando o pipe.
#[cfg(unix)]
fn kill(child: &mut Child, _job: &mut ProcessJob) {
    signal_group(child, rustix::process::Signal::KILL);
}

#[cfg(unix)]
fn signal_group(child: &Child, signal: rustix::process::Signal) {
    let group = rustix::process::Pid::from_child(child);
    if let Err(error) = rustix::process::kill_process_group(group, signal) {
        tracing::debug!(%error, "Signaling the process group");
    }
}

/// No Windows não há SIGTERM: a árvore termina de uma vez, pelo job. Sem
/// job (o sistema o recusou), o `taskkill /T /F` percorre a árvore.
#[cfg(windows)]
fn terminate(child: &mut Child, job: &mut ProcessJob) {
    if job.is_active() {
        kill(child, job);
        return;
    }
    let system32 = std::env::var("SystemRoot")
        .map(|root| Utf8PathBuf::from(root).join("System32"))
        .unwrap_or_else(|_| Utf8PathBuf::from(r"C:\Windows\System32"));
    let mut taskkill = Command::new(system32.join("taskkill.exe").as_std_path());
    taskkill
        .args(["/T", "/F", "/PID", &child.id().to_string()])
        .current_dir(system32.as_std_path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_console(&mut taskkill);
    let killed = taskkill.status().is_ok_and(|s| s.success());
    if !killed {
        let _ = child.kill();
    }
}

/// Mata o processo (código de saída 1) e, pelo job, o resto da árvore. Sem
/// job, só o processo.
#[cfg(windows)]
fn kill(child: &mut Child, job: &mut ProcessJob) {
    let _ = child.kill();
    job.kill();
}

#[cfg(not(any(unix, windows)))]
fn terminate(child: &mut Child, _job: &mut ProcessJob) {
    let _ = child.kill();
}

#[cfg(not(any(unix, windows)))]
fn kill(child: &mut Child, _job: &mut ProcessJob) {
    let _ = child.kill();
}

/// Um processo que continua rodando depois que a chamada retorna, como o
/// Surfer aberto por [`open_waveform`](crate::open_waveform).
///
/// Soltar o valor **não** mata o processo: uma janela aberta pelo usuário
/// continua aberta mesmo que a interface esqueça o handle. Para fechar, use
/// [`RunningProcess::kill`]. stdout e stderr vão para
/// [`RunningProcess::log_file`].
#[derive(Debug)]
pub struct RunningProcess {
    child: Child,
    command: Invocation,
    log: Utf8PathBuf,
}

impl RunningProcess {
    /// Identificador do processo no sistema operacional.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// O que foi executado.
    pub fn command(&self) -> &Invocation {
        &self.command
    }

    /// Arquivo que recebe stdout e stderr do processo.
    pub fn log_file(&self) -> &Utf8Path {
        &self.log
    }

    /// `Some` se já terminou, sem bloquear.
    pub fn try_wait(&mut self) -> Result<Option<Termination>> {
        self.child
            .try_wait()
            .map(|s| s.map(termination))
            .map_err(LaceError::io("Querying process", &self.command.program))
    }

    /// Espera até `timeout` pelo fim do processo. `None` se ele continua
    /// rodando. Serve para perceber uma aplicação gráfica que não abriu
    /// (sem display, arquivo inválido) sem bloquear indefinidamente.
    pub fn wait_timeout(&mut self, timeout: Duration) -> Result<Option<Termination>> {
        let started = Instant::now();
        loop {
            if let Some(t) = self.try_wait()? {
                return Ok(Some(t));
            }
            if started.elapsed() >= timeout {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Confere que o processo continua rodando depois de `grace`. Uma
    /// aplicação gráfica que não consegue abrir (sem display, arquivo
    /// inválido) termina em menos de um segundo; aqui isso vira erro, com o
    /// fim do log.
    ///
    /// # Erros
    ///
    /// [`LaceError::ProcessExitedEarly`] se o processo terminou dentro de
    /// `grace`.
    pub fn ensure_started(&mut self, grace: Duration) -> Result<()> {
        let Some(termination) = self.wait_timeout(grace)? else {
            return Ok(());
        };
        let log = std::fs::read_to_string(&self.log).unwrap_or_default();
        let lines: Vec<&str> = log.lines().collect();
        let tail = lines[lines.len().saturating_sub(12)..].join("\n");
        Err(LaceError::ProcessExitedEarly {
            program: self.command.program.clone(),
            termination,
            log: self.log.clone(),
            tail,
        })
    }

    /// Bloqueia até o processo terminar.
    pub fn wait(&mut self) -> Result<Termination> {
        self.child
            .wait()
            .map(termination)
            .map_err(LaceError::io("Waiting for process", &self.command.program))
    }

    /// Encerra o processo. Não é erro se ele já tinha terminado.
    pub fn kill(&mut self) -> Result<()> {
        if self.try_wait()?.is_some() {
            return Ok(());
        }
        self.child
            .kill()
            .map_err(LaceError::io("Stopping process", &self.command.program))?;
        self.child
            .wait()
            .map(drop)
            .map_err(LaceError::io("Stopping process", &self.command.program))
    }
}

/// Inicia `invocation` sem esperar, com stdout e stderr num arquivo de log.
pub(crate) fn spawn(invocation: &Invocation, log: &Utf8Path) -> Result<RunningProcess> {
    let file = std::fs::File::create(log).map_err(LaceError::io("Creating log", log))?;
    let file_err = file
        .try_clone()
        .map_err(LaceError::io("Creating log", log))?;
    let mut command = command(invocation);
    command.stdout(file).stderr(file_err);

    tracing::debug!(command = %invocation.display_command(), cwd = %invocation.cwd, "Starting");
    let child = command.spawn().map_err(|source| LaceError::Spawn {
        program: invocation.program.clone(),
        source,
    })?;
    Ok(RunningProcess {
        child,
        command: invocation.clone(),
        log: log.to_owned(),
    })
}

fn termination(status: std::process::ExitStatus) -> Termination {
    if let Some(code) = status.code() {
        // No Windows um processo que quebra "sai" com o NTSTATUS da exceção.
        // Os de severidade erro têm os dois bits altos ligados (0xC...).
        #[cfg(windows)]
        if code as u32 >= 0xC000_0000 {
            return Termination::Exception(code as u32);
        }
        return Termination::Exited(code);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return Termination::Signaled(signal);
        }
    }
    Termination::Unknown
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn stored_output_keeps_the_start_and_the_end() {
        let mut capture = Capture::default();
        capture.push(b"primeira linha\n");
        let line = [b'x'; 99];
        for _ in 0..(3 * OUTPUT_KEPT / 100) {
            capture.push(&line);
            capture.push(b"\n");
        }
        capture.push(b"$finish called\n");
        let text = capture.into_text();
        assert!(text.starts_with("primeira linha\n"));
        assert!(text.ends_with("$finish called\n"));
        assert!(text.contains("bytes of output left out here"));
        assert!(text.len() <= 2 * OUTPUT_KEPT + 100);
        // Pouca saída fica inteira.
        let mut small = Capture::default();
        small.push(b"a\n");
        small.push(b"b\n");
        assert_eq!(small.into_text(), "a\nb\n");
    }

    fn sh(script: &str) -> Invocation {
        Invocation::new("/bin/sh", "/").arg("-c").arg(script)
    }

    fn run(invocation: &Invocation) -> Result<ProcessOutput> {
        super::run(invocation, &Watch::default())
    }

    #[test]
    fn distinguishes_exit_code_from_signal() {
        let exited = run(&sh("exit 3")).unwrap();
        assert_eq!(exited.termination, Termination::Exited(3));

        let killed = run(&sh("kill -ABRT $$")).unwrap();
        assert_eq!(killed.termination, Termination::Signaled(6));
    }

    #[test]
    fn large_output_on_both_pipes_does_not_deadlock() {
        // ~1 MiB em cada pipe, bem acima do buffer de 64 KiB do Linux.
        let out = run(&sh("i=0; while [ $i -lt 16384 ]; do \
             echo 0123456789012345678901234567890123456789012345678901234567890123; \
             echo 0123456789012345678901234567890123456789012345678901234567890123 >&2; \
             i=$((i+1)); done"))
        .unwrap();
        assert!(out.termination.success());
        assert_eq!(out.stdout.lines().count(), 16384);
        assert_eq!(out.stderr.lines().count(), 16384);
    }

    #[test]
    fn environment_is_not_inherited() {
        // HOME existe no processo de teste; o filho não pode enxergá-la.
        assert!(std::env::var_os("HOME").is_some());
        let out = run(&sh("echo \"[$HOME]\"").env("LACE_X", "1")).unwrap();
        assert_eq!(out.stdout.trim(), "[]");
    }

    #[test]
    fn cwd_is_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = crate::paths::canonicalize(Utf8Path::from_path(dir.path()).unwrap()).unwrap();
        let out = run(&Invocation::new("/bin/sh", &cwd).arg("-c").arg("pwd")).unwrap();
        assert_eq!(out.stdout.trim(), cwd.as_str());
    }

    #[test]
    fn inherit_and_search_path_are_explicit() {
        let out = run(&sh("echo \"[$HOME][$PATH]\"")
            .inherit(&["HOME"])
            .search_path(&["/usr/bin".into()]))
        .unwrap();
        let home = std::env::var("HOME").unwrap();
        assert_eq!(out.stdout.trim(), format!("[{home}][/usr/bin]"));
    }

    #[test]
    fn spawned_process_can_be_killed() {
        let dir = tempfile::tempdir().unwrap();
        let log = Utf8PathBuf::from_path_buf(dir.path().join("log.txt")).unwrap();
        let mut early = spawn(&sh("echo falhou; exit 3"), &log).unwrap();
        match early.ensure_started(Duration::from_secs(5)) {
            Err(LaceError::ProcessExitedEarly {
                termination, tail, ..
            }) => {
                assert_eq!(termination, Termination::Exited(3));
                assert!(tail.contains("falhou"));
            }
            other => panic!("{other:?}"),
        }

        let mut proc = spawn(&sh("echo iniciou; sleep 30"), &log).unwrap();
        let started = Instant::now();
        while !std::fs::read_to_string(&log).unwrap().contains("iniciou") {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "o log não recebeu a saída"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(proc.try_wait().unwrap().is_none());
        proc.ensure_started(Duration::from_millis(50)).unwrap();
        proc.kill().unwrap();
        assert!(proc.try_wait().unwrap().is_some());
    }

    #[test]
    fn missing_program_is_a_spawn_error() {
        let err = run(&Invocation::new("/nao/existe", "/")).unwrap_err();
        assert!(matches!(err, LaceError::Spawn { .. }));
    }

    /// Pede o cancelamento de `token` depois de `delay`, de outra thread,
    /// como faria o botão de parar de uma interface.
    fn cancel_after(token: &CancelToken, delay: Duration) {
        let token = token.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            token.cancel();
        });
    }

    /// Espera o processo `pid` sumir. Um processo morto continua visível
    /// como zumbi até ser colhido, por isso a espera.
    fn gone(pid: &str) -> bool {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(5) {
            let alive = Command::new("/bin/kill")
                .args(["-0", pid])
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if !alive {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn cancel_stops_the_process_and_what_it_started() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("neto.pid");
        let script = format!(
            "sleep 30 & echo $! > '{}'; echo iniciou; wait",
            pid_file.display()
        );
        let token = CancelToken::new();
        cancel_after(&token, Duration::from_millis(300));
        let watch = Watch {
            cancel: Some(&token),
            ..Watch::default()
        };
        let started = Instant::now();
        let out = super::run(&sh(&script), &watch).unwrap();
        assert_eq!(out.termination, Termination::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(out.stdout.trim(), "iniciou");
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        assert!(gone(pid.trim()), "o sleep que o sh iniciou continuou vivo");
    }

    #[test]
    fn what_outlives_the_process_is_stopped_too() {
        // O sh termina na hora, mas o sleep que ele deixou em segundo plano
        // segura o stdout.
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("neto.pid");
        let script = format!("sleep 30 & echo $! > '{}'", pid_file.display());
        let started = Instant::now();
        let out = run(&sh(&script)).unwrap();
        assert!(out.termination.success());
        assert!(started.elapsed() < Duration::from_secs(10));
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        assert!(gone(pid.trim()), "o sleep continuou vivo");
    }

    #[test]
    fn timeout_stops_the_process_and_keeps_its_output() {
        let watch = Watch {
            timeout: Some(Duration::from_millis(300)),
            ..Watch::default()
        };
        let started = Instant::now();
        let out = super::run(&sh("echo antes; sleep 30"), &watch).unwrap();
        assert_eq!(out.termination, Termination::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(out.stdout.trim(), "antes");
    }

    #[test]
    fn lines_arrive_while_the_process_runs() {
        let started = Instant::now();
        let seen = std::sync::Mutex::new(Vec::new());
        let on_line = |stream: Stream, line: &str| {
            seen.lock()
                .unwrap()
                .push((stream, line.to_owned(), started.elapsed()));
        };
        let watch = Watch {
            on_line: Some(&on_line),
            ..Watch::default()
        };
        let out = super::run(
            &sh("echo cedo; echo erro >&2; sleep 1; printf tarde"),
            &watch,
        )
        .unwrap();
        assert!(out.termination.success());
        assert_eq!(out.stdout, "cedo\ntarde");
        assert_eq!(out.stderr, "erro\n");

        let seen = seen.into_inner().unwrap();
        let lines: Vec<_> = seen.iter().map(|(s, l, _)| (*s, l.as_str())).collect();
        assert!(lines.contains(&(Stream::Stdout, "cedo")), "{lines:?}");
        assert!(lines.contains(&(Stream::Stderr, "erro")), "{lines:?}");
        assert_eq!(
            lines.last(),
            Some(&(Stream::Stdout, "tarde")),
            "a última linha, sem fim de linha, também chega"
        );
        let early = seen.iter().find(|(_, l, _)| l == "cedo").unwrap().2;
        assert!(
            early < Duration::from_millis(800),
            "a linha só chegou no fim: {early:?}"
        );
    }

    #[test]
    fn cancel_works_while_the_process_floods_the_output() {
        let token = CancelToken::new();
        cancel_after(&token, Duration::from_millis(300));
        let count = std::sync::atomic::AtomicUsize::new(0);
        let on_line = |_: Stream, _: &str| {
            count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        };
        let watch = Watch {
            cancel: Some(&token),
            on_line: Some(&on_line),
            ..Watch::default()
        };
        let started = Instant::now();
        let out = super::run(&sh("while :; do echo 0123456789; done"), &watch).unwrap();
        assert_eq!(out.termination, Termination::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(count.load(std::sync::atomic::Ordering::Relaxed) > 0);
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    fn cmd(script: &str) -> Invocation {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let cmd = Utf8PathBuf::from(root).join("System32").join("cmd.exe");
        Invocation::new(cmd, std::env::temp_dir().to_string_lossy().into_owned())
            .arg("/C")
            .arg(script)
    }

    fn run(invocation: &Invocation) -> Result<ProcessOutput> {
        super::run(invocation, &Watch::default())
    }

    #[test]
    fn exit_code_is_reported() {
        let out = run(&cmd("exit 3")).unwrap();
        assert_eq!(out.termination, Termination::Exited(3));
    }

    #[test]
    fn environment_is_not_inherited() {
        // Com o ambiente limpo, o cmd deixa a variável sem expandir.
        assert!(std::env::var_os("USERNAME").is_some() || std::env::var_os("USER").is_some());
        let out = run(&cmd("echo [%USERNAME%]")).unwrap();
        assert!(out.stdout.contains("[%USERNAME%]"), "{}", out.stdout);
        // Só o que o Windows exige passa.
        let out = run(&cmd("echo [%SystemRoot%]")).unwrap();
        assert!(!out.stdout.contains("%SystemRoot%"), "{}", out.stdout);
    }

    #[test]
    fn cwd_is_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = crate::paths::canonicalize(Utf8Path::from_path(dir.path()).unwrap()).unwrap();
        let mut inv = cmd("cd");
        inv.cwd = cwd.clone();
        let out = run(&inv).unwrap();
        assert_eq!(
            out.stdout.trim().to_ascii_lowercase(),
            cwd.as_str().to_ascii_lowercase()
        );
    }

    #[test]
    fn large_output_on_both_pipes_does_not_deadlock() {
        let out = run(&cmd(
            "for /L %i in (1,1,4000) do @(echo 0123456789012345678901234567890123456789 & echo 0123456789012345678901234567890123456789 1>&2)",
        ))
        .unwrap();
        assert!(out.termination.success());
        assert_eq!(out.stdout.lines().count(), 4000);
        assert_eq!(out.stderr.lines().count(), 4000);
    }

    #[test]
    fn spawned_process_can_be_killed() {
        let dir = tempfile::tempdir().unwrap();
        let log = Utf8PathBuf::from_path_buf(dir.path().join("log.txt")).unwrap();
        // Laço interno do cmd: sem PATH (o ambiente é limpo), nenhum programa
        // externo como `ping` seria encontrado.
        let mut proc = spawn(&cmd("for /L %i in (0,0,1) do @rem"), &log).unwrap();
        proc.ensure_started(Duration::from_millis(300)).unwrap();
        proc.kill().unwrap();
        assert!(proc.try_wait().unwrap().is_some());
    }

    #[test]
    fn missing_program_is_a_spawn_error() {
        let err = run(&Invocation::new(r"C:\nao\existe.exe", r"C:\")).unwrap_err();
        assert!(matches!(err, LaceError::Spawn { .. }));
    }

    #[test]
    fn cancel_stops_the_process() {
        let token = CancelToken::new();
        let from_ui = token.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            from_ui.cancel();
        });
        let watch = Watch {
            cancel: Some(&token),
            ..Watch::default()
        };
        let started = Instant::now();
        let out = super::run(&cmd("echo iniciou & for /L %i in (0,0,1) do @rem"), &watch).unwrap();
        assert_eq!(out.termination, Termination::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(out.stdout.contains("iniciou"), "{}", out.stdout);
    }

    /// Um script do PowerShell, por `-EncodedCommand`: sem as aspas do `cmd`
    /// no meio. Só tipos do .NET, que não dependem dos módulos do PowerShell
    /// (o ambiente do filho é limpo).
    fn powershell(script: &str) -> Invocation {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let exe = Utf8PathBuf::from(root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        Invocation::new(exe, std::env::temp_dir().to_string_lossy().into_owned())
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-EncodedCommand")
            .arg(base64(&utf16))
    }

    fn base64(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let byte = |i: usize| u32::from(chunk.get(i).copied().unwrap_or(0));
            let group = (byte(0) << 16) | (byte(1) << 8) | byte(2);
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(char::from(TABLE[((group >> (18 - 6 * i)) & 63) as usize]));
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    /// Script que inicia um "neto": outro PowerShell, que dorme um minuto, e
    /// grava o PID dele em `pid_file`.
    fn start_grandchild(pid_file: &std::path::Path) -> String {
        format!(
            "$ps = $env:SystemRoot + '\\System32\\WindowsPowerShell\\v1.0\\powershell.exe'; \
             $info = [Diagnostics.ProcessStartInfo]::new($ps, \
                 '-NoProfile -NonInteractive -Command [Threading.Thread]::Sleep(60000)'); \
             $info.UseShellExecute = $false; \
             $neto = [Diagnostics.Process]::Start($info); \
             [IO.File]::WriteAllText('{}', [string]$neto.Id)",
            pid_file.display()
        )
    }

    /// O PID gravado em `pid_file`, esperando até ele aparecer.
    fn wait_for_pid(pid_file: &std::path::Path) -> u32 {
        let started = Instant::now();
        loop {
            if let Some(pid) = std::fs::read_to_string(pid_file)
                .ok()
                .and_then(|text| text.trim().parse().ok())
            {
                return pid;
            }
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "o PID não apareceu em {}",
                pid_file.display()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Espera o processo `pid` sumir da lista do sistema.
    fn gone(pid: u32) -> bool {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let tasklist = std::path::Path::new(&root).join(r"System32\tasklist.exe");
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            let listed = Command::new(&tasklist)
                .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\"")));
            if listed.is_ok_and(|alive| !alive) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }

    #[test]
    fn cancel_stops_the_process_and_what_it_started() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("neto.pid");
        let script = format!(
            "{}; [Console]::Out.WriteLine('iniciou'); [Threading.Thread]::Sleep(60000)",
            start_grandchild(&pid_file)
        );
        let token = CancelToken::new();
        let from_ui = token.clone();
        let pid_path = pid_file.clone();
        // Cancela só depois que o neto existe.
        std::thread::spawn(move || {
            wait_for_pid(&pid_path);
            from_ui.cancel();
        });
        let watch = Watch {
            cancel: Some(&token),
            ..Watch::default()
        };
        let out = super::run(&powershell(&script), &watch).unwrap();
        assert_eq!(out.termination, Termination::Cancelled, "{out:?}");
        let pid = wait_for_pid(&pid_file);
        assert!(
            gone(pid),
            "o processo que a ferramenta iniciou continuou vivo"
        );
    }

    #[test]
    fn what_outlives_the_process_is_stopped_too() {
        // O PowerShell termina na hora, mas o neto que ele deixou rodando
        // herdou o stdout e o segura.
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("neto.pid");
        let started = Instant::now();
        let out = run(&powershell(&start_grandchild(&pid_file))).unwrap();
        assert!(out.termination.success(), "{out:?}");
        assert!(started.elapsed() < Duration::from_secs(30));
        let pid = wait_for_pid(&pid_file);
        assert!(gone(pid), "o neto continuou vivo");
    }

    /// Roda num processo à parte, criado por
    /// `the_tools_die_with_the_lace_process`: prende uma ferramenta que dorme
    /// e grava o PID dela em `LACE_TEST_PID_FILE`. Sem a variável, não faz
    /// nada.
    #[test]
    #[ignore = "processo auxiliar de the_tools_die_with_the_lace_process"]
    fn helper_runs_a_long_tool() {
        let Ok(pid_file) = std::env::var("LACE_TEST_PID_FILE") else {
            return;
        };
        let _ = run(&powershell(&format!(
            "[IO.File]::WriteAllText('{pid_file}', [string]$PID); [Threading.Thread]::Sleep(120000)"
        )));
    }

    #[test]
    fn the_tools_die_with_the_lace_process() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("ferramenta.pid");
        let mut lace = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process::windows_tests::helper_runs_a_long_tool",
                "--ignored",
                "--test-threads=1",
            ])
            .env("LACE_TEST_PID_FILE", &pid_file)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = wait_for_pid(&pid_file);
        // Como o Gerenciador de Tarefas: TerminateProcess, sem chance de o
        // Lace limpar nada.
        lace.kill().unwrap();
        lace.wait().unwrap();
        assert!(
            gone(pid),
            "a ferramenta ficou órfã depois que o Lace morreu"
        );
    }

    /// Roda num processo à parte, criado por [`tool_console_under`]: inicia
    /// uma ferramenta e escreve a janela de console dela (0 sem janela) e os
    /// processos que dividem o console dela. Sem `LACE_TEST_CONSOLE`, não faz
    /// nada.
    #[test]
    #[ignore = "processo auxiliar dos testes de console"]
    // O stdout é o canal com o teste que criou este processo.
    #[allow(clippy::print_stdout)]
    fn helper_reports_the_tool_console() {
        if std::env::var_os("LACE_TEST_CONSOLE").is_none() {
            return;
        }
        let script = "Add-Type -Namespace Lace -Name Console -MemberDefinition \
                      '[DllImport(\"kernel32.dll\")] public static extern System.IntPtr GetConsoleWindow(); \
                       [DllImport(\"kernel32.dll\")] public static extern uint GetConsoleProcessList(uint[] list, uint count);'; \
                      $list = [uint32[]]::new(64); \
                      $n = [Lace.Console]::GetConsoleProcessList($list, 64); \
                      [Console]::Out.WriteLine([Lace.Console]::GetConsoleWindow()); \
                      [Console]::Out.WriteLine($list[0..($n - 1)] -join ',')";
        let out = run(&powershell(script).inherit(GUI_ENV)).unwrap();
        let mut lines = out.stdout.lines();
        println!(
            "LACE_CONSOLE {} {}",
            lines.next().unwrap_or("?"),
            lines.next().unwrap_or_default()
        );
    }

    /// O que a ferramenta viu, iniciada por um Lace (o processo auxiliar)
    /// criado com `flags`: a janela de console dela, os processos do console
    /// dela e o PID do Lace.
    fn tool_console_under(flags: u32) -> (String, Vec<u32>, u32) {
        use std::os::windows::process::CommandExt;
        let lace = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process::windows_tests::helper_reports_the_tool_console",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("LACE_TEST_CONSOLE", "1")
            .creation_flags(flags)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = lace.id();
        let out = lace.wait_with_output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        // O libtest escreve a saída do teste na linha do nome dele.
        let line = text
            .lines()
            .find_map(|l| l.split_once("LACE_CONSOLE ").map(|(_, rest)| rest))
            .unwrap_or_else(|| panic!("o auxiliar não respondeu: {text}"));
        let mut parts = line.split(' ');
        let window = parts.next().unwrap_or_default().to_owned();
        let attached = parts
            .next()
            .unwrap_or_default()
            .split(',')
            .filter_map(|p| p.trim().parse().ok())
            .collect();
        (window, attached, pid)
    }

    #[test]
    fn without_a_console_the_tool_gets_no_window() {
        // Um Lace sem console, como o Studio: sem CREATE_NO_WINDOW, a
        // ferramenta ganharia um console novo, com janela.
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        let (window, attached, _) = tool_console_under(DETACHED_PROCESS);
        assert_eq!(window, "0", "a ferramenta abriu janela de console");
        assert!(!attached.is_empty(), "a ferramenta ficou sem console");
    }

    #[test]
    fn with_a_console_the_tool_shares_it() {
        // Um Lace com console, aqui um sem janela e com os três fluxos fora
        // dele, como o de uma IDE: a ferramenta divide o console do Lace em
        // vez de ganhar um próprio, que custaria um conhost.exe por
        // ferramenta.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let (window, attached, lace) = tool_console_under(CREATE_NO_WINDOW);
        assert_eq!(window, "0", "a ferramenta abriu janela de console");
        assert!(
            attached.contains(&lace),
            "a ferramenta não divide o console do Lace: {attached:?}"
        );
    }
}
