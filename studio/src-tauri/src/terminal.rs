//! O terminal de shell: um pseudoterminal com o shell do usuário, o TCMD da
//! AURORA. A interface o mostra com o xterm.js.
//!
//! No Windows, o shell é o da preferência `terminal_shell` (PowerShell ou
//! Prompt de Comando); nos outros sistemas, o do usuário (`$SHELL`).
//!
//! O shell abre na pasta do projeto, com a pasta `bin/` da instalação do
//! Lace no começo do `PATH`, para o usuário poder rodar `lace` direto.
//!
//! O que o shell escreve chega à interface por um `Channel`, como texto
//! UTF-8. Uma leitura pode cortar um caractere de vários bytes ao meio: os
//! bytes do fim que ainda não formam um caractere ficam para a leitura
//! seguinte.

use std::io::{Read, Write};

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
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
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

    let mut command = if cfg!(windows) {
        windows_shell(&state.settings.get().terminal_shell)
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
    command.env("LACE_STUDIO", "1");
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
    let writer = pair.master.take_writer().map_err(terminal_error)?;
    let id = state.next_id();
    state.terminals.lock().expect("terminals lock").insert(
        id,
        Terminal {
            master: pair.master,
            writer,
            child,
        },
    );

    let reader_app = app.clone();
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
                if !text.is_empty() && channel.send(TerminalMessage::Data { data: text }).is_err() {
                    break;
                }
            }
            let terminal = reader_app
                .state::<AppState>()
                .terminals
                .lock()
                .expect("terminals lock")
                .remove(&id);
            let code = terminal
                .and_then(|mut t| t.child.wait().ok())
                .map(|status| status.exit_code());
            let _ = channel.send(TerminalMessage::Exit { code });
        })
        .map_err(terminal_error)?;
    Ok(id)
}

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

/// Manda o que o usuário digitou para o shell.
#[tauri::command]
pub fn terminal_write(app: AppHandle, id: u32, data: String) -> IpcResult<()> {
    let state = app.state::<AppState>();
    let mut terminals = state.terminals.lock().expect("terminals lock");
    let terminal = terminals
        .get_mut(&id)
        .ok_or_else(|| IpcError::new(codes::TERMINAL, "The terminal has exited"))?;
    terminal
        .writer
        .write_all(data.as_bytes())
        .and_then(|()| terminal.writer.flush())
        .map_err(terminal_error)
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
    use super::take_utf8;

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
