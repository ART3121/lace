//! Leitura do cabeçalho do programa-fonte: as diretivas de C± (`#NUBITS 23`)
//! e os pragmas de C (`#pragma yanc nubits 23`), sem os comentários.

use crate::Language;

/// A largura da palavra, `#NUBITS` (`#pragma yanc nubits` em C), entre 2 e
/// 63; sem a diretiva, 32, o padrão dos compiladores (23 + 8 + 1).
pub(crate) fn declared_nubits(text: &str, language: Language) -> u32 {
    declared(text, language, "NUBITS")
        .and_then(|n| n.parse().ok())
        .filter(|n| (2..=63).contains(n))
        .unwrap_or(32)
}

/// O valor de uma diretiva: `#<NOME> valor` em C±, `#pragma yanc <nome>
/// valor` em C. Vale a última.
fn declared(text: &str, language: Language, directive: &str) -> Option<String> {
    let code = strip_comments(text);
    let cmm = format!("#{directive}");
    let cpp = directive.to_ascii_lowercase();
    let mut found = None;
    for line in code.lines() {
        let mut words = line.split_whitespace();
        let value = match language {
            Language::Cmm => match words.next() {
                Some(word) if word == cmm => words.next(),
                _ => None,
            },
            Language::Cpp => match (words.next(), words.next(), words.next()) {
                (Some("#pragma"), Some("yanc"), Some(word)) if word == cpp => words.next(),
                _ => None,
            },
        };
        if let Some(value) = value {
            found = Some(value.to_owned());
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
    fn directives_in_comments_are_ignored() {
        let src = "// #NUBITS 16\n/* #NUBITS 8\n */\n#NUBITS 23\n";
        assert_eq!(declared_nubits(src, Language::Cmm), 23);
        assert_eq!(declared_nubits("/* #NUBITS 8 */", Language::Cmm), 32);
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
        assert_eq!(
            declared_nubits("#NUBITS 16\n#NUBITS 24\n", Language::Cmm),
            24
        );
    }

    #[test]
    fn word_width() {
        assert_eq!(declared_nubits("#NUBITS 23\n", Language::Cmm), 23);
        assert_eq!(
            declared_nubits("#pragma yanc nubits 16\n", Language::Cpp),
            16
        );
        assert_eq!(declared_nubits("void main() {}", Language::Cmm), 32);
    }

    #[test]
    fn cpp_pragma() {
        let src = "#pragma yanc nubits 24  \nint main() {}\n";
        assert_eq!(declared_nubits(src, Language::Cpp), 24);
        assert_eq!(declared_nubits(src, Language::Cmm), 32);
    }
}
