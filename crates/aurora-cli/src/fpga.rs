//! `aurora fpga`: as placas FPGA, as ligações do `fpga.json` e a compilação
//! para a placa pelo Quartus. É um recurso experimental, desligado até o
//! usuário ligar com `aurora experimental enable fpga`.

use std::time::SystemTime;

use aurora_core::Control;
use aurora_core::experimental::{self, Feature};
use aurora_core::flows::FlowRequest;
use aurora_core::fpga::{self, Cable, SignalDirection};

use crate::commands::{Printer, open, run_flow};
use crate::output::{BOLD, DIM, OK, Output, WARNING, paint};
use crate::report::{FpgaBoardsReport, FpgaBuildReport, FpgaCablesReport, FpgaCheckReport};
use crate::{Cli, FpgaCommand};

/// Roda um subcomando de `aurora fpga`. As placas são um recurso experimental:
/// desligadas, nenhum subcomando roda, e o erro diz como ligar.
pub fn run(
    cli: &Cli,
    command: &FpgaCommand,
    out: &Output,
    control: &Control,
) -> anyhow::Result<bool> {
    experimental::require(Feature::Fpga)?;
    match command {
        FpgaCommand::Boards { board } => boards(board.as_deref(), out),
        FpgaCommand::Check { show_top } => check(cli, *show_top, out),
        FpgaCommand::Build => build(cli, out, control),
        FpgaCommand::Program { cable, list } => match list {
            true => cables(cli, out),
            false => program(cli, cable.as_deref(), out, control),
        },
    }
}

/// `aurora fpga program --list`: os cabos de gravação ligados.
fn cables(cli: &Cli, out: &Output) -> anyhow::Result<bool> {
    let toolchain = cli.toolchain.resolve()?;
    let cables = fpga::cables(&toolchain)?;
    if out.is_text() {
        if cables.is_empty() {
            println(&paint(WARNING, "No programming cable found"));
        }
        for cable in &cables {
            println(&format!("  {cable}"));
        }
    }
    out.json(&FpgaCablesReport { cables })?;
    Ok(true)
}

/// `aurora fpga program`: grava o `.sof` da última `aurora fpga build`.
fn program(
    cli: &Cli,
    cable: Option<&str>,
    out: &Output,
    control: &Control,
) -> anyhow::Result<bool> {
    let started = SystemTime::now();
    let mut project = open(cli)?;
    let root = project.root().to_owned();
    let printer = Printer::new(out, &root);
    let request = FlowRequest::FpgaProgram {
        cable: cable.map(str::to_owned),
    };
    let outcome = run_flow(cli, &mut project, &request, started, &printer, control)?;
    out.json(&outcome.fpga_program.expect("a gravação rodou"))?;
    Ok(outcome.succeeded)
}

/// `aurora fpga build`: os processadores e depois o projeto para a placa,
/// pelo Quartus. O `fpga.json`, a placa e o Quartus são conferidos antes:
/// faltar um deles não deve custar o build dos processadores.
fn build(cli: &Cli, out: &Output, control: &Control) -> anyhow::Result<bool> {
    let started = SystemTime::now();
    let mut project = open(cli)?;
    let root = project.root().to_owned();
    let printer = Printer::new(out, &root);
    let outcome = run_flow(
        cli,
        &mut project,
        &FlowRequest::FpgaBuild,
        started,
        &printer,
        control,
    )?;
    if let Some(fpga) = outcome.fpga.as_ref().filter(|f| f.bitstream.is_some()) {
        let board = fpga::board(&fpga.board)?;
        out.next(&format!(
            "Program the {} with: aurora fpga program",
            board.name
        ));
    }
    out.json(&FpgaBuildReport {
        builds: outcome.builds,
        registered: outcome.registered,
        fpga: outcome.fpga,
    })?;
    Ok(outcome.succeeded)
}

/// `aurora fpga boards [<placa>]`: a lista, ou os sinais e pinos de uma.
fn boards(id: Option<&str>, out: &Output) -> anyhow::Result<bool> {
    let boards = match id {
        Some(id) => vec![fpga::board(id)?],
        None => fpga::boards()?,
    };
    if out.is_text() {
        match (id, boards.first()) {
            (Some(_), Some(board)) => {
                println(&format!("{} ({})", paint(BOLD, &board.name), board.id));
                println(&format!(
                    "  FPGA    {} {}",
                    board.device.family, board.device.part
                ));
                println(&format!(
                    "  Cable   {} (the FPGA is device {} of the JTAG chain)",
                    cable(board.jtag.cable),
                    board.jtag.position
                ));
                println(&format!("  Source  {}", board.source));
                println(&format!(
                    "{} {}",
                    paint(BOLD, "Signals"),
                    paint(DIM, "(pins from bit 0)")
                ));
                for signal in &board.signals {
                    let name = if signal.width() > 1 {
                        format!("{}[{}:0]", signal.name, signal.width() - 1)
                    } else {
                        signal.name.clone()
                    };
                    let mut notes = vec![signal.io_standard.distinct().join(" and ")];
                    if signal.active_low {
                        notes.push("active low".into());
                    }
                    if let Some(mhz) = signal.clock_mhz {
                        notes.push(format!("{mhz} MHz clock"));
                    }
                    let direction = match signal.direction {
                        SignalDirection::Input => "input",
                        SignalDirection::Output => "output",
                        _ => "?",
                    };
                    println(&format!(
                        "  {name:<12} {direction:<6} {}  {}",
                        signal.pins.join(" "),
                        paint(DIM, notes.join(", "))
                    ));
                }
            }
            _ => {
                let device = |b: &fpga::Board| format!("{} {}", b.device.family, b.device.part);
                let id_width = boards
                    .iter()
                    .map(|b| b.id.chars().count())
                    .max()
                    .unwrap_or(0);
                let name_width = boards
                    .iter()
                    .map(|b| b.name.chars().count())
                    .max()
                    .unwrap_or(0);
                let width = boards.iter().map(|b| device(b).len()).max().unwrap_or(0);
                for board in &boards {
                    println(&format!(
                        "  {:<id_width$} {:<name_width$} {:<width$}  {}",
                        board.id,
                        board.name,
                        device(board),
                        paint(DIM, cable(board.jtag.cable))
                    ));
                }
                println(&paint(
                    DIM,
                    "Show the signals and pins of one with: aurora fpga boards <id>",
                ));
            }
        }
    }
    out.json(&FpgaBoardsReport { boards })?;
    Ok(true)
}

/// `aurora fpga check`: confere o `fpga.json` e mostra as ligações.
fn check(cli: &Cli, show_top: bool, out: &Output) -> anyhow::Result<bool> {
    let project = open(cli)?;
    // Sem bundle, as portas do topo vêm do leitor embutido.
    let toolchain = cli.toolchain.resolve().ok();
    let prepared = fpga::prepare(toolchain.as_ref(), &project)?;
    let resolved = &prepared.resolved;
    if out.is_text() {
        println(&format!(
            "{} {} on {} ({})",
            paint(OK, "OK"),
            resolved.top,
            resolved.board.name,
            resolved.board.device.part
        ));
        let width = prepared
            .connections
            .iter()
            .map(|c| c.target.len())
            .max()
            .unwrap_or(0);
        for c in &prepared.connections {
            println(&format!("  {:<width$}  <-  {}", c.target, c.source));
        }
        for clock in &resolved.clocks {
            println(&paint(
                DIM,
                format!(
                    "  clock {} ({} MHz) on {}",
                    clock.signal, clock.mhz, clock.port
                ),
            ));
        }
        for note in &resolved.notes {
            println(&format!("  {} {note}", paint(WARNING, "note:")));
        }
        if show_top {
            println("");
            for line in prepared.board_top.lines() {
                println(line);
            }
        }
    }
    out.json(&FpgaCheckReport {
        config: prepared.config.clone(),
        resolved: prepared.resolved.clone(),
        board_top: prepared.board_top.clone(),
        connections: prepared.connections.clone(),
    })?;
    Ok(true)
}

/// O nome do cabo, como no openFPGALoader.
fn cable(cable: Cable) -> &'static str {
    match cable {
        Cable::UsbBlaster => "USB-Blaster",
        Cable::UsbBlasterII => "USB-Blaster II",
        _ => "?",
    }
}

fn println(text: &str) {
    anstream::println!("{text}");
}
