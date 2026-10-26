//! O `docs/PROTOCOL.md`: a referência do protocolo do `lace serve`, gerada
//! dos tipos de `lace_protocol` como os schemas de `docs/schema/`, e
//! conferida pelo mesmo comando. Mudou um método, um parâmetro ou um tipo?
//! Gere de novo:
//!
//! ```sh
//! LACE_UPDATE_SCHEMA=1 cargo test -p lace-cli schema
//! ```
//!
//! e confira o diff do `docs/PROTOCOL.md` junto com o de `docs/schema/`.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use camino::Utf8Path;
use lace_protocol::{METHODS, PROTOCOL_VERSION, request_schema, response_schema, result_schema};
use serde_json::{Map, Value};

use crate::serve::rpc;

/// Os tipos nomeados que a referência mostra, nas duas vistas: a do que o
/// cliente manda (um campo com padrão é opcional) e a do que ele recebe (um
/// campo que sai sempre é obrigatório).
struct Types {
    request: Map<String, Value>,
    response: Map<String, Value>,
    shown: BTreeSet<String>,
}

impl Types {
    fn new(request: &Value, results: &Value, responses: &Value) -> Types {
        let request_defs = defs(request);
        let mut response_defs = defs(responses);
        for (name, schema) in defs(results) {
            match response_defs.get(&name) {
                Some(known) => assert_eq!(known, &schema, "{name} difere entre os dois schemas"),
                None => {
                    response_defs.insert(name, schema);
                }
            }
        }
        // Só os tipos que os métodos, as notificações e os erros citam.
        let mut in_request = BTreeSet::new();
        for entry in request["oneOf"].as_array().expect("oneOf") {
            collect(
                &entry["properties"]["params"],
                &request_defs,
                &mut in_request,
            );
        }
        let mut in_response = BTreeSet::new();
        for root in [
            &results["properties"],
            &response_defs["Notification"],
            &responses["properties"]["shutdown"],
            &responses["properties"]["error"],
        ] {
            collect(root, &response_defs, &mut in_response);
        }
        let request: Map<String, Value> = request_defs
            .into_iter()
            .filter(|(name, _)| in_request.contains(name))
            .collect();
        let response: Map<String, Value> = response_defs
            .into_iter()
            .filter(|(name, _)| in_response.contains(name))
            .collect();
        for (name, schema) in &request {
            if let Some(other) = response.get(name) {
                assert_eq!(
                    without_required(schema),
                    without_required(other),
                    "{name} muda entre o pedido e a resposta além de `required`: \
                     ensine o gerador a mostrar as duas formas"
                );
            }
        }
        let shown = request.keys().chain(response.keys()).cloned().collect();
        Types {
            request,
            response,
            shown,
        }
    }
}

fn defs(schema: &Value) -> Map<String, Value> {
    schema["$defs"].as_object().cloned().unwrap_or_default()
}

fn ref_name(schema: &Value) -> Option<&str> {
    schema.get("$ref")?.as_str()?.strip_prefix("#/$defs/")
}

/// Os tipos que `schema` cita, direta ou indiretamente.
fn collect(schema: &Value, defs: &Map<String, Value>, found: &mut BTreeSet<String>) {
    if let Some(name) = ref_name(schema)
        && found.insert(name.to_owned())
    {
        collect(&defs[name], defs, found);
    }
    match schema {
        Value::Object(map) => map.values().for_each(|v| collect(v, defs, found)),
        Value::Array(items) => items.iter().for_each(|v| collect(v, defs, found)),
        _ => {}
    }
}

fn without_required(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| *key != "required")
                .map(|(key, value)| (key.clone(), without_required(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_required).collect()),
        other => other.clone(),
    }
}

fn type_anchor(name: &str) -> String {
    format!("tipo-{}", name.to_lowercase())
}

fn method_anchor(name: &str) -> String {
    format!("metodo-{}", name.replace(['.', '_'], "-"))
}

/// O tipo de um valor, numa linha: um link para o tipo nomeado, `string`,
/// `lista de X`, `X \| null`.
fn label(schema: &Value) -> String {
    let Some(map) = schema.as_object() else {
        return "qualquer JSON".into();
    };
    if let Some(name) = ref_name(schema) {
        return format!("[`{name}`](#{})", type_anchor(name));
    }
    if let Some(value) = map.get("const") {
        return format!("`{value}`");
    }
    if let Some(values) = map.get("enum").and_then(Value::as_array) {
        return join(values.iter().map(|v| format!("`{v}`")));
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(variants) = map.get(key).and_then(Value::as_array) {
            return join(variants.iter().map(label));
        }
    }
    let kinds: Vec<&str> = match map.get("type") {
        Some(Value::String(kind)) => vec![kind],
        Some(Value::Array(kinds)) => kinds.iter().filter_map(Value::as_str).collect(),
        _ => return "qualquer JSON".into(),
    };
    join(kinds.into_iter().map(|kind| match kind {
        "array" => {
            if let Some(items) = map.get("prefixItems").and_then(Value::as_array) {
                let items: Vec<String> = items.iter().map(label).collect();
                format!("({})", items.join(", "))
            } else if let Some(items) = map.get("items") {
                format!("lista de {}", label(items))
            } else {
                "`array`".into()
            }
        }
        "object" => match map.get("additionalProperties") {
            Some(value) if value.is_object() => format!("mapa de `string` para {}", label(value)),
            _ => "`object`".into(),
        },
        other => format!("`{other}`"),
    }))
}

fn join(labels: impl Iterator<Item = String>) -> String {
    labels.collect::<Vec<_>>().join(" \\| ")
}

impl Types {
    /// Os links da documentação do Rust (`` [`ProjectStatus`] ``,
    /// `` [`StepReport`](crate::StepReport) ``) viram links para o tipo, se
    /// ele está aqui, ou código: o caminho do Rust só abre no rustdoc. Um
    /// link para um endereço ou um arquivo fica como está.
    fn links(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find("[`") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find("`]") else {
                rest = &rest[start..];
                break;
            };
            let (inner, mut tail) = (&after[..end], &after[end + 2..]);
            let rust_path = tail
                .strip_prefix('(')
                .and_then(|t| t.split_once(')'))
                .filter(|(target, _)| {
                    target
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
                });
            if let Some((_, after_link)) = rust_path {
                tail = after_link;
            }
            if rust_path.is_none() && (tail.starts_with(['(', '[']) || inner.contains('`')) {
                out.push_str(&rest[start..start + end + 4]);
            } else if self.shown.contains(inner) {
                let _ = write!(out, "[`{inner}`](#{})", type_anchor(inner));
            } else {
                let _ = write!(out, "`{inner}`");
            }
            rest = tail;
        }
        out.push_str(rest);
        out
    }

    /// Uma descrição inteira, em parágrafos. Um bloco de código sem
    /// linguagem é Rust, como no rustdoc, e perde as linhas ocultas (`# `).
    fn prose(&self, text: &str) -> String {
        let mut out = String::new();
        let (mut fence, mut rust) = (false, false);
        for line in text.lines() {
            let trimmed = line.trim_start();
            if let Some(lang) = trimmed.strip_prefix("```").map(str::trim) {
                if fence {
                    out.push_str(line);
                } else {
                    rust = lang.is_empty() || lang.starts_with("rust");
                    out.push_str(if lang.is_empty() { "```rust" } else { line });
                }
                fence = !fence;
            } else if fence && rust && (trimmed == "#" || trimmed.starts_with("# ")) {
                continue;
            } else if fence {
                out.push_str(line);
            } else {
                out.push_str(&self.links(line));
            }
            out.push('\n');
        }
        out.trim_end().to_owned()
    }

    /// Uma descrição numa célula de tabela: o primeiro parágrafo, numa
    /// linha.
    fn cell(&self, text: &str) -> String {
        let first = text.split("\n\n").next().unwrap_or_default();
        let line = first.split_whitespace().collect::<Vec<_>>().join(" ");
        self.links(&line).replace('|', "\\|")
    }

    /// Os campos de um objeto, numa tabela. `other` é o mesmo objeto na
    /// outra vista, quando o tipo aparece nas duas; `tag` é o campo que diz
    /// a variante, que não entra.
    fn fields(&self, out: &mut String, node: &Value, other: Option<&Value>, tag: Option<&str>) {
        self.table(out, "Campo", node, other, tag);
    }

    fn table(
        &self,
        out: &mut String,
        column: &str,
        node: &Value,
        other: Option<&Value>,
        tag: Option<&str>,
    ) {
        let properties = node["properties"].as_object().cloned().unwrap_or_default();
        let rows: Vec<_> = properties
            .iter()
            .filter(|(name, _)| Some(name.as_str()) != tag)
            .collect();
        if rows.is_empty() {
            out.push_str("Sem campos.\n\n");
            return;
        }
        let _ = writeln!(
            out,
            "| {column} | Tipo | Obrigatório | Descrição |\n|---|---|---|---|"
        );
        for (name, schema) in rows {
            let here = required(node, name);
            let mut text = match other.map(|o| required(o, name)) {
                Some(true) if !here => "não; obrigatório no pedido".to_owned(),
                Some(false) if here => "sim; opcional no pedido".to_owned(),
                _ if here => "sim".to_owned(),
                _ => "não".to_owned(),
            };
            if let Some(default) = schema.get("default") {
                let _ = write!(text, " (padrão: `{default}`)");
            }
            let description = schema["description"].as_str().unwrap_or_default();
            let _ = writeln!(
                out,
                "| `{name}` | {} | {text} | {} |",
                label(schema),
                self.cell(description)
            );
        }
        out.push('\n');
    }

    fn render_type(&self, out: &mut String, name: &str) {
        let (node, other) = match self.response.get(name) {
            Some(node) => (node, self.request.get(name)),
            None => (&self.request[name], None),
        };
        let _ = writeln!(
            out,
            "<a id=\"{}\"></a>\n\n### `{name}`\n",
            type_anchor(name)
        );
        if let Some(description) = node["description"].as_str() {
            let _ = writeln!(out, "{}\n", self.prose(description));
        }
        if node.get("properties").is_some() {
            self.fields(out, node, other, None);
            if let Some(extra) = node.get("additionalProperties").filter(|v| v.is_object()) {
                let _ = writeln!(out, "Mais chaves livres, cada uma com {}.\n", label(extra));
            }
            return;
        }
        let Some((key, variants)) = ["oneOf", "anyOf"]
            .into_iter()
            .find_map(|key| Some((key, node.get(key)?.as_array()?)))
        else {
            let _ = writeln!(out, "Tipo: {}.\n", label(node));
            return;
        };
        if variants.iter().all(|v| v.get("const").is_some()) {
            out.push_str("| Valor | Descrição |\n|---|---|\n");
            for variant in variants {
                let description = variant["description"].as_str().unwrap_or_default();
                let _ = writeln!(
                    out,
                    "| `{}` | {} |",
                    variant["const"],
                    self.cell(description)
                );
            }
            out.push('\n');
            return;
        }
        out.push_str("Uma das formas:\n\n");
        for (i, variant) in variants.iter().enumerate() {
            let other = other.and_then(|o| o.get(key)?.get(i));
            let tag = variant["properties"].as_object().and_then(|properties| {
                properties
                    .iter()
                    .find_map(|(field, schema)| Some((field.as_str(), schema.get("const")?)))
            });
            let title = match tag {
                Some((field, value)) => format!("`{field}: {value}`"),
                None if variant.get("properties").is_some() => "objeto".to_owned(),
                None => label(variant),
            };
            let _ = writeln!(out, "**{title}**\n");
            if let Some(description) = variant["description"].as_str() {
                let _ = writeln!(out, "{}\n", self.prose(description));
            }
            if variant.get("properties").is_some() {
                self.fields(out, variant, other, tag.map(|(field, _)| field));
            }
        }
    }
}

fn required(node: &Value, field: &str) -> bool {
    node["required"]
        .as_array()
        .is_some_and(|names| names.iter().any(|n| n == field))
}

/// Até o fim da primeira frase fora de código.
fn first_sentence(line: &str) -> &str {
    let mut code = false;
    for (i, c) in line.char_indices() {
        match c {
            '`' => code = !code,
            '.' | '?' if !code && line[i + 1..].starts_with(' ') => return &line[..=i],
            _ => {}
        }
    }
    line
}

const HEADER: &str = "\
# Protocolo do `lace serve`

<!-- Gerado por crates/lace-cli/src/protocol_doc.rs. Não edite à mão: \
LACE_UPDATE_SCHEMA=1 cargo test -p lace-cli schema -->

Cada método do `lace serve`, com os parâmetros e o resultado, as \
notificações, os erros e os tipos. O `lace mcp` usa o mesmo contrato. Esta \
página sai dos tipos de `lace_protocol` \
(`crates/lace-protocol/src/lib.rs`), como os schemas \
[`protocol-request.json`](schema/protocol-request.json) e \
[`protocol-response.json`](schema/protocol-response.json), e um teste falha \
quando ela não confere com eles.

Como abrir o servidor, a fila por projeto, o cancelamento e o encerramento: \
[CLI.md](CLI.md#a-api-para-outros-programas-lace-serve-e-lace-mcp).

";

/// O `docs/PROTOCOL.md` inteiro.
pub fn render() -> String {
    let request = serde_json::to_value(request_schema()).unwrap();
    let results = serde_json::to_value(result_schema()).unwrap();
    let responses = serde_json::to_value(response_schema()).unwrap();
    let types = Types::new(&request, &results, &responses);
    let entries = request["oneOf"].as_array().expect("oneOf");
    let request_of = |method: &str| {
        entries
            .iter()
            .find(|e| e["properties"]["method"]["const"] == method)
            .unwrap_or_else(|| panic!("{method} sem pedido no schema"))
    };
    let result = |method: &str| &results["properties"][method];

    let mut out = HEADER.to_owned();
    let _ = writeln!(
        out,
        "Versão do protocolo: {PROTOCOL_VERSION} (`PROTOCOL_VERSION`, que o \
         `initialize` devolve).\n\n\
         Um pedido é uma linha: `{{\"jsonrpc\": \"2.0\", \"id\": 1, \"method\": \
         \"project.status\", \"params\": {{\"project\": \"/home/eu/soma\"}}}}`. Os \
         parâmetros vão por nome, e os caminhos são absolutos. A resposta traz o \
         mesmo `id` e o resultado em `result`, ou o erro em `error`.\n"
    );

    out.push_str("## Métodos\n\n| Método | Resultado | O que faz |\n|---|---|---|\n");
    for method in METHODS {
        let description = request_of(method)["description"]
            .as_str()
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "| [`{method}`](#{}) | {} | {} |",
            method_anchor(method),
            label(result(method)),
            first_sentence(&types.cell(description))
        );
    }
    out.push('\n');

    let mut group = None;
    for method in METHODS {
        let this = method.split_once('.').map(|(g, _)| g);
        if group != Some(this) {
            group = Some(this);
            let _ = writeln!(
                out,
                "### {}\n",
                this.map_or("Sessão".to_owned(), |g| format!("`{g}.*`"))
            );
        }
        let entry = request_of(method);
        let _ = writeln!(
            out,
            "<a id=\"{}\"></a>\n\n#### `{method}`\n",
            method_anchor(method)
        );
        if let Some(description) = entry["description"].as_str() {
            let _ = writeln!(out, "{}\n", types.prose(description));
        }
        let params = &entry["properties"]["params"];
        if params["properties"]
            .as_object()
            .is_some_and(|p| !p.is_empty())
        {
            types.table(&mut out, "Parâmetro", params, None, None);
        } else {
            out.push_str("Sem parâmetros.\n\n");
        }
        let _ = writeln!(out, "Resultado: {}.\n", label(result(method)));
    }

    out.push_str(
        "## Notificações do servidor\n\n\
         Enquanto um pedido roda, o servidor manda notificações dele, sempre \
         antes da resposta, com o `id` do pedido em `params.id`: \
         `{\"jsonrpc\": \"2.0\", \"method\": \"phase\", \"params\": {\"id\": 2, \
         \"phase\": \"build\"}}`.\n\n",
    );
    let notification = &responses["$defs"]["Notification"];
    for variant in notification["oneOf"].as_array().expect("oneOf") {
        let name = variant["properties"]["type"]["const"]
            .as_str()
            .expect("type");
        let _ = writeln!(out, "### `{name}`\n");
        if let Some(description) = variant["description"].as_str() {
            let _ = writeln!(out, "{}\n", types.prose(description));
        }
        types.fields(&mut out, variant, None, Some("type"));
    }
    let _ = writeln!(
        out,
        "### `shutdown`\n\n\
         O servidor vai encerrar: depois do `shutdown`, quando a entrada fecha, \
         num sinal ou depois de um `update.apply` que trocou o binário. Vem sem \
         `id`.\n\n\
         | Campo | Tipo | Obrigatório | Descrição |\n|---|---|---|---|\n\
         | `reason` | {} | sim | Por quê. |\n",
        label(&responses["properties"]["shutdown"])
    );
    out.push_str(
        "## Notificação do cliente\n\n\
         ### `$/cancel`\n\n\
         Cancela um pedido: `{\"jsonrpc\": \"2.0\", \"method\": \"$/cancel\", \
         \"params\": {\"id\": 2}}`. Um pedido que ainda não começou responde o \
         erro `-32001`; um fluxo em andamento para e responde o resultado com \
         `status: \"cancelled\"`.\n\n",
    );

    out.push_str("## Erros\n\n| `error.code` | Quando |\n|---|---|\n");
    for (code, when) in rpc::ALL {
        let _ = writeln!(out, "| `{code}` | {when} |");
    }
    let _ = writeln!(
        out,
        "\nNo `{}`, `error.data` é um {}: os valores de `code` estão em \
         [API.md, seção 8](API.md#8-erros).\n",
        rpc::LACE,
        label(&responses["properties"]["error"])
    );

    out.push_str(
        "## Tipos\n\n\
         Em ordem alfabética. Num tipo que o cliente manda, obrigatório é o \
         campo que ele precisa mandar; num que ele recebe, o campo que sempre \
         vem, mesmo que `null`.\n\n",
    );
    for name in &types.shown {
        types.render_type(&mut out, name);
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `docs/PROTOCOL.md` é o que `render()` gera. Com `LACE_UPDATE_SCHEMA`,
    /// regrava o arquivo em vez de comparar.
    #[test]
    fn protocol_doc_matches_the_schema() {
        let path = Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/PROTOCOL.md");
        let text = render();
        if std::env::var_os("LACE_UPDATE_SCHEMA").is_some() {
            std::fs::write(&path, &text).unwrap();
            return;
        }
        assert!(
            std::fs::read_to_string(&path).ok().as_deref() == Some(text.as_str()),
            "docs/PROTOCOL.md não confere com os tipos do protocolo\n\
             gere de novo com: LACE_UPDATE_SCHEMA=1 cargo test -p lace-cli schema\n\
             e confira o diff"
        );
    }

    #[test]
    fn every_anchor_is_unique_and_every_link_has_one() {
        let text = render();
        let anchors: Vec<&str> = text
            .split("<a id=\"")
            .skip(1)
            .filter_map(|s| s.split('"').next())
            .collect();
        let unique: BTreeSet<&str> = anchors.iter().copied().collect();
        assert_eq!(anchors.len(), unique.len(), "âncora repetida");
        for link in text.split("](#").skip(1) {
            let target = link.split(')').next().unwrap();
            assert!(unique.contains(target), "link sem âncora: #{target}");
        }
    }

    #[test]
    fn labels_read_like_types() {
        let types = |v: Value| label(&v);
        assert_eq!(types(serde_json::json!({"type": "string"})), "`string`");
        assert_eq!(
            types(serde_json::json!({"type": ["string", "null"]})),
            "`string` \\| `null`"
        );
        assert_eq!(
            types(serde_json::json!({"type": "array", "items": {"$ref": "#/$defs/AddedFile"}})),
            "lista de [`AddedFile`](#tipo-addedfile)"
        );
        assert_eq!(types(serde_json::json!(true)), "qualquer JSON");
        assert_eq!(first_sentence("Faz `a. b`. Depois."), "Faz `a. b`.");
        assert_eq!(first_sentence("Serve? Resposta: x."), "Serve?");
    }

    #[test]
    fn rust_doc_links_become_type_links_or_code() {
        let types = Types {
            request: Map::new(),
            response: Map::new(),
            shown: BTreeSet::from(["StepReport".to_owned()]),
        };
        assert_eq!(
            types.links("em todo [`StepReport`](crate::StepReport)."),
            "em todo [`StepReport`](#tipo-stepreport)."
        );
        assert_eq!(
            types.links("[`Display`](fmt::Display) e [`Throttle`]"),
            "`Display` e `Throttle`"
        );
        assert_eq!(types.links("[`CLI.md`](CLI.md)"), "[`CLI.md`](CLI.md)");
        assert_eq!(types.links("sem fim [`x"), "sem fim [`x");
    }
}
