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
//! `numClocks` e `showArrays`. O Lace lê só isso e preserva o resto do
//! documento intacto ao regravar, para a AURORA continuar abrindo o projeto.

use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;
use serde_json::Value;

use crate::error::{LaceError, Result};

/// Lê como a AURORA (`js/project/spf_parse.ts`): JSON estrito primeiro; se
/// falhar, tira comentários, BOM e vírgula sobrando antes de `}`/`]` e tenta
/// de novo. Um arquivo de fato quebrado continua sendo erro.
pub(crate) fn parse(path: &Utf8Path, text: &str) -> Result<Value> {
    let value = match serde_json::from_str::<Value>(text) {
        Ok(value) => value,
        Err(strict) => serde_json::from_str(&clean(text)).map_err(|_| invalid(path, strict))?,
    };
    match value.get("structure") {
        Some(Value::Object(_)) => {}
        None => {
            return Err(invalid(
                path,
                "Not a SAPHO project: the \"structure\" section is missing",
            ));
        }
        Some(_) => {
            return Err(invalid(path, "The \"structure\" section is not an object"));
        }
    }
    check_file_fields(path, &value)?;
    Ok(value)
}

/// Confere os tipos dos campos de arquivos de `structure`: as listas são
/// listas de objetos, `path` é texto, `isTopLevel` é booleano (ou o texto
/// `"true"`/`"false"`), e `topLevelFile`, `testbenchFile` e `topLevelModule`
/// são texto. Ausente ou `null` vale como vazio. Um tipo errado é erro, e
/// não um campo ignorado: a próxima gravação o trocaria por uma lista vazia.
fn check_file_fields(path: &Utf8Path, doc: &Value) -> Result<()> {
    let structure = &doc["structure"];
    for key in ["synthesizableFiles", "testbenchFiles"] {
        let items = match structure.get(key) {
            None | Some(Value::Null) => continue,
            Some(Value::Array(items)) => items,
            Some(_) => return Err(invalid(path, format!("structure.{key} is not a list"))),
        };
        for (i, item) in items.iter().enumerate() {
            let Some(entry) = item.as_object() else {
                return Err(invalid(
                    path,
                    format!("structure.{key}[{i}] is not an object with \"path\""),
                ));
            };
            if !matches!(
                entry.get("path"),
                None | Some(Value::Null | Value::String(_))
            ) {
                return Err(invalid(
                    path,
                    format!("structure.{key}[{i}].path is not a string"),
                ));
            }
            if let Some(flag) = entry.get("isTopLevel")
                && !flag.is_null()
                && boolean(Some(flag)).is_none()
            {
                return Err(invalid(
                    path,
                    format!("structure.{key}[{i}].isTopLevel is not true or false"),
                ));
            }
        }
    }
    for key in ["topLevelFile", "testbenchFile", "topLevelModule"] {
        if !matches!(
            structure.get(key),
            None | Some(Value::Null | Value::String(_))
        ) {
            return Err(invalid(path, format!("structure.{key} is not a string")));
        }
    }
    Ok(())
}

pub(crate) fn invalid(path: &Utf8Path, reason: impl ToString) -> LaceError {
    LaceError::InvalidProjectFile {
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
        .ok_or_else(|| invalid(path, "structure.processors is not a list"))?;
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
    let bad = |detail: &str| invalid(path, format!("Processor '{processor}': {field} {detail}"));
    let n = match value {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        Some(_) => None,
    }
    .ok_or_else(|| bad("is not a number"))?;
    // O asmcomp só aceita inteiro em -f e -c. A AURORA trunca em silêncio
    // (parseInt); o Lace recusa para não compilar com um valor que ninguém
    // escolheu.
    if n.fract() != 0.0 || n < 1.0 || n > f64::from(u32::MAX) {
        return Err(bad(&format!("must be a positive integer (it is {n})")));
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
    // Nome único: duas gravações ao mesmo tempo não disputam o mesmo
    // temporário (uma acabava sem ele: "No such file or directory").
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = Utf8PathBuf::from(format!("{path}.{}-{n}.tmp", std::process::id()));
    std::fs::write(&tmp, text).map_err(LaceError::io("Writing project", &tmp))?;
    // No Windows, a troca pode esbarrar num leitor que abriu o `.spf` sem
    // permitir a remoção (outro programa) ou numa troca anterior ainda em
    // curso: tenta de novo por alguns milissegundos.
    let mut attempt = 0;
    loop {
        match std::fs::rename(&tmp, path) {
            Err(e)
                if cfg!(windows)
                    && attempt < 20
                    && e.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(LaceError::io("Writing project", path)(e));
            }
            Ok(()) => return Ok(()),
        }
    }
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
        assert!(matches!(err, LaceError::InvalidProjectFile { .. }));
    }

    #[test]
    fn wrong_types_in_file_fields_are_errors() {
        let p = Utf8Path::new("p.spf");
        for (text, field) in [
            (
                r#"{"structure": {"synthesizableFiles": {"a": 1}}}"#,
                "synthesizableFiles is not a list",
            ),
            (
                r#"{"structure": {"testbenchFiles": "tb.v"}}"#,
                "testbenchFiles is not a list",
            ),
            (
                r#"{"structure": {"testbenchFiles": ["tb.v"]}}"#,
                "testbenchFiles[0] is not an object",
            ),
            (
                r#"{"structure": {"synthesizableFiles": [{"path": 3}]}}"#,
                "[0].path is not a string",
            ),
            (
                r#"{"structure": {"synthesizableFiles": [{"path": "a.v", "isTopLevel": 1}]}}"#,
                "isTopLevel",
            ),
            (
                r#"{"structure": {"topLevelFile": 42}}"#,
                "topLevelFile is not a string",
            ),
            (r#"{"structure": []}"#, "not an object"),
        ] {
            match parse(p, text) {
                Err(LaceError::InvalidProjectFile { reason, .. }) => {
                    assert!(reason.contains(field), "{text}: {reason}")
                }
                other => panic!("{text}: {other:?}"),
            }
        }
        let ok = r#"{"structure": {"synthesizableFiles": [{"path": "a.v", "isTopLevel": "true"}], "topLevelFile": null}}"#;
        assert!(parse(p, ok).is_ok());
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
