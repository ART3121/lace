//! Verilog: classificar arquivos, achar os módulos de um arquivo, ler as
//! portas e gerar os arquivos-modelo.
//!
//! - [`classify`]: módulo sintetizável ou testbench, pelo conteúdo, com a
//!   regra da AURORA (`js/project/verilog_classifier.ts`);
//! - [`modules_in`]: os nomes dos módulos declarados num texto;
//! - [`read_interfaces`]: as portas de cada módulo, pelo Yosys
//!   (`read_verilog -sv -lib` + `write_json`), ou por um leitor simples
//!   quando o Yosys não está instalado;
//! - [`module_template`] e [`testbench_template`]: os arquivos que
//!   [`Project::add_verilog`](crate::Project::add_verilog) cria.
//!
//! ```
//! use lace_core::FileRole;
//! use lace_core::verilog::{classify, modules_in};
//!
//! let tb = "module and_tb; initial begin $dumpvars(0, and_tb); #10 $finish; end endmodule";
//! assert_eq!(classify(tb, "and_tb.v"), FileRole::Testbench);
//! assert_eq!(modules_in("module a(input x); endmodule // module b"), ["a"]);
//! ```

use std::collections::HashMap;

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;
use serde_json::Value;

use crate::error::{LaceError, Result};
use crate::files::FileRole;
use crate::process;
use crate::toolchain::{Tool, Toolchain};

/// Direção de uma porta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PortDirection {
    /// `input`.
    Input,
    /// `output`.
    Output,
    /// `inout`.
    Inout,
}

/// Uma porta de um módulo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Port {
    /// Nome.
    pub name: String,
    /// Direção.
    pub direction: PortDirection,
    /// Largura em bits, com os parâmetros nos valores padrão.
    pub width: u32,
    /// `signed`.
    pub signed: bool,
}

/// Um módulo e as portas dele, na ordem da declaração.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct ModuleInterface {
    /// Nome do módulo.
    pub name: String,
    /// O arquivo que o declara.
    pub file: Utf8PathBuf,
    /// As portas.
    pub ports: Vec<Port>,
}

// ------------------------------------------------------------ classificar

/// Pontuação a partir da qual o arquivo é testbench, como na AURORA.
const TESTBENCH_THRESHOLD: u32 = 3;

/// Sintetizável ou testbench, pelo conteúdo, como a AURORA: +3 para
/// `$dumpfile`/`$dumpvars`, +3 para `$finish`/`$stop`, +3 para um módulo sem
/// portas (`module tb;`), +2 para `initial`, +1 para `$display`/`$monitor` e
/// afins, +1 para um atraso `#<n>` e +2 para `tb`, `test` ou `testbench` no
/// nome do arquivo. Com 3 ou mais, testbench; na dúvida, sintetizável.
pub fn classify(text: &str, file_name: &str) -> FileRole {
    if text.trim().is_empty() {
        return FileRole::Synthesizable;
    }
    let code = strip_comments_and_strings(text);
    let mut score = 0;
    let tasks = system_tasks(&code);
    let has = |names: &[&str]| tasks.iter().any(|t| names.contains(&t.as_str()));
    if has(&[
        "dumpfile",
        "dumpvars",
        "dumpon",
        "dumpoff",
        "dumpall",
        "dumplimit",
        "dumpflush",
    ]) {
        score += 3;
    }
    if has(&["finish", "stop"]) {
        score += 3;
    }
    if has_portless_module(&code) {
        score += 3;
    }
    if has_word(&code, "initial") {
        score += 2;
    }
    if has(&[
        "display", "write", "monitor", "strobe", "time", "realtime", "random", "sformat",
        "sformatf",
    ]) {
        score += 1;
    }
    if has_delay(&code) {
        score += 1;
    }
    if name_suggests_testbench(file_name) {
        score += 2;
    }
    if score >= TESTBENCH_THRESHOLD {
        FileRole::Testbench
    } else {
        FileRole::Synthesizable
    }
}

/// O nome é de testbench pela convenção que o topo respeita: `tb_<nome>.v`,
/// `<nome>_tb.v` ou `tb.v` (qualquer extensão, sem distinguir maiúsculas).
/// Um arquivo assim não vira o topo do projeto
/// ([`Project::set_top_level`](crate::Project::set_top_level)); qualquer
/// outro Verilog pode, inclusive o que o build de um processador gerou.
///
/// É mais estreita que a dica de nome da classificação (`test` e
/// `testbench` também contam lá): `test_alu.v` pode ser topo.
///
/// ```
/// use lace_core::verilog::is_testbench_name;
///
/// assert!(is_testbench_name("alu_tb.v"));
/// assert!(is_testbench_name("TB_alu.sv"));
/// assert!(is_testbench_name("tb.v"));
/// assert!(!is_testbench_name("alu.v"));
/// assert!(!is_testbench_name("tbuf.v"));
/// assert!(!is_testbench_name("test_alu.v"));
/// ```
pub fn is_testbench_name(file_name: &str) -> bool {
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem)
        .to_ascii_lowercase();
    stem == "tb" || stem.starts_with("tb_") || stem.ends_with("_tb")
}

/// O nome do arquivo tem `tb`, `test` ou `testbench` como palavra
/// (`and_tb.v`, `tb_and.v`, `test_alu.v`), a dica de nome da AURORA.
pub(crate) fn name_suggests_testbench(file_name: &str) -> bool {
    let name = file_name.to_ascii_lowercase();
    let bytes = name.as_bytes();
    for word in ["testbench", "test", "tb"] {
        let mut from = 0;
        while let Some(pos) = name[from..].find(word) {
            let start = from + pos;
            let end = start + word.len();
            let before = start == 0 || !bytes[start - 1].is_ascii_lowercase();
            let after = end == bytes.len() || !bytes[end].is_ascii_lowercase();
            if before && after {
                return true;
            }
            from = start + 1;
        }
    }
    false
}

/// Tira comentários e troca strings por um espaço, para palavras-chave dentro
/// deles não contarem (como a AURORA).
fn strip_comments_and_strings(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                // As linhas continuam contando.
                if chars[i] == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i += 2;
        } else if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            out.push(' ');
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Os nomes depois de cada `$` (`$dumpvars` dá `dumpvars`).
fn system_tasks(code: &str) -> Vec<String> {
    let mut tasks = Vec::new();
    let mut chars = code.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '$' {
            continue;
        }
        let mut name = String::new();
        while let Some(&(_, n)) = chars.peek() {
            if is_word_char(n) {
                name.push(n);
                chars.next();
            } else {
                break;
            }
        }
        if !name.is_empty() {
            tasks.push(name);
        }
    }
    tasks
}

/// `word` como palavra inteira (`\bword\b`).
fn has_word(code: &str, word: &str) -> bool {
    word_positions(code, word).next().is_some()
}

fn word_positions<'a>(code: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    code.match_indices(word).filter_map(move |(start, _)| {
        let before = code[..start].chars().next_back();
        let after = code[start + word.len()..].chars().next();
        let free = |c: Option<char>| c.is_none_or(|c| !is_word_char(c) && c != '$');
        (free(before) && free(after)).then_some(start)
    })
}

/// Um atraso procedural (`#10`, `# 5`), e não o `#(` de parâmetro.
fn has_delay(code: &str) -> bool {
    code.match_indices('#').any(|(i, _)| {
        code[i + 1..]
            .trim_start()
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    })
}

/// Pula espaços a partir de `i`.
fn skip_ws(code: &[char], mut i: usize) -> usize {
    while i < code.len() && code[i].is_whitespace() {
        i += 1;
    }
    i
}

/// Dado o índice de um `(`, o índice logo depois do `)` que fecha.
fn skip_parens(code: &[char], mut i: usize) -> usize {
    let mut depth = 0;
    while i < code.len() {
        match code[i] {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    i
}

fn identifier_at(code: &[char], i: usize) -> Option<(String, usize)> {
    let first = *code.get(i)?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '\\') {
        return None;
    }
    let mut end = i + 1;
    if first == '\\' {
        // Identificador escapado: até o próximo espaço.
        while end < code.len() && !code[end].is_whitespace() {
            end += 1;
        }
    } else {
        while end < code.len() && (is_word_char(code[end]) || code[end] == '$') {
            end += 1;
        }
    }
    Some((code[i..end].iter().collect(), end))
}

/// As declarações `module` do código (já sem comentários): o nome e o índice,
/// em `chars`, logo depois dele.
fn module_declarations(chars: &[char]) -> Vec<(String, usize)> {
    const KEYWORD: [char; 6] = ['m', 'o', 'd', 'u', 'l', 'e'];
    let mut found = Vec::new();
    let mut i = 0;
    while i + KEYWORD.len() <= chars.len() {
        let word_start = i == 0 || !(is_word_char(chars[i - 1]) || chars[i - 1] == '$');
        let after = chars.get(i + KEYWORD.len()).copied();
        let word_end = after.is_none_or(|c| !(is_word_char(c) || c == '$'));
        if word_start && word_end && chars[i..i + KEYWORD.len()] == KEYWORD {
            let at = skip_ws(chars, i + KEYWORD.len());
            if let Some((name, end)) = identifier_at(chars, at) {
                found.push((name, end));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    found
}

fn has_portless_module(code: &str) -> bool {
    let chars: Vec<char> = code.chars().collect();
    module_declarations(&chars).into_iter().any(|(_, end)| {
        let mut i = skip_ws(&chars, end);
        if chars.get(i) == Some(&'#') {
            i = skip_ws(&chars, i + 1);
            if chars.get(i) == Some(&'(') {
                i = skip_ws(&chars, skip_parens(&chars, i));
            }
        }
        match chars.get(i) {
            Some(';') => true,
            Some('(') => chars.get(skip_ws(&chars, i + 1)) == Some(&')'),
            _ => false,
        }
    })
}

/// As palavras do código (já sem comentários): cada sequência de letras,
/// dígitos, `_` e `$`, menos as tarefas de sistema (`$display`): a mesma
/// noção de palavra inteira de `has_word`.
fn words(code: &str) -> impl Iterator<Item = &str> {
    code.split(|c: char| !(is_word_char(c) || c == '$'))
        .filter(|w| !w.is_empty() && !w.starts_with('$'))
}

/// As raízes de um design: os módulos de `texts` que nenhum outro
/// instancia, isto é, cujo nome aparece (fora de comentários e strings) só
/// na própria declaração.
///
/// Quando nenhum sobra (um módulo que se instancia num `generate`, com
/// recursão parametrizada), as raízes vêm com `explicit`: o `top` pedido,
/// se o design o declara, senão os módulos que só o próprio arquivo cita.
/// Quem elabora passa cada uma com `-s`, porque sem ele o Icarus também não
/// acha raiz ("No top level modules").
///
/// Um passe por texto, com a contagem de palavras num mapa: um projeto de
/// 1500 arquivos leva milissegundos (antes, cada módulo era procurado em
/// cada texto).
pub(crate) fn design_roots(texts: &[String], top: Option<&str>) -> DesignRoots {
    let mut total: HashMap<&str, usize> = HashMap::new();
    let mut per_text: Vec<HashMap<&str, usize>> = Vec::with_capacity(texts.len());
    let codes: Vec<String> = texts
        .iter()
        .map(|t| strip_comments_and_strings(t))
        .collect();
    let mut modules: Vec<(String, usize)> = Vec::new();
    for (i, code) in codes.iter().enumerate() {
        let chars: Vec<char> = code.chars().collect();
        modules.extend(module_declarations(&chars).into_iter().map(|(n, _)| (n, i)));
        let mut here: HashMap<&str, usize> = HashMap::new();
        for word in words(code) {
            *here.entry(word).or_default() += 1;
            *total.entry(word).or_default() += 1;
        }
        per_text.push(here);
    }
    let count = |m: &str| total.get(m).copied().unwrap_or(0);
    let implicit: Vec<String> = modules
        .iter()
        .filter(|(m, _)| count(m) <= 1)
        .map(|(m, _)| m.clone())
        .collect();
    if !implicit.is_empty() || modules.is_empty() {
        return DesignRoots {
            names: implicit,
            explicit: false,
        };
    }
    let names = match top.filter(|t| modules.iter().any(|(m, _)| m == t)) {
        Some(top) => vec![top.to_owned()],
        None => modules
            .iter()
            .filter(|(m, i)| per_text[*i].get(m.as_str()).copied().unwrap_or(0) == count(m))
            .map(|(m, _)| m.clone())
            .collect(),
    };
    DesignRoots {
        names,
        explicit: true,
    }
}

/// Os módulos de `text` que se instanciam: o nome aparece, fora de
/// comentários e strings, mais de uma vez no próprio arquivo (a declaração
/// e a instância). Não separa a recursão com condição de parada da sem.
pub(crate) fn self_instantiating(text: &str) -> Vec<String> {
    let code = strip_comments_and_strings(text);
    let chars: Vec<char> = code.chars().collect();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for word in words(&code) {
        *counts.entry(word).or_default() += 1;
    }
    module_declarations(&chars)
        .into_iter()
        .map(|(name, _)| name)
        .filter(|name| counts.get(name.as_str()).copied().unwrap_or(0) > 1)
        .collect()
}

/// Os módulos de `text` que se instanciam sem nenhum `if`, `case` ou `for`
/// no corpo: sem condição, a recursão não tem como parar.
pub(crate) fn unconditional_recursion(text: &str) -> Vec<String> {
    let code = strip_comments_and_strings(text);
    let chars: Vec<char> = code.chars().collect();
    module_declarations(&chars)
        .into_iter()
        .filter_map(|(name, end)| {
            let rest: String = chars[end..].iter().collect();
            let end = word_positions(&rest, "endmodule")
                .next()
                .unwrap_or(rest.len());
            let mut used = false;
            for word in words(&rest[..end]) {
                if matches!(word, "if" | "case" | "casex" | "casez" | "for") {
                    return None;
                }
                used |= word == name;
            }
            used.then_some(name)
        })
        .collect()
}

/// O resultado de [`design_roots`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesignRoots {
    /// Os módulos elaborados como raiz.
    pub names: Vec<String>,
    /// As raízes precisam ir com `-s`: o Icarus não as acharia sozinho.
    pub explicit: bool,
}

/// Os nomes dos módulos declarados em `text`, na ordem, sem os que estão em
/// comentário ou em string.
pub fn modules_in(text: &str) -> Vec<String> {
    let chars: Vec<char> = strip_comments_and_strings(text).chars().collect();
    module_declarations(&chars)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

// ---------------------------------------------------------------- modelos

/// O módulo-modelo de um arquivo novo: uma entrada e uma saída de 8 bits que
/// compilam e simulam, para o arquivo já nascer como sintetizável.
pub fn module_template(name: &str) -> String {
    format!(
        "`timescale 1ns / 1ps\n\
         \n\
         module {name} (\n\
         \x20   input  wire [7:0] a,\n\
         \x20   output wire [7:0] y\n\
         );\n\
         \n\
         \x20   assign y = a;\n\
         \n\
         endmodule\n"
    )
}

fn is_clock(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "clk" | "clock" | "clk_i" | "i_clk" | "clk_in" | "sys_clk"
    )
}

/// `Some(true)` para reset ativo em alto, `Some(false)` em baixo.
fn reset_polarity(name: &str) -> Option<bool> {
    match name.to_ascii_lowercase().as_str() {
        "rst" | "reset" | "rst_i" | "i_rst" | "areset" => Some(true),
        "rst_n" | "rstn" | "reset_n" | "resetn" | "nrst" | "arst_n" | "rst_ni" => Some(false),
        _ => None,
    }
}

fn range(port: &Port) -> String {
    if port.width > 1 {
        format!("[{}:0]", port.width - 1)
    } else {
        String::new()
    }
}

/// O testbench-modelo `testbench`. Com `dut`, instancia o módulo com todas as
/// portas: `reg` para as entradas, `wire` para as saídas e bidirecionais,
/// clock de 10 ns se houver uma porta `clk`/`clock`, reset nos primeiros 20
/// ns se houver `rst`/`reset` (ou `rst_n`, ativo em baixo). Grava a onda
/// (`$dumpfile("<testbench>.vcd")`, `$dumpvars(0, <testbench>)`, todos os
/// sinais) e termina com `$finish`.
pub fn testbench_template(testbench: &str, dut: Option<&ModuleInterface>) -> String {
    let mut out = String::from("`timescale 1ns / 1ps\n\n");
    let Some(dut) = dut else {
        out.push_str(&format!(
            "module {testbench};\n\
             \n\
             \x20   initial begin\n\
             \x20       $dumpfile(\"{testbench}.vcd\");\n\
             \x20       $dumpvars(0, {testbench});\n\
             \n\
             \x20       #100;\n\
             \x20       $finish;\n\
             \x20   end\n\
             \n\
             endmodule\n"
        ));
        return out;
    };

    out.push_str(&format!("module {testbench};\n\n"));
    let decls: Vec<(String, &Port)> = dut
        .ports
        .iter()
        .map(|p| {
            let kind = if p.direction == PortDirection::Input {
                "reg "
            } else {
                "wire"
            };
            let signed = if p.signed { " signed" } else { "" };
            (format!("{kind}{signed}"), p)
        })
        .collect();
    let kind_width = decls.iter().map(|(k, _)| k.len()).max().unwrap_or(4);
    let range_width = dut.ports.iter().map(|p| range(p).len()).max().unwrap_or(0);
    for (kind, port) in &decls {
        let r = range(port);
        if range_width > 0 {
            out.push_str(&format!(
                "    {kind:<kind_width$} {r:<range_width$} {};\n",
                port.name
            ));
        } else {
            out.push_str(&format!("    {kind:<kind_width$} {};\n", port.name));
        }
    }
    if !dut.ports.is_empty() {
        out.push('\n');
    }

    let name_width = dut.ports.iter().map(|p| p.name.len()).max().unwrap_or(0);
    if dut.ports.is_empty() {
        out.push_str(&format!("    {} dut ();\n\n", dut.name));
    } else {
        out.push_str(&format!("    {} dut (\n", dut.name));
        for (i, port) in dut.ports.iter().enumerate() {
            let comma = if i + 1 < dut.ports.len() { "," } else { "" };
            out.push_str(&format!(
                "        .{:<name_width$}({}){comma}\n",
                port.name, port.name
            ));
        }
        out.push_str("    );\n\n");
    }

    let inputs: Vec<&Port> = dut
        .ports
        .iter()
        .filter(|p| p.direction == PortDirection::Input)
        .collect();
    let clock = inputs.iter().find(|p| is_clock(&p.name));
    if let Some(clk) = clock {
        out.push_str(&format!(
            "    // Clock de 10 ns.\n    always #5 {} = ~{};\n\n",
            clk.name, clk.name
        ));
    }
    let reset = inputs
        .iter()
        .find_map(|p| reset_polarity(&p.name).map(|high| (p, high)));

    out.push_str("    initial begin\n");
    out.push_str(&format!(
        "        $dumpfile(\"{testbench}.vcd\");\n        $dumpvars(0, {testbench});\n\n"
    ));
    for port in &inputs {
        // O reset começa ativo; o resto, em zero.
        let value = match reset {
            Some((r, true)) if r.name == port.name => "1",
            _ => "0",
        };
        out.push_str(&format!("        {} = {value};\n", port.name));
    }
    if let Some((r, high)) = reset {
        let release = if high { "0" } else { "1" };
        out.push_str(&format!("        #20 {} = {release};\n", r.name));
    }
    out.push_str("\n        #100;\n        $finish;\n    end\n\nendmodule\n");
    out
}

// ------------------------------------------------------------------ portas

/// As portas dos módulos declarados em `files` (sintetizáveis: o Yosys não
/// lê testbench). Com o Yosys do bundle, `read_verilog -sv -lib` +
/// `write_json`, que entende portas ANSI e não ANSI e calcula as larguras com
/// os parâmetros padrão. Sem ele (`toolchain` `None` ou componente não
/// instalado), ou se ele falhar (erro de sintaxe), um leitor simples de
/// declarações de porta.
///
/// # Erros
///
/// Só os de leitura dos arquivos e de execução do Yosys (que não começou).
pub fn read_interfaces(
    toolchain: Option<&Toolchain>,
    files: &[Utf8PathBuf],
) -> Result<Vec<ModuleInterface>> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(toolchain) = toolchain
        && toolchain.tool(Tool::Yosys).is_ok()
        && let Some(found) = interfaces_by_yosys(toolchain, files)?
    {
        return Ok(found);
    }
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(file).map_err(LaceError::io("Reading", file))?;
        found.extend(interfaces_by_text(&text, file));
    }
    Ok(found)
}

/// `None` se o Yosys rodou e falhou (o leitor de texto tenta então).
fn interfaces_by_yosys(
    toolchain: &Toolchain,
    files: &[Utf8PathBuf],
) -> Result<Option<Vec<ModuleInterface>>> {
    let dir = std::env::temp_dir().join(format!(
        "lace-portas-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let dir = Utf8PathBuf::from_path_buf(dir)
        .map_err(|p| LaceError::NonUtf8Path(p.display().to_string()))?;
    std::fs::create_dir_all(&dir).map_err(LaceError::io("Creating directory", &dir))?;
    let json = dir.join("ports.json");
    let mut script = String::from("read_verilog -sv -lib");
    for file in files {
        script.push(' ');
        script.push_str(&crate::synth::yosys_quoted(file)?);
    }
    script.push_str(&format!(
        "; write_json {}",
        crate::synth::yosys_quoted(&json)?
    ));
    let invocation = toolchain
        .invocation(Tool::Yosys, &dir)?
        .arg("-q")
        .arg("-p")
        .arg(&script);
    let output = process::run(&invocation, &process::Watch::default());
    let parsed = match output {
        Ok(out) if out.termination.success() => std::fs::read_to_string(&json)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .map(|value| interfaces_from_json(&value)),
        Ok(_) => None,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    let _ = std::fs::remove_dir_all(&dir);
    Ok(parsed)
}

fn interfaces_from_json(value: &Value) -> Vec<ModuleInterface> {
    let Some(modules) = value["modules"].as_object() else {
        return Vec::new();
    };
    modules
        .iter()
        .map(|(name, module)| {
            let file = module["attributes"]["src"]
                .as_str()
                .and_then(|src| src.rsplit_once(':').map(|(f, _)| f))
                .unwrap_or_default();
            let ports = module["ports"]
                .as_object()
                .map(|ports| {
                    ports
                        .iter()
                        .filter_map(|(port, info)| {
                            let direction = match info["direction"].as_str()? {
                                "input" => PortDirection::Input,
                                "output" => PortDirection::Output,
                                _ => PortDirection::Inout,
                            };
                            Some(Port {
                                name: port.clone(),
                                direction,
                                width: info["bits"].as_array().map_or(1, |b| b.len() as u32),
                                signed: info["signed"].as_i64() == Some(1),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            ModuleInterface {
                name: name.trim_start_matches('\\').to_owned(),
                file: Utf8PathBuf::from(file),
                ports,
            }
        })
        .collect()
}

/// Leitor simples de portas, para quando não há Yosys: a lista de portas do
/// cabeçalho (ANSI) ou as declarações `input`/`output`/`inout` do corpo (não
/// ANSI). Faixas com números (`[7:0]`) dão a largura; com parâmetros, vale 1.
fn interfaces_by_text(text: &str, file: &Utf8Path) -> Vec<ModuleInterface> {
    let chars: Vec<char> = strip_comments_and_strings(text).chars().collect();
    let mut found = Vec::new();
    for (name, end) in module_declarations(&chars) {
        let mut i = skip_ws(&chars, end);
        if chars.get(i) == Some(&'#') {
            i = skip_ws(&chars, i + 1);
            if chars.get(i) == Some(&'(') {
                i = skip_ws(&chars, skip_parens(&chars, i));
            }
        }
        let mut header = String::new();
        if chars.get(i) == Some(&'(') {
            let close = skip_parens(&chars, i);
            header = chars[i + 1..close.saturating_sub(1)].iter().collect();
            i = close;
        }
        let rest: String = chars[i..].iter().collect();
        let body_end = word_positions(&rest, "endmodule")
            .next()
            .unwrap_or(rest.len());
        let body = &rest[..body_end];

        let mut ports = parse_declarations(&header, ',');
        if ports.is_empty() || ports.iter().all(|(dir, _)| dir.is_none()) {
            // Não ANSI: os nomes vêm do cabeçalho, as direções do corpo.
            let order: Vec<String> = header
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
            let declared: Vec<(Option<PortDirection>, Port)> = parse_declarations(body, ';')
                .into_iter()
                .filter(|(dir, _)| dir.is_some())
                .collect();
            ports = order
                .iter()
                .filter_map(|n| {
                    declared
                        .iter()
                        .find(|(_, p)| &p.name == n)
                        .map(|(d, p)| (*d, p.clone()))
                })
                .collect();
        }
        found.push(ModuleInterface {
            name,
            file: file.to_owned(),
            ports: ports
                .into_iter()
                .filter(|(d, _)| d.is_some())
                .map(|(_, p)| p)
                .collect(),
        });
    }
    found
}

/// Declarações separadas por `separator`: `input wire [7:0] a, b` dá duas
/// portas (a direção e a faixa valem para as seguintes).
fn parse_declarations(text: &str, separator: char) -> Vec<(Option<PortDirection>, Port)> {
    let mut out = Vec::new();
    // No cabeçalho ANSI a direção e a faixa passam de uma porta para a
    // seguinte (`input [3:0] a, b`): é uma declaração só.
    let statements: Vec<&str> = if separator == ',' {
        vec![text]
    } else {
        text.split(separator).collect()
    };
    for statement in statements {
        let mut direction = None;
        let mut width = 1;
        let mut signed = false;
        let mut seen_direction = false;
        for item in statement.split(',') {
            let mut words: Vec<String> = Vec::new();
            let mut item_width = None;
            let mut rest = item.trim();
            while !rest.is_empty() {
                if let Some(stripped) = rest.strip_prefix('[') {
                    let close = stripped.find(']').unwrap_or(stripped.len());
                    item_width = Some(range_width(&stripped[..close]));
                    rest = stripped.get(close + 1..).unwrap_or("").trim_start();
                    continue;
                }
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == '[')
                    .unwrap_or(rest.len());
                words.push(rest[..end].to_owned());
                rest = rest[end..].trim_start();
            }
            let mut name = None;
            for word in &words {
                match word.as_str() {
                    "input" => {
                        direction = Some(PortDirection::Input);
                        seen_direction = true;
                        width = 1;
                        signed = false;
                    }
                    "output" => {
                        direction = Some(PortDirection::Output);
                        seen_direction = true;
                        width = 1;
                        signed = false;
                    }
                    "inout" => {
                        direction = Some(PortDirection::Inout);
                        seen_direction = true;
                        width = 1;
                        signed = false;
                    }
                    "signed" => signed = true,
                    "wire" | "reg" | "logic" | "tri" | "unsigned" | "var" => {}
                    w if w.contains('=') => break,
                    w => name = Some(w.trim_end_matches(';').to_owned()),
                }
            }
            if let Some(w) = item_width {
                width = w;
            }
            if let Some(name) = name.filter(|n| {
                n.chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            }) {
                out.push((
                    if seen_direction { direction } else { None },
                    Port {
                        name,
                        direction: direction.unwrap_or(PortDirection::Input),
                        width,
                        signed,
                    },
                ));
            }
        }
    }
    out
}

/// A largura de `msb:lsb` com números; 1 se não der para calcular.
fn range_width(range: &str) -> u32 {
    let Some((msb, lsb)) = range.split_once(':') else {
        return 1;
    };
    match (msb.trim().parse::<i64>(), lsb.trim().parse::<i64>()) {
        (Ok(m), Ok(l)) => (m - l).unsigned_abs() as u32 + 1,
        _ => {
            // `N-1:0`, a forma mais comum com número.
            let msb = msb.trim();
            match msb
                .strip_suffix("-1")
                .and_then(|n| n.trim().parse::<u32>().ok())
            {
                Some(n) if lsb.trim() == "0" => n,
                _ => 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_follows_the_aurora_scores() {
        let module =
            "module and_gate(input a, input b, output c);\n  assign c = a & b;\nendmodule\n";
        assert_eq!(classify(module, "and_gate.v"), FileRole::Synthesizable);
        // Só o nome não basta (+2).
        assert_eq!(classify(module, "and_gate_tb.v"), FileRole::Synthesizable);
        // initial de ROM (+2) não basta.
        let rom = "module rom(input [3:0] a, output [7:0] d);\n reg [7:0] m[0:15];\n initial $readmemh(\"rom.hex\", m);\n assign d = m[a];\nendmodule";
        assert_eq!(classify(rom, "rom.v"), FileRole::Synthesizable);
        // Sem portas (+3).
        assert_eq!(classify("module tb; endmodule", "x.v"), FileRole::Testbench);
        assert_eq!(
            classify("module tb(); endmodule", "x.v"),
            FileRole::Testbench
        );
        assert_eq!(
            classify("module tb #(parameter N = 1) (); endmodule", "x.v"),
            FileRole::Testbench
        );
        // $finish e $dumpvars dentro de comentário ou string não contam.
        let commented = "module m(input a, output [55:0] s);\n // $finish\n assign s = \"$dumpvars\";\nendmodule";
        assert_eq!(classify(commented, "m.v"), FileRole::Synthesizable);
        // initial (+2) e atraso (+1).
        assert_eq!(
            classify("module m(input a); initial #5; endmodule", "m.v"),
            FileRole::Testbench
        );
        // #( de parâmetro não é atraso.
        assert_eq!(
            classify("module m(input a); sub #(4) u(); initial; endmodule", "m.v"),
            FileRole::Synthesizable
        );
        assert_eq!(classify("", "tb.v"), FileRole::Synthesizable);
    }

    #[test]
    fn testbench_names() {
        for name in [
            "and_tb.v",
            "tb_and.v",
            "TB.v",
            "test_alu.v",
            "alu_testbench.sv",
        ] {
            assert!(name_suggests_testbench(name), "{name}");
        }
        for name in ["latest.v", "stbl.v", "contest.v", "and.v"] {
            assert!(!name_suggests_testbench(name), "{name}");
        }
    }

    #[test]
    fn modules_are_found_outside_comments() {
        let text = "/* module fake(); */\nmodule a #(parameter N = 2) (input x);\nendmodule\n// module b\nmodule c; endmodule\n";
        assert_eq!(modules_in(text), ["a", "c"]);
        assert!(modules_in("endmodule").is_empty());
    }

    #[test]
    fn templates_are_classified_as_intended() {
        let module = module_template("alu");
        assert_eq!(modules_in(&module), ["alu"]);
        assert_eq!(classify(&module, "alu.v"), FileRole::Synthesizable);
        let tb = testbench_template("alu_tb", None);
        assert_eq!(modules_in(&tb), ["alu_tb"]);
        assert_eq!(classify(&tb, "alu_tb.v"), FileRole::Testbench);
    }

    #[test]
    fn testbench_template_instantiates_every_port() {
        let port = |name: &str, direction, width| Port {
            name: name.into(),
            direction,
            width,
            signed: false,
        };
        let dut = ModuleInterface {
            name: "counter".into(),
            file: "counter.v".into(),
            ports: vec![
                port("clk", PortDirection::Input, 1),
                port("rst_n", PortDirection::Input, 1),
                port("en", PortDirection::Input, 1),
                port("q", PortDirection::Output, 8),
            ],
        };
        let tb = testbench_template("counter_tb", Some(&dut));
        for line in [
            "reg        clk;",
            "wire [7:0] q;",
            "counter dut (",
            ".clk  (clk),",
            ".q    (q)",
            "always #5 clk = ~clk;",
            "rst_n = 0;",
            "#20 rst_n = 1;",
            "en = 0;",
            "$dumpvars(0, counter_tb);",
            "$finish;",
        ] {
            assert!(tb.contains(line), "falta {line:?} em:\n{tb}");
        }
        assert_eq!(classify(&tb, "counter_tb.v"), FileRole::Testbench);
    }

    #[test]
    fn ports_without_yosys() {
        let ansi = "module m #(parameter W = 8) (\n  input wire clk,\n  input [3:0] a, b,\n  output reg [W-1:0] y,\n  inout io\n);\nendmodule";
        let found = interfaces_by_text(ansi, "m.v".into());
        let ports: Vec<(&str, PortDirection, u32)> = found[0]
            .ports
            .iter()
            .map(|p| (p.name.as_str(), p.direction, p.width))
            .collect();
        assert_eq!(
            ports,
            [
                ("clk", PortDirection::Input, 1),
                ("a", PortDirection::Input, 4),
                ("b", PortDirection::Input, 4),
                ("y", PortDirection::Output, 1),
                ("io", PortDirection::Inout, 1),
            ]
        );
        let old = "module n(a, y);\n input [7:0] a;\n output y;\n assign y = ^a;\nendmodule";
        let found = interfaces_by_text(old, "n.v".into());
        let ports: Vec<(&str, PortDirection, u32)> = found[0]
            .ports
            .iter()
            .map(|p| (p.name.as_str(), p.direction, p.width))
            .collect();
        assert_eq!(
            ports,
            [
                ("a", PortDirection::Input, 8),
                ("y", PortDirection::Output, 1)
            ]
        );
    }

    #[test]
    fn design_roots_in_one_pass() {
        let texts = vec![
            "module top(input a); m1 u(.a(a)); m2 v(); endmodule\n".to_owned(),
            "module m1(input a); endmodule // m2\nmodule m2; endmodule\n".to_owned(),
            "module solto; endmodule\n".to_owned(),
        ];
        let roots = design_roots(&texts, None);
        assert_eq!(roots.names, ["top", "solto"]);
        assert!(!roots.explicit);
    }

    #[test]
    fn parameterized_recursion_gets_explicit_roots() {
        let rec = "module p #(parameter N = 4) (input a);\n\
                   generate if (N > 1) begin : g p #(N - 1) sub (.a(a)); end endgenerate\n\
                   endmodule\n";
        let roots = design_roots(&[rec.to_owned()], None);
        assert_eq!(roots.names, ["p"]);
        assert!(roots.explicit);
        let with_top = design_roots(&[rec.to_owned()], Some("p"));
        assert_eq!(with_top.names, ["p"]);
        // Sem módulo nenhum, nada a fazer.
        assert!(
            design_roots(&["`define X 1\n".to_owned()], None)
                .names
                .is_empty()
        );
    }

    #[test]
    fn recursion_without_a_condition_never_ends() {
        let endless = "module r #(parameter N = 1) (input a, output y);\n  r #(.N(N + 1)) sub (.a(a), .y(y));\nendmodule\n";
        assert_eq!(unconditional_recursion(endless), ["r"]);
        let guarded = "module p #(N = 4) (input a);\n  generate if (N > 1) begin : g p #(N - 1) s (.a(a)); end endgenerate\nendmodule\n";
        assert!(unconditional_recursion(guarded).is_empty());
        assert!(unconditional_recursion("module a; b u(); endmodule\n").is_empty());
    }
}
