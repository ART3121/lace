//! Tradução da saída das ferramentas em [`Diagnostic`]s.
//!
//! Cada ferramenta tem seu dialeto (todos conferidos executando-as):
//!
//! - `cmmcomp`, `appcomp` e `asmcomp` escrevem frases em inglês (o Lace sempre
//!   passa `-en`) com a linha no meio do texto e sem nome de arquivo:
//!   `Error on line 42: there's no variable 'x'!`,
//!   `Syntax error on line 3. You're a confused soul!`,
//!   `Heads up on line 7: ...`, `Info: ...`. O `asmcomp` cita arquivos de
//!   dados: `Error: line 4 of file 'x.txt' isn't a valid integer!`.
//! - `cppcomp`, `iverilog` e o compilador C++ que o Verilator chama usam o
//!   formato de compilador C: `<arquivo>:<linha>[:<coluna>]: error: ...`. O
//!   Icarus também escreve `<arquivo>:<linha>: syntax error`.
//!   `cppcomp` e `cpppp` emitem `<ferramenta>: <mensagem>`.
//! - Verilator: `%Error: <arquivo>:<linha>:<coluna>: ...`,
//!   `%Warning-<CÓDIGO>: ...`, seguidos de linhas indentadas com o trecho do
//!   fonte, que são anexadas ao diagnóstico anterior.
//! - Yosys: `<arquivo>:<linha>: ERROR: ...`, `Warning: ...`.
//! - Graphviz (`dot`): `Error: <arquivo>: syntax error in line <n> ...`.
//!
//! O parser é tolerante: linha não reconhecida vira um diagnóstico com
//! `severity = Unknown`, e todo diagnóstico guarda o texto original em `raw`.
//! A exceção é o stdout de ferramentas cujo stdout é saída do programa e não
//! mensagem (o `$display` de uma simulação, o log do Yosys): ali só entram as
//! linhas reconhecidas, e o texto completo continua no relatório do passo.

use camino::{Utf8Path, Utf8PathBuf};
use schemars::JsonSchema;
use serde::Serialize;

use crate::control::Stream;
use crate::toolchain::Tool;

/// Gravidade de um [`Diagnostic`]. A ordem (`Error < Warning < Info <
/// Unknown`) serve para ordenar do mais grave ao menos grave.
///
/// Em JSON: `"error"`, `"warning"`, `"info"`, `"unknown"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Severity {
    /// A ferramenta recusou a entrada.
    Error,
    /// A ferramenta aceitou, mas avisou (`Heads up on line`, `%Warning-`).
    Warning,
    /// Mensagem informativa (`Info:`, `Sweet:`, resumos de contagem).
    Info,
    /// Linha que o parser não reconheceu.
    Unknown,
}

/// Uma mensagem de uma ferramenta, com os campos que deu para extrair.
///
/// `file`, `line` e `column` são opcionais porque o `cmmcomp` não informa
/// arquivo nem coluna, e várias mensagens não informam nem a linha. Quando a
/// ferramenta não cita o arquivo mas o Lace sabe qual é (o `cmmcomp` compila
/// um fonte só), `file` vem preenchido pelo Lace.
///
/// Em JSON:
///
/// ```json
/// { "tool": "cmmcomp", "severity": "error",
///   "message": "c'mon dude, declare the variable 'total' properly!",
///   "file": "/p/conta/Software/conta.cmm", "line": 16, "column": null,
///   "raw": "Error on line 16: c'mon dude, declare the variable 'total' properly!" }
/// ```
///
/// No fluxo C, o `cppcomp` numera as linhas do arquivo pré-processado
/// (`<temp>/pp.cpp`), e é esse o `file` que aparece: o `cpppp` não emite
/// marcadores `#line` que permitam voltar ao fonte original.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Diagnostic {
    /// A ferramenta que escreveu a mensagem.
    pub tool: Tool,
    /// A gravidade.
    pub severity: Severity,
    /// A mensagem, sem o prefixo de gravidade e de localização. Vem no idioma
    /// da ferramenta (inglês: o Lace sempre pede `-en` ao YANC).
    pub message: String,
    /// O arquivo a que a mensagem se refere, como a ferramenta o escreveu
    /// (normalmente absoluto, porque o Lace passa caminhos absolutos).
    #[schemars(with = "Option<String>")]
    pub file: Option<Utf8PathBuf>,
    /// Linha, a partir de 1.
    pub line: Option<u32>,
    /// Coluna, a partir de 1 (Verilator e compilador C++).
    pub column: Option<u32>,
    /// O texto exatamente como a ferramenta o escreveu (várias linhas quando
    /// a ferramenta continua a mensagem em linhas indentadas).
    pub raw: String,
}

/// Interpreta a saída de uma execução.
///
/// `source` é o arquivo que o passo compilou; ele vira o `file` das mensagens
/// com número de linha que não citam arquivo (caso do `cmmcomp`, que compila
/// um único arquivo, já que C± não tem `#include`).
pub(crate) fn parse(
    tool: Tool,
    stdout: &str,
    stderr: &str,
    source: Option<&Utf8Path>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    // stderr primeiro: é onde ficam os erros, que o usuário quer ver no topo.
    parse_stream(tool, stderr, source, true, &mut out);
    parse_stream(tool, stdout, source, stdout_is_messages(tool), &mut out);
    out
}

/// Se `line`, escrita por `tool` em `stream`, volta como [`Diagnostic`] no
/// resultado: tudo do stderr, todo o stdout dos compiladores e, no stdout de
/// uma simulação, só as linhas que o simulador escreve por conta própria
/// (`$finish called at`, `VCD info:`, `ERROR:` do `$error`). O resto é saída
/// do programa, como o `$display` do testbench.
///
/// Decide linha a linha: a continuação indentada de uma mensagem do Icarus ou
/// do Verilator conta como saída comum.
pub(crate) fn is_message(tool: Tool, stream: Stream, line: &str) -> bool {
    !line.trim().is_empty()
        && (stream == Stream::Stderr
            || stdout_is_messages(tool)
            || parse_line(tool, line, None).is_some()
            || (tool == Tool::Vvp && is_vvp_continuation(line)))
}

/// `       Time: 1000  Scope: tb`: a segunda linha que o `vvp` escreve
/// depois do `ERROR:` de um `$error` (ou `$fatal`, `$warning`).
fn is_vvp_continuation(line: &str) -> bool {
    line.starts_with(char::is_whitespace)
        && line.trim_start().starts_with("Time:")
        && line.contains("Scope:")
}

/// O stdout desta ferramenta é feito de mensagens (e não de saída do programa)?
fn stdout_is_messages(tool: Tool) -> bool {
    tool.is_yanc() || tool == Tool::Iverilog
}

fn parse_stream(
    tool: Tool,
    text: &str,
    source: Option<&Utf8Path>,
    keep_unknown: bool,
    out: &mut Vec<Diagnostic>,
) {
    let mut continuable = false;
    // O índice do diagnóstico de `*** These modules were missing:`, até o
    // `***` que fecha a lista.
    let mut missing: Option<usize> = None;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if tool == Tool::Iverilog {
            // `*** These modules were missing:`, os módulos, um por linha, e
            // `***`: um resumo só (o erro de cada um já veio antes).
            if line.trim() == "*** These modules were missing:" {
                missing = Some(out.len());
                out.push(Diagnostic {
                    tool,
                    severity: Severity::Info,
                    message: "These modules were missing:".into(),
                    file: None,
                    line: None,
                    column: None,
                    raw: line.to_owned(),
                });
                continue;
            }
            if let Some(at) = missing {
                if line.trim() == "***" {
                    missing = None;
                } else {
                    let summary = &mut out[at];
                    summary.message = format!("{} {}", summary.message, line.trim());
                    summary.raw.push('\n');
                    summary.raw.push_str(line);
                }
                continue;
            }
            // `x.v:5:        : Padding 4 high bits of the port.`: com o local,
            // mas continuação do aviso anterior.
            if let Some((_, rest)) = split_location(line.trim())
                && rest.starts_with(':')
                && let Some(last) = out.last_mut()
                && last.severity != Severity::Unknown
            {
                last.message = format!("{} {}", last.message, rest.trim_start_matches(':').trim());
                last.raw.push('\n');
                last.raw.push_str(line);
                continue;
            }
        }
        // `       Time: 0  Scope: tb` depois do `ERROR:` de um `$error`.
        if tool == Tool::Vvp
            && is_vvp_continuation(line)
            && let Some(last) = out.last_mut()
            && last.tool == Tool::Vvp
            && matches!(last.severity, Severity::Error | Severity::Warning)
        {
            let more = line.split_whitespace().collect::<Vec<_>>().join(" ");
            last.message = format!("{} ({more})", last.message);
            last.raw.push('\n');
            last.raw.push_str(line);
            continue;
        }
        // Continuação do Icarus: um aviso longo continua em linhas que começam
        // com `:` (`: confusing timing results.`, `:   -- module x declared
        // here: x.v:1`).
        if tool == Tool::Iverilog
            && line.trim_start().starts_with(':')
            && let Some(last) = out.last_mut()
            && last.severity != Severity::Unknown
        {
            let more = line.trim_start().trim_start_matches(':').trim();
            last.message = format!("{} {}", last.message, more)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            last.raw.push('\n');
            last.raw.push_str(line);
            if last.file.is_none()
                && let Some((_, place)) = more.rsplit_once("declared here: ")
                && let Some((file, number)) = place.rsplit_once(':')
                && let Ok(number) = number.trim().parse()
            {
                last.file = Some(file.trim().into());
                last.line = Some(number);
            }
            continue;
        }
        // Continuação indentada do Verilator (trecho do fonte, `^~~`, "... See").
        if continuable
            && line.starts_with(' ')
            && let Some(last) = out.last_mut()
        {
            last.raw.push('\n');
            last.raw.push_str(line);
            continue;
        }
        let parsed = parse_line(tool, line, source);
        continuable = tool == Tool::Verilator && parsed.is_some();
        let parsed = match parsed {
            Some(parsed) => parsed,
            None if keep_unknown => Parsed::new(Severity::Unknown, line.trim()),
            None => continue,
        };
        out.push(Diagnostic {
            tool,
            severity: parsed.severity,
            message: parsed.message,
            file: parsed.file,
            line: parsed.line,
            column: parsed.column,
            raw: line.to_owned(),
        });
    }
}

fn parse_line(tool: Tool, raw: &str, source: Option<&Utf8Path>) -> Option<Parsed> {
    match tool {
        Tool::Cmmcomp | Tool::Appcomp | Tool::Asmcomp => parse_yanc_phrase(raw, source),
        Tool::Cpppp | Tool::Cppcomp => {
            parse_c_style(raw).or_else(|| parse_tool_prefixed(tool, raw))
        }
        Tool::Iverilog | Tool::Vvp => parse_c_style(raw)
            .or_else(|| parse_icarus(raw, tool == Tool::Vvp))
            .map(undefined_macro_is_an_error),
        Tool::Verilator => parse_verilator(raw).or_else(|| parse_c_style(raw)),
        Tool::Yosys => parse_yosys(raw),
        Tool::Dot => parse_graphviz(raw),
        _ => None,
    }
}

struct Parsed {
    severity: Severity,
    message: String,
    file: Option<Utf8PathBuf>,
    line: Option<u32>,
    column: Option<u32>,
}

impl Parsed {
    fn new(severity: Severity, message: &str) -> Self {
        Parsed {
            severity,
            message: message.trim().to_owned(),
            file: None,
            line: None,
            column: None,
        }
    }

    fn at(mut self, location: Location<'_>) -> Self {
        self.file = Some(Utf8PathBuf::from(location.file));
        self.line = Some(location.line);
        self.column = location.column;
        self
    }
}

/// Prefixos do dialeto de frases, na ordem em que precisam ser testados
/// (`Error on line` antes de `Error:`). O `bool` diz se o prefixo é seguido de
/// `<n>` e de um separador (`:` ou `.`).
const PHRASES: &[(&str, Severity, bool)] = &[
    ("Syntax error on line ", Severity::Error, true),
    ("Error on line ", Severity::Error, true),
    ("Warning on line ", Severity::Warning, true),
    ("Heads up on line ", Severity::Warning, true),
    ("Error:", Severity::Error, false),
    ("Heads up:", Severity::Warning, false),
    ("Warning:", Severity::Warning, false),
    ("Info:", Severity::Info, false),
    ("Sweet:", Severity::Info, false),
];

fn parse_yanc_phrase(raw: &str, source: Option<&Utf8Path>) -> Option<Parsed> {
    let text = raw.trim();
    // `Error in function f: where's the return of ...` (`messages.h`): a
    // única mensagem de erro do cmmcomp sem linha; é do fonte inteiro.
    if text.starts_with("Error in function ") {
        let mut parsed = Parsed::new(Severity::Error, text);
        parsed.file = source.map(Utf8Path::to_owned);
        return Some(parsed);
    }
    for &(prefix, severity, numbered) in PHRASES {
        let Some(rest) = text.strip_prefix(prefix) else {
            continue;
        };
        if numbered {
            let (line, rest) = split_number(rest)?;
            let message = rest.trim_start_matches([':', '.']).trim();
            // "Syntax error on line 3. You're a confused soul!": a frase inteira
            // é a mensagem, porque o resto sozinho não diz que é de sintaxe.
            let message = if prefix.starts_with("Syntax") {
                format!("Syntax error. {message}")
            } else {
                message.to_owned()
            };
            let mut parsed = Parsed::new(severity, &message);
            parsed.file = source.map(Utf8Path::to_owned);
            parsed.line = Some(line);
            return Some(parsed);
        }
        let mut parsed = Parsed::new(severity, rest);
        if let Some((file, line)) = data_file_location(rest) {
            parsed.file = Some(file);
            parsed.line = Some(line);
        }
        return Some(parsed);
    }
    None
}

/// `line <n> of file '<arquivo>'` dentro de uma mensagem do `asmcomp`.
fn data_file_location(message: &str) -> Option<(Utf8PathBuf, u32)> {
    let after = &message[message.find("line ")? + "line ".len()..];
    let (line, rest) = split_number(after)?;
    let rest = rest.strip_prefix(" of file '")?;
    let file = &rest[..rest.find('\'')?];
    Some((Utf8PathBuf::from(file), line))
}

/// `"42: resto"` vira `(42, ": resto")`.
fn split_number(s: &str) -> Option<(u32, &str)> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let n = s[..end].parse().ok()?;
    Some((n, &s[end..]))
}

struct Location<'a> {
    file: &'a str,
    line: u32,
    column: Option<u32>,
}

/// Separa `<arquivo>:<linha>[:<coluna>]: <resto>`. Procura o primeiro `:`
/// seguido de dígitos, para não confundir o `C:` de um caminho Windows.
fn split_location(text: &str) -> Option<(Location<'_>, &str)> {
    for (i, _) in text.match_indices(':') {
        if i == 0 {
            continue;
        }
        let Some((line, rest)) = split_number(&text[i + 1..]) else {
            continue;
        };
        let (column, rest) = match rest.strip_prefix(':').and_then(split_number) {
            Some((col, rest)) => (Some(col), rest),
            None => (None, rest),
        };
        let Some(rest) = rest.strip_prefix(':') else {
            continue;
        };
        let location = Location {
            file: &text[..i],
            line,
            column,
        };
        return Some((location, rest.trim_start()));
    }
    None
}

/// `<arquivo>:<linha>[:<coluna>]: error|warning|note|sorry: <mensagem>`
fn parse_c_style(raw: &str) -> Option<Parsed> {
    let (location, rest) = split_location(raw.trim())?;
    let (severity, message) = [
        ("error:", Severity::Error),
        ("fatal error:", Severity::Error),
        ("sorry:", Severity::Error),
        ("warning:", Severity::Warning),
        ("note:", Severity::Info),
    ]
    .into_iter()
    .find_map(|(tag, sev)| rest.strip_prefix(tag).map(|m| (sev, m)))?;
    Some(Parsed::new(severity, message).at(location))
}

/// `cppcomp: internal error: ...`, `cpppp: cannot open 'x'`,
/// `cppcomp: warning: could not create ...`
fn parse_tool_prefixed(tool: Tool, raw: &str) -> Option<Parsed> {
    let rest = raw
        .trim()
        .strip_prefix(tool.binary_name())?
        .strip_prefix(": ")?;
    Some(match rest.strip_prefix("warning: ") {
        Some(message) => Parsed::new(Severity::Warning, message),
        None => Parsed::new(Severity::Error, rest),
    })
}

/// `macro NBITS undefined (and assumed null) at this point.`: o Icarus só
/// avisa e segue com a macro vazia, e o design compila com outra largura
/// (a síntese depois falha). Quase sempre é a ordem dos arquivos: um
/// `` `define `` só vale para os arquivos depois dele na lista do projeto.
/// Vira erro, com a explicação.
fn undefined_macro_is_an_error(mut parsed: Parsed) -> Parsed {
    if parsed.message.contains("undefined (and assumed null)") {
        parsed.severity = Severity::Error;
        parsed.message.push_str(
            " A `define only reaches the files after it in the project list: move its file up, or define it before use.",
        );
    }
    parsed
}

/// O que sobra do Icarus fora do formato C. `simulation` é o `vvp`: lá, um
/// `ERROR:` sem local é o testbench escrevendo (`$display("ERROR: ...")`),
/// e fica como saída dele; o `$error` do Verilog sempre traz o local.
fn parse_icarus(raw: &str, simulation: bool) -> Option<Parsed> {
    let text = raw.trim();
    // `ERROR: tb.v:12: ...` e `FATAL: tb.v:3: ...` do vvp ($error, $fatal):
    // o prefixo vem antes do local, e não faz parte do nome do arquivo.
    for (prefix, severity) in [
        ("ERROR: ", Severity::Error),
        ("FATAL: ", Severity::Error),
        ("WARNING: ", Severity::Warning),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            // `$readmemb's file name argument contains non-printable
            // characters`: o vvp não abre o arquivo (acento no caminho) e
            // segue sem os dados. É erro, não aviso.
            let unreadable = rest.contains("contains non-printable characters");
            let severity = if unreadable {
                Severity::Error
            } else {
                severity
            };
            return match split_location(rest) {
                Some((location, message)) => Some(Parsed::new(severity, message).at(location)),
                None if simulation && !unreadable => None,
                None => Some(Parsed::new(severity, rest)),
            };
        }
    }
    if let Some((location, rest)) = split_location(text) {
        // `bad.v:2: syntax error`, `x.v:3: Include file y.v not found`,
        // `tb.v:61: $finish called at 985000 (1ps)`
        let lower = rest.to_ascii_lowercase();
        let severity = if lower.contains("error")
            || lower.contains("not found")
            || lower.contains("contains non-printable characters")
        {
            Severity::Error
        } else if lower.contains("warning") {
            Severity::Warning
        } else {
            Severity::Info
        };
        return Some(Parsed::new(severity, rest).at(location));
    }
    if let Some(file) = text.strip_suffix(": No such file or directory") {
        let mut parsed = Parsed::new(Severity::Error, "File not found");
        parsed.file = Some(file.into());
        return Some(parsed);
    }
    // `2 error(s) during elaboration.`: resumo, a contagem já está nos erros.
    if text.contains(" error(s) during ") {
        return Some(Parsed::new(Severity::Info, text));
    }
    // `warning: Some modules have no timescale. ...`, sem local.
    if let Some(rest) = text.strip_prefix("warning: ") {
        return Some(Parsed::new(Severity::Warning, rest));
    }
    if text == "No top level modules, and no -s option." {
        return Some(Parsed::new(Severity::Error, text));
    }
    if text.starts_with("VCD info:") || text.starts_with("FST info:") {
        return Some(Parsed::new(Severity::Info, text));
    }
    None
}

/// `%Error: a.v:2:17: msg`, `%Warning-WIDTH: a.v:3:4: msg`, `%Error: msg`.
fn parse_verilator(raw: &str) -> Option<Parsed> {
    let text = raw.trim_end().strip_prefix('%')?;
    let (tag, rest) = text.split_once(": ")?;
    let severity = if tag.starts_with("Error") {
        Severity::Error
    } else if tag.starts_with("Warning") {
        Severity::Warning
    } else {
        return None;
    };
    // `%Error: Exiting due to 1 error(s)`: resumo.
    if rest.starts_with("Exiting due to") {
        return Some(Parsed::new(Severity::Info, rest));
    }
    let code = tag.split_once('-').map(|(_, code)| code);
    let with_code = |m: &str| match code {
        Some(code) => format!("{m} [{code}]"),
        None => m.to_owned(),
    };
    Some(match split_location(rest) {
        Some((location, message)) => Parsed::new(severity, &with_code(message)).at(location),
        None => Parsed::new(severity, &with_code(rest)),
    })
}

/// `bad.v:2: ERROR: msg`, `x.v:4: Warning: msg`, `ERROR: msg`, `Warning: msg`,
/// com ou sem espaço depois dos dois-pontos (`rom.v:7: ERROR:Can not open`).
///
/// A gravidade no começo da linha vem antes do local: o aviso de latch
/// (`Warning: Latch inferred ... $auto$proc_dlatch.cc:542:...`) traz um
/// `arquivo:linha:` do próprio Yosys no meio da mensagem.
fn parse_yosys(raw: &str) -> Option<Parsed> {
    let text = raw.trim();
    if let Some((severity, message)) = yosys_tag(text) {
        return Some(Parsed::new(severity, message));
    }
    let (location, rest) = split_location(text)?;
    let (severity, message) = yosys_tag(rest)?;
    Some(Parsed::new(severity, message).at(location))
}

/// `ERROR:` ou `Warning:` no começo, e o resto sem espaço na frente.
fn yosys_tag(text: &str) -> Option<(Severity, &str)> {
    [("ERROR:", Severity::Error), ("Warning:", Severity::Warning)]
        .into_iter()
        .find_map(|(tag, sev)| text.strip_prefix(tag).map(|m| (sev, m.trim_start())))
}

/// Graphviz: `Error: x.dot: syntax error in line 3 near 'y'`, `Warning: ...`.
fn parse_graphviz(raw: &str) -> Option<Parsed> {
    let text = raw.trim();
    let (severity, rest) = [
        ("Error: ", Severity::Error),
        ("Warning: ", Severity::Warning),
    ]
    .into_iter()
    .find_map(|(tag, sev)| text.strip_prefix(tag).map(|m| (sev, m)))?;
    let mut parsed = Parsed::new(severity, rest);
    // `<arquivo>: syntax error in line <n> ...`
    if let Some((file, message)) = rest.split_once(": ")
        && let Some(after) = message
            .find(" in line ")
            .map(|i| &message[i + " in line ".len()..])
        && let Some((line, _)) = split_number(after)
    {
        parsed.file = Some(file.into());
        parsed.line = Some(line);
    }
    Some(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(tool: Tool, line: &str) -> Diagnostic {
        let mut d = parse(tool, "", line, Some(Utf8Path::new("/p/Software/prog.cmm")));
        assert_eq!(d.len(), 1);
        d.remove(0)
    }

    #[test]
    fn cmmcomp_error_with_line() {
        let d = one(
            Tool::Cmmcomp,
            "Error on line 19: c'mon dude, declare the variable 'zz' properly!",
        );
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.line, Some(19));
        assert_eq!(
            d.file.as_deref(),
            Some(Utf8Path::new("/p/Software/prog.cmm"))
        );
        assert_eq!(d.message, "c'mon dude, declare the variable 'zz' properly!");
    }

    #[test]
    fn cmmcomp_syntax_error() {
        let d = one(
            Tool::Cmmcomp,
            "Syntax error on line 3. You're a confused soul!",
        );
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.line, Some(3));
        assert_eq!(d.message, "Syntax error. You're a confused soul!");
    }

    #[test]
    fn cmmcomp_warning() {
        let d = one(
            Tool::Cmmcomp,
            "Heads up on line 7: if you don't want this warning, use 'out'.",
        );
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.line, Some(7));
    }

    #[test]
    fn error_without_line_has_no_file() {
        let d = one(Tool::Cmmcomp, "Error: yo, where's the main() function?");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.line, None);
        assert_eq!(d.file, None);
    }

    #[test]
    fn asmcomp_data_file_error() {
        let d = one(
            Tool::Asmcomp,
            "Error: line 4 of file 'dados.txt' isn't a valid integer!",
        );
        assert_eq!(d.file.as_deref(), Some(Utf8Path::new("dados.txt")));
        assert_eq!(d.line, Some(4));
    }

    #[test]
    fn cppcomp_error_windows_path() {
        let d = one(
            Tool::Cppcomp,
            r"C:\proj\Temp\pp.cpp:12: error: unknown type 'foo'",
        );
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(
            d.file.as_deref(),
            Some(Utf8Path::new(r"C:\proj\Temp\pp.cpp"))
        );
        assert_eq!(d.line, Some(12));
        assert_eq!(d.message, "unknown type 'foo'");
    }

    #[test]
    fn cppcomp_tool_prefixed() {
        let d = one(Tool::Cppcomp, "cppcomp: internal error: bad node");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.message, "internal error: bad node");

        let d = one(
            Tool::Cppcomp,
            "cppcomp: warning: could not create /x (errno=13)",
        );
        assert_eq!(d.severity, Severity::Warning);
    }

    #[test]
    fn unknown_lines_are_kept() {
        let d = one(Tool::Asmcomp, "Bad format!");
        assert_eq!(d.severity, Severity::Unknown);
        assert_eq!(d.raw, "Bad format!");
    }

    #[test]
    fn stderr_comes_first_and_blank_lines_are_skipped() {
        let d = parse(
            Tool::Cmmcomp,
            "Info: 16 assembly instructions generated\n\n",
            "Error: x\n",
            None,
        );
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[1].severity, Severity::Info);
    }

    #[test]
    fn iverilog_formats() {
        let d = parse(
            Tool::Iverilog,
            "",
            "bad.v:2: syntax error\nbad.v:2: error: Syntax error in continuous assignment\n2 error(s) during elaboration.\nnope.v: No such file or directory\n",
            None,
        );
        assert_eq!(d.len(), 4);
        assert_eq!((d[0].severity, d[0].line), (Severity::Error, Some(2)));
        assert_eq!(d[1].message, "Syntax error in continuous assignment");
        assert_eq!(d[2].severity, Severity::Info);
        assert_eq!(d[3].file.as_deref(), Some(Utf8Path::new("nope.v")));
    }

    #[test]
    fn verilator_with_continuation_lines() {
        let stderr = "%Error: bad.v:2:17: syntax error, unexpected ';'\n    2 |   assign b = a &;\n      |                 ^\n%Warning-WIDTH: C:\\p\\w.v:3:14: Operator ASSIGNW expects 4 bits\n%Error: Exiting due to 1 error(s)\n";
        let d = parse(Tool::Verilator, "", stderr, None);
        assert_eq!(d.len(), 3);
        assert_eq!((d[0].line, d[0].column), (Some(2), Some(17)));
        assert!(d[0].raw.contains("assign b = a &;"));
        assert_eq!(d[1].severity, Severity::Warning);
        assert_eq!(d[1].file.as_deref(), Some(Utf8Path::new("C:\\p\\w.v")));
        assert!(d[1].message.ends_with("[WIDTH]"));
        assert_eq!(d[2].severity, Severity::Info);
    }

    #[test]
    fn yosys_formats() {
        let d = parse(
            Tool::Yosys,
            "",
            "bad.v:2: ERROR: syntax error, unexpected ';'\nERROR: Module `nada' not found!\n",
            None,
        );
        assert_eq!((d[0].severity, d[0].line), (Severity::Error, Some(2)));
        assert_eq!(d[1].message, "Module `nada' not found!");
        assert_eq!(d[1].file, None);
    }

    #[test]
    fn simulation_stdout_is_not_diagnostics() {
        let d = parse(
            Tool::Vvp,
            "resultado = 55\nVCD info: dumpfile x.vcd opened for output.\n",
            "",
            None,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Info);
    }

    #[test]
    fn graphviz_errors() {
        let d = parse(
            Tool::Dot,
            "",
            "Error: /t/soma.dot: syntax error in line 3 near 'x'\nWarning: node a, port b unrecognized\n",
            None,
        );
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[0].file.as_deref(), Some(Utf8Path::new("/t/soma.dot")));
        assert_eq!(d[0].line, Some(3));
        assert_eq!(d[1].severity, Severity::Warning);
    }

    #[test]
    fn icarus_warning_continues_on_colon_lines() {
        // Como o iverilog 14 imprime: continuação com espaços e `:`.
        let stderr = "warning: Some modules have no timescale. This may cause\n       : confusing timing results.\tAffected modules are:\n       :   -- module and_gate declared here: /p/and_gate.v:1\n";
        let found = parse(Tool::Iverilog, "", stderr, None);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(
            found[0].message,
            "Some modules have no timescale. This may cause confusing timing results. Affected modules are: -- module and_gate declared here: /p/and_gate.v:1"
        );
        assert_eq!(
            found[0].file.as_deref(),
            Some(Utf8Path::new("/p/and_gate.v"))
        );
        assert_eq!(found[0].line, Some(1));
    }

    #[test]
    fn yosys_error_without_space_and_latch_warning() {
        let d = one(
            Tool::Yosys,
            "rom.v:7: ERROR:Can not open file `dados.mif` for reading.",
        );
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.file.as_deref(), Some(Utf8Path::new("rom.v")));
        assert_eq!(d.line, Some(7));
        assert!(d.message.starts_with("Can not open"));
        let d = one(
            Tool::Yosys,
            "Warning: Latch inferred for signal `\\top.\\q' from process `\\top.$proc$x.v:5$1': $auto$proc_dlatch.cc:542:proc_dlatch$7",
        );
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.file, None);
        assert!(d.message.starts_with("Latch inferred"));
    }

    #[test]
    fn cmmcomp_function_without_return() {
        let d = one(
            Tool::Cmmcomp,
            "Error in function f: where's the return? The function is declared int.",
        );
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(
            d.file.as_deref(),
            Some(Utf8Path::new("/p/Software/prog.cmm"))
        );
        assert!(d.message.starts_with("Error in function f"));
    }

    #[test]
    fn undefined_macro_is_an_error() {
        let d = one(
            Tool::Iverilog,
            "/p/alu.v:3: warning: macro NBITS undefined (and assumed null) at this point.",
        );
        assert_eq!(d.severity, Severity::Error);
        assert!(d.message.contains("project list"));
    }

    #[test]
    fn icarus_continuations_and_missing_modules_are_merged() {
        let stderr = "/p/top.v:5: warning: Port 1 (a) of m expects 8 bits, got 4.\n\
                      /p/top.v:5:        : Padding 4 high bits of the port.\n\
                      /p/top.v:2: error: Unknown module type: falta\n\
                      *** These modules were missing:\n\
                      \x20       falta referenced 1 times.\n\
                      ***\n";
        let found = parse(Tool::Iverilog, "", stderr, None);
        assert_eq!(found.len(), 3, "{found:#?}");
        assert!(
            found[0]
                .message
                .ends_with("Padding 4 high bits of the port.")
        );
        assert_eq!(found[1].severity, Severity::Error);
        assert_eq!(found[2].severity, Severity::Info);
        assert_eq!(
            found[2].message,
            "These modules were missing: falta referenced 1 times."
        );
    }

    #[test]
    fn what_the_testbench_prints_stays_its_output() {
        let stdout = "ERROR: valor errado na saida\n\
                      ERROR: /p/tb.v:12: boom\n\
                      \x20      Time: 1000  Scope: tb\n";
        let found = parse(Tool::Vvp, stdout, "", None);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].line, Some(12));
        assert_eq!(found[0].message, "boom (Time: 1000 Scope: tb)");
        assert!(!is_message(
            Tool::Vvp,
            Stream::Stdout,
            "ERROR: valor errado na saida"
        ));
        assert!(is_message(
            Tool::Vvp,
            Stream::Stdout,
            "       Time: 1000  Scope: tb"
        ));
    }
}
