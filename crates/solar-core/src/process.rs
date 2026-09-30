//! O único módulo do Core que cria processos.
//!
//! Cada [`Invocation`] declara programa (caminho absoluto), argumentos,
//! diretório de trabalho e ambiente. Nada é herdado implicitamente: o CWD é
//! parte do contrato dos compiladores YANC (o Verilog gerado embute caminhos),
//! e o ambiente parte de vazio mais uma lista mínima de variáveis que o sistema
//! operacional exige.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;

use crate::error::{Result, SolarError};

/// Variáveis repassadas do ambiente do Solar ao filho. No Windows, sem
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
/// que o Solar rodou, presente em todo [`StepReport`](crate::StepReport).
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Invocation {
    /// Caminho absoluto do executável. Nunca é procurado no `PATH`.
    pub program: Utf8PathBuf,
    /// Argumentos, na ordem. Caminhos já estão no formato nativo do sistema.
    pub args: Vec<String>,
    /// Diretório de trabalho. Faz parte do contrato de várias ferramentas (o
    /// `appcomp` e o `asmcomp` leem `app_log.txt` relativo a ele, o testbench
    /// grava a onda nele), por isso nunca é herdado.
    pub cwd: Utf8PathBuf,
    /// Variáveis definidas pelo Solar, somadas às de `INHERITED_ENV`.
    pub env: Vec<(String, String)>,
    /// Nomes de variáveis copiadas do ambiente do Solar, quando existirem.
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

    /// Copia do ambiente do Solar as variáveis `keys`, se existirem.
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
/// `{"kind": "unknown"}`.
///
/// Um código de saída e um sinal são coisas diferentes: `msg_internal` do
/// `cppcomp` chama `abort()`, e um `asmcomp` que perde um `fopen` morre por
/// segfault. Nenhum dos dois é um "erro de compilação" do usuário.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

/// Executa `invocation` até o fim e coleta stdout e stderr.
///
/// `Err` só quando o processo nem começou (binário ausente, sem permissão,
/// CWD inexistente). Um processo que roda e falha volta como `Ok` com a
/// [`Termination`] correspondente.
fn command(invocation: &Invocation) -> Command {
    let mut command = Command::new(invocation.program.as_std_path());
    command
        .args(&invocation.args)
        .current_dir(invocation.cwd.as_std_path())
        .env_clear()
        .stdin(Stdio::null());

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

pub(crate) fn run(invocation: &Invocation) -> Result<ProcessOutput> {
    let mut command = command(invocation);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    tracing::debug!(command = %invocation.display_command(), cwd = %invocation.cwd, "executando");

    let started = Instant::now();
    // `output()` lê stdout e stderr ao mesmo tempo (threads internas da std).
    // Ler um depois do outro trava quando o pipe do segundo enche.
    let output = command.output().map_err(|source| SolarError::Spawn {
        program: invocation.program.clone(),
        source,
    })?;
    let duration = started.elapsed();

    let result = ProcessOutput {
        termination: termination(output.status),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        duration,
    };
    tracing::debug!(termination = ?result.termination, ?duration, "terminou");
    Ok(result)
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
            .map_err(SolarError::io(
                "consultando processo",
                &self.command.program,
            ))
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
    /// [`SolarError::ProcessExitedEarly`] se o processo terminou dentro de
    /// `grace`.
    pub fn ensure_started(&mut self, grace: Duration) -> Result<()> {
        let Some(termination) = self.wait_timeout(grace)? else {
            return Ok(());
        };
        let log = std::fs::read_to_string(&self.log).unwrap_or_default();
        let lines: Vec<&str> = log.lines().collect();
        let tail = lines[lines.len().saturating_sub(12)..].join("\n");
        Err(SolarError::ProcessExitedEarly {
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
            .map_err(SolarError::io("esperando processo", &self.command.program))
    }

    /// Encerra o processo. Não é erro se ele já tinha terminado.
    pub fn kill(&mut self) -> Result<()> {
        if self.try_wait()?.is_some() {
            return Ok(());
        }
        self.child
            .kill()
            .map_err(SolarError::io("encerrando processo", &self.command.program))?;
        self.child
            .wait()
            .map(drop)
            .map_err(SolarError::io("encerrando processo", &self.command.program))
    }
}

/// Inicia `invocation` sem esperar, com stdout e stderr num arquivo de log.
pub(crate) fn spawn(invocation: &Invocation, log: &Utf8Path) -> Result<RunningProcess> {
    let file = std::fs::File::create(log).map_err(SolarError::io("criando log", log))?;
    let file_err = file
        .try_clone()
        .map_err(SolarError::io("criando log", log))?;
    let mut command = command(invocation);
    command.stdout(file).stderr(file_err);

    tracing::debug!(command = %invocation.display_command(), cwd = %invocation.cwd, "iniciando");
    let child = command.spawn().map_err(|source| SolarError::Spawn {
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

    fn sh(script: &str) -> Invocation {
        Invocation::new("/bin/sh", "/").arg("-c").arg(script)
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
        let out = run(&sh("echo \"[$HOME]\"").env("SOLAR_X", "1")).unwrap();
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
            Err(SolarError::ProcessExitedEarly {
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
        assert!(matches!(err, SolarError::Spawn { .. }));
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
        assert!(matches!(err, SolarError::Spawn { .. }));
    }
}
