//! A cópia da solução que o testbench instancia como referência. Cada
//! módulo que a solução declara ganha `_ref` no nome, nas declarações e nas
//! instâncias, para não colidir com o do aluno; os módulos dados ao aluno
//! (que a solução só instancia) ficam como estão, e os dois lados usam os
//! mesmos.

use crate::testbench::GENERATED;

/// O texto de `<módulo>_ref.v`: o cabeçalho, o `timescale` do testbench e a
/// solução com os módulos renomeados.
pub fn reference_text(solution: &str) -> String {
    let declared = lace_core::verilog::modules_in(solution);
    format!(
        "// A referência: a solução, com `_ref` no nome de cada módulo.\n{GENERATED}\n`timescale 1ns / 1ps\n`default_nettype wire\n\n{}",
        rename(solution, &declared)
    )
}

/// `text` com cada identificador de `names` trocado por `<nome>_ref`, fora
/// de comentário, de texto entre aspas e de nome de macro.
fn rename(text: &str, names: &[String]) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + 16 * names.len());
    let mut i = 0;
    let mut copied = 0;
    // A última palavra antes da atual, sem nada além de espaço e comentário
    // entre elas: diz se a atual vem depois de `module`.
    let mut previous: Option<&str> = None;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            b'"' => {
                previous = None;
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b'\n' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
            }
            // `define, `include, `nome_de_macro: o nome não é identificador
            // do Verilog, nem o da macro que `define e afins declaram.
            b'`' => {
                previous = None;
                i += 1;
                let start = i;
                while i < bytes.len() && is_ident(bytes[i]) {
                    i += 1;
                }
                if matches!(
                    &text[start..i],
                    "define" | "undef" | "ifdef" | "ifndef" | "elsif"
                ) {
                    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                        i += 1;
                    }
                    while i < bytes.len() && is_ident(bytes[i]) {
                        i += 1;
                    }
                }
            }
            b if is_ident_start(b) => {
                let start = i;
                while i < bytes.len() && is_ident(bytes[i]) {
                    i += 1;
                }
                let word = &text[start..i];
                // Só a declaração (`module x`) e as instâncias (`x u1 (...)`):
                // uma porta ou um fio com o nome do módulo fica como está.
                let declared = matches!(previous, Some("module" | "macromodule"));
                if names.iter().any(|n| n == word) && (declared || is_instance(bytes, i)) {
                    out.push_str(&text[copied..i]);
                    out.push_str("_ref");
                    copied = i;
                }
                previous = Some(word);
                continue;
            }
            b => {
                if !b.is_ascii_whitespace() {
                    previous = None;
                }
                i += 1;
            }
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// Depois do nome de um módulo em `i`, o resto de uma instância: os
/// parâmetros opcionais (`#(...)`), o nome da instância, um vetor de
/// instâncias opcional (`[3:0]`) e o `(` das ligações.
fn is_instance(bytes: &[u8], i: usize) -> bool {
    let mut i = skip_space(bytes, i);
    if bytes.get(i) == Some(&b'#') {
        i = skip_space(bytes, i + 1);
        if bytes.get(i) != Some(&b'(') {
            return false;
        }
        let mut depth = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        i = skip_space(bytes, i);
    }
    if !bytes.get(i).is_some_and(|b| is_ident_start(*b)) {
        return false;
    }
    while i < bytes.len() && is_ident(bytes[i]) {
        i += 1;
    }
    i = skip_space(bytes, i);
    if bytes.get(i) == Some(&b'[') {
        while i < bytes.len() && bytes[i] != b']' {
            i += 1;
        }
        i = skip_space(bytes, i + 1);
    }
    bytes.get(i) == Some(&b'(')
}

/// Pula espaços e comentários a partir de `i`.
fn skip_space(bytes: &[u8], mut i: usize) -> usize {
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'/') && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes.get(i) == Some(&b'/') && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else {
            return i;
        }
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_modules_and_their_instances_are_renamed() {
        let solution = "// soma4: quatro somadores (soma4 aqui no comentário fica)\nmodule soma4(input [3:0] a, output [3:0] s);\n  meio m0(.a(a[0]), .s(s[0]));\n  soma4_x x();\nendmodule\n\nmodule meio(input a, output s);\n  assign s = a; // meio\n  initial $display(\"meio\");\nendmodule\n";
        let text = rename(solution, &["soma4".into(), "meio".into()]);
        assert_eq!(
            text,
            "// soma4: quatro somadores (soma4 aqui no comentário fica)\nmodule soma4_ref(input [3:0] a, output [3:0] s);\n  meio_ref m0(.a(a[0]), .s(s[0]));\n  soma4_x x();\nendmodule\n\nmodule meio_ref(input a, output s);\n  assign s = a; // meio\n  initial $display(\"meio\");\nendmodule\n"
        );
    }

    #[test]
    fn a_port_or_wire_with_the_module_name_keeps_it() {
        // A porta `gray` do módulo `gray` e o fio `top` do módulo `top`
        // ficam; as instâncias, com parâmetros, vetor ou comentário, mudam.
        let solution = "module gray(input [3:0] bin, output [3:0] gray);\n  assign gray = bin ^ (bin >> 1);\nendmodule\n\nmodule top(input a, output y);\n  wire top;\n  gray #(.N(4)) g [1:0] (.bin(a), .gray(top));\n  gray /* c */ h (.bin(a), .gray());\nendmodule\n";
        let text = rename(solution, &["gray".into(), "top".into()]);
        assert_eq!(
            text,
            "module gray_ref(input [3:0] bin, output [3:0] gray);\n  assign gray = bin ^ (bin >> 1);\nendmodule\n\nmodule top_ref(input a, output y);\n  wire top;\n  gray_ref #(.N(4)) g [1:0] (.bin(a), .gray(top));\n  gray_ref /* c */ h (.bin(a), .gray());\nendmodule\n"
        );
    }

    #[test]
    fn given_modules_and_macros_keep_their_names() {
        let solution = "`define top 1\nmodule top(input a, output s);\n  dado d(.a(a), .s(s));\n  wire w = `top;\nendmodule\n";
        let text = reference_text(solution);
        assert!(text.contains("module top_ref("), "{text}");
        assert!(text.contains("dado d("), "{text}");
        assert!(text.contains("`define top 1"), "{text}");
        assert!(text.contains("wire w = `top;"), "{text}");
        assert!(text.contains("`timescale 1ns / 1ps"), "{text}");
    }
}
