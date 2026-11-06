//! A placa virtual rodando: o executável de [`build`](super::build) como
//! processo filho, as entradas pelo stdin dele, uma ordem por linha, e o
//! estado, os quadros e o fim pelo stdout, em mensagens binárias
//! (`runtime/aurora_board.hpp` descreve as duas direções). O `$display` do
//! projeto chega pelo stderr.

use std::io::{BufRead, Read, Write};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{BoardBuild, BoardInput, BoardLayout, BoardState, Frame, SessionEvent, keys};
use crate::control::Control;
use crate::error::{AuroraError, Result};
use crate::fpga::Peripheral;
use crate::process::{Invocation, Termination};
use crate::project::Project;
use crate::toolchain::Toolchain;

/// Como a placa virtual roda.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RunOptions {
    /// Segundos simulados por segundo de relógio, no máximo: 1 é o tempo
    /// real (um LED que pisca a 1 Hz pisca a 1 Hz); 0 é o mais rápido
    /// possível. Um projeto grande simula mais devagar que o tempo real de
    /// qualquer jeito.
    pub limit: f64,
    /// Quadros de vídeo por segundo, no máximo.
    pub fps: f64,
    /// Parar sozinha nesse tempo simulado, em picossegundos: o fim exato,
    /// que um cancelamento pelo estado (cerca de 30 por segundo de relógio)
    /// não dá. `None` roda até o cancelamento ou o `$finish`.
    pub until_ps: Option<u64>,
}

impl RunOptions {
    /// O limite de velocidade ([`RunOptions::limit`]) e os quadros por
    /// segundo ([`RunOptions::fps`]).
    pub fn new(limit: f64, fps: f64) -> RunOptions {
        RunOptions {
            limit,
            fps,
            until_ps: None,
        }
    }

    /// Parar sozinha em `until_ps` picossegundos simulados
    /// ([`RunOptions::until_ps`]).
    pub fn until(mut self, until_ps: u64) -> RunOptions {
        self.until_ps = Some(until_ps);
        self
    }
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions {
            limit: 1.0,
            fps: 30.0,
            until_ps: None,
        }
    }
}

/// Como a placa virtual terminou.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct SessionOutcome {
    /// Por que terminou.
    pub reason: EndReason,
    /// O tempo simulado no fim, em picossegundos.
    pub time_ps: u64,
    /// Como o processo terminou.
    pub termination: Termination,
    /// Quanto rodou, em milissegundos.
    pub duration_ms: u64,
    /// As últimas linhas do stderr, quando o processo quebrou.
    pub tail: Vec<String>,
}

/// Por que a placa virtual terminou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EndReason {
    /// Quem roda pediu para parar (cancelamento).
    Stopped,
    /// O projeto chamou `$finish`.
    Finished,
    /// O processo terminou sem avisar: quebrou.
    Crashed,
}

impl BoardLayout {
    /// O periférico ligado com esse identificador.
    fn connected(&self, device: &str) -> Option<&Peripheral> {
        self.peripherals
            .iter()
            .find(|p| p.connected && p.id == device)
            .map(|p| &p.peripheral)
    }

    fn available(&self, accepts: impl Fn(&Peripheral) -> bool) -> Vec<String> {
        self.peripherals
            .iter()
            .filter(|p| p.connected && accepts(&p.peripheral))
            .map(|p| p.id.clone())
            .collect()
    }

    /// Confere que a entrada vai para um periférico ligado que a recebe.
    ///
    /// # Erros
    ///
    /// [`AuroraError::VirtualBoardDevice`] com os periféricos que a recebem.
    pub fn check(&self, input: &BoardInput) -> Result<()> {
        let (device, accepts): (&str, fn(&Peripheral) -> bool) = match input {
            BoardInput::Set { device, .. } => (device, |p| {
                matches!(p, Peripheral::Switches { .. } | Peripheral::Buttons { .. })
            }),
            BoardInput::Key { device, .. } => (device, |p| matches!(p, Peripheral::Ps2 { .. })),
            BoardInput::Text { device, .. } | BoardInput::Bytes { device, .. } => (device, |p| {
                matches!(
                    p,
                    Peripheral::Ps2 { .. } | Peripheral::Uart { rx: Some(_), .. }
                )
            }),
            _ => return Ok(()),
        };
        match self.connected(device) {
            Some(peripheral) if accepts(peripheral) => {}
            _ => {
                return Err(AuroraError::VirtualBoardDevice {
                    device: device.to_owned(),
                    available: self.available(accepts),
                });
            }
        }
        // Chaves e botões: o valor cabe na largura do periférico.
        if let BoardInput::Set { device, value } = input
            && let Some(width) = self
                .peripherals
                .iter()
                .find(|p| p.id == *device)
                .and_then(|p| p.signals.first())
                .map(|s| s.width)
            && width < 64
            && value >> width != 0
        {
            return Err(AuroraError::VirtualBoardValue {
                device: device.clone(),
                width,
                value: *value,
            });
        }
        Ok(())
    }

    /// A ordem para o executável; `None` para uma entrada que não vai a
    /// lugar nenhum (periférico errado, tecla sem código).
    pub(super) fn command(&self, input: &BoardInput) -> Option<String> {
        self.check(input).ok()?;
        let send = |device: &str, bytes: &[u8]| {
            (!bytes.is_empty()).then(|| {
                let hex: Vec<String> = bytes.iter().map(|b| format!("{b:x}")).collect();
                format!("send {device} {}", hex.join(" "))
            })
        };
        match input {
            BoardInput::Set { device, value } => Some(format!("set {device} {value:x}")),
            BoardInput::Key {
                device,
                code,
                pressed,
            } => send(device, &keys::key_bytes(code, *pressed)?),
            BoardInput::Text { device, text } => match self.connected(device)? {
                Peripheral::Ps2 { .. } => send(device, &keys::text_bytes(text)),
                _ => send(device, text.as_bytes()),
            },
            BoardInput::Bytes { device, bytes } => send(device, bytes),
            BoardInput::Pause => Some("pause".into()),
            BoardInput::Resume => Some("run".into()),
            BoardInput::Step { edges } => Some(format!("step {}", (*edges).max(1))),
            BoardInput::Speed { limit } => Some(format!("limit {}", limit.max(0.0))),
        }
    }
}

/// O que o executável manda.
enum Message {
    Hello,
    State(Wire),
    Frame(Frame),
    Exit(Exit),
    Line(String),
    Broken(String),
}

/// O estado como o executável o escreve.
#[derive(Deserialize)]
struct Wire {
    time_ps: u64,
    #[serde(default)]
    speed: f64,
    #[serde(default)]
    paused: bool,
    #[serde(default)]
    lights: std::collections::BTreeMap<String, Vec<u8>>,
    #[serde(default)]
    uart: std::collections::BTreeMap<String, Vec<u8>>,
    #[serde(default)]
    uart_baud: std::collections::BTreeMap<String, u32>,
}

#[derive(Deserialize)]
struct Exit {
    reason: String,
    time_ps: u64,
}

/// As linhas do projeto esperando o próximo lote ([`SessionEvent::Output`]):
/// as primeiras [`super::OUTPUT_BATCH`] e a contagem das outras.
#[derive(Default)]
struct OutputBatch {
    lines: Vec<String>,
    omitted: u64,
    since: Option<Instant>,
}

impl OutputBatch {
    fn push(&mut self, line: String) {
        self.since.get_or_insert_with(Instant::now);
        if self.lines.len() < super::OUTPUT_BATCH {
            self.lines.push(line);
        } else {
            self.omitted += 1;
        }
    }

    /// Há linhas esperando há mais que o intervalo entre dois estados.
    fn is_stale(&self) -> bool {
        self.since
            .is_some_and(|at| at.elapsed() > Duration::from_millis(50))
    }

    fn flush(&mut self, on_event: &mut dyn FnMut(SessionEvent)) {
        self.since = None;
        if self.lines.is_empty() && self.omitted == 0 {
            return;
        }
        on_event(SessionEvent::Output {
            lines: std::mem::take(&mut self.lines),
            omitted: std::mem::take(&mut self.omitted),
        });
    }
}

/// Roda a placa virtual compilada por [`build`](super::build) até o
/// cancelamento de `control` ou o `$finish` do projeto. As entradas chegam
/// por `inputs` enquanto ela roda; uma entrada para um periférico que a
/// placa não tem é ignorada (quem a manda confere antes com
/// [`BoardLayout::check`]). Cada estado, quadro e linha do projeto vai para
/// `on_event`, na thread de quem chama.
///
/// # Erros
///
/// [`AuroraError::VirtualBoardNotBuilt`] sem o executável, e
/// [`AuroraError::Spawn`] se ele não sobe. O projeto que quebra no meio é
/// `Ok` com [`EndReason::Crashed`].
pub fn run(
    toolchain: &Toolchain,
    project: &Project,
    built: &BoardBuild,
    options: &RunOptions,
    inputs: &Receiver<BoardInput>,
    on_event: &mut dyn FnMut(SessionEvent),
    control: &Control,
) -> Result<SessionOutcome> {
    let not_built = || AuroraError::VirtualBoardNotBuilt(super::executable_path(project));
    let executable = built
        .executable
        .clone()
        .filter(|e| e.is_file())
        .ok_or_else(not_built)?;
    let layout = built.layout.as_ref().ok_or_else(not_built)?;
    let started = Instant::now();
    // Na raiz do projeto, como a simulação: os `$readmem` do projeto leem
    // os arquivos pelo caminho relativo a ela ([`super::build`]).
    let invocation =
        Invocation::new(executable, project.root()).search_path(&toolchain.verilated_model_path());
    let mut process = crate::process::spawn_interactive(&invocation)?;
    let (tx, rx) = mpsc::channel::<Message>();
    if let Some(stdout) = process.stdout.take() {
        let tx = tx.clone();
        std::thread::spawn(move || read_messages(stdout, &tx));
    }
    if let Some(stderr) = process.stderr.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stderr).split(b'\n') {
                let Ok(line) = line else { break };
                let line = crate::process::decode_line(&line)
                    .trim_end_matches('\r')
                    .to_owned();
                if tx.send(Message::Line(line)).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    let mut stdin = process.stdin.take();
    let mut send = |command: &str| {
        if let Some(pipe) = stdin.as_mut()
            && writeln!(pipe, "{command}")
                .and_then(|()| pipe.flush())
                .is_err()
        {
            stdin = None;
        }
    };
    send(&format!("limit {}", options.limit.max(0.0)));
    send(&format!("fps {}", options.fps.max(1.0)));
    if let Some(until) = options.until_ps {
        send(&format!("until {}", until.max(1)));
    }

    let mut tail: Vec<String> = Vec::new();
    let mut time_ps = 0;
    let mut ended: Option<EndReason> = None;
    let mut asked_to_stop: Option<Instant> = None;
    // A saída do projeto em lotes: sai junto com cada estado, ou sozinha se
    // nenhum estado vier logo (a placa pausada, o começo).
    let mut output = OutputBatch::default();
    loop {
        if control.is_cancelled() && asked_to_stop.is_none() {
            send("quit");
            asked_to_stop = Some(Instant::now());
        }
        while let Ok(input) = inputs.try_recv() {
            if let Some(command) = layout.command(&input) {
                send(&command);
            }
        }
        match rx.recv_timeout(Duration::from_millis(15)) {
            Ok(Message::Hello) => on_event(SessionEvent::Ready),
            Ok(Message::State(wire)) => {
                time_ps = wire.time_ps;
                output.flush(on_event);
                on_event(SessionEvent::State(BoardState {
                    time_ps: wire.time_ps,
                    speed: wire.speed,
                    paused: wire.paused,
                    lights: wire.lights,
                    uart: wire.uart,
                    uart_baud: wire.uart_baud,
                }));
            }
            Ok(Message::Frame(frame)) => on_event(SessionEvent::Frame(frame)),
            Ok(Message::Line(line)) => {
                if tail.len() == 20 {
                    tail.remove(0);
                }
                tail.push(line.clone());
                output.push(line);
            }
            Ok(Message::Exit(exit)) => {
                time_ps = exit.time_ps;
                ended = Some(if exit.reason == "finish" {
                    EndReason::Finished
                } else {
                    EndReason::Stopped
                });
            }
            Ok(Message::Broken(reason)) => {
                tracing::warn!(%reason, "Virtual board protocol");
                tail.push(reason);
            }
            Err(RecvTimeoutError::Timeout) => {
                // Pediu para parar e o processo não respondeu: encerra.
                if asked_to_stop.is_some_and(|at| at.elapsed() > Duration::from_secs(3)) {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if output.is_stale() {
            output.flush(on_event);
        }
    }
    output.flush(on_event);
    drop(stdin);
    let termination = process.stop(Duration::from_secs(2));
    let reason = match ended {
        Some(reason) => reason,
        None if asked_to_stop.is_some() => EndReason::Stopped,
        None => EndReason::Crashed,
    };
    Ok(SessionOutcome {
        reason,
        time_ps,
        termination,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        tail: if reason == EndReason::Crashed {
            tail
        } else {
            Vec::new()
        },
    })
}

/// Lê as mensagens do stdout do executável até ele fechar.
fn read_messages(stdout: impl Read, tx: &mpsc::Sender<Message>) {
    let mut reader = std::io::BufReader::with_capacity(1 << 20, stdout);
    loop {
        let mut head = [0u8; 5];
        if reader.read_exact(&mut head).is_err() {
            return;
        }
        let size = u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize;
        if size > 64 << 20 {
            let _ = tx.send(Message::Broken(format!("message of {size} bytes")));
            return;
        }
        let mut body = vec![0u8; size];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        let message = match head[0] {
            b'H' => Message::Hello,
            b'S' => match serde_json::from_slice::<Wire>(&body) {
                Ok(wire) => Message::State(wire),
                Err(e) => Message::Broken(format!("state: {e}")),
            },
            b'X' => match serde_json::from_slice::<Exit>(&body) {
                Ok(exit) => Message::Exit(exit),
                Err(e) => Message::Broken(format!("exit: {e}")),
            },
            b'F' => match frame(&body) {
                Some(frame) => Message::Frame(frame),
                None => Message::Broken("frame".into()),
            },
            other => Message::Broken(format!("message type {other}")),
        };
        if tx.send(message).is_err() {
            return;
        }
    }
}

/// Um quadro: o periférico (texto terminado em 0), largura e altura (u16 LE)
/// e os pixels RGB; sai em PNG.
fn frame(body: &[u8]) -> Option<Frame> {
    let end = body.iter().position(|&b| b == 0)?;
    let device = String::from_utf8_lossy(&body[..end]).into_owned();
    let rest = &body[end + 1..];
    let width = u32::from(u16::from_le_bytes([*rest.first()?, *rest.get(1)?]));
    let height = u32::from(u16::from_le_bytes([*rest.get(2)?, *rest.get(3)?]));
    let pixels = rest.get(4..)?;
    if pixels.len() != (width * height * 3) as usize || width == 0 || height == 0 {
        return None;
    }
    Some(Frame {
        device,
        width,
        height,
        png: png(width, height, pixels)?,
    })
}

/// Os pixels RGB em PNG, com a compressão rápida: um quadro de texto de
/// 640x480 vira poucos kB.
pub(super) fn png(width: u32, height: u32, rgb: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgb).ok()?;
    writer.finish().ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_message_becomes_a_png() {
        let mut body = b"VGA_R\0".to_vec();
        body.extend([2, 0, 1, 0]);
        body.extend([255, 0, 0, 0, 0, 255]);
        let frame = frame(&body).unwrap();
        assert_eq!(
            (frame.device.as_str(), frame.width, frame.height),
            ("VGA_R", 2, 1)
        );
        assert_eq!(&frame.png[..4], b"\x89PNG");
        // Tamanho que não bate com os pixels: descartado.
        let mut short = b"VGA_R\0".to_vec();
        short.extend([3, 0, 1, 0, 1, 2]);
        assert!(super::frame(&short).is_none());
    }

    #[test]
    fn the_output_goes_in_batches_with_the_rest_counted() {
        let mut batch = OutputBatch::default();
        let mut events = Vec::new();
        batch.flush(&mut |e| events.push(e));
        assert!(events.is_empty(), "nada para mandar, nenhum lote");
        for n in 0..250 {
            batch.push(format!("linha {n}"));
        }
        batch.flush(&mut |e| events.push(e));
        batch.flush(&mut |e| events.push(e));
        assert_eq!(events.len(), 1);
        let SessionEvent::Output { lines, omitted } = &events[0] else {
            panic!("um lote de saída");
        };
        assert_eq!(lines.len(), super::super::OUTPUT_BATCH);
        assert_eq!(lines[0], "linha 0");
        assert_eq!(*omitted, 150);
    }
}
