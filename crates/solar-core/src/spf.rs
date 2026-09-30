//! Leitura e escrita do `.spf`, o arquivo de projeto da AURORA.
//!
//! Formato (AURORA, `main/ipc/project.js`, classe `ProjectFile`):
//!
//! ```json
//! { "metadata":  { "projectName", "createdAt", "lastModified", "computerName",
//!                  "appVersion", "projectPath" },
//!   "structure": { "basePath", "processors": [], "folders": [], "topLevelFile",
//!                  "testbenchFile", "synthesizableFiles": [], "testbenchFiles": [] } }
//! ```
//!
//! Cada processador é uma string (formato antigo) ou um objeto com `name`,
//! `language` (`"cpp"` ou ausente), `sourceFile`/`cmmFile`, `clk` (MHz),
//! `numClocks` e `showArrays`. O Solar lê só isso e preserva o resto do
//! documento intacto ao regravar, para a AURORA continuar abrindo o projeto.

use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;
use serde_json::Value;

use crate::error::{Result, SolarError};

/// Lê como a AURORA (`js/project/spf_parse.ts`): JSON estrito primeiro; se
/// falhar, tira comentários, BOM e vírgula sobrando antes de `}`/`]` e tenta
/// de novo. Um arquivo de fato quebrado continua sendo erro.
pub(crate) fn parse(path: &Utf8Path, text: &str) -> Result<Value> {
    let value = match serde_json::from_str::<Value>(text) {
        Ok(value) => value,
        Err(strict) => serde_json::from_str(&clean(text)).map_err(|_| invalid(path, strict))?,
    };
    if !value.get("structure").is_some_and(Value::is_object) {
        return Err(invalid(
            path,
            "não é um projeto SAPHO: falta a seção \"structure\"",
        ));
    }
    Ok(value)
}

pub(crate) fn invalid(path: &Utf8Path, reason: impl ToString) -> SolarError {
    SolarError::InvalidProjectFile {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}

/// Remove comentários `//` e `/* */` e vírgulas finais, sem tocar no conteúdo
/// de strings (um `"https://..."` tem que sobreviver).
fn clean(text: &str) -> String {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => in_string = false,
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
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
            }
            ('}' | ']', _) => {
                // vírgula sobrando: `, }` e `,\n]`
                let kept = out.trim_end().len();
                if out[..kept].ends_with(',') {
                    out.truncate(kept - 1);
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Um processador como aparece no `.spf`: string (formato antigo) ou objeto.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawEntry {
    pub name: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub source_file: Option<String>,
    #[serde(default)]
    pub cmm_file: Option<String>,
    #[serde(default)]
    pub clk: Option<Value>,
    #[serde(default)]
    pub num_clocks: Option<Value>,
    #[serde(default)]
    pub show_arrays: Option<Value>,
}

/// Os processadores de `structure.processors`, na ordem do arquivo.
pub(crate) fn processors(path: &Utf8Path, doc: &Value) -> Result<Vec<RawEntry>> {
    let Some(list) = doc["structure"].get("processors") else {
        return Ok(Vec::new());
    };
    let items = list
        .as_array()
        .ok_or_else(|| invalid(path, "structure.processors não é uma lista"))?;
    items
        .iter()
        .enumerate()
        .map(|(i, item)| match item {
            Value::String(name) => Ok(RawEntry {
                name: name.clone(),
                ..RawEntry::default()
            }),
            _ => RawEntry::deserialize(item)
                .map_err(|e| invalid(path, format!("structure.processors[{i}]: {e}"))),
        })
        .collect()
}

/// Inteiro positivo de um campo que a AURORA grava como número, mas que um
/// `.spf` editado à mão pode trazer como texto (`"100"`). `None` quando o
/// campo está ausente ou nulo.
pub(crate) fn positive_int(
    path: &Utf8Path,
    processor: &str,
    field: &str,
    value: Option<&Value>,
) -> Result<Option<u32>> {
    let bad = |detail: &str| invalid(path, format!("processador '{processor}': {field} {detail}"));
    let n = match value {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        Some(_) => None,
    }
    .ok_or_else(|| bad("não é um número"))?;
    // O asmcomp só aceita inteiro em -f e -c. A AURORA trunca em silêncio
    // (parseInt); o Solar recusa para não compilar com um valor que ninguém
    // escolheu.
    if n.fract() != 0.0 || n < 1.0 || n > f64::from(u32::MAX) {
        return Err(bad(&format!("precisa ser um inteiro positivo (é {n})")));
    }
    Ok(Some(n as u32))
}

pub(crate) fn boolean(value: Option<&Value>) -> Option<bool> {
    match value? {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Grava de forma atômica (arquivo temporário + rename), como a AURORA, com
/// indentação de 2 espaços como o `JSON.stringify(dados, null, 2)` dela.
pub(crate) fn write(path: &Utf8Path, doc: &Value) -> Result<()> {
    let mut text = serde_json::to_string_pretty(doc).map_err(|e| invalid(path, e))?;
    text.push('\n');
    let tmp = Utf8PathBuf::from(format!("{path}.tmp"));
    std::fs::write(&tmp, text).map_err(SolarError::io("gravando projeto", &tmp))?;
    std::fs::rename(&tmp, path).map_err(SolarError::io("gravando projeto", path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerant_parse() {
        let text = "\u{feff}  {\n // comentário\n \"structure\": { \"basePath\": \"https://x/y\", /* a */ \"processors\": [\"a\", ], },\n}";
        let doc = parse(Utf8Path::new("p.spf"), text).unwrap();
        assert_eq!(doc["structure"]["basePath"], "https://x/y");
        assert_eq!(doc["structure"]["processors"][0], "a");
    }

    #[test]
    fn comma_inside_string_is_kept() {
        let text = "{\"structure\": {\"x\": \"a, }\",}}";
        let doc = parse(Utf8Path::new("p.spf"), text).unwrap();
        assert_eq!(doc["structure"]["x"], "a, }");
    }

    #[test]
    fn rejects_json_without_structure() {
        let err = parse(Utf8Path::new("package.json"), "{\"name\": \"x\"}").unwrap_err();
        assert!(matches!(err, SolarError::InvalidProjectFile { .. }));
    }

    #[test]
    fn legacy_string_and_object_entries() {
        let doc: Value = serde_json::from_str(
            r#"{"structure": {"processors": ["velho", {"name": "novo", "language": "cpp", "clk": "50", "exists": true}]}}"#,
        )
        .unwrap();
        let list = processors(Utf8Path::new("p.spf"), &doc).unwrap();
        assert_eq!(list[0].name, "velho");
        assert_eq!(list[1].language.as_deref(), Some("cpp"));
        let clk =
            positive_int(Utf8Path::new("p.spf"), "novo", "clk", list[1].clk.as_ref()).unwrap();
        assert_eq!(clk, Some(50));
    }

    #[test]
    fn fractional_clock_is_rejected() {
        let v = Value::from(12.5);
        assert!(positive_int(Utf8Path::new("p.spf"), "p", "clk", Some(&v)).is_err());
    }
}
