//! Leitura do cabeçalho do programa-fonte.
//!
//! O nome dos artefatos de hardware NÃO vem do `-n` passado ao compilador: o
//! `asmcomp` usa o `#PRNAME` que o `appcomp` registrou em `app_log.txt`.
//! Conferido no YANC 5.6:
//!
//! - `#PRNAME outro` num processador `foo` gera `Hardware/outro.v` ao lado de
//!   `Temp/pc_foo_mem.txt`, e o Verilog lê o arquivo de memória pelo nome
//!   errado;
//! - sem `#PRNAME`, os artefatos saem com nome de lixo (`Hardware/5.v`).
//!
//! Por isso o Solar lê o nome declarado no fonte antes de compilar.

use crate::Language;

/// O nome declarado no fonte: `#PRNAME <nome>` em C±, `#pragma yanc prname
/// <nome>` em C. Se houver mais de uma declaração, vale a última, como nos
/// compiladores. Comentários `//` e `/* */` são ignorados.
pub(crate) fn declared_name(text: &str, language: Language) -> Option<String> {
    let code = strip_comments(text);
    let mut found = None;
    for line in code.lines() {
        let mut words = line.split_whitespace();
        let name = match language {
            Language::Cmm => match words.next() {
                Some("#PRNAME") => words.next(),
                _ => None,
            },
            Language::Cpp => match (words.next(), words.next(), words.next()) {
                (Some("#pragma"), Some("yanc"), Some("prname")) => words.next(),
                _ => None,
            },
        };
        if let Some(name) = name {
            found = Some(name.to_owned());
        }
    }
    found
}

/// Troca comentários `//` e `/* */` por espaço, preservando as quebras de
/// linha e o conteúdo de strings (`"..."`, com `\"` escapado), para que um
/// `"//"` dentro de um `$display` não seja tomado por comentário. Serve para
/// C±, C e Verilog, que têm a mesma sintaxe de comentário e de string.
pub(crate) fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' | '\n' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut prev = '\0';
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                    }
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmm_prname() {
        let src = "// #PRNAME comentado\n#PRNAME soma\n#NUBITS 32\n";
        assert_eq!(declared_name(src, Language::Cmm).as_deref(), Some("soma"));
    }

    #[test]
    fn cmm_prname_inside_block_comment_is_ignored() {
        let src = "/* #PRNAME velho\n */\nvoid main() {}\n";
        assert_eq!(declared_name(src, Language::Cmm), None);
    }

    #[test]
    fn slashes_inside_strings_are_not_comments() {
        let code = strip_comments("$display(\"// nao\"); // sim\n$dumpfile(\"a.vcd\");");
        assert!(code.contains("\"// nao\""));
        assert!(!code.contains("sim"));
        assert!(code.contains("$dumpfile"));
    }

    #[test]
    fn last_declaration_wins() {
        let src = "#PRNAME a\n#PRNAME b\n";
        assert_eq!(declared_name(src, Language::Cmm).as_deref(), Some("b"));
    }

    #[test]
    fn cpp_pragma() {
        let src = "#pragma yanc nubits 32\n#pragma yanc prname filtro  \nint main() {}\n";
        assert_eq!(declared_name(src, Language::Cpp).as_deref(), Some("filtro"));
        assert_eq!(declared_name(src, Language::Cmm), None);
    }
}
