//! Gravar na placa o `.sof` da compilação ([`super::build()`]) pelo Quartus
//! Programmer, sem abrir a interface dele.
//!
//! ```text
//! quartus_pgm -l                                                        os cabos
//! quartus_pgm -c "USB-Blaster [USB-0]" -m jtag -o "p;output_files/lace_board_top.sof@1"
//! ```
//!
//! Os dois rodam na pasta da placa ([`super::build_dir`]). O `@n` é a
//! posição da FPGA na cadeia JTAG da placa ([`super::Jtag`]): 1 na
//! DE2-115; 2 na DE10-Nano, em que o HPS vem antes. A gravação vai para a
//! SRAM da FPGA e some quando a placa é desligada.
//!
//! Conferido com o Quartus Prime 25.1 Lite no Windows e uma DE2-115: o
//! `quartus_pgm -l` lista os cabos (`1) USB-Blaster [USB-0]`), ou escreve
//! `No JTAG hardware available` no stderr, e sai com 0 nos dois casos; a
//! gravação termina com `Configuration succeeded -- 1 device(s) configured`
//! em uns 9 s.

use std::time::Instant;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::Serialize;

use super::Quartus;
use super::board::Board;
use super::config::FpgaConfig;
use super::manifest;
use crate::control::Control;
use crate::diagnostics::Diagnostic;
use crate::error::{LaceError, Result};
use crate::pipeline::{PlannedStep, Runner, Status, Step, StepReport, elapsed_ms};
use crate::process::{self, Watch};
use crate::project::Project;
use crate::toolchain::{Tool, Toolchain};

/// O resultado de [`program`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct FpgaProgramResult {
    /// A placa (o `id`).
    pub board: String,
    /// O `.sof` gravado.
    #[schemars(with = "String")]
    pub bitstream: Utf8PathBuf,
    /// O cabo usado, como o `quartus_pgm -l` o escreve
    /// (`USB-Blaster [USB-0]`).
    pub cable: String,
    /// A posição da FPGA na cadeia JTAG (o `@n`).
    pub position: u32,
    /// O Quartus que gravou.
    pub quartus: Quartus,
    /// Como a gravação terminou.
    pub status: Status,
    /// `program` quando falhou.
    pub failed_step: Option<Step>,
    /// Um passo: `program` (`quartus_pgm`).
    pub steps: Vec<StepReport>,
    /// Os erros e avisos do Programmer.
    pub diagnostics: Vec<Diagnostic>,
    /// Quanto a gravação levou, do começo ao fim, em milissegundos.
    pub duration_ms: u64,
}

impl FpgaProgramResult {
    /// `status == Succeeded`.
    pub fn succeeded(&self) -> bool {
        self.status == Status::Succeeded
    }
}

/// Os cabos de gravação ligados ao computador, como o `quartus_pgm -l` os
/// escreve (`USB-Blaster [USB-0]`), na ordem dele. Vazio: nenhum.
///
/// # Erros
///
/// [`LaceError::QuartusMissing`] sem Quartus; os de rodar um processo.
pub fn cables(toolchain: &Toolchain) -> Result<Vec<String>> {
    let quartus = toolchain.quartus().ok_or(LaceError::QuartusMissing)?;
    let cwd = std::env::temp_dir();
    let cwd = Utf8PathBuf::from_path_buf(cwd).unwrap_or_else(|_| quartus.bin.clone());
    let invocation = quartus.invocation("quartus_pgm", cwd)?.arg("-l");
    let output = process::run(&invocation, &Watch::default())?;
    Ok(parse_cables(&format!(
        "{}\n{}",
        output.stderr, output.stdout
    )))
}

/// O arquivo de gravação da placa do `fpga.json`: se existe e, se existe,
/// por que não descreve o projeto de agora ([`bitstream_state`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct BitstreamState {
    /// A placa (o `id`).
    pub board: String,
    /// O `.sof`, se a placa já foi compilada.
    #[schemars(with = "Option<String>")]
    pub bitstream: Option<Utf8PathBuf>,
    /// Quando o `.sof` foi gravado, em milissegundos desde 1970.
    pub built_at_ms: Option<u64>,
    /// Por que o `.sof` não serve para gravar, uma razão por item: vazio
    /// com `bitstream`, ele descreve o projeto de agora.
    pub reasons: Vec<String>,
}

impl BitstreamState {
    /// Pronto para gravar: há `.sof` e ele descreve o projeto.
    pub fn ready(&self) -> bool {
        self.bitstream.is_some() && self.reasons.is_empty()
    }
}

/// O estado do arquivo de gravação da placa do `fpga.json`, como o
/// [`program`] o confere, sem procurar cabo nem rodar o Quartus.
///
/// # Erros
///
/// Os do `fpga.json` ([`LaceError::NoFpgaConfig`],
/// [`LaceError::InvalidFpgaConfig`], [`LaceError::BoardNotFound`]) e os de
/// ler os fontes.
pub fn bitstream_state(toolchain: &Toolchain, project: &Project) -> Result<BitstreamState> {
    let config = FpgaConfig::read(project.root())?;
    let board = super::board(&config.board)?;
    state_of(toolchain, project, &board)
}

fn state_of(toolchain: &Toolchain, project: &Project, board: &Board) -> Result<BitstreamState> {
    let dir = super::build_dir(project, &board.id);
    let bitstream = super::build::bitstream_path(&dir);
    let mut state = BitstreamState {
        board: board.id.clone(),
        bitstream: None,
        built_at_ms: None,
        reasons: Vec::new(),
    };
    if !bitstream.is_file() {
        return Ok(state);
    }
    state.built_at_ms = std::fs::metadata(&bitstream)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| u64::try_from(d.as_millis()).ok());
    state.reasons = match manifest::read(&dir) {
        None => vec![
            "the last build for this board did not finish, or was made by an earlier Lace"
                .to_owned(),
        ],
        Some(built) => {
            let now = manifest::inputs(toolchain, project, board)?;
            manifest::differences(&built, board, &manifest::hash_file(&bitstream)?, &now)
        }
    };
    state.bitstream = Some(bitstream);
    Ok(state)
}

/// As linhas `1) USB-Blaster [USB-0]` da saída do `quartus_pgm -l`.
fn parse_cables(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let (number, name) = line.trim().split_once(") ")?;
            (!number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
                .then(|| name.trim().to_owned())
        })
        .filter(|name| !name.is_empty())
        .collect()
}

/// Grava na placa do `fpga.json` o `.sof` da última compilação para ela.
/// Sem `cable`, o primeiro que o `quartus_pgm -l` lista.
///
/// Só grava o `.sof` que o registro da última compilação descreve, e só se
/// o projeto de agora é o que ela compilou: os mesmos fontes, as mesmas
/// memórias dos processadores, o mesmo `fpga.json` e a mesma definição da
/// placa, comparados pelo conteúdo. Qualquer diferença recusa, antes de
/// procurar o cabo.
///
/// # Erros
///
/// - [`LaceError::QuartusMissing`] sem Quartus;
/// - [`LaceError::NoFpgaConfig`], [`LaceError::InvalidFpgaConfig`] e
///   [`LaceError::BoardNotFound`], do `fpga.json`;
/// - [`LaceError::NoBitstream`] sem a compilação para a placa;
/// - [`LaceError::StaleBitstream`] se o `.sof` não descreve o projeto de
///   agora, com cada diferença;
/// - [`LaceError::NoCable`] sem `cable` e sem cabo ligado.
pub fn program(
    toolchain: &Toolchain,
    project: &Project,
    cable: Option<&str>,
    control: &Control,
) -> Result<FpgaProgramResult> {
    let started = Instant::now();
    let quartus = toolchain
        .quartus()
        .ok_or(LaceError::QuartusMissing)?
        .clone();
    let config = FpgaConfig::read(project.root())?;
    let board = super::board(&config.board)?;
    let _span = tracing::info_span!("fpga_program", board = %board.id).entered();
    let dir = super::build_dir(project, &board.id);
    let state = state_of(toolchain, project, &board)?;
    let Some(bitstream) = state.bitstream else {
        return Err(LaceError::NoBitstream(super::build::bitstream_path(&dir)));
    };
    if !state.reasons.is_empty() {
        return Err(LaceError::StaleBitstream {
            reasons: state.reasons,
        });
    }
    let cable = match cable {
        Some(cable) => cable.to_owned(),
        None => cables(toolchain)?
            .into_iter()
            .next()
            .ok_or(LaceError::NoCable)?,
    };

    // O `.sof` relativo à pasta da placa: sem caminho com espaço ou acento
    // dentro do `-o`.
    let relative = bitstream
        .strip_prefix(&dir)
        .map_or_else(|_| bitstream.to_string(), |p| p.as_str().replace('\\', "/"));
    let position = board.jtag.position;
    let invocation = quartus
        .invocation("quartus_pgm", &dir)?
        .arg("-c")
        .arg(&cable)
        .arg("-m")
        .arg("jtag")
        .arg("-o")
        .arg(format!("p;{relative}@{position}"));
    let mut runner = Runner::new(control);
    runner.run(PlannedStep::new(Step::Program, Tool::Quartus, invocation))?;
    Ok(FpgaProgramResult {
        board: board.id,
        bitstream,
        cable,
        position,
        quartus,
        status: runner.status,
        failed_step: runner.failed_step,
        steps: runner.steps,
        diagnostics: runner.diagnostics,
        duration_ms: elapsed_ms(started),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cables_come_from_the_numbered_lines() {
        // O stderr do `quartus_pgm -l` do Quartus 25.1 sem cabo, conferido.
        let none = "TBBmalloc: skip allocation functions replacement in ucrtbase.dll: unknown prologue for function _msize\r\nNo JTAG hardware available\r\n";
        assert!(parse_cables(none).is_empty());
        let two =
            "1) USB-Blaster [USB-0]\n2) USB-Blaster II [USB-1]\nInfo: Command: quartus_pgm -l\n";
        assert_eq!(
            parse_cables(two),
            ["USB-Blaster [USB-0]", "USB-Blaster II [USB-1]"]
        );
    }
}
