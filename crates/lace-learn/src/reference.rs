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
                if names.iter().any(|n| n == word) {
                    out.push_str(&text[copied..i]);
                    out.push_str("_ref");
                    copied = i;
                }
            }
            _ => i += 1,
        }
    }
    out.push_str(&text[copied..]);
    out
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
