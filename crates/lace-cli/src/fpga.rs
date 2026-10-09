//! `lace fpga`: as placas FPGA, as ligações do `fpga.json` e a compilação
//! para a placa pelo Quartus.

use lace_core::fpga::{self, Cable, FpgaConfig, SignalDirection};
use lace_core::{BuildResult, Control, LaceError};

use crate::output::{BOLD, DIM, OK, Output, WARNING, paint};
use crate::report::{FpgaBoardsReport, FpgaBuildReport, FpgaCablesReport, FpgaCheckReport};
use crate::{Cli, FpgaCommand};

/// Roda um subcomando de `lace fpga`.
pub fn run(
    cli: &Cli,
    command: &FpgaCommand,
    out: &Output,
    control: &Control,
) -> anyhow::Result<bool> {
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

/// `lace fpga program --list`: os cabos de gravação ligados.
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

/// `lace fpga program`: grava o `.sof` da última `lace fpga build`.
fn program(
    cli: &Cli,
    cable: Option<&str>,
    out: &Output,
    control: &Control,
) -> anyhow::Result<bool> {
    let project = lace_core::Project::discover(&cli.project)?;
    let toolchain = cli.toolchain.resolve()?;
    let result = fpga::program(&toolchain, &project, cable, control)?;
    out.fpga_program(&result, project.root());
    let ok = result.succeeded();
    out.json(&result)?;
    Ok(ok)
}

/// `lace fpga build`: os processadores e depois o projeto para a placa,
/// pelo Quartus. O `fpga.json`, a placa e o Quartus são conferidos antes:
/// faltar um deles não deve custar o build dos processadores.
fn build(cli: &Cli, out: &Output, control: &Control) -> anyhow::Result<bool> {
    let project = lace_core::Project::discover(&cli.project)?;
    let toolchain = cli.toolchain.resolve()?;
    let config = FpgaConfig::read(project.root())?;
    let board = fpga::board(&config.board)?;
    if toolchain.quartus().is_none() {
        return Err(LaceError::QuartusMissing.into());
    }
    let builds = crate::commands::build_first(
        &toolchain,
        &project,
        project.buildable_processors(),
        out,
        control,
    )?;
    if !builds.iter().all(BuildResult::succeeded) {
        out.not_run("FPGA build", &builds);
        out.json(&FpgaBuildReport { builds, fpga: None })?;
        return Ok(false);
    }
    let result = fpga::build(&toolchain, &project, control)?;
    out.fpga_build(&result, project.root());
    if result.bitstream.is_some() {
        out.next(&format!(
            "Program the {} with: lace fpga program",
            board.name
        ));
    }
    let ok = result.succeeded();
    out.json(&FpgaBuildReport {
        builds,
        fpga: Some(result),
    })?;
    Ok(ok)
}

/// `lace fpga boards [<placa>]`: a lista, ou os sinais e pinos de uma.
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
                let width = boards.iter().map(|b| device(b).len()).max().unwrap_or(0);
                for board in &boards {
                    println(&format!(
                        "  {:<10} {:<20} {:<width$}  {}",
                        board.id,
                        board.name,
                        device(board),
                        paint(DIM, cable(board.jtag.cable))
                    ));
                }
                println(&paint(
                    DIM,
                    "Show the signals and pins of one with: lace fpga boards <id>",
                ));
            }
        }
    }
    out.json(&FpgaBoardsReport { boards })?;
    Ok(true)
}

/// `lace fpga check`: confere o `fpga.json` e mostra as ligações.
fn check(cli: &Cli, show_top: bool, out: &Output) -> anyhow::Result<bool> {
    let project = lace_core::Project::discover(&cli.project)?;
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
