//! `aurora vboard`: a placa virtual, sem janela. Compila o projeto com os
//! periféricos da placa, roda um tempo simulado com as entradas dadas e diz
//! como ficaram os LEDs, os displays e a UART, com o último quadro de vídeo
//! num PNG. A janela é a do Studio; aqui é para conferir e para testes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, mpsc};

use aurora_core::experimental::{self, Feature};
use aurora_core::fpga::Peripheral;
use aurora_core::virtual_board::{
    self, BoardInput, BoardLayout, BoardState, BuildOptions, RunOptions, SessionEvent,
};
use aurora_core::{CancelToken, Control};
use camino::Utf8PathBuf;

use crate::commands::open;
use crate::output::{BOLD, DIM, OK, Output, WARNING, paint};
use crate::report::VboardRunReport;
use crate::{Cli, VboardCommand, VboardRunArgs};

/// Roda um subcomando de `aurora vboard`. A placa virtual é parte da placa
/// FPGA, um recurso experimental.
pub fn run(
    cli: &Cli,
    command: &VboardCommand,
    out: &Output,
    control: &Control,
) -> anyhow::Result<bool> {
    experimental::require(Feature::Fpga)?;
    match command {
        VboardCommand::Run(args) => run_board(cli, args, out, control),
    }
}

/// `chave=valor` de `--set` e `--press`, com o valor em decimal, `0x` ou
/// `0b`.
fn assignment(text: &str) -> anyhow::Result<(String, u64)> {
    let (name, value) = text
        .split_once('=')
        .ok_or_else(|| anyhow::anyhow!("{text}: use NAME=VALUE (SW=0x3)"))?;
    let value = value.trim();
    let parsed = if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
    } else if let Some(bin) = value.strip_prefix("0b") {
        u64::from_str_radix(bin, 2)
    } else {
        value.parse()
    };
    Ok((
        name.trim().to_owned(),
        parsed.map_err(|_| {
            anyhow::anyhow!(
                "{text}: the value is a number of zero or more, in decimal, 0x (hex) or 0b (binary)"
            )
        })?,
    ))
}

fn run_board(
    cli: &Cli,
    args: &VboardRunArgs,
    out: &Output,
    control: &Control,
) -> anyhow::Result<bool> {
    let toolchain = cli.toolchain.resolve()?;
    let project = open(cli)?;
    let root = project.root().to_owned();
    let mut options = BuildOptions::default();
    options.board.clone_from(&args.board);
    let built = virtual_board::build(&toolchain, &project, &options, control)?;
    out.vboard_build(&built, &root);
    let Some(layout) = built.layout.clone().filter(|_| built.succeeded()) else {
        out.json(&VboardRunReport::new(
            built,
            None,
            None,
            None,
            Vec::new(),
            0,
        ))?;
        return Ok(false);
    };

    // As entradas antes de começar: conferidas contra a placa, como a
    // interface confere as dela.
    let mut first = Vec::new();
    for text in &args.set {
        let (device, value) = assignment(text)?;
        first.push(BoardInput::Set { device, value });
    }
    for text in &args.press {
        let (device, value) = assignment(text)?;
        first.push(BoardInput::Set { device, value });
    }
    if let Some(keys) = &args.keys {
        first.push(BoardInput::Text {
            device: device_of(
                &layout,
                |p| matches!(p, Peripheral::Ps2 { .. }),
                "PS/2 keyboard",
            )?,
            text: keys.clone(),
        });
    }
    if let Some(text) = &args.uart {
        first.push(BoardInput::Text {
            device: device_of(&layout, |p| matches!(p, Peripheral::Uart { .. }), "UART")?,
            text: text.clone(),
        });
    }
    for input in &first {
        layout.check(input)?;
    }
    let (inputs, receiver) = mpsc::channel();
    for input in first {
        let _ = inputs.send(input);
    }

    // Para no tempo simulado pedido, ou no Ctrl+C.
    let stop = CancelToken::new();
    let session_control = Control::new().with_cancel(stop.clone());
    let until_ps = (args.time * 1e9) as u64;
    let outer = control.cancel_token().clone();
    let last_state: Arc<Mutex<Option<BoardState>>> = Arc::default();
    let mut uart: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut lines = Vec::new();
    let mut omitted = 0u64;
    let mut last_frame = None;
    let mut frames = 0u32;
    let mut on_event = |event: SessionEvent| {
        match event {
            SessionEvent::State(state) => {
                for (device, bytes) in &state.uart {
                    uart.entry(device.clone()).or_default().extend(bytes);
                }
                if state.time_ps >= until_ps {
                    stop.cancel();
                }
                *last_state.lock().expect("estado") = Some(state);
            }
            SessionEvent::Frame(frame) => {
                frames += 1;
                last_frame = Some(frame);
            }
            SessionEvent::Output {
                lines: batch,
                omitted: skipped,
            } => {
                if out.is_text() {
                    for line in &batch {
                        println!("{}", paint(DIM, line));
                    }
                    if skipped > 0 {
                        println!("{}", paint(DIM, format!("... {skipped} lines omitted")));
                    }
                }
                lines.extend(batch);
                omitted += skipped;
            }
            SessionEvent::Ready => {}
            _ => {}
        }
        if outer.is_cancelled() {
            stop.cancel();
        }
    };
    let run_options = RunOptions::new(args.speed, 30.0);
    let outcome = virtual_board::run(
        &toolchain,
        &project,
        &built,
        &run_options,
        &receiver,
        &mut on_event,
        &session_control,
    )?;
    drop(inputs);
    let state = last_state.lock().expect("estado").take();

    let png = match (&args.png, &last_frame) {
        (Some(path), Some(frame)) => {
            std::fs::write(path, &frame.png)?;
            Some(path.clone())
        }
        _ => None,
    };
    if out.is_text() {
        print_state(
            &layout,
            state.as_ref(),
            &uart,
            frames,
            png.as_ref(),
            &outcome,
        );
    }
    let crashed = outcome.reason == virtual_board::EndReason::Crashed;
    out.json(&VboardRunReport::new(
        built,
        Some(outcome),
        state,
        png,
        lines,
        omitted,
    ))?;
    Ok(!crashed)
}

/// O primeiro periférico ligado do tipo pedido.
fn device_of(
    layout: &BoardLayout,
    kind: impl Fn(&Peripheral) -> bool,
    what: &str,
) -> anyhow::Result<String> {
    layout
        .peripherals
        .iter()
        .find(|p| p.connected && kind(&p.peripheral))
        .map(|p| p.id.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "The project does not use the {what} of {}",
                layout.board_name
            )
        })
}

/// O dígito que os segmentos acesos desenham (bit 0 = a, ..., bit 6 = g).
fn digit(segments: &[u8]) -> char {
    let mut bits = 0u8;
    for (i, level) in segments.iter().take(7).enumerate() {
        if *level >= 128 {
            bits |= 1 << i;
        }
    }
    match bits {
        0x00 => ' ',
        0x3F => '0',
        0x06 => '1',
        0x5B => '2',
        0x4F => '3',
        0x66 => '4',
        0x6D => '5',
        0x7D => '6',
        0x07 => '7',
        0x7F => '8',
        0x6F => '9',
        0x77 => 'A',
        0x7C => 'b',
        0x39 => 'C',
        0x5E => 'd',
        0x79 => 'E',
        0x71 => 'F',
        0x40 => '-',
        _ => '?',
    }
}

fn print_state(
    layout: &BoardLayout,
    state: Option<&BoardState>,
    uart: &BTreeMap<String, Vec<u8>>,
    frames: u32,
    png: Option<&Utf8PathBuf>,
    outcome: &virtual_board::SessionOutcome,
) {
    let ms = outcome.time_ps as f64 / 1e9;
    println!(
        "{} {} ms simulated in {:.1} s",
        paint(BOLD, &layout.board_name),
        format_args!("{ms:.3}"),
        outcome.duration_ms as f64 / 1000.0
    );
    let lights = state.map(|s| &s.lights);
    for item in layout.peripherals.iter().filter(|p| p.connected) {
        match &item.peripheral {
            Peripheral::Leds { signal, .. } => {
                let levels = lights.and_then(|l| l.get(signal));
                let bits: String = levels
                    .map(|l| {
                        l.iter()
                            .rev()
                            .map(|&v| if v >= 128 { '1' } else { '0' })
                            .collect()
                    })
                    .unwrap_or_default();
                println!("  {signal:<12} {}", paint(OK, &bits));
            }
            Peripheral::SevenSegment { digits } => {
                let shown: String = digits
                    .iter()
                    .map(|d| lights.and_then(|l| l.get(d)).map_or(' ', |s| digit(s)))
                    .collect();
                println!("  {:<12} {}", item.id, paint(OK, &shown));
            }
            Peripheral::Uart { .. } => {
                let text = uart
                    .get(&item.id)
                    .map(|b| String::from_utf8_lossy(b).into_owned())
                    .unwrap_or_default();
                println!("  {:<12} {}", item.id, paint(OK, format!("{text:?}")));
            }
            _ => {}
        }
    }
    if frames > 0 {
        let saved = png.map_or_else(String::new, |p| format!(", the last one in {p}"));
        println!(
            "  video        {frames} frame{}{saved}",
            if frames == 1 { "" } else { "s" }
        );
    }
    if outcome.reason == virtual_board::EndReason::Crashed {
        println!(
            "{}",
            paint(WARNING, "The simulation stopped without finishing")
        );
        for line in &outcome.tail {
            println!("  {}", paint(DIM, line));
        }
    }
}
