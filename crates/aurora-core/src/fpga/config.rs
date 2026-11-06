//! O `fpga.json` do projeto: a placa e as ligações das portas do topo aos
//! sinais dela.
//!
//! ```json
//! {
//!   "board": "de2-115",
//!   "top": "media_movel",
//!   "connect": {
//!     "clk": "CLOCK_50",
//!     "rst": "!KEY[0]",
//!     "in[3:0]": "SW[3:0]",
//!     "out[7:0]": "LEDR[7:0]"
//!   }
//! }
//! ```
//!
//! Cada ligação é `porta[faixa]` → `[!]SINAL[faixa]`; uma entrada também pode
//! receber `0` ou `1`. O `!` inverte: o `rst` do SAPHO é ativo em alto e os
//! botões da placa, em baixo. As faixas são do bit mais alto ao mais baixo
//! (`[7:0]`) ou um bit só (`[3]`); sem faixa, a porta ou o sinal inteiro. Uma
//! saída pode ir a vários sinais da placa, numa lista
//! (`"seg": ["HEX0", "HEX1"]`).
//!
//! Larguras diferentes: com faixa dos dois lados, é erro; senão, ligam-se os
//! bits de baixo, a entrada que sobra recebe 0 e a saída que sobra não
//! aparece na placa, e o [`Resolved`] anota o ajuste. Entrada sem ligação
//! recebe 0; bit de saída da placa sem ligação fica no nível inativo.

use camino::{Utf8Path, Utf8PathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::board::{Board, SignalDirection, is_identifier};
use crate::error::{AuroraError, Result};
use crate::verilog::{ModuleInterface, PortDirection};

/// O nome do arquivo, na raiz do projeto, ao lado do `.spf`.
pub const CONFIG_FILE: &str = "fpga.json";

/// O que o `fpga.json` guarda.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct FpgaConfig {
    /// A placa ([`super::board()`]).
    pub board: String,
    /// O módulo que vai para a placa. Sem ele, o topo do projeto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<String>,
    /// As ligações, na ordem do arquivo. Uma porta com vários sinais (uma
    /// saída que vai a mais de um lugar da placa) tem uma [`Link`] para cada.
    #[serde(default, with = "links")]
    #[schemars(with = "std::collections::BTreeMap<String, LinkTargets>")]
    pub connect: Vec<Link>,
}

/// O valor de uma ligação no `fpga.json`: um sinal da placa (`"LEDR[7:0]"`,
/// `"!KEY[0]"`, `"0"`), ou uma lista deles para uma saída que vai a vários.
/// Só descreve o formato; o [`FpgaConfig`] guarda as ligações em [`Link`].
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
enum LinkTargets {
    /// Um sinal.
    One(String),
    /// Vários sinais, para uma saída do topo.
    Many(Vec<String>),
}

/// Uma ligação, como escrita no `fpga.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[non_exhaustive]
pub struct Link {
    /// A porta do topo, com faixa ou não (`out[7:0]`).
    pub port: String,
    /// O sinal da placa (`!KEY[0]`), ou `0`/`1`.
    pub signal: String,
}

impl Link {
    /// Uma ligação.
    pub fn new(port: impl Into<String>, signal: impl Into<String>) -> Link {
        Link {
            port: port.into(),
            signal: signal.into(),
        }
    }
}

impl FpgaConfig {
    /// Uma configuração sem ligações.
    pub fn new(board: impl Into<String>) -> FpgaConfig {
        FpgaConfig {
            board: board.into(),
            top: None,
            connect: Vec::new(),
        }
    }

    /// O caminho do `fpga.json` de um projeto.
    pub fn path(root: &Utf8Path) -> Utf8PathBuf {
        root.join(CONFIG_FILE)
    }

    /// Lê o `fpga.json` da raiz do projeto.
    ///
    /// # Erros
    ///
    /// [`AuroraError::NoFpgaConfig`] se ele não existe;
    /// [`AuroraError::InvalidFpgaConfig`] se não é um JSON válido.
    pub fn read(root: &Utf8Path) -> Result<FpgaConfig> {
        let path = Self::path(root);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(AuroraError::NoFpgaConfig(path));
            }
            Err(e) => return Err(AuroraError::io("Reading", &path)(e)),
        };
        serde_json::from_str(&text).map_err(|e| AuroraError::InvalidFpgaConfig {
            path: path.clone(),
            reason: e.to_string(),
        })
    }

    /// Grava o `fpga.json` na raiz do projeto, de forma atômica.
    pub fn write(&self, root: &Utf8Path) -> Result<()> {
        let path = Self::path(root);
        let value = serde_json::to_value(self).map_err(|e| AuroraError::InvalidFpgaConfig {
            path: path.clone(),
            reason: e.to_string(),
        })?;
        crate::spf::write(&path, &value)
    }

    /// As ligações de quem deu às portas do topo os nomes dos sinais da
    /// placa, como nos topos de exemplo da Terasic (`SW`, `KEY`, `LEDR`,
    /// `HEX0`): cada porta com o nome e a direção de um sinal vai para ele, e
    /// as outras ficam de fora (as entradas soltas vão a 0 no
    /// [`resolve`](Self::resolve)). A largura não precisa bater: o `resolve`
    /// liga os bits de baixo e anota.
    pub fn by_name(board: &Board, top: &ModuleInterface) -> FpgaConfig {
        let mut config = FpgaConfig::new(board.id.clone());
        config.top = Some(top.name.clone());
        for port in &top.ports {
            let Some(signal) = board.signal(&port.name) else {
                continue;
            };
            let same_direction = matches!(
                (port.direction, signal.direction),
                (PortDirection::Input, SignalDirection::Input)
                    | (PortDirection::Output, SignalDirection::Output)
            );
            if same_direction {
                config.connect.push(Link::new(&port.name, &signal.name));
            }
        }
        config
    }

    /// As ligações com uma mudança feita na placa ([`Reconnect`]): o bit (ou
    /// o sinal inteiro) passa a ir na porta pedida, ou fica solto. O bit da
    /// placa fica só com a porta pedida (as que estavam nele saem); uma saída
    /// do topo que já ia a outros lugares da placa continua neles. As outras
    /// ligações continuam, reescritas em faixas contínuas
    /// (`b[7:0] -> SW[7:0]`, `b[8] -> SW[12]`), com uma lista para a saída
    /// que vai a vários sinais.
    ///
    /// # Erros
    ///
    /// Os problemas das ligações de antes (como em [`resolve`](Self::resolve)),
    /// um sinal ou uma porta que não existem, direções que não casam, e os
    /// problemas das ligações novas.
    pub fn reconnect(
        &self,
        board: &Board,
        top: &ModuleInterface,
        change: &Reconnect,
    ) -> std::result::Result<FpgaConfig, Vec<String>> {
        let mut resolver = Resolver::new(board, top);
        for link in &self.connect {
            if let Err(problem) = resolver.link(link) {
                resolver
                    .problems
                    .push(format!("\"{}\": \"{}\": {problem}", link.port, link.signal));
            }
        }
        if !resolver.problems.is_empty() {
            return Err(resolver.problems);
        }
        let mut inputs = resolver.inputs;
        // As saídas pela porta: (porta, bit) -> os (sinal, bit, invertido) da
        // placa que esse bit move; pode ser mais de um.
        type Places = Vec<(String, u32, bool)>;
        let mut outputs: std::collections::BTreeMap<(String, u32), Places> =
            std::collections::BTreeMap::new();
        for (signal, bits) in &resolver.outputs {
            for (bit, slot) in bits.iter().enumerate() {
                if let Some(p) = slot {
                    outputs.entry((p.port.clone(), p.bit)).or_default().push((
                        signal.clone(),
                        bit as u32,
                        p.invert,
                    ));
                }
            }
        }

        let signal = board
            .signal(&change.signal)
            .ok_or_else(|| vec![format!("{} has no signal {}", board.name, change.signal)])?;
        let targets: Vec<u32> = match change.bit {
            Some(bit) if bit >= signal.width() => {
                return Err(vec![format!("{} has {} bits", signal.name, signal.width())]);
            }
            Some(bit) => vec![bit],
            None => (0..signal.width()).collect(),
        };
        let port = match &change.port {
            None => None,
            Some(choice) => {
                let port = top
                    .ports
                    .iter()
                    .find(|p| p.name == choice.port)
                    .ok_or_else(|| vec![format!("{} has no port {}", top.name, choice.port)])?;
                let wanted = match signal.direction {
                    SignalDirection::Input => PortDirection::Input,
                    SignalDirection::Output => PortDirection::Output,
                };
                if port.direction != wanted {
                    return Err(vec![format!(
                        "{} is an {} of the board, and {} is not an {} of the top",
                        signal.name,
                        if wanted == PortDirection::Input {
                            "input"
                        } else {
                            "output"
                        },
                        port.name,
                        if wanted == PortDirection::Input {
                            "input"
                        } else {
                            "output"
                        },
                    )]);
                }
                let start = choice.bit.unwrap_or(0);
                if start >= port.width {
                    return Err(vec![format!("{} has {} bits", port.name, port.width)]);
                }
                Some((port, start, choice.invert))
            }
        };

        match signal.direction {
            SignalDirection::Input => {
                // Quem lia esses bits da placa deixa de ler.
                for (_, bits) in &mut inputs {
                    for slot in bits.iter_mut() {
                        if let Some(BitSource::Pin { signal: s, bit, .. }) = slot
                            && *s == signal.name
                            && targets.contains(bit)
                        {
                            *slot = None;
                        }
                    }
                }
                if let Some((port, start, invert)) = port {
                    let (_, bits) = inputs
                        .iter_mut()
                        .find(|(name, _)| *name == port.name)
                        .expect("toda entrada do topo tem uma linha");
                    for (i, &bit) in targets.iter().enumerate() {
                        let Some(slot) = bits.get_mut((start + i as u32) as usize) else {
                            break;
                        };
                        *slot = Some(BitSource::Pin {
                            signal: signal.name.clone(),
                            bit,
                            invert,
                        });
                    }
                }
            }
            SignalDirection::Output => {
                // Esses bits da placa trocam de porta; a porta que os movia
                // continua nos outros lugares aonde ia.
                for destinations in outputs.values_mut() {
                    destinations
                        .retain(|(s, bit, _)| !(*s == signal.name && targets.contains(bit)));
                }
                if let Some((port, start, invert)) = port {
                    for (i, &bit) in targets.iter().enumerate() {
                        let port_bit = start + i as u32;
                        if port_bit >= port.width {
                            break;
                        }
                        outputs
                            .entry((port.name.clone(), port_bit))
                            .or_default()
                            .push((signal.name.clone(), bit, invert));
                    }
                }
            }
        }

        // As ligações de novo, na ordem das portas, em faixas contínuas: cada
        // item é (bit da porta, (sinal e bit da placa | constante, invertido,
        // valor da constante)); uma saída tem um item por lugar aonde vai.
        type Item = (u32, (Option<(String, u32)>, bool, bool));
        let mut connect = Vec::new();
        for port in &top.ports {
            let mut items: Vec<Item> = match port.direction {
                PortDirection::Input => {
                    let (_, bits) = inputs
                        .iter()
                        .find(|(name, _)| *name == port.name)
                        .expect("toda entrada do topo tem uma linha");
                    bits.iter()
                        .enumerate()
                        .filter_map(|(bit, slot)| {
                            slot.as_ref().map(|source| {
                                let target = match source {
                                    BitSource::Pin {
                                        signal,
                                        bit,
                                        invert,
                                    } => (Some((signal.clone(), *bit)), *invert, false),
                                    BitSource::Constant { value } => (None, false, *value),
                                };
                                (bit as u32, target)
                            })
                        })
                        .collect()
                }
                // Os lugares de um bit na ordem dos sinais na placa, para o
                // arquivo não mudar de ordem a cada edição.
                PortDirection::Output => (0..port.width)
                    .flat_map(|bit| {
                        let mut places: Vec<_> = outputs
                            .get(&(port.name.clone(), bit))
                            .into_iter()
                            .flatten()
                            .collect();
                        places.sort_by_key(|(s, b, _)| {
                            (board.signals.iter().position(|sig| sig.name == *s), *b)
                        });
                        places.into_iter().map(move |(s, b, invert)| {
                            (bit, (Some((s.clone(), *b)), *invert, false))
                        })
                    })
                    .collect(),
                PortDirection::Inout => continue,
            };
            while !items.is_empty() {
                let (lsb, first) = items.remove(0);
                let mut width = 1u32;
                while let Some(next) = items.iter().position(|(bit, target)| {
                    *bit == lsb + width
                        && match (target, &first) {
                            ((Some((s, b)), inv, _), (Some((s0, b0)), inv0, _)) => {
                                s == s0 && inv == inv0 && *b == b0 + width
                            }
                            ((None, _, v), (None, _, v0)) => v == v0,
                            _ => false,
                        }
                }) {
                    items.remove(next);
                    width += 1;
                }
                let key = if lsb == 0 && width == port.width {
                    port.name.clone()
                } else {
                    show(&port.name, lsb, width)
                };
                let value = match &first {
                    (Some((s, b)), invert, _) => {
                        let whole = board
                            .signal(s)
                            .is_some_and(|sig| *b == 0 && sig.width() == width);
                        let name = if whole { s.clone() } else { show(s, *b, width) };
                        if *invert { format!("!{name}") } else { name }
                    }
                    (None, _, value) => if *value { "1" } else { "0" }.to_owned(),
                };
                connect.push(Link::new(key, value));
            }
        }
        let config = FpgaConfig {
            board: self.board.clone(),
            top: self.top.clone().or_else(|| Some(top.name.clone())),
            connect,
        };
        config.resolve(board, top)?;
        Ok(config)
    }

    /// Confere as ligações contra a placa e as portas do topo e diz o que vai
    /// em cada bit. Junta todos os problemas, um por linha, em vez de parar no
    /// primeiro.
    pub fn resolve(
        &self,
        board: &Board,
        top: &ModuleInterface,
    ) -> std::result::Result<Resolved, Vec<String>> {
        Resolver::new(board, top).run(&self.connect)
    }
}

/// Uma mudança de ligação feita na placa ([`FpgaConfig::reconnect`]): o sinal
/// da placa, um bit dele ou ele inteiro, e a porta do topo que passa a ir
/// nele.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Reconnect {
    /// O sinal da placa (`SW`, `HEX0`).
    pub signal: String,
    /// O bit do sinal; `None`, o sinal inteiro, do bit 0 em diante.
    #[serde(default)]
    pub bit: Option<u32>,
    /// A porta que vai nele; `None` deixa o bit (ou o sinal) solto.
    #[serde(default)]
    pub port: Option<PortChoice>,
}

/// A porta do topo de uma [`Reconnect`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PortChoice {
    /// A porta.
    pub port: String,
    /// O bit da porta que vai no bit do sinal (no sinal inteiro, o que vai no
    /// bit 0 dele); `None`, o bit 0.
    #[serde(default)]
    pub bit: Option<u32>,
    /// Invertido (`!`): o botão apertado chega como 1, o LED acende com 0.
    #[serde(default)]
    pub invert: bool,
}

impl Reconnect {
    /// O bit `bit` (ou o sinal inteiro) de `signal` recebe `port`, ou fica
    /// solto.
    pub fn new(signal: impl Into<String>, bit: Option<u32>, port: Option<PortChoice>) -> Reconnect {
        Reconnect {
            signal: signal.into(),
            bit,
            port,
        }
    }
}

impl PortChoice {
    /// A porta, a partir do bit `bit`, invertida ou não.
    pub fn new(port: impl Into<String>, bit: Option<u32>, invert: bool) -> PortChoice {
        PortChoice {
            port: port.into(),
            bit,
            invert,
        }
    }
}

// ------------------------------------------------------------ resultado

/// As ligações conferidas, bit a bit: o que o topo da placa precisa para ser
/// gerado.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Resolved {
    /// A placa.
    pub board: Board,
    /// O módulo do topo.
    pub top: String,
    /// As portas do topo, na ordem da declaração.
    pub ports: Vec<TopPort>,
    /// As entradas do topo, com a origem de cada bit, do bit 0 em diante.
    pub inputs: Vec<PortDrive>,
    /// Os sinais de saída da placa usados, com a porta que dirige cada bit.
    pub board_outputs: Vec<SignalDrive>,
    /// Os sinais de entrada da placa usados, na ordem em que aparecem.
    pub board_inputs: Vec<String>,
    /// As portas ligadas a um clock da placa.
    pub clocks: Vec<Clock>,
    /// Os ajustes feitos: larguras completadas ou cortadas, entradas soltas.
    pub notes: Vec<String>,
}

/// Uma porta do topo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct TopPort {
    /// O nome.
    pub name: String,
    /// `true` para entrada, `false` para saída.
    pub input: bool,
    /// A largura.
    pub width: u32,
}

/// A origem de cada bit de uma entrada do topo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct PortDrive {
    /// A porta.
    pub port: String,
    /// Um item por bit, do bit 0 em diante.
    pub bits: Vec<BitSource>,
}

/// De onde vem um bit de entrada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum BitSource {
    /// Um bit de um sinal da placa, invertido ou não.
    Pin {
        /// O sinal.
        signal: String,
        /// O bit.
        bit: u32,
        /// Invertido (`!`).
        invert: bool,
    },
    /// Um valor fixo.
    Constant {
        /// O valor.
        value: bool,
    },
}

/// O que dirige cada bit de um sinal de saída da placa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct SignalDrive {
    /// O sinal.
    pub signal: String,
    /// Ativo em nível baixo: os bits sem ligação ficam em 1.
    pub active_low: bool,
    /// Um item por bit, do bit 0 em diante; `None` fica no nível inativo.
    pub bits: Vec<Option<PortBit>>,
}

/// Um bit de uma saída do topo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct PortBit {
    /// A porta.
    pub port: String,
    /// O bit.
    pub bit: u32,
    /// Invertido (`!`).
    pub invert: bool,
}

/// Uma porta ligada a um clock da placa: vai para o `.sdc`.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Clock {
    /// O sinal da placa.
    pub signal: String,
    /// A porta do topo.
    pub port: String,
    /// A frequência.
    pub mhz: f64,
}

// ------------------------------------------------------------ sintaxe

/// Uma faixa `[msb:lsb]`, do bit mais alto ao mais baixo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Bits {
    msb: u32,
    lsb: u32,
}

impl Bits {
    fn width(self) -> u32 {
        self.msb - self.lsb + 1
    }
}

/// `nome`, `nome[7:0]` ou `nome[3]`.
fn parse_select(text: &str) -> std::result::Result<(String, Option<Bits>), String> {
    let text = text.trim();
    let Some(open) = text.find('[') else {
        return if is_identifier(text) {
            Ok((text.to_owned(), None))
        } else {
            Err(format!("{text:?} is not a name"))
        };
    };
    let name = text[..open].trim();
    let inner = text[open + 1..]
        .strip_suffix(']')
        .ok_or_else(|| format!("{text:?}: missing ]"))?;
    if !is_identifier(name) {
        return Err(format!("{text:?}: {name:?} is not a name"));
    }
    let number = |s: &str| {
        s.trim()
            .parse::<u32>()
            .map_err(|_| format!("{text:?}: {s:?} is not a bit number"))
    };
    let bits = match inner.split_once(':') {
        Some((msb, lsb)) => Bits {
            msb: number(msb)?,
            lsb: number(lsb)?,
        },
        None => {
            let bit = number(inner)?;
            Bits { msb: bit, lsb: bit }
        }
    };
    if bits.msb < bits.lsb {
        return Err(format!(
            "{text:?}: write the range from the high bit to the low one ([{}:{}])",
            bits.lsb, bits.msb
        ));
    }
    Ok((name.to_owned(), Some(bits)))
}

/// O lado da placa de uma ligação.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    Signal {
        name: String,
        bits: Option<Bits>,
        invert: bool,
    },
    Constant(bool),
}

fn parse_source(text: &str) -> std::result::Result<Source, String> {
    let text = text.trim();
    match text {
        "0" => return Ok(Source::Constant(false)),
        "1" => return Ok(Source::Constant(true)),
        _ => {}
    }
    let (invert, rest) = match text.strip_prefix('!') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (name, bits) = parse_select(rest)?;
    Ok(Source::Signal { name, bits, invert })
}

/// Os bits em faixas contínuas, da mais alta à mais baixa (`x[15:2]`, `x[0]`).
fn runs(name: &str, bits: &[u32]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bits.len() {
        let start = bits[i];
        let mut end = start;
        while i + 1 < bits.len() && bits[i + 1] == end + 1 {
            i += 1;
            end = bits[i];
        }
        out.push(show(name, start, end - start + 1));
        i += 1;
    }
    out.reverse();
    out
}

/// `x[7:0]`, `x[3]` ou `x`, para as mensagens.
fn show(name: &str, lsb: u32, width: u32) -> String {
    match width {
        0 => name.to_owned(),
        1 => format!("{name}[{lsb}]"),
        w => format!("{name}[{}:{lsb}]", lsb + w - 1),
    }
}

// ------------------------------------------------------------ resolução

struct Resolver<'a> {
    board: &'a Board,
    top: &'a ModuleInterface,
    problems: Vec<String>,
    notes: Vec<String>,
    /// Por entrada do topo: a origem de cada bit, ainda sem as soltas.
    inputs: Vec<(String, Vec<Option<BitSource>>)>,
    /// Por sinal de saída da placa: a porta de cada bit.
    outputs: Vec<(String, Vec<Option<PortBit>>)>,
    board_inputs: Vec<String>,
    clocks: Vec<Clock>,
}

impl<'a> Resolver<'a> {
    fn new(board: &'a Board, top: &'a ModuleInterface) -> Resolver<'a> {
        let inputs = top
            .ports
            .iter()
            .filter(|p| p.direction == PortDirection::Input)
            .map(|p| (p.name.clone(), vec![None; p.width as usize]))
            .collect();
        Resolver {
            board,
            top,
            problems: Vec::new(),
            notes: Vec::new(),
            inputs,
            outputs: Vec::new(),
            board_inputs: Vec::new(),
            clocks: Vec::new(),
        }
    }

    fn run(mut self, links: &[Link]) -> std::result::Result<Resolved, Vec<String>> {
        for link in links {
            if let Err(problem) = self.link(link) {
                self.problems
                    .push(format!("\"{}\": \"{}\": {problem}", link.port, link.signal));
            }
        }
        if !self.problems.is_empty() {
            return Err(self.problems);
        }
        let mut inputs = Vec::new();
        for (port, bits) in self.inputs {
            let loose: Vec<u32> = (0..bits.len() as u32)
                .filter(|&b| bits[b as usize].is_none())
                .collect();
            if loose.len() == bits.len() {
                self.notes
                    .push(format!("{port} is tied to 0 (no connection)"));
            } else if !loose.is_empty() {
                self.notes.push(format!(
                    "{} is tied to 0 (no connection)",
                    runs(&port, &loose).join(", ")
                ));
            }
            let bits = bits
                .into_iter()
                .map(|b| b.unwrap_or(BitSource::Constant { value: false }))
                .collect();
            inputs.push(PortDrive { port, bits });
        }
        let board_outputs = self
            .outputs
            .into_iter()
            .map(|(signal, bits)| SignalDrive {
                active_low: self.board.signal(&signal).is_some_and(|s| s.active_low),
                signal,
                bits,
            })
            .collect();
        if self.clocks.is_empty() {
            self.notes
                .push("no port is connected to a clock of the board".to_owned());
        }
        Ok(Resolved {
            board: self.board.clone(),
            top: self.top.name.clone(),
            ports: self
                .top
                .ports
                .iter()
                .map(|p| TopPort {
                    name: p.name.clone(),
                    input: p.direction == PortDirection::Input,
                    width: p.width,
                })
                .collect(),
            inputs,
            board_outputs,
            board_inputs: self.board_inputs,
            clocks: self.clocks,
            notes: self.notes,
        })
    }

    fn link(&mut self, link: &Link) -> std::result::Result<(), String> {
        let (port_name, port_bits) = parse_select(&link.port)?;
        let source = parse_source(&link.signal)?;
        let top = self.top;
        let port = top
            .ports
            .iter()
            .find(|p| p.name == port_name)
            .ok_or_else(|| {
                let names: Vec<&str> = top.ports.iter().map(|p| p.name.as_str()).collect();
                format!(
                    "{} has no port {port_name} (ports: {})",
                    top.name,
                    names.join(", ")
                )
            })?;
        let port_range = port_bits.unwrap_or(Bits {
            msb: port.width.saturating_sub(1),
            lsb: 0,
        });
        if port_range.msb >= port.width {
            return Err(format!("{port_name} has {} bits", port.width));
        }
        match port.direction {
            PortDirection::Inout => Err(format!(
                "{port_name} is inout, which the board top does not connect"
            )),
            PortDirection::Input => {
                self.drive_input(&port_name, port_range, port_bits.is_some(), source)
            }
            PortDirection::Output => {
                self.drive_output(&port_name, port_range, port_bits.is_some(), source)
            }
        }
    }

    /// O sinal da placa e a faixa usada dele.
    fn signal_range(
        &self,
        name: &str,
        bits: Option<Bits>,
        direction: SignalDirection,
    ) -> std::result::Result<(Bits, Option<f64>), String> {
        let signal = self.board.signal(name).ok_or_else(|| {
            let names: Vec<&str> = self.board.signals.iter().map(|s| s.name.as_str()).collect();
            format!(
                "{} has no signal {name} (signals: {})",
                self.board.name,
                names.join(", ")
            )
        })?;
        if signal.direction != direction {
            return Err(match direction {
                SignalDirection::Input => {
                    format!("{name} is an output of the board; it cannot drive an input of the top")
                }
                SignalDirection::Output => {
                    format!("{name} is an input of the board; an output of the top cannot drive it")
                }
            });
        }
        let range = bits.unwrap_or(Bits {
            msb: signal.width() - 1,
            lsb: 0,
        });
        if range.msb >= signal.width() {
            return Err(format!("{name} has {} bits", signal.width()));
        }
        Ok((range, signal.clock_mhz))
    }

    fn drive_input(
        &mut self,
        port: &str,
        range: Bits,
        explicit: bool,
        source: Source,
    ) -> std::result::Result<(), String> {
        let (signal, signal_range, invert, clock) = match source {
            Source::Constant(value) => {
                for bit in range.lsb..=range.msb {
                    self.set_input(port, bit, BitSource::Constant { value })?;
                }
                return Ok(());
            }
            Source::Signal { name, bits, invert } => {
                let (signal_range, clock) =
                    self.signal_range(&name, bits, SignalDirection::Input)?;
                if explicit && bits.is_some() && range.width() != signal_range.width() {
                    return Err(format!(
                        "{} bits on the top and {} on the board",
                        range.width(),
                        signal_range.width()
                    ));
                }
                (name, signal_range, invert, clock)
            }
        };
        let n = range.width().min(signal_range.width());
        for i in 0..n {
            self.set_input(
                port,
                range.lsb + i,
                BitSource::Pin {
                    signal: signal.clone(),
                    bit: signal_range.lsb + i,
                    invert,
                },
            )?;
        }
        if range.width() > n {
            for bit in range.lsb + n..=range.msb {
                self.set_input(port, bit, BitSource::Constant { value: false })?;
            }
            self.notes.push(format!(
                "{} receives 0 ({} has {n} bits)",
                show(port, range.lsb + n, range.width() - n),
                show(&signal, signal_range.lsb, n)
            ));
        }
        if !self.board_inputs.contains(&signal) {
            self.board_inputs.push(signal.clone());
        }
        if let Some(mhz) = clock {
            self.clocks.push(Clock {
                signal,
                port: port.to_owned(),
                mhz,
            });
        }
        Ok(())
    }

    fn set_input(
        &mut self,
        port: &str,
        bit: u32,
        source: BitSource,
    ) -> std::result::Result<(), String> {
        let (_, bits) = self
            .inputs
            .iter_mut()
            .find(|(name, _)| name == port)
            .expect("toda entrada do topo tem uma linha");
        let slot = &mut bits[bit as usize];
        if slot.is_some() {
            return Err(format!("{port}[{bit}] is connected twice"));
        }
        *slot = Some(source);
        Ok(())
    }

    fn drive_output(
        &mut self,
        port: &str,
        range: Bits,
        explicit: bool,
        source: Source,
    ) -> std::result::Result<(), String> {
        let Source::Signal { name, bits, invert } = source else {
            return Err(format!(
                "{port} is an output of the top; it cannot receive a constant"
            ));
        };
        let (signal_range, _) = self.signal_range(&name, bits, SignalDirection::Output)?;
        if explicit && bits.is_some() && range.width() != signal_range.width() {
            return Err(format!(
                "{} bits on the top and {} on the board",
                range.width(),
                signal_range.width()
            ));
        }
        let width = self.board.signal(&name).map_or(0, |s| s.width()) as usize;
        let index = match self.outputs.iter().position(|(s, _)| *s == name) {
            Some(i) => i,
            None => {
                self.outputs.push((name.clone(), vec![None; width]));
                self.outputs.len() - 1
            }
        };
        let n = range.width().min(signal_range.width());
        for i in 0..n {
            let bit = signal_range.lsb + i;
            let slot = &mut self.outputs[index].1[bit as usize];
            if let Some(other) = slot {
                return Err(format!(
                    "{name}[{bit}] is already driven by {}[{}]",
                    other.port, other.bit
                ));
            }
            *slot = Some(PortBit {
                port: port.to_owned(),
                bit: range.lsb + i,
                invert,
            });
        }
        if range.width() > n {
            self.notes.push(format!(
                "{} does not reach the board ({} has {n} bits)",
                show(port, range.lsb + n, range.width() - n),
                show(&name, signal_range.lsb, n)
            ));
        }
        Ok(())
    }
}

// ------------------------------------------------------------ serde

/// As ligações como um objeto `porta → sinal`, na ordem do arquivo; as de
/// uma mesma porta (uma saída que vai a vários sinais) numa lista.
mod links {
    use serde::de::{MapAccess, Visitor};
    use serde::ser::SerializeMap;
    use serde::{Deserialize, Deserializer, Serializer};

    use super::Link;

    pub fn serialize<S: Serializer>(links: &[Link], serializer: S) -> Result<S::Ok, S::Error> {
        // As portas na ordem em que aparecem pela primeira vez.
        let mut ports: Vec<(&str, Vec<&str>)> = Vec::new();
        for link in links {
            match ports.iter_mut().find(|(port, _)| *port == link.port) {
                Some((_, signals)) => signals.push(&link.signal),
                None => ports.push((&link.port, vec![&link.signal])),
            }
        }
        let mut map = serializer.serialize_map(Some(ports.len()))?;
        for (port, signals) in ports {
            match signals.as_slice() {
                [one] => map.serialize_entry(port, one)?,
                many => map.serialize_entry(port, many)?,
            }
        }
        map.end()
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Targets {
        One(String),
        Many(Vec<String>),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Link>, D::Error> {
        struct Links;
        impl<'de> Visitor<'de> for Links {
            type Value = Vec<Link>;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object from top ports to board signals")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<Link>, A::Error> {
                let mut links: Vec<Link> = Vec::new();
                while let Some(port) = map.next_key::<String>()? {
                    let targets = map.next_value::<Targets>().map_err(|_| {
                        serde::de::Error::custom(format!(
                            "port {port}: the value is a board signal or a list of them"
                        ))
                    })?;
                    if links.iter().any(|l| l.port == port) {
                        return Err(serde::de::Error::custom(format!(
                            "port {port} appears twice; an output that goes to several board signals takes a list"
                        )));
                    }
                    let signals = match targets {
                        Targets::One(signal) => vec![signal],
                        Targets::Many(signals) if signals.is_empty() => {
                            return Err(serde::de::Error::custom(format!(
                                "port {port} has an empty list"
                            )));
                        }
                        Targets::Many(signals) => signals,
                    };
                    for signal in signals {
                        links.push(Link {
                            port: port.clone(),
                            signal,
                        });
                    }
                }
                Ok(links)
            }
        }
        deserializer.deserialize_map(Links)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::verilog::Port;

    /// Uma placa de teste: os pinos não são de placa nenhuma.
    pub(crate) fn test_board() -> Board {
        let text = r#"{
          "id": "teste", "name": "Placa de teste", "source": "Terasic, nenhum manual: teste",
          "device": {"family": "Cyclone IV E", "part": "EP4CE115F29C7"},
          "jtag": {"cable": "usb-blaster", "position": 1},
          "signals": [
            {"name": "CLOCK_50", "direction": "input", "pins": ["PIN_A1"], "io_standard": "3.3-V LVTTL", "clock_mhz": 50, "description": "clock"},
            {"name": "KEY", "direction": "input", "pins": ["PIN_B1", "PIN_B2"], "io_standard": "3.3-V LVTTL", "active_low": true, "description": "botões"},
            {"name": "SW", "direction": "input", "pins": ["PIN_C1", "PIN_C2", "PIN_C3", "PIN_C4"], "io_standard": "3.3-V LVTTL", "description": "chaves"},
            {"name": "LED", "direction": "output", "pins": ["PIN_D1", "PIN_D2", "PIN_D3", "PIN_D4", "PIN_D5", "PIN_D6"], "io_standard": "3.3-V LVTTL", "description": "LEDs"},
            {"name": "HEX0", "direction": "output", "pins": ["PIN_E1", "PIN_E2"], "io_standard": "3.3-V LVTTL", "active_low": true, "description": "display"}
          ]}"#;
        let board: Board = serde_json::from_str(text).unwrap();
        super::super::board::validate(&board).unwrap();
        board
    }

    pub(crate) fn port(name: &str, direction: PortDirection, width: u32) -> Port {
        Port {
            name: name.into(),
            direction,
            width,
            signed: false,
        }
    }

    /// O topo de um processador SAPHO de uma porta de 16 bits.
    pub(crate) fn sapho_top() -> ModuleInterface {
        ModuleInterface {
            name: "proc".into(),
            file: "proc.v".into(),
            ports: vec![
                port("clk", PortDirection::Input, 1),
                port("rst", PortDirection::Input, 1),
                port("in", PortDirection::Input, 16),
                port("out", PortDirection::Output, 16),
                port("req_in", PortDirection::Output, 1),
                port("out_en", PortDirection::Output, 1),
            ],
        }
    }

    fn config(links: &[(&str, &str)]) -> FpgaConfig {
        FpgaConfig {
            board: "teste".into(),
            top: None,
            connect: links.iter().map(|(p, s)| Link::new(*p, *s)).collect(),
        }
    }

    #[test]
    fn selections_parse() {
        assert_eq!(parse_select("clk").unwrap(), ("clk".into(), None));
        assert_eq!(
            parse_select(" out[7:0] ").unwrap(),
            ("out".into(), Some(Bits { msb: 7, lsb: 0 }))
        );
        assert_eq!(
            parse_select("KEY[3]").unwrap(),
            ("KEY".into(), Some(Bits { msb: 3, lsb: 3 }))
        );
        assert!(parse_select("out[0:7]").unwrap_err().contains("[7:0]"));
        assert!(parse_select("out[7:0").is_err());
        assert!(parse_select("7out").is_err());
        assert_eq!(parse_source("1").unwrap(), Source::Constant(true));
        assert_eq!(
            parse_source("!KEY[0]").unwrap(),
            Source::Signal {
                name: "KEY".into(),
                bits: Some(Bits { msb: 0, lsb: 0 }),
                invert: true
            }
        );
    }

    #[test]
    fn the_file_keeps_the_order_and_refuses_repeated_ports() {
        let text = r#"{"board":"de2-115","connect":{"rst":"!KEY[0]","clk":"CLOCK_50"}}"#;
        let config: FpgaConfig = serde_json::from_str(text).unwrap();
        assert_eq!(config.connect[0].port, "rst");
        assert_eq!(config.connect[1].port, "clk");
        let again = serde_json::to_string(&config).unwrap();
        assert_eq!(
            again,
            r#"{"board":"de2-115","connect":{"rst":"!KEY[0]","clk":"CLOCK_50"}}"#
        );
        let repeated = r#"{"board":"x","connect":{"clk":"A","clk":"B"}}"#;
        let err = serde_json::from_str::<FpgaConfig>(repeated).unwrap_err();
        assert!(err.to_string().contains("clk appears twice"), "{err}");
        assert!(serde_json::from_str::<FpgaConfig>(r#"{"board":"x","pins":{}}"#).is_err());
    }

    #[test]
    fn a_sapho_processor_on_the_board() {
        let resolved = config(&[
            ("clk", "CLOCK_50"),
            ("rst", "!KEY[0]"),
            ("in", "SW"),
            ("out", "LED"),
        ])
        .resolve(&test_board(), &sapho_top())
        .unwrap();
        assert_eq!(resolved.board_inputs, ["CLOCK_50", "KEY", "SW"]);
        assert_eq!(
            resolved.clocks,
            [Clock {
                signal: "CLOCK_50".into(),
                port: "clk".into(),
                mhz: 50.0
            }]
        );
        let input = resolved.inputs.iter().find(|p| p.port == "in").unwrap();
        assert_eq!(
            input.bits[3],
            BitSource::Pin {
                signal: "SW".into(),
                bit: 3,
                invert: false
            }
        );
        assert_eq!(input.bits[4], BitSource::Constant { value: false });
        let rst = resolved.inputs.iter().find(|p| p.port == "rst").unwrap();
        assert_eq!(
            rst.bits[0],
            BitSource::Pin {
                signal: "KEY".into(),
                bit: 0,
                invert: true
            }
        );
        assert_eq!(resolved.board_outputs.len(), 1);
        assert_eq!(resolved.board_outputs[0].bits[5].as_ref().unwrap().bit, 5);
        assert_eq!(
            resolved.notes,
            [
                "in[15:4] receives 0 (SW[3:0] has 4 bits)",
                "out[15:6] does not reach the board (LED[5:0] has 6 bits)"
            ]
        );
    }

    #[test]
    fn explicit_ranges_must_have_the_same_width() {
        let err = config(&[("out[7:0]", "LED[5:0]")])
            .resolve(&test_board(), &sapho_top())
            .unwrap_err();
        assert_eq!(
            err,
            [r#""out[7:0]": "LED[5:0]": 8 bits on the top and 6 on the board"#]
        );
    }

    #[test]
    fn every_problem_is_reported() {
        let err = config(&[
            ("clk", "LED[0]"),
            ("out", "SW"),
            ("out_en", "1"),
            ("nada", "KEY"),
            ("rst", "KEY[5]"),
            ("in[3:0]", "SW"),
            ("in[0]", "KEY[1]"),
        ])
        .resolve(&test_board(), &sapho_top())
        .unwrap_err();
        let expected = [
            r#""clk": "LED[0]": LED is an output of the board; it cannot drive an input of the top"#,
            r#""out": "SW": SW is an input of the board; an output of the top cannot drive it"#,
            r#""out_en": "1": out_en is an output of the top; it cannot receive a constant"#,
            r#""nada": "KEY": proc has no port nada (ports: clk, rst, in, out, req_in, out_en)"#,
            r#""rst": "KEY[5]": KEY has 2 bits"#,
            r#""in[0]": "KEY[1]": in[0] is connected twice"#,
        ];
        assert_eq!(err, expected);
    }

    fn reconnect_top() -> ModuleInterface {
        ModuleInterface {
            name: "topo".into(),
            file: "topo.v".into(),
            ports: vec![
                port("CLOCK_50", PortDirection::Input, 1),
                port("b", PortDirection::Input, 4),
                port("a", PortDirection::Input, 1),
                port("q", PortDirection::Output, 6),
                port("seg", PortDirection::Output, 2),
            ],
        }
    }

    fn links_of(config: &FpgaConfig) -> Vec<(String, String)> {
        config
            .connect
            .iter()
            .map(|l| (l.port.clone(), l.signal.clone()))
            .collect()
    }

    #[test]
    fn a_bit_moves_to_another_port_and_the_ranges_are_rewritten() {
        let board = test_board();
        let top = reconnect_top();
        let mut config = FpgaConfig::new("teste");
        config.connect = vec![
            Link::new("CLOCK_50", "CLOCK_50"),
            Link::new("b", "SW"),
            Link::new("q", "LED"),
        ];
        // A chave 2 passa a ir na entrada a: o b[2] fica solto.
        let next = config
            .reconnect(
                &board,
                &top,
                &Reconnect::new("SW", Some(2), Some(PortChoice::new("a", None, false))),
            )
            .unwrap();
        assert_eq!(
            links_of(&next),
            [
                ("CLOCK_50".into(), "CLOCK_50".into()),
                ("b[1:0]".into(), "SW[1:0]".into()),
                ("b[3]".into(), "SW[3]".into()),
                ("a".into(), "SW[2]".into()),
                ("q".into(), "LED".into()),
            ]
        );
        // O LED 0 solto; os outros continuam com o q.
        let next = next
            .reconnect(&board, &top, &Reconnect::new("LED", Some(0), None))
            .unwrap();
        assert!(
            links_of(&next).contains(&("q[5:1]".into(), "LED[5:1]".into())),
            "{next:?}"
        );
        // O sinal inteiro numa porta, e invertido: o botão apertado chega como 1.
        let next = next
            .reconnect(
                &board,
                &top,
                &Reconnect::new("HEX0", None, Some(PortChoice::new("seg", None, false))),
            )
            .unwrap()
            .reconnect(
                &board,
                &top,
                &Reconnect::new("KEY", Some(0), Some(PortChoice::new("b", Some(2), true))),
            )
            .unwrap();
        let links = links_of(&next);
        assert!(links.contains(&("seg".into(), "HEX0".into())), "{links:?}");
        assert!(
            links.contains(&("b[2]".into(), "!KEY[0]".into())),
            "{links:?}"
        );
        // E o que sai do reconnect resolve.
        assert!(next.resolve(&board, &top).is_ok());
    }

    #[test]
    fn an_output_keeps_its_other_places_and_goes_to_a_list() {
        let board = test_board();
        let top = reconnect_top();
        let mut config = FpgaConfig::new("teste");
        config.connect = vec![Link::new("q", "LED"), Link::new("seg", "HEX0")];
        // O display também no q: os LEDs continuam com ele.
        let next = config
            .reconnect(
                &board,
                &top,
                &Reconnect::new("HEX0", None, Some(PortChoice::new("q", Some(0), false))),
            )
            .unwrap();
        assert_eq!(
            links_of(&next),
            [("q".into(), "LED".into()), ("q[1:0]".into(), "HEX0".into())]
        );
        // Os LEDs 1 e 0 no seg, que também vai ao HEX0: a mesma porta em dois
        // lugares vira uma lista no arquivo, na ordem dos sinais na placa.
        let next = config
            .reconnect(
                &board,
                &top,
                &Reconnect::new("LED", Some(0), Some(PortChoice::new("seg", Some(0), false))),
            )
            .unwrap()
            .reconnect(
                &board,
                &top,
                &Reconnect::new("LED", Some(1), Some(PortChoice::new("seg", Some(1), false))),
            )
            .unwrap();
        assert_eq!(
            links_of(&next),
            [
                ("q[5:2]".into(), "LED[5:2]".into()),
                ("seg".into(), "LED[1:0]".into()),
                ("seg".into(), "HEX0".into()),
            ]
        );
        assert!(next.resolve(&board, &top).is_ok());
        let json = serde_json::to_value(&next).unwrap();
        assert_eq!(
            json["connect"]["seg"],
            serde_json::json!(["LED[1:0]", "HEX0"])
        );
        assert_eq!(json["connect"]["q[5:2]"], "LED[5:2]");
        let back: FpgaConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, next);
    }

    #[test]
    fn a_port_twice_or_an_empty_list_is_refused() {
        let twice = r#"{"board": "teste", "connect": {"q": "LED", "q": "HEX0"}}"#;
        let error = serde_json::from_str::<FpgaConfig>(twice).unwrap_err();
        assert!(error.to_string().contains("takes a list"), "{error}");
        let empty = r#"{"board": "teste", "connect": {"q": []}}"#;
        let error = serde_json::from_str::<FpgaConfig>(empty).unwrap_err();
        assert!(error.to_string().contains("empty list"), "{error}");
        let number = r#"{"board": "teste", "connect": {"q": 3}}"#;
        let error = serde_json::from_str::<FpgaConfig>(number).unwrap_err();
        assert!(error.to_string().contains("list of them"), "{error}");
    }

    #[test]
    fn a_reconnect_needs_the_right_direction() {
        let board = test_board();
        let top = reconnect_top();
        let config = FpgaConfig::new("teste");
        let problems = config
            .reconnect(
                &board,
                &top,
                &Reconnect::new("SW", Some(0), Some(PortChoice::new("q", None, false))),
            )
            .unwrap_err();
        assert!(problems[0].contains("is not an input"), "{problems:?}");
        let problems = config
            .reconnect(&board, &top, &Reconnect::new("SW", Some(9), None))
            .unwrap_err();
        assert!(problems[0].contains("has 4 bits"), "{problems:?}");
    }

    #[test]
    fn ports_named_like_board_signals_connect_by_name() {
        let board = test_board();
        let first_input = board
            .signals
            .iter()
            .find(|s| s.direction == SignalDirection::Input)
            .unwrap();
        let first_output = board
            .signals
            .iter()
            .find(|s| s.direction == SignalDirection::Output)
            .unwrap();
        let top = ModuleInterface {
            name: "topo".into(),
            file: "topo.v".into(),
            ports: vec![
                port(&first_input.name, PortDirection::Input, 1),
                port(&first_output.name, PortDirection::Output, 1),
                // O nome de uma entrada da placa numa saída: fica de fora.
                port(&first_input.name.to_lowercase(), PortDirection::Input, 1),
                port("outro", PortDirection::Output, 1),
            ],
        };
        let config = FpgaConfig::by_name(&board, &top);
        assert_eq!(config.top.as_deref(), Some("topo"));
        let links: Vec<(&str, &str)> = config
            .connect
            .iter()
            .map(|l| (l.port.as_str(), l.signal.as_str()))
            .collect();
        assert_eq!(
            links,
            [
                (first_input.name.as_str(), first_input.name.as_str()),
                (first_output.name.as_str(), first_output.name.as_str()),
            ]
        );
    }

    #[test]
    fn loose_inputs_are_tied_to_zero() {
        let resolved = config(&[("clk", "CLOCK_50")])
            .resolve(&test_board(), &sapho_top())
            .unwrap();
        assert_eq!(
            resolved.notes,
            [
                "rst is tied to 0 (no connection)",
                "in is tied to 0 (no connection)"
            ]
        );
        let resolved = config(&[("in[1:0]", "SW[1:0]")])
            .resolve(&test_board(), &sapho_top())
            .unwrap();
        assert!(
            resolved
                .notes
                .iter()
                .any(|n| n == "clk is tied to 0 (no connection)")
        );
        assert!(
            resolved
                .notes
                .iter()
                .any(|n| n == "no port is connected to a clock of the board")
        );
        assert!(
            resolved
                .notes
                .iter()
                .any(|n| n == "in[15:2] is tied to 0 (no connection)"),
            "{:?}",
            resolved.notes
        );
        assert_eq!(runs("x", &[0, 1, 3, 5, 6, 7]), ["x[7:5]", "x[3]", "x[1:0]"]);
    }

    #[test]
    fn a_board_output_bit_has_one_driver() {
        let err = config(&[("out[1:0]", "LED[1:0]"), ("out_en", "LED[1]")])
            .resolve(&test_board(), &sapho_top())
            .unwrap_err();
        assert_eq!(
            err,
            [r#""out_en": "LED[1]": LED[1] is already driven by out[1]"#]
        );
    }

    #[test]
    fn the_config_is_read_and_written_next_to_the_spf() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        assert_eq!(FpgaConfig::read(root).unwrap_err().code(), "no_fpga_config");
        let mut config = FpgaConfig::new("de2-115");
        config.connect.push(Link::new("rst", "!KEY[0]"));
        config.write(root).unwrap();
        let text = std::fs::read_to_string(root.join(CONFIG_FILE)).unwrap();
        assert!(text.contains("\"rst\": \"!KEY[0]\""), "{text}");
        assert_eq!(FpgaConfig::read(root).unwrap(), config);
        std::fs::write(root.join(CONFIG_FILE), "{").unwrap();
        assert_eq!(
            FpgaConfig::read(root).unwrap_err().code(),
            "invalid_fpga_config"
        );
    }
}
