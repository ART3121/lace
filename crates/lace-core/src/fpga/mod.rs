//! Placas FPGA: compilar um projeto para uma placa e gravar nela.
//!
//! O projeto diz a placa e a ligação das portas do topo aos sinais da placa
//! no `fpga.json` ([`config`]), ao lado do `.spf`; a AURORA não o lê. O Lace
//! gera um topo da placa ([`wrapper`]) que instancia o topo do projeto com
//! essas ligações (invertendo, cortando e completando larguras), e o
//! compila ([`build()`]) e grava na placa ([`program()`]) pelo Quartus Prime
//! instalado no sistema ([`quartus`]), sem abrir a interface dele. É a segunda exceção à regra do
//! bundle, depois do compilador C++ do Verilator: o Quartus é proprietário,
//! grande e não pode ir no bundle. Um fluxo aberto para o Cyclone V da
//! DE10-Nano (Yosys e `nextpnr-mistral`) ficou para depois.
//!
//! As placas ([`mod@board`]) vêm com o Core, com os pinos tirados dos manuais do
//! fabricante.

pub mod board;
pub mod build;
pub mod config;
mod manifest;
mod pins;
pub mod program;
pub mod qsf;
pub mod quartus;
pub mod summary;
pub mod wrapper;

pub use board::{
    Board, BoardSignal, Cable, Device, IoStandard, Jtag, SignalDirection, board, boards,
};
pub use build::{FpgaBuildResult, bitstream_path, build, build_dir};
pub use config::{CONFIG_FILE, FpgaConfig, Link, Resolved};
pub use program::{BitstreamState, FpgaProgramResult, bitstream_state, cables, program};
pub use quartus::Quartus;
pub use summary::{ClockTiming, ResourceUsage, TimingSummary};

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::Serialize;

use crate::error::{LaceError, Result};
use crate::project::Project;
use crate::toolchain::Toolchain;
use crate::verilog::ModuleInterface;

/// O `fpga.json` conferido: a placa, as ligações bit a bit e o topo da
/// placa gerado.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Prepared {
    /// O `fpga.json`.
    #[schemars(with = "String")]
    pub config: Utf8PathBuf,
    /// As ligações conferidas.
    pub resolved: Resolved,
    /// O Verilog do topo da placa ([`wrapper::generate`]).
    pub board_top: String,
    /// Cada ligação como texto ([`wrapper::connections`]): cada entrada do
    /// topo e cada sinal de saída da placa usado, com o que o dirige.
    pub connections: Vec<Connection>,
}

/// Uma ligação como texto, para mostrar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Connection {
    /// A entrada do topo (`in[15:0]`) ou o sinal de saída da placa
    /// (`LEDR[17:0]`).
    pub target: String,
    /// O que a dirige (`~KEY[0]`, `{10'b0, out[7:0]}`).
    pub source: String,
}

/// Lê o `fpga.json` do projeto e a placa, acha o topo e as portas dele e
/// confere as ligações.
///
/// O topo é o do `fpga.json`; sem ele, o topo do projeto; sem esse, o
/// processador, se o projeto tiver um só. As portas vêm do Verilog (o Yosys
/// do bundle, se houver): um processador SAPHO precisa estar compilado.
///
/// # Erros
///
/// [`LaceError::NoFpgaConfig`] e [`LaceError::InvalidFpgaConfig`] (com um
/// problema por linha), [`LaceError::BoardNotFound`],
/// [`LaceError::NoTopLevel`] e [`LaceError::ModuleNotFound`].
pub fn prepare(toolchain: Option<&Toolchain>, project: &Project) -> Result<Prepared> {
    let config = FpgaConfig::read(project.root())?;
    let path = FpgaConfig::path(project.root());
    let board = board::board(&config.board)?;
    let interface = top_interface(toolchain, project, config.top.as_deref())?;
    let resolved =
        config
            .resolve(&board, &interface)
            .map_err(|problems| LaceError::InvalidFpgaConfig {
                path: path.clone(),
                reason: format!(
                    "{} problem{}\n  {}",
                    problems.len(),
                    if problems.len() == 1 { "" } else { "s" },
                    problems.join("\n  ")
                ),
            })?;
    let board_top = wrapper::generate(&resolved);
    let connections = wrapper::connections(&resolved)
        .into_iter()
        .map(|(target, source)| Connection { target, source })
        .collect();
    Ok(Prepared {
        config: path,
        resolved,
        board_top,
        connections,
    })
}

/// Os módulos que podem ir para a placa: os dos fontes do projeto (sem os
/// testbenches), na ordem dos arquivos. As portas vêm do Verilog, como em
/// [`top_interface`].
///
/// # Erros
///
/// Os de ler os fontes do projeto.
pub fn modules(toolchain: Option<&Toolchain>, project: &Project) -> Result<Vec<String>> {
    let files = crate::synth::project_sources(project)?;
    Ok(crate::verilog::read_interfaces(toolchain, &files)?
        .into_iter()
        .map(|i| i.name)
        .collect())
}

/// O módulo que vai para a placa e as portas dele: `top`, se dado (o do
/// `fpga.json`); senão o topo do projeto; senão o processador, se o projeto
/// tiver um só. As portas vêm do Verilog (o Yosys do bundle, se houver): um
/// processador SAPHO precisa estar compilado. Serve para quem monta as
/// ligações antes de elas estarem certas.
///
/// # Erros
///
/// [`LaceError::NoTopLevel`] e [`LaceError::ModuleNotFound`], e os de ler os
/// fontes do projeto.
pub fn top_interface(
    toolchain: Option<&Toolchain>,
    project: &Project,
    top: Option<&str>,
) -> Result<ModuleInterface> {
    let top = match top {
        Some(top) => top.to_owned(),
        None => match project.top_module()? {
            Some(top) => top,
            None => match project.processors() {
                [only] => only.name.clone(),
                _ => return Err(LaceError::NoTopLevel(project.spf_path().to_owned())),
            },
        },
    };
    let files = crate::synth::project_sources(project)?;
    let mut interfaces = crate::verilog::read_interfaces(toolchain, &files)?;
    match interfaces.iter().position(|i| i.name == top) {
        Some(at) => Ok(interfaces.swap_remove(at)),
        None => Err(LaceError::ModuleNotFound {
            name: top,
            available: interfaces.into_iter().map(|i| i.name).collect(),
        }),
    }
}
