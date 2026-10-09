//! As placas que o Lace conhece: o FPGA, o cabo de gravação e os sinais da
//! placa ligados ao FPGA, com os pinos.
//!
//! Cada placa é um JSON em `crates/lace-core/boards/<id>.json`, embutido no
//! Core. Os pinos vêm do manual do fabricante, que o campo `source` cita com
//! a versão e as tabelas; nenhum pino é escrito de memória. Os testes
//! conferem que todo arquivo é válido.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{LaceError, Result};

/// As placas, na ordem da lista: o identificador e o JSON.
const BOARDS: &[(&str, &str)] = &[
    ("de2-115", include_str!("../../boards/de2-115.json")),
    ("de10-nano", include_str!("../../boards/de10-nano.json")),
];

/// Uma placa.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Board {
    /// O identificador, usado no `fpga.json` e na CLI (`de2-115`).
    pub id: String,
    /// O nome para mostrar (`Terasic DE2-115`).
    pub name: String,
    /// De onde vieram os pinos: o manual, a versão e as tabelas.
    pub source: String,
    /// O FPGA.
    pub device: Device,
    /// A gravação pela USB da placa.
    pub jtag: Jtag,
    /// Os sinais da placa ligados ao FPGA.
    pub signals: Vec<BoardSignal>,
}

/// O FPGA de uma placa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Device {
    /// A família, como o Quartus a escreve no `.qsf` (`Cyclone IV E`).
    pub family: String,
    /// O modelo, como no `.qsf` (`EP4CE115F29C7`).
    pub part: String,
}

/// O cabo de gravação embutido na placa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[non_exhaustive]
pub enum Cable {
    /// USB-Blaster (FT245 e um CPLD).
    #[serde(rename = "usb-blaster")]
    UsbBlaster,
    /// USB-Blaster II (EZ-USB FX2). O openFPGALoader só o usa com o firmware
    /// `blaster_6810.hex`, que vem com o Quartus.
    #[serde(rename = "usb-blasterII")]
    UsbBlasterII,
}

/// Como gravar a placa pela USB dela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Jtag {
    /// O cabo.
    pub cable: Cable,
    /// A posição do FPGA na cadeia JTAG, a partir de 1. Na DE10-Nano o FPGA
    /// vem depois do processador ARM (HPS).
    pub position: u32,
    /// O nome da placa no openFPGALoader (`-b`), se ele a conhece.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openfpgaloader: Option<String>,
}

/// A direção de um sinal da placa, do ponto de vista do FPGA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum SignalDirection {
    /// Entra no FPGA: clock, botão, chave.
    Input,
    /// Sai do FPGA: LED, display.
    Output,
}

/// Um sinal da placa: um fio (`CLOCK_50`) ou um barramento (`SW`), com um
/// pino por bit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct BoardSignal {
    /// O nome, como no manual (`KEY`, `LEDR`).
    pub name: String,
    /// A direção.
    pub direction: SignalDirection,
    /// Os pinos, do bit 0 em diante (`PIN_Y2`).
    pub pins: Vec<String>,
    /// O padrão de I/O, como no `.qsf` (`3.3-V LVTTL`): um para o sinal
    /// inteiro ou um por bit, quando os bits ficam em bancos de tensões
    /// diferentes.
    pub io_standard: IoStandard,
    /// Ativo em nível baixo: o botão apertado lê 0, o segmento acende em 0.
    /// Num sinal de saída, os bits que nada liga ficam no nível inativo.
    #[serde(default)]
    pub active_low: bool,
    /// A frequência, se for um clock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_mhz: Option<f64>,
    /// O que é, como no manual.
    pub description: String,
}

/// O padrão de I/O de um sinal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
#[non_exhaustive]
pub enum IoStandard {
    /// O mesmo para todos os bits.
    All(String),
    /// Um por bit, do bit 0 em diante.
    PerBit(Vec<String>),
}

impl IoStandard {
    /// O padrão de um bit.
    pub fn of(&self, bit: usize) -> Option<&str> {
        match self {
            IoStandard::All(standard) => Some(standard),
            IoStandard::PerBit(standards) => standards.get(bit).map(String::as_str),
        }
    }

    /// Os padrões distintos, na ordem em que aparecem.
    pub fn distinct(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        let all: Vec<&str> = match self {
            IoStandard::All(standard) => vec![standard.as_str()],
            IoStandard::PerBit(standards) => standards.iter().map(String::as_str).collect(),
        };
        for standard in all {
            if !out.contains(&standard) {
                out.push(standard);
            }
        }
        out
    }
}

impl BoardSignal {
    /// A largura em bits.
    pub fn width(&self) -> u32 {
        u32::try_from(self.pins.len()).unwrap_or(u32::MAX)
    }
}

impl Board {
    /// O sinal com esse nome (diferenciando maiúsculas, como o Verilog).
    pub fn signal(&self, name: &str) -> Option<&BoardSignal> {
        self.signals.iter().find(|s| s.name == name)
    }
}

/// As placas conhecidas, na ordem da lista.
///
/// # Erros
///
/// [`LaceError::InvalidBoard`] se um JSON embutido for inválido, o que os
/// testes impedem.
pub fn boards() -> Result<Vec<Board>> {
    BOARDS.iter().map(|(id, text)| parse(id, text)).collect()
}

/// A placa com esse identificador, sem diferenciar maiúsculas.
///
/// # Erros
///
/// [`LaceError::BoardNotFound`], com as placas conhecidas.
pub fn board(id: &str) -> Result<Board> {
    match BOARDS
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(id))
    {
        Some((known, text)) => parse(known, text),
        None => Err(LaceError::BoardNotFound {
            name: id.to_owned(),
            available: BOARDS
                .iter()
                .map(|(known, _)| (*known).to_owned())
                .collect(),
        }),
    }
}

fn parse(id: &str, text: &str) -> Result<Board> {
    let invalid = |reason: String| LaceError::InvalidBoard {
        id: id.to_owned(),
        reason,
    };
    let board: Board = serde_json::from_str(text).map_err(|e| invalid(e.to_string()))?;
    if board.id != id {
        return Err(invalid(format!("the file says id {}", board.id)));
    }
    validate(&board).map_err(invalid)?;
    Ok(board)
}

/// Confere o que o JSON não garante: nomes, pinos e clocks.
pub(crate) fn validate(board: &Board) -> std::result::Result<(), String> {
    if board.jtag.position == 0 {
        return Err("the JTAG position starts at 1".into());
    }
    let mut names = std::collections::BTreeSet::new();
    let mut pins = std::collections::BTreeMap::new();
    for signal in &board.signals {
        if !is_identifier(&signal.name) {
            return Err(format!("{} is not a Verilog identifier", signal.name));
        }
        if !names.insert(signal.name.as_str()) {
            return Err(format!("signal {} appears twice", signal.name));
        }
        if signal.pins.is_empty() {
            return Err(format!("{} has no pins", signal.name));
        }
        match &signal.io_standard {
            IoStandard::All(standard) if standard.trim().is_empty() => {
                return Err(format!("{} has no I/O standard", signal.name));
            }
            IoStandard::PerBit(standards)
                if standards.len() != signal.pins.len()
                    || standards.iter().any(|s| s.trim().is_empty()) =>
            {
                return Err(format!(
                    "{} needs one I/O standard per pin ({} pins)",
                    signal.name,
                    signal.pins.len()
                ));
            }
            _ => {}
        }
        for (bit, pin) in signal.pins.iter().enumerate() {
            if !is_pin(pin) {
                return Err(format!(
                    "{}[{bit}]: {pin} is not a pin name (PIN_Y2)",
                    signal.name
                ));
            }
            if let Some(other) = pins.insert(pin.as_str(), format!("{}[{bit}]", signal.name)) {
                return Err(format!(
                    "{pin} is used by {other} and {}[{bit}]",
                    signal.name
                ));
            }
        }
        if let Some(mhz) = signal.clock_mhz {
            if signal.width() != 1 || signal.direction != SignalDirection::Input {
                return Err(format!("clock {} must be a 1-bit input", signal.name));
            }
            if !(mhz.is_finite() && mhz > 0.0) {
                return Err(format!("clock {} has frequency {mhz}", signal.name));
            }
        }
    }
    Ok(())
}

/// Um identificador Verilog simples (sem escape).
pub(crate) fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// `PIN_` seguido de uma ou duas letras e de um número (`PIN_Y2`, `PIN_AB28`).
fn is_pin(pin: &str) -> bool {
    let Some(rest) = pin.strip_prefix("PIN_") else {
        return false;
    };
    let letters = rest.chars().take_while(char::is_ascii_uppercase).count();
    let digits = &rest[letters..];
    (1..=2).contains(&letters)
        && !digits.is_empty()
        && digits.len() <= 2
        && digits.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_board_is_valid() {
        let boards = boards().unwrap();
        assert_eq!(boards.len(), BOARDS.len());
        for board in &boards {
            assert!(!board.signals.is_empty(), "{}", board.id);
            assert!(board.source.contains("Terasic"), "{}", board.source);
        }
    }

    #[test]
    fn boards_are_found_ignoring_case() {
        assert_eq!(board("DE2-115").unwrap().id, "de2-115");
        let err = board("zybo").unwrap_err();
        assert_eq!(err.code(), "board_not_found");
        assert!(err.to_string().contains("de10-nano"), "{err}");
    }

    #[test]
    fn pin_names_are_checked() {
        for good in ["PIN_Y2", "PIN_AB28", "PIN_V11", "PIN_AH17"] {
            assert!(is_pin(good), "{good}");
        }
        for bad in ["Y2", "PIN_", "PIN_2", "PIN_ABC1", "PIN_Y123", "PIN_y2"] {
            assert!(!is_pin(bad), "{bad}");
        }
    }

    #[test]
    fn a_pin_used_twice_is_refused() {
        let text = r#"{"id":"t","name":"T","source":"Terasic, teste","device":{"family":"Cyclone IV E","part":"EP4CE6"},
            "jtag":{"cable":"usb-blaster","position":1},
            "signals":[
              {"name":"A","direction":"input","pins":["PIN_A1"],"io_standard":"3.3-V LVTTL","description":"a"},
              {"name":"B","direction":"output","pins":["PIN_A1"],"io_standard":"3.3-V LVTTL","description":"b"}]}"#;
        let err = parse("t", text).unwrap_err();
        assert!(
            err.to_string().contains("PIN_A1 is used by A[0] and B[0]"),
            "{err}"
        );
    }
}
