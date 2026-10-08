//! O texto da trilha: o título, o corpo e as dicas de um `.md`, e a escolha
//! do arquivo pelo idioma.
//!
//! O formato é o markdown comum, com duas regras: a primeira linha com texto
//! é o título (`# Título`), e as dicas vêm no fim, cada uma numa seção cujo
//! título começa por "Dica" ou "Hint" (`## Dica`, `## Dica 2`). Da primeira
//! dica em diante, cada seção `## ` é uma dica.

use camino::{Utf8Path, Utf8PathBuf};

use crate::error::{LearnError, Result};

/// Um `.md` da trilha, separado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Document {
    /// O título, sem o `# `.
    pub title: String,
    /// O texto entre o título e a primeira dica, sem espaços nas pontas.
    pub body: String,
    /// As dicas, na ordem, cada uma sem o título da seção.
    pub hints: Vec<String>,
}

/// Separa o título, o corpo e as dicas. `None` sem título (`# ` na primeira
/// linha com texto).
pub(crate) fn parse(text: &str) -> Option<Document> {
    let text = text.replace("\r\n", "\n");
    let mut lines = text.lines().skip_while(|line| line.trim().is_empty());
    let title = lines.next()?.strip_prefix("# ")?.trim().to_owned();
    if title.is_empty() {
        return None;
    }
    let mut body = Vec::new();
    let mut hints: Vec<Vec<&str>> = Vec::new();
    for line in lines {
        let section = line.strip_prefix("## ").map(str::trim);
        let starts_hint = section.is_some_and(is_hint_title);
        if starts_hint || (!hints.is_empty() && section.is_some()) {
            hints.push(Vec::new());
        } else if let Some(hint) = hints.last_mut() {
            hint.push(line);
        } else {
            body.push(line);
        }
    }
    Some(Document {
        title,
        body: body.join("\n").trim().to_owned(),
        hints: hints
            .into_iter()
            .map(|lines| lines.join("\n").trim().to_owned())
            .filter(|hint| !hint.is_empty())
            .collect(),
    })
}

/// O título de seção abre uma dica: "Dica", "Dica 2", "Hint".
fn is_hint_title(title: &str) -> bool {
    let first = title.split_whitespace().next().unwrap_or_default();
    first.eq_ignore_ascii_case("dica") || first.eq_ignore_ascii_case("hint")
}

/// O `.md` de `stem` em `dir` no idioma pedido (`prompt.en.md`) se existir;
/// senão o padrão, em português (`prompt.md`). `None` se nem o padrão
/// existir.
pub(crate) fn localized(dir: &Utf8Path, stem: &str, lang: Option<&str>) -> Option<Utf8PathBuf> {
    if let Some(lang) = lang.filter(|l| !l.is_empty() && *l != "pt") {
        let path = dir.join(format!("{stem}.{lang}.md"));
        if path.is_file() {
            return Some(path);
        }
    }
    let path = dir.join(format!("{stem}.md"));
    path.is_file().then_some(path)
}

/// Lê e separa o `.md` de `stem`. Erro se faltar ou se não tiver título.
pub(crate) fn read(dir: &Utf8Path, stem: &str, lang: Option<&str>) -> Result<Document> {
    let path = localized(dir, stem, lang)
        .ok_or_else(|| LearnError::track(&dir.join(format!("{stem}.md")), "missing"))?;
    let text = std::fs::read_to_string(&path).map_err(LearnError::io("Reading", &path))?;
    parse(&text).ok_or_else(|| {
        LearnError::track(
            &path,
            "the first line with text must be the title (# Title)",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_body_and_hints_are_split() {
        let doc = parse(
            "\n# Multiplexador\r\n\nEscolha `a` ou `b`.\n\n```verilog\nassign y = a;\n```\n\n## Dica\n\nUse `?:`.\n\n## Dica 2\n\nOu um `if`.\n",
        )
        .unwrap();
        assert_eq!(doc.title, "Multiplexador");
        assert_eq!(
            doc.body,
            "Escolha `a` ou `b`.\n\n```verilog\nassign y = a;\n```"
        );
        assert_eq!(doc.hints, ["Use `?:`.", "Ou um `if`."]);
    }

    #[test]
    fn sections_before_the_first_hint_stay_in_the_body() {
        let doc = parse("# T\n\nTexto.\n\n## Interface\n\nPortas.\n\n## Hint\n\nH.").unwrap();
        assert_eq!(doc.body, "Texto.\n\n## Interface\n\nPortas.");
        assert_eq!(doc.hints, ["H."]);
    }

    #[test]
    fn a_document_without_a_title_is_refused() {
        assert_eq!(parse("Sem título\n\n## Dica\n\nx"), None);
        assert_eq!(parse("#    \n"), None);
        assert_eq!(parse(""), None);
    }
}
