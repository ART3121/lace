//! `lace mcp`: o orquestrador para um agente, como servidor MCP (Model
//! Context Protocol) por stdin e stdout.
//!
//! O MCP também é JSON-RPC 2.0 com uma mensagem por linha, então este é um
//! adaptador fino sobre o despacho do `lace serve` ([`Server::handle`]):
//! uma ferramenta por método do protocolo que faz sentido para um agente,
//! com o `inputSchema` tirado do mesmo schema dos pedidos
//! (`lace_core::protocol::request_schema`) e a descrição tirada das docs.
//! `flow.run` é desdobrado em `build`, `check`, `hierarchy`, `sim` e
//! `synth`, cada um com os campos do próprio fluxo.
//!
//! O resultado de uma ferramenta vai como texto (o JSON do resultado) em
//! `content`; `isError` marca o Lace que não conseguiu rodar, com o
//! `ErrorInfo` no texto. A saída ao vivo das ferramentas não vai: o agente
//! lê o resultado. As ferramentas que mudam a instalação (`update_apply`,
//! `install_add`) só existem com `--allow-update`.
//!
//! Versão do protocolo MCP: `2025-06-18`. Métodos: `initialize`,
//! `notifications/initialized`, `ping`, `tools/list`, `tools/call`. Sem
//! recursos nem prompts.

use std::collections::BTreeSet;
use std::io::{BufRead, Write};

use lace_core::CancelToken;
use lace_core::protocol::request_schema;
use serde_json::{Map, Value, json};

use crate::serve::{Server, Writer, parse_request};
use crate::settings::ToolchainArgs;

/// A versão do MCP que este servidor fala.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// As opções de `lace mcp`.
#[derive(clap::Args, Debug, Clone)]
pub struct McpArgs {
    /// Also expose update_apply and install_add, which change the Lace installation
    #[arg(long)]
    pub allow_update: bool,
}

/// Uma ferramenta: o nome para o agente e o método do protocolo por trás.
struct Tool {
    name: &'static str,
    method: &'static str,
    /// Um fluxo de `flow.run`, desdobrado.
    flow: Option<&'static str>,
    /// Muda a instalação do Lace: só com `--allow-update`.
    installation: bool,
}

const fn tool(name: &'static str, method: &'static str) -> Tool {
    Tool {
        name,
        method,
        flow: None,
        installation: false,
    }
}

const fn flow(name: &'static str, flow: &'static str) -> Tool {
    Tool {
        name,
        method: "flow.run",
        flow: Some(flow),
        installation: false,
    }
}

const fn installer(name: &'static str, method: &'static str) -> Tool {
    Tool {
        name,
        method,
        flow: None,
        installation: true,
    }
}

/// As ferramentas, na ordem em que o agente as vê.
const TOOLS: &[Tool] = &[
    tool("project_create", "project.create"),
    tool("project_status", "project.status"),
    tool("project_add", "project.add"),
    tool("project_remove", "project.remove"),
    tool("project_top", "project.top"),
    tool("project_testbench", "project.testbench"),
    tool("project_order", "project.order"),
    tool("project_move", "project.move"),
    tool("project_convert", "project.convert"),
    tool("processor_add", "processor.add"),
    tool("processor_set", "processor.set"),
    tool("processor_remove", "processor.remove"),
    tool("processor_outputs", "processor.outputs"),
    flow("build", "build"),
    flow("check", "check"),
    flow("hierarchy", "hierarchy"),
    flow("sim", "simulate"),
    flow("synth", "synthesize"),
    tool("wave_signals", "wave.signals"),
    tool("wave_choose", "wave.choose"),
    tool("history_list", "history.list"),
    tool("history_show", "history.show"),
    tool("history_compare", "history.compare"),
    tool("toolchain_info", "toolchain.info"),
    tool("toolchain_verify", "toolchain.verify"),
    tool("experimental_list", "experimental.list"),
    tool("fpga_boards", "fpga.boards"),
    tool("fpga_config", "fpga.config"),
    tool("fpga_config_set", "fpga.config_set"),
    tool("fpga_check", "fpga.check"),
    tool("fpga_modules", "fpga.modules"),
    tool("fpga_status", "fpga.status"),
    tool("learn_check", "learn.check"),
    tool("update_check", "update.check"),
    installer("update_apply", "update.apply"),
    installer("install_add", "install.add"),
];

/// `lace mcp`. Cada linha da entrada é tratada na ordem; uma chamada de
/// ferramenta roda até o fim antes da próxima linha. `cancel` é o pedido
/// dos sinais, que cancela a chamada em andamento e encerra.
pub fn run(
    toolchain: &ToolchainArgs,
    args: &McpArgs,
    cancel: &CancelToken,
) -> anyhow::Result<bool> {
    let mcp = Mcp::new(
        Server::new(toolchain.clone(), Writer::silent()),
        args.allow_update,
    );
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if cancel.is_cancelled() {
            break;
        }
        if let Some(reply) = mcp.handle_line(&line, cancel) {
            let mut out = stdout.lock();
            writeln!(out, "{}", serde_json::to_string(&reply)?)?;
            out.flush()?;
        }
    }
    Ok(true)
}

/// O servidor MCP: o despacho do Lace mais o schema dos pedidos, de onde
/// saem as ferramentas.
pub(crate) struct Mcp {
    server: Server,
    allow_update: bool,
    schema: Value,
}

impl Mcp {
    pub(crate) fn new(server: Server, allow_update: bool) -> Self {
        Mcp {
            server,
            allow_update,
            schema: serde_json::to_value(request_schema()).unwrap_or(Value::Null),
        }
    }

    /// Uma linha da entrada: a resposta, ou `None` para uma notificação.
    pub(crate) fn handle_line(&self, line: &str, cancel: &CancelToken) -> Option<Value> {
        if line.trim().is_empty() {
            return None;
        }
        let message: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                return Some(error_reply(
                    Value::Null,
                    -32700,
                    &format!("Invalid JSON: {error}"),
                ));
            }
        };
        let id = message.get("id").cloned();
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Some(error_reply(
                id.unwrap_or(Value::Null),
                -32600,
                "The request has no method",
            ));
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        // Uma notificação não tem resposta.
        let id = id?;
        Some(match method {
            "initialize" => reply(id, self.initialize(&params)),
            "ping" => reply(id, json!({})),
            "tools/list" => reply(id, json!({ "tools": self.tools() })),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match self.call(name, arguments, cancel) {
                    Ok(result) => reply(id, result),
                    Err(message) => error_reply(id, -32602, &message),
                }
            }
            other => error_reply(id, -32601, &format!("Method not found: {other}")),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let client = params
            .get("clientInfo")
            .and_then(|c| c.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("?");
        tracing::info!(client, "MCP session started");
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "lace", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "Lace orchestrates Verilog and SAPHO processor projects: every tool \
                takes the absolute path of the project (.spf file or folder). Use build, check, \
                sim and synth for the flows, project_status to read the project, history_show to \
                read a past report.",
        })
    }

    /// Cada ferramenta com o `inputSchema` do método dela.
    pub(crate) fn tools(&self) -> Vec<Value> {
        TOOLS
            .iter()
            .filter(|t| self.allow_update || !t.installation)
            .filter_map(|t| {
                let (description, schema) = self.tool_schema(t)?;
                Some(json!({
                    "name": t.name,
                    "description": description,
                    "inputSchema": schema,
                }))
            })
            .collect()
    }

    /// A descrição e o `inputSchema` de uma ferramenta, do schema dos pedidos.
    fn tool_schema(&self, tool: &Tool) -> Option<(String, Value)> {
        let variants = self.schema.get("oneOf")?.as_array()?;
        let variant = variants.iter().find(|v| {
            v.pointer("/properties/method/const")
                .and_then(Value::as_str)
                .is_some_and(|m| m == tool.method)
        })?;
        let mut description = clean(variant.get("description").and_then(Value::as_str)?);
        let mut params = variant
            .pointer("/properties/params")
            .cloned()
            .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
        if let Some(flow) = tool.flow {
            // Os campos do fluxo no lugar de `request`, mais o projeto.
            let flows = self
                .schema
                .pointer("/$defs/FlowRequest/oneOf")?
                .as_array()?;
            let variant = flows.iter().find(|v| {
                v.pointer("/properties/flow/const")
                    .and_then(Value::as_str)
                    .is_some_and(|f| f == flow)
            })?;
            description = clean(variant.get("description").and_then(Value::as_str)?);
            let mut properties = Map::new();
            properties.insert(
                "project".to_owned(),
                json!({ "description": "The project: its .spf file or its folder (absolute path).", "type": "string" }),
            );
            if let Some(Value::Object(fields)) = variant.get("properties") {
                for (name, field) in fields {
                    if name != "flow" {
                        properties.insert(name.clone(), field.clone());
                    }
                }
            }
            let mut required = vec![Value::from("project")];
            if let Some(Value::Array(fields)) = variant.get("required") {
                required.extend(
                    fields
                        .iter()
                        .filter(|f| f.as_str() != Some("flow"))
                        .cloned(),
                );
            }
            params = json!({ "type": "object", "properties": properties, "required": required });
        }
        if params.get("type").is_none() {
            params["type"] = json!("object");
        }
        if params.get("properties").is_none() {
            params["properties"] = json!({});
        }
        let defs = self.referenced_defs(&params);
        if !defs.is_empty() {
            params["$defs"] = Value::Object(defs);
        }
        Some((description, params))
    }

    /// As definições de `$defs` que `schema` referencia, e as que elas
    /// referenciam.
    fn referenced_defs(&self, schema: &Value) -> Map<String, Value> {
        let Some(all) = self.schema.get("$defs").and_then(Value::as_object) else {
            return Map::new();
        };
        let mut wanted = BTreeSet::new();
        collect_refs(schema, &mut wanted);
        let mut done = BTreeSet::new();
        let mut defs = Map::new();
        while let Some(name) = wanted.iter().next().cloned() {
            wanted.remove(&name);
            if !done.insert(name.clone()) {
                continue;
            }
            if let Some(def) = all.get(&name) {
                collect_refs(def, &mut wanted);
                defs.insert(name, def.clone());
            }
        }
        defs
    }

    /// Roda uma ferramenta. `Err` é um pedido que o MCP recusa (ferramenta
    /// desconhecida); um erro do Lace volta como resultado com `isError`.
    pub(crate) fn call(
        &self,
        name: &str,
        arguments: Value,
        cancel: &CancelToken,
    ) -> Result<Value, String> {
        let tool = TOOLS
            .iter()
            .filter(|t| self.allow_update || !t.installation)
            .find(|t| t.name == name)
            .ok_or_else(|| format!("Unknown tool: {name}"))?;
        let params = match tool.flow {
            Some(flow) => {
                let Value::Object(mut fields) = arguments else {
                    return Ok(tool_error(json!({
                        "error": { "code": "invalid_argument", "message": "The arguments must be an object" }
                    })));
                };
                let project = fields.remove("project").unwrap_or(Value::Null);
                fields.insert("flow".to_owned(), json!(flow));
                // Sem a saída ao vivo: o agente lê o resultado, e o `vvp`
                // roda com buffer, que é mais rápido.
                json!({ "project": project, "request": Value::Object(fields), "output": false })
            }
            None => arguments,
        };
        let request = match parse_request(tool.method, Some(params)) {
            Ok(request) => request,
            Err(error) => {
                return Ok(tool_error(json!({
                    "error": { "code": "invalid_argument", "message": format!("Invalid arguments for {name}: {error}") }
                })));
            }
        };
        match self.server.handle(&json!(name), request, cancel.clone()) {
            Ok(response) => {
                let value = serde_json::to_value(&response).unwrap_or(Value::Null);
                Ok(json!({
                    "content": [{ "type": "text", "text": pretty(&value) }],
                    "structuredContent": value,
                }))
            }
            Err(failure) => {
                let info = failure.into_info();
                Ok(tool_error(json!({ "error": info })))
            }
        }
    }
}

/// Um resultado de ferramenta que é erro do Lace.
fn tool_error(value: Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": pretty(&value) }],
        "isError": true,
    })
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn reply(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_reply(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Os nomes em `"$ref": "#/$defs/<nome>"` dentro de `value`.
fn collect_refs(value: &Value, into: &mut BTreeSet<String>) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields {
                if key == "$ref"
                    && let Some(name) = field.as_str().and_then(|r| r.strip_prefix("#/$defs/"))
                {
                    into.insert(name.to_owned());
                } else {
                    collect_refs(field, into);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|i| collect_refs(i, into)),
        _ => {}
    }
}

/// Uma doc do Rust como descrição: sem os links (`` [`X`] `` vira `X`) e numa
/// linha só.
fn clean(doc: &str) -> String {
    let mut text = String::with_capacity(doc.len());
    let mut rest = doc;
    while let Some(start) = rest.find("[`") {
        text.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("`]") {
            Some(end) => {
                let name = &after[..end];
                text.push_str(name.rsplit("::").next().unwrap_or(name));
                rest = &after[end + 2..];
            }
            None => {
                text.push_str("[`");
                rest = after;
            }
        }
    }
    text.push_str(rest);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(allow_update: bool) -> Mcp {
        let args = ToolchainArgs {
            toolchain: None,
            compiler: None,
            quartus: None,
        };
        Mcp::new(Server::new(args, Writer::silent()), allow_update)
    }

    fn line(mcp: &Mcp, text: &str) -> Value {
        mcp.handle_line(text, &CancelToken::new())
            .expect("uma resposta")
    }

    #[test]
    fn initialize_ping_and_unknown_methods() {
        let mcp = server(false);
        let reply = line(
            &mcp,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"t"}}}"#,
        );
        assert_eq!(reply["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(reply["result"]["serverInfo"]["name"], "lace");
        assert!(reply["result"]["capabilities"]["tools"].is_object());
        assert!(
            mcp.handle_line(
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                &CancelToken::new()
            )
            .is_none()
        );
        assert_eq!(
            line(&mcp, r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#)["result"],
            json!({})
        );
        assert_eq!(
            line(
                &mcp,
                r#"{"jsonrpc":"2.0","id":3,"method":"resources/list"}"#
            )["error"]["code"],
            -32601
        );
        assert_eq!(line(&mcp, "nope")["error"]["code"], -32700);
    }

    #[test]
    fn every_tool_has_a_schema_from_the_protocol() {
        let mcp = server(false);
        let tools = mcp.tools();
        assert_eq!(
            tools.len(),
            TOOLS.iter().filter(|t| !t.installation).count(),
            "toda ferramenta acha o método dela no schema"
        );
        for tool in &tools {
            let name = tool["name"].as_str().unwrap();
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "{name}"
            );
            assert!(!tool["description"].as_str().unwrap().is_empty(), "{name}");
            assert!(
                !tool["description"].as_str().unwrap().contains("[`"),
                "{name}: links na descrição"
            );
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object", "{name}");
            assert!(schema["properties"].is_object(), "{name}");
        }
        let build = tools.iter().find(|t| t["name"] == "build").unwrap();
        let props = build["inputSchema"]["properties"].as_object().unwrap();
        assert!(props.contains_key("project") && props.contains_key("processors"));
        assert!(!props.contains_key("flow"));
        assert_eq!(build["inputSchema"]["required"], json!(["project"]));
        let sim = tools.iter().find(|t| t["name"] == "sim").unwrap();
        assert_eq!(
            sim["inputSchema"]["required"],
            json!(["project", "simulator"])
        );
        assert!(sim["inputSchema"]["$defs"]["Simulator"].is_object());
        let add = tools.iter().find(|t| t["name"] == "processor_add").unwrap();
        assert!(add["inputSchema"]["$defs"]["NewProcessor"].is_object());
        assert!(add["inputSchema"]["$defs"]["Language"].is_object());
        assert!(tools.iter().all(|t| t["name"] != "update_apply"));
        assert!(
            server(true)
                .tools()
                .iter()
                .any(|t| t["name"] == "update_apply")
        );
    }

    #[test]
    fn tool_calls_return_the_result_or_the_lace_error() {
        let mcp = server(false);
        let reply = line(
            &mcp,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"toolchain_info","arguments":{}}}"#,
        );
        let result = &reply["result"];
        assert!(result.get("isError").is_none(), "{reply}");
        assert!(result["structuredContent"]["found"].is_boolean(), "{reply}");
        assert_eq!(result["content"][0]["type"], "text");

        let missing = std::env::temp_dir().join("lace-mcp-nao-existe");
        let missing = missing.to_str().unwrap().replace('\\', "/");
        let reply = line(
            &mcp,
            &format!(
                r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"project_status","arguments":{{"project":"{missing}"}}}}}}"#
            ),
        );
        assert_eq!(reply["result"]["isError"], true, "{reply}");
        let text: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert!(text["error"]["code"].is_string(), "{text}");

        let reply = line(
            &mcp,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"build","arguments":{"processors":"x"}}}"#,
        );
        assert_eq!(reply["result"]["isError"], true, "{reply}");
        assert!(
            reply["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Invalid arguments"),
            "{reply}"
        );

        let reply = line(
            &mcp,
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"update_apply","arguments":{}}}"#,
        );
        assert_eq!(reply["error"]["code"], -32602, "{reply}");
    }

    #[test]
    fn docs_lose_their_links() {
        assert_eq!(
            clean("O retrato do projeto. Resposta: [`ProjectStatus`].\nMais."),
            "O retrato do projeto. Resposta: ProjectStatus. Mais."
        );
        assert_eq!(clean("[`crate::a::B`] e [`C"), "B e [`C");
    }
}
