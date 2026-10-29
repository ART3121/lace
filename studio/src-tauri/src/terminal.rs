//! O terminal de shell: um pseudoterminal com o shell do usuário, o TCMD da
//! AURORA. A interface o mostra com o xterm.js.
//!
//! No Windows, o shell é o da preferência `terminal_shell` (PowerShell ou
//! Prompt de Comando); nos outros sistemas, o do usuário (`$SHELL`).
//!
//! O shell abre na pasta do projeto, com a pasta `bin/` da instalação do
//! Lace no começo do `PATH`, para o usuário poder rodar `aurora` direto. Ao
//! trocar de projeto, a interface pede [`terminal_cd`], que digita no shell
//! o `cd` para a pasta nova, como o "abrir o terminal aqui" da AURORA: o
//! histórico e o que estava na tela ficam.
//!
//! O que o shell escreve chega à interface por um `Channel`, como texto
//! UTF-8. Uma leitura pode cortar um caractere de vários bytes ao meio: os
//! bytes do fim que ainda não formam um caractere ficam para a leitura
//! seguinte.
//!
//! O que vai para o shell (o que se digita, o `cd`) entra numa fila, e uma
//! thread por terminal o escreve: quem chama não espera o shell ler. Com um
//! programa que não lê a entrada, colar um bloco grande travava a janela, porque
//! a escrita rodava na thread principal, com a trava de todos os terminais.
//!
//! O que sai do shell tem controle de fluxo ([`Flow`]): a interface confirma
//! ([`terminal_ack`]) o que o xterm já escreveu, e a leitura para quando ela
//! fica para trás. Sem isso, um `cat` de um VCD grande ou um `vvp` sem
//! `$finish` enchiam a memória da janela, e o Ctrl+C só aparecia depois de
//! tudo o que já estava a caminho.

use std::io::{Read, Write};
use std::sync::{Arc, Condvar, Mutex, mpsc};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::error::{IpcError, IpcResult, codes};
use crate::state::AppState;
use crate::toolchain;

/// Um terminal aberto.
pub struct Terminal {
    master: Box<dyn MasterPty + Send>,
    /// A fila do que vai para o shell, na ordem; a thread de escrita do
    /// terminal termina quando a fila sai (o terminal foi encerrado).
    input: mpsc::Sender<Vec<u8>>,
    child: Box<dyn Child + Send + Sync>,
    shell: ShellKind,
    /// O controle de fluxo da saída, com a thread de leitura.
    flow: Arc<Flow>,
}

/// Fora da lista (encerrado, ou o shell saiu), a leitura não espera mais a
/// interface: lê o resto até o pseudoterminal fechar.
impl Drop for Terminal {
    fn drop(&mut self) {
        self.flow.close();
    }
}

/// O controle de fluxo da saída de um terminal. A thread de leitura conta o
/// que manda à interface e para de ler quando a interface fica mais de
/// [`HIGH_WATER`] atrás; volta quando as confirmações ([`terminal_ack`])
/// deixam o atraso em [`LOW_WATER`]. Com a leitura parada, o pseudoterminal
/// enche, e o programa que escreve nele espera.
#[derive(Default)]
struct Flow {
    state: Mutex<FlowState>,
    changed: Condvar,
}

#[derive(Default)]
struct FlowState {
    /// O que foi mandado e a interface não confirmou, em unidades UTF-16 (o
    /// `length` de uma string do JavaScript, que é o que a interface conta).
    pending: usize,
    /// O terminal saiu da lista: ninguém mais confirma.
    closed: bool,
}

/// O atraso da interface que para a leitura, em unidades UTF-16.
const HIGH_WATER: usize = 128 * 1024;
/// O atraso com que a leitura volta. A interface confirma em lotes menores
/// que isso: o resto de um lote não deixa a leitura parada para sempre.
const LOW_WATER: usize = 16 * 1024;

impl Flow {
    /// Conta `units` mandadas à interface e, se ela ficou para trás, espera.
    fn sent(&self, units: usize) {
        let mut state = self.state.lock().expect("flow lock");
        state.pending += units;
        if state.pending <= HIGH_WATER {
            return;
        }
        while state.pending > LOW_WATER && !state.closed {
            state = self.changed.wait(state).expect("flow lock");
        }
    }

    /// A interface escreveu `units` do que recebeu.
    fn ack(&self, units: usize) {
        let mut state = self.state.lock().expect("flow lock");
        state.pending = state.pending.saturating_sub(units);
        if state.pending <= LOW_WATER {
            self.changed.notify_all();
        }
    }

    fn close(&self) {
        self.state.lock().expect("flow lock").closed = true;
        self.changed.notify_all();
    }
}

impl Terminal {
    /// Põe `bytes` na fila do shell. Erro só se a escrita já parou (o shell
    /// terminou e fechou a entrada).
    fn send(&self, bytes: Vec<u8>) -> IpcResult<()> {
        self.input
            .send(bytes)
            .map_err(|_| IpcError::new(codes::TERMINAL, "The terminal has exited"))
    }

    /// Um programa (vim, python, um comando que ainda roda) está em primeiro
    /// plano no terminal, no lugar do shell: o grupo de processos em primeiro
    /// plano do pseudoterminal não é o do shell, que o abriu como líder de
    /// sessão. No Windows o ConPTY não informa, e a resposta é sempre não.
    #[cfg(unix)]
    fn has_foreground_program(&self) -> bool {
        match (self.master.process_group_leader(), self.child.process_id()) {
            (Some(group), Some(shell)) => u32::try_from(group).is_ok_and(|group| group != shell),
            _ => false,
        }
    }

    #[cfg(not(unix))]
    fn has_foreground_program(&self) -> bool {
        false
    }
}

/// O shell de um terminal, pelo jeito de escrever um `cd` nele.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellKind {
    /// O Windows PowerShell.
    PowerShell,
    /// O Prompt de Comando.
    Cmd,
    /// Um shell POSIX (bash, zsh, sh).
    Posix,
    /// O fish, que escapa aspas de outro jeito.
    Fish,
}

impl ShellKind {
    /// O shell que [`terminal_spawn`] abre: no Windows, o da preferência
    /// `terminal_shell`; nos outros sistemas, o `$SHELL`.
    fn current(choice: &str) -> ShellKind {
        if cfg!(windows) {
            if choice == "cmd" {
                ShellKind::Cmd
            } else {
                ShellKind::PowerShell
            }
        } else if std::env::var("SHELL").is_ok_and(|shell| shell.ends_with("fish")) {
            ShellKind::Fish
        } else {
            ShellKind::Posix
        }
    }

    /// A linha que leva o shell para `path`, com o Enter, ou `None` quando o
    /// caminho tem um caractere de controle.
    ///
    /// A linha vai para o editor de linha do shell como se fosse digitada, e
    /// ali um caractere de controle é tecla, não texto, mesmo entre aspas:
    /// um Enter (CR, LF) executa o que veio antes, um Ctrl+U apaga a linha,
    /// um Ctrl+C a abandona. Um nome de pasta pode trazer esses caracteres no
    /// Linux e no macOS, e o que viesse depois deles rodaria como comando.
    ///
    /// Antes do `cd` vão as teclas que limpam a linha ([`ShellKind::clear_line`]):
    /// o que o usuário digitou e ainda não executou não vira começo do `cd`
    /// (um `rm -rf ` pela metade viraria `rm -rf cd -- '/home/u/proj2'`).
    fn cd_line(self, path: &str) -> Option<String> {
        if path.chars().any(char::is_control) {
            return None;
        }
        let cd = match self {
            // Aspas simples: nada é interpretado dentro. O PowerShell fecha
            // a string também com as aspas tipográficas, e cada uma das
            // cinco se escreve dobrada. `-LiteralPath` não trata `[` como
            // curinga.
            ShellKind::PowerShell => {
                format!("Set-Location -LiteralPath '{}'\r", powershell_literal(path))
            }
            // Um caminho do Windows não tem aspas duplas; o `/d` troca o disco.
            ShellKind::Cmd => format!("cd /d \"{path}\"\r"),
            ShellKind::Posix => format!("cd -- '{}'\r", path.replace('\'', r"'\''")),
            ShellKind::Fish => format!(
                "cd -- '{}'\r",
                path.replace('\\', r"\\").replace('\'', r"\'")
            ),
        };
        Some(format!("{}{cd}", self.clear_line()))
    }

    /// As teclas que apagam a linha do shell, de onde estiver o cursor.
    ///
    /// No PowerShell e no Prompt de Comando, Ctrl+End (apaga até o fim) e
    /// Ctrl+Home (apaga até o começo), nas sequências do xterm, que o ConPTY
    /// traduz para as teclas. O Esc também limparia, mas um Esc seguido de
    /// texto chega ao Windows como Alt mais a letra seguinte.
    ///
    /// No bash, no zsh e no fish, Ctrl+E (fim da linha) e Ctrl+U (apaga até
    /// o começo). No modo vi, o Ctrl+U de inserção apaga o que veio antes do
    /// cursor.
    fn clear_line(self) -> &'static str {
        match self {
            ShellKind::PowerShell | ShellKind::Cmd => "\u{1b}[1;5F\u{1b}[1;5H",
            ShellKind::Posix | ShellKind::Fish => "\u{5}\u{15}",
        }
    }
}

/// Os caracteres que abrem e fecham uma string de aspas simples no
/// PowerShell: a aspa ASCII e as tipográficas U+2018, U+2019, U+201A e U+201B.
/// Dentro da string, qualquer um deles a fecha, e dois seguidos valem um.
const POWERSHELL_QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

/// `text` para ir entre aspas simples do PowerShell: cada aspa dobrada, para
/// valer como texto e não fechar a string.
fn powershell_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for c in text.chars() {
        if POWERSHELL_QUOTES.contains(&c) {
            out.push(c);
        }
        out.push(c);
    }
    out
}

/// O que a interface recebe de um terminal.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalMessage {
    /// Texto escrito pelo shell.
    Data {
        /// O texto.
        data: String,
    },
    /// O shell terminou.
    Exit {
        /// O código de saída, se o sistema informou.
        code: Option<u32>,
    },
}

fn terminal_error(error: impl std::fmt::Display) -> IpcError {
    IpcError::new(codes::TERMINAL, error.to_string())
}

/// O shell do terminal no Windows, pela preferência `terminal_shell`:
/// `cmd` é o Prompt de Comando; qualquer outro valor, o Windows PowerShell,
/// sem o cabeçalho de versão.
fn windows_shell(choice: &str) -> CommandBuilder {
    if choice == "cmd" {
        CommandBuilder::new("cmd.exe")
    } else {
        let mut command = CommandBuilder::new("powershell.exe");
        command.arg("-NoLogo");
        command
    }
}

/// Abre um terminal e devolve o identificador.
#[tauri::command]
pub async fn terminal_spawn(
    app: AppHandle,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    channel: Channel<TerminalMessage>,
) -> IpcResult<u32> {
    let state = app.state::<AppState>();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: rows.max(2),
            cols: cols.max(10),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(terminal_error)?;

    let choice = state.settings.get().terminal_shell;
    let shell = ShellKind::current(&choice);
    let mut command = if cfg!(windows) {
        windows_shell(&choice)
    } else {
        CommandBuilder::new_default_prog()
    };
    let cwd = cwd
        .or_else(|| {
            state
                .spf()
                .ok()
                .and_then(|spf| spf.parent().map(|p| p.to_string()))
        })
        .or_else(|| std::env::var("HOME").ok())
        .or_else(|| std::env::var("USERPROFILE").ok());
    if let Some(cwd) = cwd {
        command.cwd(cwd);
    }
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("AURORA_IDE", "1");
    if let Some(bin) = toolchain::install_bin_dir(&state.settings.get()) {
        let mut paths = vec![bin.into_std_path_buf()];
        if let Some(current) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&current));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            command.env("PATH", joined);
        }
    }

    let child = pair.slave.spawn_command(command).map_err(terminal_error)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(terminal_error)?;
    let mut writer = pair.master.take_writer().map_err(terminal_error)?;
    let id = state.next_id();
    let (input, queued) = mpsc::channel::<Vec<u8>>();
    std::thread::Builder::new()
        .name(format!("terminal-{id}-input"))
        .spawn(move || {
            for bytes in queued {
                if writer
                    .write_all(&bytes)
                    .and_then(|()| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        })
        .map_err(terminal_error)?;
    let flow = Arc::new(Flow::default());
    state.terminals.lock().expect("terminals lock").insert(
        id,
        Terminal {
            master: pair.master,
            input,
            child,
            shell,
            flow: Arc::clone(&flow),
        },
    );

    let output = channel.clone();
    let (read_all, reader_done) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name(format!("terminal-{id}"))
        .spawn(move || {
            let mut buffer = [0u8; 8192];
            let mut pending: Vec<u8> = Vec::new();
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                pending.extend_from_slice(&buffer[..read]);
                let text = take_utf8(&mut pending);
                if text.is_empty() {
                    continue;
                }
                let units = text.encode_utf16().count();
                if output.send(TerminalMessage::Data { data: text }).is_err() {
                    break;
                }
                flow.sent(units);
            }
            let _ = read_all.send(());
        })
        .map_err(terminal_error)?;

    // Quem percebe o fim do shell é esta thread, e não a de leitura: no
    // Windows, a leitura do ConPTY só termina quando o pseudoterminal fecha,
    // e ele fechava só quando a leitura terminava. Depois de um `exit`, o
    // terminal ficava mudo, com o `conhost.exe` vivo.
    let waiter_app = app.clone();
    std::thread::Builder::new()
        .name(format!("terminal-{id}-exit"))
        .spawn(move || {
            let code = loop {
                std::thread::sleep(EXIT_POLL);
                let state = waiter_app.state::<AppState>();
                let mut terminals = state.terminals.lock().expect("terminals lock");
                let Some(terminal) = terminals.get_mut(&id) else {
                    // Encerrado pela interface (terminal_kill, kill_all).
                    break None;
                };
                let status = match terminal.child.try_wait() {
                    Ok(None) => continue,
                    Ok(Some(status)) => Some(status.exit_code()),
                    Err(_) => None,
                };
                // Fora da lista, o pseudoterminal fecha (depois de soltar a
                // trava: fechar o ConPTY pode esperar a saída ser lida), e a
                // leitura termina.
                let finished = terminals.remove(&id);
                drop(terminals);
                drop(finished);
                break status;
            };
            // O que o shell escreveu antes de sair chega antes do aviso.
            let _ = reader_done.recv_timeout(READER_DRAIN);
            let _ = channel.send(TerminalMessage::Exit { code });
        })
        .map_err(terminal_error)?;
    Ok(id)
}

/// De quanto em quanto tempo a thread de fim confere se o shell saiu.
const EXIT_POLL: std::time::Duration = std::time::Duration::from_millis(150);
/// Quanto esperar a leitura entregar o que o shell escreveu antes de sair.
const READER_DRAIN: std::time::Duration = std::time::Duration::from_secs(2);

/// Tira de `pending` o maior começo que é UTF-8 válido. Bytes inválidos no
/// meio viram `U+FFFD`; um caractere cortado no fim fica para depois.
fn take_utf8(pending: &mut Vec<u8>) -> String {
    match std::str::from_utf8(pending) {
        Ok(text) => {
            let text = text.to_owned();
            pending.clear();
            text
        }
        Err(error) if error.error_len().is_none() => {
            let valid = error.valid_up_to();
            let text = String::from_utf8_lossy(&pending[..valid]).into_owned();
            pending.drain(..valid);
            text
        }
        Err(_) => {
            let text = String::from_utf8_lossy(pending).into_owned();
            pending.clear();
            text
        }
    }
}

/// Manda o que o usuário digitou para o shell, sem esperar ele ler.
#[tauri::command]
pub fn terminal_write(app: AppHandle, id: u32, data: String) -> IpcResult<()> {
    let state = app.state::<AppState>();
    let terminals = state.terminals.lock().expect("terminals lock");
    terminals
        .get(&id)
        .ok_or_else(|| IpcError::new(codes::TERMINAL, "The terminal has exited"))?
        .send(data.into_bytes())
}

/// A interface escreveu no xterm `units` (unidades UTF-16) do que o terminal
/// mandou. Um terminal que já terminou fica como está.
#[tauri::command]
pub fn terminal_ack(app: AppHandle, id: u32, units: usize) {
    let state = app.state::<AppState>();
    if let Some(terminal) = state.terminals.lock().expect("terminals lock").get(&id) {
        terminal.flow.ack(units);
    }
}

/// Leva o shell do terminal para `path`, digitando o `cd` do shell dele.
/// Um terminal que já terminou fica como está. Um caminho com caractere de
/// controle não é digitado ([`codes::UNSAFE_PATH`]): o shell fica onde está.
/// Com um programa em primeiro plano, no Linux e no macOS, também não
/// ([`codes::TERMINAL_BUSY`]): a linha iria para o programa.
#[tauri::command]
pub fn terminal_cd(app: AppHandle, id: u32, path: String) -> IpcResult<()> {
    let state = app.state::<AppState>();
    let terminals = state.terminals.lock().expect("terminals lock");
    let Some(terminal) = terminals.get(&id) else {
        return Ok(());
    };
    if terminal.has_foreground_program() {
        return Err(IpcError::new(
            codes::TERMINAL_BUSY,
            "A program is running in the terminal and would receive the cd: the terminal stays in the previous folder",
        ));
    }
    let line = terminal.shell.cd_line(&path).ok_or_else(|| {
        IpcError::new(
            codes::UNSAFE_PATH,
            "The project path has a control character, which the shell would read as a key: the terminal stays in the previous folder",
        )
    })?;
    terminal.send(line.into_bytes())
}

/// Avisa o shell do novo tamanho da janela.
#[tauri::command]
pub fn terminal_resize(app: AppHandle, id: u32, cols: u16, rows: u16) -> IpcResult<()> {
    let state = app.state::<AppState>();
    let terminals = state.terminals.lock().expect("terminals lock");
    if let Some(terminal) = terminals.get(&id) {
        terminal
            .master
            .resize(PtySize {
                rows: rows.max(2),
                cols: cols.max(10),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(terminal_error)?;
    }
    Ok(())
}

/// Encerra um terminal.
#[tauri::command]
pub fn terminal_kill(app: AppHandle, id: u32) {
    let state = app.state::<AppState>();
    let terminal = state.terminals.lock().expect("terminals lock").remove(&id);
    if let Some(mut terminal) = terminal {
        let _ = terminal.child.kill();
    }
}

/// Encerra todos os terminais, ao fechar o Studio.
pub fn kill_all(app: &AppHandle) {
    let state = app.state::<AppState>();
    let terminals: Vec<Terminal> = state
        .terminals
        .lock()
        .expect("terminals lock")
        .drain()
        .map(|(_, t)| t)
        .collect();
    for mut terminal in terminals {
        let _ = terminal.child.kill();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    use super::{Flow, HIGH_WATER, LOW_WATER, ShellKind, take_utf8};

    /// `flow.sent(units)` numa thread; o canal avisa quando volta.
    fn send_in_thread(flow: &Arc<Flow>, units: usize) -> mpsc::Receiver<()> {
        let (done, returned) = mpsc::channel();
        let flow = Arc::clone(flow);
        std::thread::spawn(move || {
            flow.sent(units);
            let _ = done.send(());
        });
        returned
    }

    #[test]
    fn the_reader_waits_while_the_interface_is_behind() {
        let flow = Arc::new(Flow::default());
        // Até o limite, a leitura segue.
        flow.sent(HIGH_WATER);
        let returned = send_in_thread(&flow, 1);
        assert!(returned.recv_timeout(Duration::from_millis(200)).is_err());
        // Uma confirmação que deixa o atraso acima do retorno não basta.
        flow.ack(HIGH_WATER - LOW_WATER);
        assert!(returned.recv_timeout(Duration::from_millis(200)).is_err());
        flow.ack(1);
        assert!(returned.recv_timeout(Duration::from_secs(5)).is_ok());
    }

    #[test]
    fn a_closed_terminal_does_not_keep_the_reader_waiting() {
        let flow = Arc::new(Flow::default());
        let returned = send_in_thread(&flow, HIGH_WATER + 1);
        assert!(returned.recv_timeout(Duration::from_millis(200)).is_err());
        flow.close();
        assert!(returned.recv_timeout(Duration::from_secs(5)).is_ok());
        // Depois de fechado, não espera mais.
        flow.sent(10 * HIGH_WATER);
    }

    #[test]
    fn cd_quotes_the_path_for_each_shell() {
        let path = r"C:\Users\Ana's [x]\proj";
        assert_eq!(
            ShellKind::PowerShell.cd_line(path).as_deref(),
            Some("\u{1b}[1;5F\u{1b}[1;5HSet-Location -LiteralPath 'C:\\Users\\Ana''s [x]\\proj'\r")
        );
        assert_eq!(
            ShellKind::Cmd.cd_line(path).as_deref(),
            Some("\u{1b}[1;5F\u{1b}[1;5Hcd /d \"C:\\Users\\Ana's [x]\\proj\"\r")
        );
        assert_eq!(
            ShellKind::Posix.cd_line("/home/ana/o'projeto").as_deref(),
            Some("\u{5}\u{15}cd -- '/home/ana/o'\\''projeto'\r")
        );
        assert_eq!(
            ShellKind::Fish.cd_line(r"/home/ana/o'pro\jeto").as_deref(),
            Some("\u{5}\u{15}cd -- '/home/ana/o\\'pro\\\\jeto'\r")
        );
    }

    #[test]
    fn powershell_doubles_every_quote_it_accepts() {
        // As quatro aspas tipográficas fecham a string do PowerShell como a
        // ASCII: dobradas, ficam dentro dela.
        let path = "C:\\p\u{2018}a\u{2019}b\u{201A}c\u{201B}d'e";
        assert_eq!(
            ShellKind::PowerShell.cd_line(path).as_deref(),
            Some(
                "\u{1b}[1;5F\u{1b}[1;5HSet-Location -LiteralPath \
                 'C:\\p\u{2018}\u{2018}a\u{2019}\u{2019}b\u{201A}\u{201A}c\u{201B}\u{201B}d''e'\r"
            )
        );
    }

    #[test]
    fn a_path_with_a_control_character_is_not_typed() {
        for shell in [
            ShellKind::PowerShell,
            ShellKind::Cmd,
            ShellKind::Posix,
            ShellKind::Fish,
        ] {
            for path in [
                "/home/x\ry",
                "/home/x\ny",
                "/home/x\u{15}y",
                "/home/x\u{3}y",
                "/home/x\u{1b}y",
                "/home/x\u{7f}y",
                "/home/x\u{85}y",
            ] {
                assert_eq!(shell.cd_line(path), None, "{shell:?} {path:?}");
            }
        }
    }

    #[test]
    fn keeps_a_split_character_for_the_next_read() {
        let bytes = "aç".as_bytes();
        let mut pending = bytes[..2].to_vec(); // "a" e o primeiro byte de "ç"
        assert_eq!(take_utf8(&mut pending), "a");
        assert_eq!(pending, vec![bytes[1]]);
        pending.push(bytes[2]);
        assert_eq!(take_utf8(&mut pending), "ç");
        assert!(pending.is_empty());
    }

    #[test]
    fn invalid_bytes_become_replacement_characters() {
        let mut pending = vec![b'a', 0xFF, b'b'];
        assert_eq!(take_utf8(&mut pending), "a\u{FFFD}b");
    }
}
