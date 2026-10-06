//! O layout do Surfer para a onda de um processador SAPHO: o que a AURORA
//! fazia em `js/wave/surfer_layout_writer.ts` e `js/compilation/layout_do_surfer.ts`.
//!
//! O YANC já põe na onda o que interessa. O `<proc>.v` traz, no bloco de
//! simulação (`YANC_SIM_VIS`), uma cópia de cada variável do programa
//! (`me1_f_<função>_v_<nome>_e_` para `int`, `me2_` para `float`, já como
//! `real`, `comp_me3_` para complexo, `arr_me<T>_..._e_<NNNN>` para cada
//! elemento de array), o PC atrasado de dois ciclos (`valr2`) e a linha do
//! fonte da instrução (`linetabs`). O testbench grava todos eles. E o
//! `asmcomp` e o `cmmcomp` deixam, na pasta temporária do processador, as
//! tabelas que dão nome a esses números: `trad_opcode.txt` (`<índice>
//! <mnemônico> <operando>`) e `trad_cmm.txt` (`<linha> <texto da linha>`).
//!
//! Este módulo lê o cabeçalho da onda, acha cada processador (o escopo que
//! tem `valr2` e `linetabs`), transforma as tabelas em tradutores de valor
//! do Surfer (*mapping translators*) e monta o estado do Surfer
//! (`.surf.ron`) com os sinais em grupos: os de fora dos processadores e,
//! para cada processador, `clk`/`rst`, I/O, instruções (o assembly e a linha
//! do C± no lugar dos números), variáveis e flags (pilhas e ULA).
//!
//! O `.surf.ron` segue o formato do surfer-aurora do bundle (base Surfer
//! 0.7.0): é o estado que o próprio Surfer grava, e ele reacha cada sinal
//! pelo caminho e pelo nome, então os identificadores do arquivo são só
//! marcadores. Os tradutores vão para `.surfer/mappings/` na pasta onde o
//! Surfer roda, que é onde ele os procura além da pasta de configuração do
//! usuário.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufRead, BufReader};

use camino::{Utf8Path, Utf8PathBuf};
use fst_reader::{
    FstFilter, FstHierarchyEntry, FstReader, FstSignalHandle, FstSignalValue, ReadSignalsError,
};
use schemars::JsonSchema;
use serde::Serialize;

use crate::error::{LaceError, Result};
use crate::project::Project;

/// A pasta, dentro da de layout, de onde o Surfer lê os tradutores.
pub(crate) const MAPPINGS_DIR: &str = ".surfer/mappings";

/// Um `.` dentro do nome de um escopo (`\u.pa `, identificador escapado
/// do Verilog) é guardado como este caractere, para os caminhos poderem ser
/// separados por `.`; volta a ser `.` no `.surf.ron` e no
/// [`WaveProcessor::instance`].
const ESCAPED_DOT: char = '\u{2024}';

/// Teto do cabeçalho lido. Um cabeçalho de VCD maior que isso não é de um
/// testbench de processador.
const MAX_HEADER: u64 = 64 * 1024 * 1024;

/// Teto de valores distintos de complexos traduzidos. Complexos são
/// variáveis do programa e costumam mudar pouco; um complexo que muda a cada
/// clock passa disso, e os valores seguintes aparecem em binário (API.md,
/// 5.7.1).
const MAX_COMPLEX_VALUES: usize = 16384;

/// O layout de uma onda, em memória.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WaveLayout {
    /// O conteúdo do `.surf.ron`.
    pub state: String,
    /// Os tradutores de valor que o estado usa, pelo nome.
    pub mappings: Vec<MappingTranslator>,
    /// Os processadores achados na onda, na ordem do cabeçalho.
    pub processors: Vec<WaveProcessor>,
}

/// Um tradutor de valor do Surfer: um arquivo `Name = <nome>`, `Bits = <n>`
/// e uma linha `<valor> <texto>` por valor.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MappingTranslator {
    /// O nome, que é também o nome do arquivo e o `format` dos sinais.
    pub name: String,
    /// O conteúdo do arquivo.
    pub content: String,
}

/// Um processador SAPHO achado na onda.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct WaveProcessor {
    /// O escopo da instância na onda (`soma_tb.proc`).
    pub instance: String,
    /// O processador (o `#PRNAME`, nome da pasta no projeto).
    pub processor: String,
    /// Quantas variáveis do programa estão na onda (cada elemento de array
    /// conta).
    pub variables: usize,
    /// O PC aparece como instrução de assembly (havia `trad_opcode.txt`).
    pub assembly: bool,
    /// A linha do fonte aparece como texto (havia `trad_cmm.txt`).
    pub source: bool,
    /// As tabelas são mais novas que a onda: o processador foi compilado de
    /// novo depois da simulação, e o assembly e a linha do C± sairiam
    /// deslocados. O PC e a linha aparecem como números até simular de novo;
    /// as interfaces avisam.
    pub outdated: bool,
}

/// Um layout gravado em disco, pronto para [`open_waveform`](crate::open_waveform).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PreparedLayout {
    /// A pasta do layout. O Surfer roda nela para achar os tradutores em
    /// `.surfer/mappings/`.
    pub dir: Utf8PathBuf,
    /// O `.surf.ron`.
    pub state: Utf8PathBuf,
    /// Os arquivos dos tradutores.
    pub mappings: Vec<Utf8PathBuf>,
    /// O que foi gravado.
    pub layout: WaveLayout,
}

/// Monta o layout de `waveform` sem gravar nada. `Ok(None)` quando a onda
/// não é VCD nem FST (o GHW abre sem layout) ou não tem sinal nenhum na raiz
/// nem processador.
///
/// Uma onda sem processador SAPHO (projeto só de Verilog) recebe o grupo
/// Top-level com os sinais do testbench, como a AURORA; o resto do design
/// fica na hierarquia do Surfer.
///
/// As tabelas de cada processador vêm da pasta temporária dele no projeto
/// que contém a onda ([`Project::discover`]); fora de projeto, da pasta da
/// onda, se ela for a de um processador (tem `pc_<nome>_mem.txt`). Sem as
/// tabelas, ou com tabelas mais novas que a onda
/// ([`WaveProcessor::outdated`]), o PC e a linha aparecem como números.
///
/// # Erros
///
/// [`LaceError::Io`] se a onda ou uma tabela existente não puder ser lida.
pub fn wave_layout(waveform: &Utf8Path) -> Result<Option<WaveLayout>> {
    let mut wave = Wave::open(waveform)?;
    let Some(header) = wave.header(waveform)? else {
        return Ok(None);
    };
    let scopes = parse_scopes(&header);
    let processors = detect_processors(&scopes);
    let root_signals = scopes
        .iter()
        .any(|s| !s.path.contains('.') && !s.signals.is_empty());
    if processors.is_empty() && !root_signals {
        return Ok(None);
    }
    let project = Project::discover(waveform).ok();
    let recorded = std::fs::metadata(waveform).and_then(|m| m.modified()).ok();
    let mut tables = HashMap::new();
    let mut c_processors = Vec::new();
    for proc in &processors {
        if !tables.contains_key(&proc.processor) {
            let dir = tables_dir(project.as_ref(), waveform, &proc.processor);
            let read = Tables::read(dir.as_deref(), &proc.processor, recorded)?;
            tables.insert(proc.processor.clone(), read);
        }
        let language = project
            .as_ref()
            .and_then(|p| p.processor(&proc.processor))
            .map(|p| p.language);
        if language == Some(crate::Language::Cpp) {
            c_processors.push(proc.processor.clone());
        }
    }
    let complex_ids = complex_ids(&scopes);
    let complex = if complex_ids.is_empty() {
        None
    } else {
        complex_mapping(&wave.complex_values(&complex_ids, waveform)?)
    };
    Ok(Some(build_layout(
        waveform,
        &scopes,
        &processors,
        &tables,
        &c_processors,
        complex,
    )))
}

/// Monta o layout de `waveform` e grava em `<projeto>/.lace/Temp/surfer/<onda>-<hash>/`
/// (fora de projeto, na pasta de cache do usuário, como o log do Surfer): o
/// `<onda>.surf.ron` e os tradutores em `.surfer/mappings/`. Tradutores
/// antigos dessa pasta saem antes. `Ok(None)` como em [`wave_layout`].
///
/// # Erros
///
/// [`LaceError::Io`] se a onda não puder ser lida ou a pasta não puder ser
/// gravada.
pub fn prepare_wave_layout(waveform: &Utf8Path) -> Result<Option<PreparedLayout>> {
    let Some(layout) = wave_layout(waveform)? else {
        return Ok(None);
    };
    let name = waveform.file_name().expect("a file has a name");
    let stem = waveform.file_stem().unwrap_or(name);
    let dir = crate::wave::work_dir(waveform)?.join(layout_dir_name(waveform));
    let mappings_dir = dir.join(MAPPINGS_DIR);
    if mappings_dir.is_dir() {
        std::fs::remove_dir_all(&mappings_dir)
            .map_err(LaceError::io("Removing old translators", &mappings_dir))?;
    }
    std::fs::create_dir_all(&mappings_dir)
        .map_err(LaceError::io("Creating folder", &mappings_dir))?;
    let state = dir.join(format!("{stem}.surf.ron"));
    std::fs::write(&state, &layout.state).map_err(LaceError::io("Writing", &state))?;
    let mut mappings = Vec::new();
    for mapping in &layout.mappings {
        let path = mappings_dir.join(&mapping.name);
        std::fs::write(&path, &mapping.content).map_err(LaceError::io("Writing", &path))?;
        mappings.push(path);
    }
    Ok(Some(PreparedLayout {
        dir,
        state,
        mappings,
        layout,
    }))
}

/// O nome da pasta do layout preparado de `waveform`: o nome da onda e um
/// hash do caminho inteiro (`soma_tb-1a2b3c4d`). Duas ondas com o mesmo
/// nome em pastas diferentes do projeto não dividem a pasta, e uma não apaga
/// os tradutores da outra.
fn layout_dir_name(waveform: &Utf8Path) -> String {
    let stem = waveform
        .file_stem()
        .or(waveform.file_name())
        .unwrap_or("wave");
    // FNV-1a de 32 bits: estável entre execuções e versões do Rust.
    let hash = waveform.as_str().bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    format!("{stem}-{hash:08x}")
}

// Cabeçalho da onda ----------------------------------------------------------

/// Uma onda aberta: VCD (texto) ou FST (binário, o que o Icarus grava).
enum Wave {
    Vcd(BufReader<File>),
    Fst(Box<FstReader<BufReader<File>>>),
}

impl Wave {
    fn open(waveform: &Utf8Path) -> Result<Wave> {
        let file = File::open(waveform).map_err(LaceError::io("Reading", waveform))?;
        let mut reader = BufReader::new(file);
        if fst_reader::is_fst_file(&mut reader) {
            let fst = FstReader::open(reader).map_err(|e| fst_error(waveform, e))?;
            return Ok(Wave::Fst(Box::new(fst)));
        }
        Ok(Wave::Vcd(reader))
    }

    /// O cabeçalho como texto de VCD (`$scope`, `$var`, `$upscope`). `None`
    /// se a onda não for VCD nem FST (o GHW).
    fn header(&mut self, waveform: &Utf8Path) -> Result<Option<String>> {
        match self {
            Wave::Vcd(reader) => read_header(reader, waveform),
            Wave::Fst(fst) => fst_header(fst, waveform).map(Some),
        }
    }

    /// Os valores distintos que os sinais `ids` (id do cabeçalho e largura)
    /// assumem, em bits, até [`MAX_COMPLEX_VALUES`].
    fn complex_values(
        &mut self,
        ids: &HashMap<String, u32>,
        waveform: &Utf8Path,
    ) -> Result<BTreeSet<String>> {
        match self {
            Wave::Vcd(reader) => vcd_complex_values(reader, ids, waveform),
            Wave::Fst(fst) => fst_complex_values(fst, ids, waveform),
        }
    }
}

/// Um erro de leitura do FST como o de qualquer arquivo que não se lê.
fn fst_error(waveform: &Utf8Path, error: impl std::fmt::Display) -> LaceError {
    LaceError::io("Reading", waveform)(std::io::Error::other(error.to_string()))
}

/// O cabeçalho de uma onda FST escrito como o de um VCD, para o resto do
/// layout ler os dois formatos do mesmo jeito. O id de cada sinal é o índice
/// do handle dele no FST.
fn fst_header(fst: &mut FstReader<BufReader<File>>, waveform: &Utf8Path) -> Result<String> {
    let mut header = String::new();
    fst.read_hierarchy(|entry| match entry {
        FstHierarchyEntry::Scope { name, .. } => {
            let _ = writeln!(header, "$scope module {name} $end");
        }
        FstHierarchyEntry::UpScope => header.push_str("$upscope $end\n"),
        FstHierarchyEntry::Var {
            tpe,
            name,
            length,
            handle,
            ..
        } => {
            let kind = if tpe.is_real() { "real" } else { "wire" };
            let _ = writeln!(
                header,
                "$var {kind} {length} {} {name} $end",
                handle.get_index()
            );
        }
        _ => {}
    })
    .map_err(|e| fst_error(waveform, e))?;
    header.push_str("$enddefinitions $end\n");
    Ok(header)
}

/// Os bits de um valor com a largura do sinal: o VCD e o FST podem omitir os
/// zeros à esquerda.
fn fit_bits(bits: &str, width: usize) -> String {
    if bits.len() >= width {
        bits[bits.len() - width..].to_owned()
    } else {
        format!("{}{bits}", "0".repeat(width - bits.len()))
    }
}

// Cabeçalho do VCD ---------------------------------------------------------

/// Um escopo do cabeçalho, com o caminho inteiro (`soma_tb.proc`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Scope {
    path: String,
    signals: Vec<Signal>,
}

/// Um `$var` do cabeçalho.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Signal {
    name: String,
    width: u32,
    id: String,
    real: bool,
}

/// O texto do cabeçalho, até `$enddefinitions`. `None` se o arquivo não for
/// VCD (o FST e o GHW são binários).
fn read_header(reader: &mut impl BufRead, waveform: &Utf8Path) -> Result<Option<String>> {
    let mut header = String::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(LaceError::io("Reading", waveform))?;
        if read == 0 {
            return Ok(None);
        }
        let Ok(text) = std::str::from_utf8(&line) else {
            return Ok(None);
        };
        if header.is_empty() && !text.trim().is_empty() && !text.trim_start().starts_with('$') {
            return Ok(None);
        }
        header.push_str(text);
        if text.contains("$enddefinitions") {
            return Ok(Some(header));
        }
        if header.len() as u64 > MAX_HEADER {
            return Ok(None);
        }
    }
}

/// Os escopos do cabeçalho, na ordem em que aparecem. O Icarus abre o mesmo
/// escopo uma vez por `$dumpvars`; os sinais vão todos para o mesmo.
fn parse_scopes(header: &str) -> Vec<Scope> {
    let mut scopes: Vec<Scope> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut stack: Vec<String> = Vec::new();
    let mut tokens = header.split_whitespace();
    while let Some(token) = tokens.next() {
        match token {
            "$scope" => {
                let words = until_end(&mut tokens);
                if let Some(name) = words.get(1) {
                    stack.push(name.replace('.', &ESCAPED_DOT.to_string()));
                    let path = stack.join(".");
                    if !index.contains_key(&path) {
                        index.insert(path.clone(), scopes.len());
                        scopes.push(Scope {
                            path,
                            signals: Vec::new(),
                        });
                    }
                }
            }
            "$upscope" => {
                until_end(&mut tokens);
                stack.pop();
            }
            "$var" => {
                let words = until_end(&mut tokens);
                if stack.is_empty() || words.len() < 4 {
                    continue;
                }
                let name = words[3];
                // `valr2[15:0]`, quando o simulador cola a faixa no nome.
                let name = match name.find('[') {
                    Some(i) if name.ends_with(']') && name[i..].contains(':') => &name[..i],
                    _ => name,
                };
                let scope = &mut scopes[index[&stack.join(".")]];
                if scope.signals.iter().any(|s| s.name == name) {
                    continue;
                }
                scope.signals.push(Signal {
                    name: name.to_owned(),
                    width: words[1].parse().unwrap_or(0),
                    id: words[2].to_owned(),
                    real: words[0] == "real",
                });
            }
            "$enddefinitions" => break,
            t if t.starts_with('$') && t != "$end" => {
                until_end(&mut tokens);
            }
            _ => {}
        }
    }
    scopes
}

/// As palavras até o próximo `$end`, sem ele.
fn until_end<'a>(tokens: &mut impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    tokens.take_while(|t| *t != "$end").collect()
}

impl Scope {
    fn signal(&self, name: &str) -> Option<&Signal> {
        self.signals.iter().find(|s| s.name == name)
    }
}

fn scope<'a>(scopes: &'a [Scope], path: &str) -> Option<&'a Scope> {
    scopes.iter().find(|s| s.path == path)
}

// Processadores --------------------------------------------------------------

/// Um processador achado no cabeçalho.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Detected {
    /// O escopo com `valr2` e `linetabs`.
    instance: String,
    /// O processador (pasta das tabelas).
    processor: String,
    /// Onde estão `clk`, `rst`, a pilha e a ULA: `<instância>.p_<nome>.core`
    /// se existir.
    core: String,
}

/// Os processadores: cada escopo com `valr2` e `linetabs`. O nome sai, nesta
/// ordem, do subescopo `p_<nome>.core` que o YANC cria, do pai `<nome>_inst`,
/// da raiz `<nome>_tb` ou do próprio nome da instância (a regra da AURORA,
/// `gtkw_proc_writer.ts`, sem a leitura do Verilog).
fn detect_processors(scopes: &[Scope]) -> Vec<Detected> {
    let mut found = Vec::new();
    for scope in scopes {
        if scope.signal("valr2").is_none() || scope.signal("linetabs").is_none() {
            continue;
        }
        let parts: Vec<&str> = scope.path.split('.').collect();
        let instance_name = parts[parts.len() - 1];
        let prefix = format!("{}.", scope.path);
        let core = scopes.iter().find_map(|other| {
            let tail = other.path.strip_prefix(&prefix)?;
            let name = tail.strip_prefix("p_")?;
            let name = name.split('.').next()?;
            let rest = &tail[2 + name.len()..];
            (rest == ".core" || rest.starts_with(".core.")).then(|| name.to_owned())
        });
        let processor = core
            .clone()
            .or_else(|| {
                (parts.len() >= 2)
                    .then(|| parts[parts.len() - 2])
                    .and_then(|p| strip_suffix_ci(p, "_inst"))
            })
            .or_else(|| strip_suffix_ci(parts[0], "_tb"))
            .unwrap_or_else(|| {
                strip_suffix_ci(instance_name, "_inst").unwrap_or_else(|| instance_name.to_owned())
            });
        found.push(Detected {
            instance: scope.path.clone(),
            core: core.map_or_else(
                || scope.path.clone(),
                |name| format!("{}.p_{name}.core", scope.path),
            ),
            processor,
        });
    }
    found
}

fn strip_suffix_ci(text: &str, suffix: &str) -> Option<String> {
    let cut = text.len().checked_sub(suffix.len())?;
    (cut > 0 && text.is_char_boundary(cut) && text[cut..].eq_ignore_ascii_case(suffix))
        .then(|| text[..cut].to_owned())
}

/// A pasta das tabelas do YANC de `processor`.
fn tables_dir(
    project: Option<&Project>,
    waveform: &Utf8Path,
    processor: &str,
) -> Option<Utf8PathBuf> {
    if let Some(proc) = project.and_then(|p| p.processor(processor)) {
        return Some(proc.temp_dir.clone());
    }
    let dir = waveform.parent()?;
    dir.join(format!("pc_{processor}_mem.txt"))
        .is_file()
        .then(|| dir.to_owned())
}

/// As tabelas do YANC de um processador.
#[derive(Debug, Default)]
struct Tables {
    opcode: Option<String>,
    source: Option<String>,
    /// Mais novas que a onda: não valem para ela.
    outdated: bool,
    /// Os pares (função, variável) do `cmm_log.txt`: desfazem o nome de um
    /// sinal como `me1_f_get_v_x_v_p_e_`, em que a função tem `_v_`.
    variables: Vec<(String, String)>,
}

impl Tables {
    /// Lê as tabelas de `dir`. Se alguma (ou o `pc_<nome>_mem.txt` do mesmo
    /// build) foi gravada depois da onda (`recorded`), o build é de outro
    /// programa: nada é lido, e `outdated` diz por quê.
    fn read(
        dir: Option<&Utf8Path>,
        processor: &str,
        recorded: Option<std::time::SystemTime>,
    ) -> Result<Tables> {
        let Some(dir) = dir else {
            return Ok(Tables::default());
        };
        let newer = |name: &str| {
            let modified = std::fs::metadata(dir.join(name)).and_then(|m| m.modified());
            matches!((modified, recorded), (Ok(table), Some(wave)) if table > wave)
        };
        let memory = format!("pc_{processor}_mem.txt");
        if ["trad_opcode.txt", "trad_cmm.txt", memory.as_str()]
            .into_iter()
            .any(newer)
        {
            return Ok(Tables {
                outdated: true,
                ..Tables::default()
            });
        }
        let read = |name: &str| -> Result<Option<String>> {
            let path = dir.join(name);
            if !path.is_file() {
                return Ok(None);
            }
            let bytes = std::fs::read(&path).map_err(LaceError::io("Reading", &path))?;
            // O fonte pode ter acento em Latin-1; um caractere trocado não
            // impede a tabela.
            Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
        };
        // `<função> <variável> <tipo> [...]`, uma por declaração; `num_ins`
        // e outras linhas de dois campos ficam de fora.
        let variables = read("cmm_log.txt")?
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                let (func, var, kind) = (words.next()?, words.next()?, words.next()?);
                kind.parse::<u32>()
                    .ok()
                    .map(|_| (func.to_owned(), var.to_owned()))
            })
            .collect();
        Ok(Tables {
            opcode: read("trad_opcode.txt")?,
            source: read("trad_cmm.txt")?,
            outdated: false,
            variables,
        })
    }
}

// Tradutores -----------------------------------------------------------------

/// O nome de um tradutor, só com letras, dígitos e `_` (vira nome de
/// arquivo).
fn mapping_name(kind: &str, processor: &str) -> String {
    format!("lace_{kind}_{processor}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Uma tabela do YANC (`<número> <texto>` por linha) como tradutor do
/// Surfer, que casa pelo valor dos bits do sinal de `bits` de largura:
///
/// - um número negativo (as linhas especiais -1, -2 e -3 do `trad_cmm.txt`)
///   vira o padrão de bits sem sinal (`0xFFFFF` para -1 em 20 bits), porque
///   o Surfer não casa valor com sinal; sem largura, sai;
/// - linha sem texto sai: o Surfer recusa o arquivo inteiro por ela;
/// - o texto vai como está, menos os espaços do fim.
fn convert_table(name: &str, bits: u32, table: &str) -> String {
    let mut out = format!("Name = {name}\n");
    if bits > 0 {
        let _ = writeln!(out, "Bits = {bits}");
    }
    for line in table.lines() {
        let (key, text) = line.split_once(' ').unwrap_or((line, ""));
        let text = text.trim_end();
        if key.is_empty() || text.is_empty() {
            continue;
        }
        let key = match key.strip_prefix('-') {
            Some(digits) => {
                let (Ok(n), true) = (digits.parse::<u64>(), (1..=64).contains(&bits)) else {
                    continue;
                };
                let modulus = if bits == 64 { 0 } else { 1u128 << bits };
                let value = if modulus == 0 {
                    0u64.wrapping_sub(n)
                } else {
                    ((modulus - u128::from(n) % modulus) % modulus) as u64
                };
                format!("0x{value:X}")
            }
            None if key.bytes().all(|b| b.is_ascii_digit()) => key.to_owned(),
            None => continue,
        };
        let _ = writeln!(out, "{key} {text}");
    }
    out
}

/// Os identificadores do VCD dos sinais complexos e a largura de cada um.
fn complex_ids(scopes: &[Scope]) -> HashMap<String, u32> {
    scopes
        .iter()
        .flat_map(|s| &s.signals)
        .filter(|s| is_complex(&s.name))
        .map(|s| (s.id.clone(), s.width))
        .collect()
}

fn is_complex(name: &str) -> bool {
    name.starts_with("comp_me3_") || name.starts_with("comp_arr_me3_")
}

/// O tradutor dos complexos: o Surfer não roda programa para traduzir, então
/// os valores distintos que os complexos assumem na onda (`values`, lidos do
/// corpo do VCD ou dos blocos do FST) viram cada um uma linha
/// `0b<bits> <re> <im>i`. Um tradutor só, sem largura, serve a todos: a
/// tradução só depende dos bits.
fn complex_mapping(values: &BTreeSet<String>) -> Option<MappingTranslator> {
    let name = "lace_complex".to_owned();
    let mut content = format!("Name = {name}\n");
    let mut any = false;
    for bits in values {
        if let Some(text) = decode_complex(bits) {
            let _ = writeln!(content, "0b{bits} {text}");
            any = true;
        }
    }
    any.then_some(MappingTranslator { name, content })
}

/// Os valores dos complexos no corpo de um VCD.
fn vcd_complex_values(
    reader: &mut impl BufRead,
    ids: &HashMap<String, u32>,
    waveform: &Utf8Path,
) -> Result<BTreeSet<String>> {
    // O corpo de uma onda longa tem milhões de linhas: a leitura é em bytes,
    // num buffer só, e só as linhas `b<bits> <id>` de um id complexo viram
    // texto.
    let ids: HashMap<&[u8], usize> = ids
        .iter()
        .map(|(id, width)| (id.as_bytes(), *width as usize))
        .collect();
    let mut values = BTreeSet::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(LaceError::io("Reading", waveform))?;
        if read == 0 || values.len() >= MAX_COMPLEX_VALUES {
            break;
        }
        if !matches!(line.first(), Some(b'b' | b'B')) {
            continue;
        }
        let text = line[1..].trim_ascii();
        let Some(space) = text.iter().position(|&b| b == b' ') else {
            continue;
        };
        let (bits, id) = (&text[..space], text[space + 1..].trim_ascii());
        let Some(&width) = ids.get(id) else {
            continue;
        };
        if bits.iter().all(|&b| b == b'0' || b == b'1') {
            let bits = std::str::from_utf8(bits).expect("0 and 1 are ASCII");
            values.insert(fit_bits(bits, width));
        }
    }
    Ok(values)
}

/// Os valores dos complexos numa onda FST: só os blocos desses sinais são
/// lidos, e a leitura para no teto.
fn fst_complex_values(
    fst: &mut FstReader<BufReader<File>>,
    ids: &HashMap<String, u32>,
    waveform: &Utf8Path,
) -> Result<BTreeSet<String>> {
    let widths: HashMap<usize, usize> = ids
        .iter()
        .filter_map(|(id, width)| Some((id.parse().ok()?, *width as usize)))
        .collect();
    let filter = FstFilter::filter_signals(
        widths
            .keys()
            .map(|&index| FstSignalHandle::from_index(index))
            .collect(),
    );
    let mut values = BTreeSet::new();
    let read = fst.read_signals(&filter, |_time, handle, value| {
        if values.len() >= MAX_COMPLEX_VALUES {
            return Err(());
        }
        if let FstSignalValue::String(bits) = value
            && let Some(&width) = widths.get(&handle.get_index())
            && bits.iter().all(|&b| b == b'0' || b == b'1')
        {
            let bits = std::str::from_utf8(bits).expect("0 and 1 are ASCII");
            values.insert(fit_bits(bits, width));
        }
        Ok(())
    });
    match read {
        Ok(()) | Err(ReadSignalsError::CallbackError(())) => Ok(values),
        Err(error) => Err(fst_error(waveform, error)),
    }
}

/// Um complexo do SAPHO como texto, como o `comp2gtkw` do YANC: os 8
/// primeiros bits são a largura da mantissa, os 8 seguintes a do expoente,
/// depois a parte real e a imaginária, cada uma `1 + expoente + mantissa`.
fn decode_complex(bits: &str) -> Option<String> {
    let mantissa = usize::from_str_radix(bits.get(0..8)?, 2).ok()?;
    let exponent = usize::from_str_radix(bits.get(8..16)?, 2).ok()?;
    let word = 1 + exponent + mantissa;
    let real = sapho_float(bits.get(16..16 + word)?, mantissa, exponent)?;
    let imag = sapho_float(bits.get(16 + word..16 + 2 * word)?, mantissa, exponent)?;
    Some(format!("{real:.3} {imag:.3}i"))
}

/// O ponto flutuante do SAPHO (não é o IEEE 754): sinal, expoente em
/// complemento de dois e mantissa sem bit implícito; valor = ±mantissa ·
/// 2^expoente. Arredondado para `f32`, como no `comp2gtkw`.
fn sapho_float(bits: &str, mantissa: usize, exponent: usize) -> Option<f64> {
    if exponent == 0 || exponent > 32 || mantissa == 0 || mantissa > 52 {
        return None;
    }
    let negative = bits.get(0..1)? == "1";
    let raw = i64::from_str_radix(bits.get(1..1 + exponent)?, 2).ok()?;
    let exp = if raw >= 1 << (exponent - 1) {
        raw - (1 << exponent)
    } else {
        raw
    };
    let mant = u64::from_str_radix(bits.get(1 + exponent..1 + exponent + mantissa)?, 2).ok()?;
    let value = (mant as f64 * 2f64.powi(i32::try_from(exp).ok()?)) as f32;
    Some(f64::from(if negative { -value } else { value }))
}

// Itens do estado ------------------------------------------------------------

/// As cores do tema do Surfer que o layout usa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    Red,
    Yellow,
    Orange,
    Violet,
}

impl Color {
    fn ron(self) -> &'static str {
        match self {
            Color::Red => "Red",
            Color::Yellow => "Yellow",
            Color::Orange => "Orange",
            Color::Violet => "Violet",
        }
    }
}

/// Um item da lista de sinais do Surfer.
#[derive(Debug, Clone, PartialEq)]
enum Item {
    Variable(Variable),
    Group {
        name: String,
        open: bool,
        children: Vec<Item>,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct Variable {
    scope: String,
    name: String,
    label: Option<String>,
    format: Option<String>,
    color: Option<Color>,
    height: Option<f32>,
    analog: bool,
}

impl Variable {
    fn new(scope: &str, name: &str) -> Variable {
        Variable {
            scope: scope.to_owned(),
            name: name.to_owned(),
            label: None,
            format: None,
            color: None,
            height: None,
            analog: false,
        }
    }

    fn format(mut self, format: &str) -> Variable {
        self.format = Some(format.to_owned());
        self
    }

    fn label(mut self, label: impl Into<String>) -> Variable {
        self.label = Some(label.into());
        self
    }

    fn color(mut self, color: Color) -> Variable {
        self.color = Some(color);
        self
    }
}

/// Um grupo de seção (cabeçalho vermelho), só se tiver itens.
fn push_group(items: &mut Vec<Item>, name: &str, children: Vec<Item>, open: bool) {
    if !children.is_empty() {
        items.push(Item::Group {
            name: name.to_owned(),
            open,
            children,
        });
    }
}

/// O tradutor de um sinal de um bit: `Bit` desenha a onda quadrada.
fn plain_format(signal: &Signal) -> Option<&'static str> {
    match (signal.real, signal.width) {
        (true, _) => None,
        (false, 1) => Some("Bit"),
        _ => Some("Unsigned"),
    }
}

/// Monta o layout: o grupo dos sinais de fora dos processadores e um grupo
/// por processador (a ordem da AURORA, `buildSurferLayout`).
fn build_layout(
    waveform: &Utf8Path,
    scopes: &[Scope],
    processors: &[Detected],
    tables: &HashMap<String, Tables>,
    c_processors: &[String],
    complex: Option<MappingTranslator>,
) -> WaveLayout {
    let mut items = Vec::new();
    let mut mappings: BTreeMap<String, MappingTranslator> = BTreeMap::new();

    // Fora dos processadores, só os sinais das raízes (o clock e o reset do
    // testbench). O resto do projeto continua na hierarquia do Surfer.
    let inside = |path: &str| {
        processors
            .iter()
            .any(|p| path == p.instance || path.starts_with(&format!("{}.", p.instance)))
    };
    let top: Vec<Item> = scopes
        .iter()
        .filter(|s| !s.path.contains('.') && !inside(&s.path))
        .flat_map(|s| {
            s.signals.iter().map(|sig| {
                let mut var = Variable::new(&s.path, &sig.name);
                var.format = plain_format(sig).map(str::to_owned);
                Item::Variable(var)
            })
        })
        .collect();
    push_group(&mut items, "Top-level", top, true);

    let many = processors.len() > 1;
    let mut summaries = Vec::new();
    for proc in processors {
        let instance = scope(scopes, &proc.instance).expect("detected from the scopes");
        let root = proc.instance.split('.').next().unwrap_or_default();
        let mut children = Vec::new();

        for name in ["clk", "rst", "itr"] {
            let found = [proc.core.as_str(), root]
                .into_iter()
                .find(|path| scope(scopes, path).is_some_and(|s| s.signal(name).is_some()));
            if let Some(path) = found {
                let mut var = Variable::new(path, name).format("Bit");
                var.height = Some(0.8);
                children.push(Item::Variable(var));
            }
        }

        push_group(&mut children, "I/O", io_items(instance), true);

        let table = tables.get(&proc.processor);
        let mut assembly = false;
        let mut source = false;
        let mut asm_format = "Unsigned".to_owned();
        let mut src_format = "Signed".to_owned();
        if let Some(text) = table.and_then(|t| t.opcode.as_deref()) {
            let name = mapping_name("asm", &proc.processor);
            let bits = instance.signal("valr2").map_or(0, |s| s.width);
            mappings
                .entry(name.clone())
                .or_insert_with(|| MappingTranslator {
                    content: convert_table(&name, bits, text),
                    name: name.clone(),
                });
            asm_format = name;
            assembly = true;
        }
        if let Some(text) = table.and_then(|t| t.source.as_deref()) {
            let name = mapping_name("src", &proc.processor);
            let bits = instance.signal("linetabs").map_or(0, |s| s.width);
            mappings
                .entry(name.clone())
                .or_insert_with(|| MappingTranslator {
                    content: convert_table(&name, bits, text),
                    name: name.clone(),
                });
            src_format = name;
            source = true;
        }
        let instructions = vec![
            Item::Variable(
                Variable::new(&proc.instance, "valr2")
                    .format(&asm_format)
                    .color(Color::Violet)
                    .label(format!("Assembly ({})", proc.processor)),
            ),
            Item::Variable(
                Variable::new(&proc.instance, "linetabs")
                    .format(&src_format)
                    .color(Color::Violet)
                    // A linha é do fonte, C± ou C (no C, a do `pp.cpp`).
                    .label(if c_processors.contains(&proc.processor) {
                        format!("C ({})", proc.processor)
                    } else {
                        format!("C± ({})", proc.processor)
                    }),
            ),
        ];
        push_group(&mut children, "Instructions", instructions, true);

        let tag = if many {
            format!(" ({})", proc.processor)
        } else {
            String::new()
        };
        let complex_format = complex.as_ref().map(|m| m.name.as_str());
        let names = table.map_or(&[][..], |t| t.variables.as_slice());
        let (variables, count) = variable_items(instance, &tag, complex_format, names);
        push_group(&mut children, "Variables", variables, true);
        push_group(
            &mut children,
            "Flags",
            flag_items(scopes, &proc.core),
            false,
        );

        // O grupo leva o nome do processador; a instância vai junto quando
        // é outro nome (`proc (soma)` no testbench do YANC).
        let instance_name = proc.instance.rsplit('.').next().unwrap_or(&proc.instance);
        let title = if instance_name == proc.processor {
            proc.processor.clone()
        } else {
            format!("{instance_name} ({})", proc.processor)
        };
        push_group(&mut items, &title, children, true);
        summaries.push(WaveProcessor {
            instance: proc.instance.replace(ESCAPED_DOT, "."),
            processor: proc.processor.clone(),
            variables: count,
            assembly,
            source,
            outdated: table.is_some_and(|t| t.outdated),
        });
    }

    let mut mappings: Vec<MappingTranslator> = mappings.into_values().collect();
    let uses_complex = scopes.iter().any(|s| {
        processors.iter().any(|p| p.instance == s.path)
            && s.signals.iter().any(|v| is_complex(&v.name))
    });
    if let Some(complex) = complex.filter(|_| uses_complex) {
        mappings.push(complex);
    }
    WaveLayout {
        state: state_ron(waveform, &items),
        mappings,
        processors: summaries,
    }
}

/// `req_in N`, `input N`, `out_en N` e `output N`, em amarelo.
fn io_items(instance: &Scope) -> Vec<Item> {
    let numbered = |prefix: &str| {
        let mut found: Vec<(u32, &Signal)> = instance
            .signals
            .iter()
            .filter_map(|s| {
                let rest = s.name.strip_prefix(prefix)?;
                let rest = rest.strip_prefix('_').unwrap_or(rest);
                Some((rest.parse().ok()?, s))
            })
            .collect();
        found.sort_by_key(|(n, _)| *n);
        found
    };
    let var = |signal: &Signal, format: &str, label: String| {
        Item::Variable(
            Variable::new(&instance.path, &signal.name)
                .format(format)
                .color(Color::Yellow)
                .label(label),
        )
    };
    // O rótulo leva o número da porta, que está no nome do sinal: um
    // programa que só usa `in(2)` tem só `in_sim_2`, que é a entrada 2.
    let mut items = Vec::new();
    for (control, data) in [
        (("req_in_sim", "req_in"), ("in_sim", "input")),
        (("out_en_sim", "out_en"), ("out_sig", "output")),
    ] {
        let (controls, values) = (numbered(control.0), numbered(data.0));
        let ports: BTreeSet<u32> = controls.iter().chain(&values).map(|(n, _)| *n).collect();
        for port in ports {
            if let Some((_, s)) = controls.iter().find(|(n, _)| *n == port) {
                items.push(var(s, "Bit", format!("{} {port}", control.1)));
            }
            if let Some((_, s)) = values.iter().find(|(n, _)| *n == port) {
                items.push(var(s, "Signed", format!("{} {port}", data.1)));
            }
        }
    }
    items
}

/// `(função, variável)` de `me1_f_main_v_soma_e_`; `global` fica `global`.
/// Com os pares do `cmm_log.txt` (`names`), o par cujo sufixo
/// `_f_<função>_v_<variável>_e_` fecha o sinal: sem eles, uma função com
/// `_v_` no nome (`get_v_x`) seria cortada no lugar errado.
fn variable_name(signal: &str, names: &[(String, String)]) -> Option<(String, String)> {
    let shown = |func: &str| {
        if func == "global" {
            "global".to_owned()
        } else {
            format!("{func}()")
        }
    };
    let known = names
        .iter()
        .filter(|(func, var)| signal.ends_with(&format!("_f_{func}_v_{var}_e_")))
        .max_by_key(|(func, var)| func.len() + var.len());
    if let Some((func, var)) = known {
        return Some((shown(func), var.clone()));
    }
    let rest = signal.split_once("_f_")?.1;
    let (func, rest) = rest.split_once("_v_")?;
    let var = rest.rsplit_once("_e_")?.0;
    Some((shown(func), var.to_owned()))
}

/// As variáveis do programa: `int` com sinal, `float` como o número real
/// que o YANC já calcula na simulação, complexo pelo tradutor (sem ele, em
/// binário), e cada array num grupo fechado. Devolve também quantos sinais.
fn variable_items(
    instance: &Scope,
    tag: &str,
    complex: Option<&str>,
    names: &[(String, String)],
) -> (Vec<Item>, usize) {
    let complex = complex.unwrap_or("Binary");
    let kinds = [
        ("int", "me1_", "arr_me1_", Some("Signed")),
        ("float", "me2_", "arr_me2_", None),
        ("comp", "comp_me3_", "comp_arr_me3_", Some(complex)),
    ];
    let mut sorted: Vec<&Signal> = instance.signals.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let mut items = Vec::new();
    let mut count = 0;
    for (label, scalar, _, format) in kinds {
        for signal in sorted
            .iter()
            .filter(|s| s.name.starts_with(scalar) && s.name.ends_with("_e_"))
        {
            let Some((func, var)) = variable_name(&signal.name, names) else {
                continue;
            };
            let mut item = Variable::new(&instance.path, &signal.name)
                .color(Color::Orange)
                .label(format!("{label} {var} in {func}{tag}"));
            item.format = format.map(str::to_owned);
            items.push(Item::Variable(item));
            count += 1;
        }
    }
    for (label, _, array, format) in kinds {
        let mut arrays: BTreeMap<&str, Vec<(u32, &Signal)>> = BTreeMap::new();
        for signal in instance
            .signals
            .iter()
            .filter(|s| s.name.starts_with(array))
        {
            // `arr_me1_f_main_v_x_e_0001`: o índice é tudo depois do último
            // `_e_` (o asmcomp usa `%04d`, então o 10000 tem cinco dígitos).
            let name = signal.name.as_str();
            let Some(cut) = name.rfind("_e_").map(|at| at + "_e_".len()) else {
                continue;
            };
            let (base, digits) = name.split_at(cut);
            if let (false, true, Ok(index)) = (
                digits.is_empty(),
                digits.bytes().all(|b| b.is_ascii_digit()),
                digits.parse(),
            ) {
                arrays.entry(base).or_default().push((index, signal));
            }
        }
        for (base, mut elements) in arrays {
            elements.sort_by_key(|(i, _)| *i);
            let (func, var) =
                variable_name(base, names).unwrap_or_else(|| (String::new(), base.to_owned()));
            let children: Vec<Item> = elements
                .iter()
                .map(|(index, signal)| {
                    let mut item = Variable::new(&instance.path, &signal.name)
                        .color(Color::Orange)
                        .label(format!("{var} {index}"));
                    item.format = format.map(str::to_owned);
                    Item::Variable(item)
                })
                .collect();
            count += children.len();
            push_group(
                &mut items,
                &format!("{label} {var} in {func}{tag}"),
                children,
                false,
            );
        }
    }
    (items, count)
}

/// As pilhas de dados e de instruções e o erro de arredondamento da ULA.
fn flag_items(scopes: &[Scope], core: &str) -> Vec<Item> {
    let mut stack = Vec::new();
    for (sub, what) in [("sp", "Data"), ("isp", "Inst")] {
        let path = format!("{core}.{sub}");
        let Some(found) = scope(scopes, &path) else {
            continue;
        };
        for (name, format, analog, label) in [
            ("pointeri", "Signed", true, "Stack Pointer"),
            ("fl_max", "Unsigned", false, "Stack Max"),
            ("fl_full", "Bit", false, "Stack Overflow"),
        ] {
            if found.signal(name).is_some() {
                let mut var = Variable::new(&path, name)
                    .format(format)
                    .label(format!("{what} {label}"));
                var.analog = analog;
                stack.push(Item::Variable(var));
            }
        }
    }
    let mut alu = Vec::new();
    let path = format!("{core}.ula");
    if let Some(found) = scope(scopes, &path) {
        for (name, label) in [
            ("delta_int", "Rounding Error (int)"),
            ("delta_float", "Rounding Error (float)"),
        ] {
            if let Some(signal) = found.signal(name) {
                // O Icarus grava `delta_int` e `delta_float` como `real`: um
                // tradutor de bits não vale para eles.
                let mut var = Variable::new(&path, name).label(label);
                if !signal.real {
                    var.format = Some("Hexadecimal".to_owned());
                }
                var.analog = true;
                alu.push(Item::Variable(var));
            }
        }
    }
    let mut items = Vec::new();
    push_group(&mut items, "Stack", stack, true);
    push_group(&mut items, "ULA", alu, true);
    items
}

// O `.surf.ron` ---------------------------------------------------------------

fn ron_str(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn ron_opt(text: Option<&str>) -> String {
    text.map_or_else(|| "None".to_owned(), |t| format!("Some({})", ron_str(t)))
}

/// O estado do Surfer com `items`. A lista vira o `items_tree` (o grupo
/// antes dos filhos, que ficam um nível abaixo) e o `displayed_items`; o
/// resto é o estado padrão do Surfer. O `WaveData` do surfer-aurora não tem
/// valor padrão para `annotation_groups`, `annotation_list_visible` e
/// `last_active_viewport_idx`: sem eles, o Surfer ignora o arquivo inteiro.
fn state_ron(waveform: &Utf8Path, items: &[Item]) -> String {
    struct Out {
        tree: String,
        displayed: String,
        next_ref: u32,
        scopes: HashMap<String, u32>,
        next_var: u32,
    }
    fn visit(out: &mut Out, item: &Item, level: u32) {
        let reference = out.next_ref;
        out.next_ref += 1;
        let unfolded = match item {
            Item::Group { open, .. } => *open,
            Item::Variable(_) => true,
        };
        let _ = write!(
            out.tree,
            "                (\n                    item_ref: ({reference}),\n                    level: {level},\n                    unfolded: {unfolded},\n                    selected: false,\n                ),\n"
        );
        match item {
            Item::Group {
                name,
                open,
                children,
            } => {
                for child in children {
                    visit(out, child, level + 1);
                }
                let _ = write!(
                    out.displayed,
                    "            ({reference}): Group((\n                name: {},\n                color: Some(\"{}\"),\n                background_color: None,\n                content: [],\n                is_open: {open},\n            )),\n",
                    ron_str(name),
                    Color::Red.ron(),
                );
            }
            Item::Variable(var) => {
                let count = out.scopes.len() as u32 + 1;
                let scope_id = *out.scopes.entry(var.scope.clone()).or_insert(count);
                let var_id = out.next_var;
                out.next_var += 1;
                let strs: String = var
                    .scope
                    .split('.')
                    .map(|s| {
                        format!(
                            "                            {},\n",
                            ron_str(&s.replace(ESCAPED_DOT, "."))
                        )
                    })
                    .collect();
                let height = var
                    .height
                    .map_or_else(|| "None".to_owned(), |h| format!("Some({h:?})"));
                let analog = if var.analog {
                    "Some((\n                    settings: (\n                        render_style: Step,\n                        y_axis_scale: TypeLimits,\n                    ),\n                ))"
                } else {
                    "None"
                };
                let _ = write!(
                    out.displayed,
                    "            ({reference}): Variable((\n                variable_ref: (\n                    path: (\n                        strs: [\n{strs}                        ],\n                        id: Wellen(({scope_id})),\n                    ),\n                    name: {name},\n                    id: Wellen(({var_id})),\n                    index: None,\n                ),\n                color: {color},\n                background_color: None,\n                display_name: {name},\n                display_name_type: Unique,\n                manual_name: {label},\n                format: {format},\n                field_formats: [],\n                height_scaling_factor: {height},\n                analog: {analog},\n            )),\n",
                    name = ron_str(&var.name),
                    color = ron_opt(var.color.map(Color::ron)),
                    label = ron_opt(var.label.as_deref()),
                    format = ron_opt(var.format.as_deref()),
                );
            }
        }
    }
    let mut out = Out {
        tree: String::new(),
        displayed: String::new(),
        next_ref: 1,
        scopes: HashMap::new(),
        next_var: 1,
    };
    for item in items {
        visit(&mut out, item, 0);
    }
    let source = ron_str(waveform.as_str());
    format!(
        r#"(
    show_hierarchy: None,
    show_menu: None,
    show_ticks: None,
    show_toolbar: None,
    show_tooltip: None,
    show_scope_tooltip: None,
    show_default_timeline: None,
    show_overview: None,
    show_statusbar: None,
    align_names_right: None,
    show_variable_indices: None,
    show_variable_direction: None,
    show_empty_scopes: None,
    show_hierarchy_icons: None,
    show_parameters_in_scopes: None,
    parameter_display_location: None,
    highlight_focused: None,
    fill_high_values: None,
    primary_button_drag_behavior: None,
    arrow_key_bindings: None,
    clock_highlight_type: None,
    hierarchy_style: None,
    autoload_sibling_state_files: None,
    autoreload_files: None,
    waves: Some((
        source: File({source}),
        format: Vcd,
        active_scope: None,
        items_tree: (
            items: [
{tree}            ],
        ),
        displayed_items: {{
{displayed}        }},
        display_item_ref_counter: {counter},
        viewports: [
            (
                curr_left: (0.0),
                curr_right: (1.0),
                target_left: (0.0),
                target_right: (1.0),
                move_start_left: (0.0),
                move_start_right: (1.0),
                move_duration: None,
                move_strategy: Instant,
            ),
        ],
        cursor: None,
        markers: {{}},
        annotation_groups: [],
        annotation_list_visible: false,
        last_active_viewport_idx: 0,
        focused_item: None,
        focused_transaction: (None, None),
        default_variable_name_type: Unique,
        scroll_offset: 0.0,
        display_variable_indices: true,
        graphics: {{}},
    )),
    drag_started: false,
    drag_source_idx: None,
    drag_target_idx: None,
    previous_waves: None,
    count: None,
    blacklisted_translators: [],
    show_about: false,
    show_keys: false,
    show_gestures: false,
    show_quick_start: false,
    show_license: false,
    show_performance: false,
    show_logs: false,
    show_cursor_window: false,
    frame_buffer: {{
        "pixels_per_row": 16,
        "square_pixels": true,
        "color_mode": Grayscale,
        "grayscale_bits": 1,
        "r_bits": 3,
        "g_bits": 3,
        "b_bits": 2,
        "y_bits": 8,
        "cb_bits": 8,
        "cr_bits": 8,
    }},
    wanted_timeunit: PicoSeconds,
    time_string_format: None,
    show_url_entry: false,
    variable_name_filter_focused: false,
    variable_filter: (
        name_filter_type: Contain,
        name_filter_str: "",
        name_filter_case_insensitive: true,
        include_inputs: true,
        include_outputs: true,
        include_inouts: true,
        include_others: true,
        group_by_direction: false,
    ),
    sidepanel_width: Some(300.0),
    ui_zoom_factor: None,
    animation_enabled: None,
    use_dinotrace_style: None,
    transition_value: None,
)
"#,
        tree = out.tree,
        displayed = out.displayed,
        counter = out.next_ref,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O cabeçalho que o Icarus grava para o testbench do YANC: cada
    /// `$dumpvars` abre o escopo de novo.
    const HEADER: &str = "$date today $end
$version Icarus Verilog $end
$timescale 1ps $end
$scope module soma_tb $end
$var reg 1 ! clk $end
$upscope $end
$scope module soma_tb $end
$var reg 1 \" rst $end
$upscope $end
$scope module soma_tb $end
$scope module proc $end
$var reg 1 # req_in_sim_0 $end
$var reg 16 $ in_sim_0 [15:0] $end
$var reg 1 % out_en_sim_0 $end
$var reg 16 & out_sig_0 [15:0] $end
$var reg 16 ' valr2 [15:0] $end
$var reg 20 ( linetabs [19:0] $end
$var reg 16 ) me1_f_main_v_soma_e_ [15:0] $end
$var real 1 * me2_f_global_v_ganho_e_ $end
$var wire 32 + comp_me3_f_main_v_z_e_ [31:0] $end
$var reg 16 , arr_me1_f_main_v_x_e_0001 [15:0] $end
$var reg 16 - arr_me1_f_main_v_x_e_0000 [15:0] $end
$upscope $end
$upscope $end
$scope module soma_tb $end
$scope module proc $end
$scope module p_soma $end
$scope module core $end
$scope module sp $end
$var reg 2 . pointeri [1:0] $end
$var reg 1 / fl_full $end
$upscope $end
$scope module ula $end
$var reg 16 0 delta_int [15:0] $end
$upscope $end
$upscope $end
$upscope $end
$upscope $end
$upscope $end
$enddefinitions $end
";

    /// Um complexo do SAPHO com mantissa de 4 bits e expoente de 3:
    /// real = +3·2¹ = 6, imaginária = -1·2⁻¹ = -0,5.
    const COMPLEX: &str = "00000100000000110001001111110001";

    fn scopes() -> Vec<Scope> {
        parse_scopes(HEADER)
    }

    #[test]
    fn repeated_scopes_are_merged_and_ranges_dropped() {
        let scopes = scopes();
        let paths: Vec<&str> = scopes.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "soma_tb",
                "soma_tb.proc",
                "soma_tb.proc.p_soma",
                "soma_tb.proc.p_soma.core",
                "soma_tb.proc.p_soma.core.sp",
                "soma_tb.proc.p_soma.core.ula"
            ]
        );
        let tb = scope(&scopes, "soma_tb").unwrap();
        assert_eq!(tb.signals.len(), 2);
        let proc = scope(&scopes, "soma_tb.proc").unwrap();
        let valr2 = proc.signal("valr2").unwrap();
        assert_eq!((valr2.width, valr2.id.as_str()), (16, "'"));
        assert!(proc.signal("me2_f_global_v_ganho_e_").unwrap().real);
    }

    #[test]
    fn the_processor_comes_from_the_core_scope() {
        let found = detect_processors(&scopes());
        assert_eq!(
            found,
            [Detected {
                instance: "soma_tb.proc".into(),
                processor: "soma".into(),
                core: "soma_tb.proc.p_soma.core".into(),
            }]
        );
    }

    #[test]
    fn without_the_core_the_processor_comes_from_the_names() {
        let header = "$scope module top $end $scope module fir_inst $end $scope module u0 $end
$var reg 8 ! valr2 $end $var reg 20 \" linetabs $end
$upscope $end $upscope $end $upscope $end
$scope module soma_tb $end $scope module proc $end
$var reg 8 # valr2 $end $var reg 20 $ linetabs $end
$upscope $end $upscope $end $enddefinitions $end";
        let found = detect_processors(&parse_scopes(header));
        let names: Vec<(&str, &str)> = found
            .iter()
            .map(|p| (p.instance.as_str(), p.processor.as_str()))
            .collect();
        assert_eq!(
            names,
            [("top.fir_inst.u0", "fir"), ("soma_tb.proc", "soma")]
        );
        assert_eq!(found[0].core, "top.fir_inst.u0");
    }

    #[test]
    fn tables_become_surfer_mappings() {
        let table =
            "-1 INTERNAL\n-2 void main();\n-3 END\n1 #PRNAME soma\n2 \n3     int x;  \nlixo\n";
        assert_eq!(
            convert_table("lace_src_soma", 20, table),
            "Name = lace_src_soma\nBits = 20\n0xFFFFF INTERNAL\n0xFFFFE void main();\n0xFFFFD END\n1 #PRNAME soma\n3     int x;\n"
        );
        // Sem largura, os negativos não têm como virar bits.
        assert_eq!(
            convert_table("t", 0, "-1 INTERNAL\n0 NOP\n"),
            "Name = t\n0 NOP\n"
        );
    }

    #[test]
    fn complex_numbers_decode_like_comp2gtkw() {
        assert_eq!(decode_complex(COMPLEX).as_deref(), Some("6.000 -0.500i"));
        assert_eq!(decode_complex("0101"), None);
    }

    /// Uma onda de processador com as tabelas do YANC na mesma pasta, como a
    /// `.lace/Temp/<proc>/` de um projeto.
    fn wave_with_tables(dir: &Utf8Path) -> Utf8PathBuf {
        let wave = dir.join("soma_tb.vcd");
        let body = format!("#0\nb{COMPLEX} +\nb0 '\n#10\nb1 '\nbxx +\n");
        std::fs::write(&wave, format!("{HEADER}{body}")).unwrap();
        std::fs::write(dir.join("pc_soma_mem.txt"), "11111111111111111111\n").unwrap();
        std::fs::write(dir.join("trad_opcode.txt"), "0 NOP \n1 LOD 5\n").unwrap();
        std::fs::write(dir.join("trad_cmm.txt"), "-1 INTERNAL\n1 int soma;\n").unwrap();
        // A onda depois do build, como numa simulação de verdade: o relógio
        // do sistema de arquivos pode dar a mesma hora às quatro gravações.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
        std::fs::File::options()
            .write(true)
            .open(&wave)
            .unwrap()
            .set_modified(later)
            .unwrap();
        wave
    }

    #[test]
    fn a_processor_wave_gets_the_whole_layout() {
        let guard = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(guard.path()).unwrap();
        let wave = wave_with_tables(dir);
        let layout = wave_layout(&wave).unwrap().unwrap();

        assert_eq!(
            layout.processors,
            [WaveProcessor {
                instance: "soma_tb.proc".into(),
                processor: "soma".into(),
                variables: 5,
                assembly: true,
                source: true,
                outdated: false,
            }]
        );
        let names: Vec<&str> = layout.mappings.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["lace_asm_soma", "lace_src_soma", "lace_complex"]);
        assert_eq!(
            layout.mappings[0].content,
            "Name = lace_asm_soma\nBits = 16\n0 NOP\n1 LOD 5\n"
        );
        assert_eq!(
            layout.mappings[2].content,
            format!("Name = lace_complex\n0b{COMPLEX} 6.000 -0.500i\n")
        );

        let state = &layout.state;
        for expected in [
            "name: \"proc (soma)\"",
            "manual_name: Some(\"Assembly (soma)\")",
            "format: Some(\"lace_asm_soma\")",
            "manual_name: Some(\"C± (soma)\")",
            "manual_name: Some(\"int soma in main()\")",
            "manual_name: Some(\"float ganho in global\")",
            "format: Some(\"lace_complex\")",
            "name: \"int x in main()\"",
            "manual_name: Some(\"Data Stack Pointer\")",
            "annotation_groups: []",
            "last_active_viewport_idx: 0",
        ] {
            assert!(state.contains(expected), "{expected} is missing:\n{state}");
        }
        // O elemento 0 do array vem antes do 1, mesmo fora de ordem no VCD.
        let first = state.find("arr_me1_f_main_v_x_e_0000").unwrap();
        let second = state.find("arr_me1_f_main_v_x_e_0001").unwrap();
        assert!(first < second);
        // O `items_tree` e o `displayed_items` têm um item para cada ref.
        let refs = state.matches("item_ref:").count();
        assert_eq!(
            state.matches("Variable((").count() + state.matches("Group((").count(),
            refs
        );
        assert!(state.contains(&format!("display_item_ref_counter: {}", refs + 1)));
    }

    #[test]
    fn without_tables_the_numbers_stay() {
        let guard = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(guard.path()).unwrap();
        let wave = wave_with_tables(dir);
        std::fs::remove_file(dir.join("pc_soma_mem.txt")).unwrap();
        let layout = wave_layout(&wave).unwrap().unwrap();
        assert!(!layout.processors[0].assembly && !layout.processors[0].source);
        assert!(layout.state.contains("format: Some(\"Unsigned\")"));
        assert_eq!(layout.mappings.len(), 1, "only the complex translator");
    }

    #[test]
    fn a_verilog_wave_gets_the_testbench_signals() {
        let guard = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(guard.path()).unwrap();
        let plain = dir.join("a_tb.vcd");
        std::fs::write(&plain, "$scope module a_tb $end $var reg 1 ! clk $end $var reg 8 \" y $end $scope module dut $end $var wire 1 # q $end $upscope $end $upscope $end $enddefinitions $end\n#0\n").unwrap();
        let layout = wave_layout(&plain).unwrap().unwrap();
        assert!(layout.processors.is_empty());
        assert!(layout.state.contains("name: \"Top-level\""));
        assert!(layout.state.contains("name: \"clk\""));
        assert!(layout.state.contains("name: \"y\""));
        assert!(
            !layout.state.contains("name: \"q\""),
            "o DUT fica na hierarquia"
        );
        // Sem sinal nenhum, e o GHW, ficam sem layout.
        let empty = dir.join("b_tb.vcd");
        std::fs::write(
            &empty,
            "$scope module b_tb $end $upscope $end $enddefinitions $end\n",
        )
        .unwrap();
        assert_eq!(wave_layout(&empty).unwrap(), None);
        let ghw = dir.join("a_tb.ghw");
        std::fs::write(&ghw, b"GHDLwave\n\x00\x01\x02\xff\xfe").unwrap();
        assert_eq!(wave_layout(&ghw).unwrap(), None);
    }

    #[test]
    fn layout_details_from_the_real_waves() {
        // Escopo escapado com ponto, array com mais de 9999 elementos,
        // `delta_*` real e função com `_v_` no nome.
        let header = "$scope module esc_tb $end $scope module u.pa $end
$var reg 8 ! valr2 $end $var reg 20 \" linetabs $end
$var reg 16 # arr_me1_f_main_v_x_e_9999 $end $var reg 16 $ arr_me1_f_main_v_x_e_10000 $end
$var reg 16 % me1_f_get_v_x_v_loc_e_e_ $end
$scope module p_pa $end $scope module core $end $scope module ula $end
$var real 1 & delta_int $end
$upscope $end $upscope $end $upscope $end
$upscope $end $upscope $end $enddefinitions $end";
        let scopes = parse_scopes(header);
        let processors = detect_processors(&scopes);
        assert_eq!(processors[0].processor, "pa");
        let mut tables = HashMap::new();
        tables.insert(
            "pa".to_owned(),
            Tables {
                variables: vec![("get_v_x".into(), "loc_e".into())],
                ..Tables::default()
            },
        );
        let layout = build_layout(
            Utf8Path::new("/r/esc_tb.vcd"),
            &scopes,
            &processors,
            &tables,
            &["pa".to_owned()],
            None,
        );
        let state = &layout.state;
        assert!(state.contains("\"u.pa\""), "{state}");
        assert_eq!(layout.processors[0].instance, "esc_tb.u.pa");
        assert!(state.contains("manual_name: Some(\"x 9999\")"));
        assert!(state.contains("manual_name: Some(\"x 10000\")"));
        assert_eq!(state.matches("int x in main()").count(), 1, "um grupo só");
        assert!(state.contains("int loc_e in get_v_x()"));
        assert!(state.contains("manual_name: Some(\"C (pa)\")"));
        let delta = state.find("name: \"delta_int\"").unwrap();
        assert!(
            !state[delta..]
                .lines()
                .take(14)
                .any(|l| l.contains("Hexadecimal"))
        );
    }

    #[test]
    fn two_waves_with_the_same_name_get_two_folders() {
        let a = layout_dir_name(Utf8Path::new("/p/.lace/Temp/pa/pa_tb.vcd"));
        let b = layout_dir_name(Utf8Path::new("/p/x/pa_tb.vcd"));
        assert!(a.starts_with("pa_tb-") && b.starts_with("pa_tb-"));
        assert_ne!(a, b);
        assert_eq!(
            a,
            layout_dir_name(Utf8Path::new("/p/.lace/Temp/pa/pa_tb.vcd"))
        );
    }

    #[test]
    fn io_labels_use_the_port_number() {
        let header = "$scope module io_tb $end $scope module proc $end
$var reg 1 ! req_in_sim_2 $end $var reg 16 \" in_sim_2 $end
$var reg 1 # out_en_sim_1 $end $var reg 16 $ out_sig_1 $end
$var reg 1 % out_en_sim_3 $end $var reg 16 & out_sig_3 $end
$var reg 8 ' valr2 $end $var reg 20 ( linetabs $end
$upscope $end $upscope $end $enddefinitions $end";
        let scopes = parse_scopes(header);
        let items = io_items(scope(&scopes, "io_tb.proc").unwrap());
        let labels: Vec<String> = items
            .iter()
            .filter_map(|i| match i {
                Item::Variable(v) => v.label.clone(),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            [
                "req_in 2", "input 2", "out_en 1", "output 1", "out_en 3", "output 3"
            ]
        );
    }

    #[test]
    fn tables_newer_than_the_wave_are_not_used() {
        let guard = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(guard.path()).unwrap();
        let wave = wave_with_tables(dir);
        // Um build depois da simulação.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(dir.join("trad_opcode.txt"))
            .unwrap()
            .set_modified(later)
            .unwrap();
        let layout = wave_layout(&wave).unwrap().unwrap();
        let soma = &layout.processors[0];
        assert!(soma.outdated && !soma.assembly && !soma.source);
    }

    #[test]
    fn the_prepared_layout_goes_to_the_project_temp_folder() {
        let guard = tempfile::tempdir().unwrap();
        let root = crate::paths::canonicalize(Utf8Path::from_path(guard.path()).unwrap()).unwrap();
        let project = Project::create(&root, "p").unwrap();
        let dir = project.temp_dir().join("soma");
        std::fs::create_dir_all(&dir).unwrap();
        let wave = wave_with_tables(&dir);
        // Um tradutor velho de outra simulação sai.
        let old = project
            .temp_dir()
            .join("surfer")
            .join(layout_dir_name(&wave))
            .join(MAPPINGS_DIR)
            .join("lace_asm_velho");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, "Name = lace_asm_velho\n").unwrap();

        let prepared = prepare_wave_layout(&wave).unwrap().unwrap();
        let name = layout_dir_name(&wave);
        assert!(name.starts_with("soma_tb-"), "{name}");
        assert_eq!(prepared.dir, project.temp_dir().join("surfer").join(&name));
        assert_eq!(prepared.state, prepared.dir.join("soma_tb.surf.ron"));
        assert_eq!(
            std::fs::read_to_string(&prepared.state).unwrap(),
            prepared.layout.state
        );
        let mut written: Vec<String> = std::fs::read_dir(prepared.dir.join(MAPPINGS_DIR))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        written.sort();
        assert_eq!(written, ["lace_asm_soma", "lace_complex", "lace_src_soma"]);
        let options = crate::ViewerOptions::with_layout(&prepared);
        assert_eq!(options.working_dir.as_deref(), Some(prepared.dir.as_path()));
    }
}
