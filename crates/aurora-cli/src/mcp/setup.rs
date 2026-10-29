//! `aurora mcp setup`: registra o servidor MCP da AURORA num cliente, com o
//! caminho certo do `aurora` e do bundle, em vez de a pessoa escrever a
//! configuração à mão.

use std::process::Command;

use anyhow::{Context, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::{Map, Value, json};

use crate::settings::ToolchainArgs;

/// As opções de `aurora mcp setup`.
#[derive(clap::Args, Debug, Clone)]
pub struct SetupArgs {
    /// Where to register the server
    #[arg(value_enum)]
    pub client: Client,
    /// Claude Code only: where the entry is saved
    #[arg(long, value_enum, default_value_t = Scope::Local)]
    pub scope: Scope,
    /// Start the server with --allow-update (update_apply and install_add)
    #[arg(long)]
    pub allow_update: bool,
    /// Start the server inside this project (aurora -C PATH mcp): the tools use it when a call names none
    #[arg(long, value_name = "PATH")]
    pub default_project: Option<Utf8PathBuf>,
    /// The config file to edit, instead of the client's own (for tests)
    #[arg(long, hide = true, value_name = "FILE")]
    pub config: Option<Utf8PathBuf>,
}

/// O cliente MCP.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Client {
    /// Claude Code: `claude mcp add` (or .mcp.json with --scope project)
    ClaudeCode,
    /// Claude Desktop: claude_desktop_config.json
    ClaudeDesktop,
    /// Only print the JSON entry, for any client
    Print,
}

/// O escopo do Claude Code.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// This folder, for you only
    Local,
    /// Every folder, for you only
    User,
    /// This folder, shared through .mcp.json
    Project,
}

impl Scope {
    fn as_str(self) -> &'static str {
        match self {
            Scope::Local => "local",
            Scope::User => "user",
            Scope::Project => "project",
        }
    }
}

/// O que o cliente precisa saber para abrir o servidor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerEntry {
    pub command: Utf8PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl ServerEntry {
    /// O `aurora` que está rodando, com `mcp` e as opções pedidas; a variável
    /// do bundle só quando ele não é o instalado ao lado do executável.
    pub(crate) fn current(
        toolchain: &ToolchainArgs,
        allow_update: bool,
        project: Option<&Utf8Path>,
    ) -> anyhow::Result<ServerEntry> {
        let exe = std::env::current_exe().context("Could not find the aurora executable")?;
        let exe = dunce::canonicalize(&exe).unwrap_or(exe);
        let command = Utf8PathBuf::from_path_buf(exe)
            .map_err(|p| anyhow::anyhow!("Path is not UTF-8: {}", p.display()))?;
        Ok(ServerEntry::new(
            command,
            toolchain.toolchain.as_deref(),
            allow_update,
            project,
        ))
    }

    pub(crate) fn new(
        command: Utf8PathBuf,
        toolchain_dir: Option<&Utf8Path>,
        allow_update: bool,
        project: Option<&Utf8Path>,
    ) -> ServerEntry {
        let mut args = Vec::new();
        if let Some(project) = project {
            args.push("-C".to_owned());
            args.push(project.to_string());
        }
        args.push("mcp".to_owned());
        if allow_update {
            args.push("--allow-update".to_owned());
        }
        let env = toolchain_dir
            .map(|dir| vec![("AURORA_TOOLCHAIN".to_owned(), dir.to_string())])
            .unwrap_or_default();
        ServerEntry { command, args, env }
    }

    /// A entrada como os clientes a leem (`command`, `args`, `env`).
    pub(crate) fn json(&self) -> Value {
        let mut entry = json!({ "command": self.command, "args": self.args });
        if !self.env.is_empty() {
            let env: Map<String, Value> = self
                .env
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            entry["env"] = Value::Object(env);
        }
        entry
    }

    /// O trecho inteiro, para colar num arquivo de configuração.
    pub(crate) fn snippet(&self) -> String {
        serde_json::to_string_pretty(&json!({ "mcpServers": { "aurora": self.json() } }))
            .unwrap_or_default()
    }

    /// Os argumentos de `claude mcp add` que registram esta entrada.
    pub(crate) fn claude_add_args(&self, scope: Scope) -> Vec<String> {
        let mut args = vec![
            "mcp".to_owned(),
            "add".to_owned(),
            "aurora".to_owned(),
            "--scope".to_owned(),
            scope.as_str().to_owned(),
        ];
        for (key, value) in &self.env {
            args.push("-e".to_owned());
            args.push(format!("{key}={value}"));
        }
        args.push("--".to_owned());
        args.push(self.command.to_string());
        args.extend(self.args.iter().cloned());
        args
    }
}

pub fn run(toolchain: &ToolchainArgs, args: &SetupArgs) -> anyhow::Result<()> {
    let project = args
        .default_project
        .as_deref()
        .map(crate::settings::absolute)
        .transpose()?;
    let entry = ServerEntry::current(toolchain, args.allow_update, project.as_deref())?;
    match args.client {
        Client::Print => {
            println!("{}", entry.snippet());
        }
        Client::ClaudeDesktop => {
            let path = match &args.config {
                Some(path) => path.clone(),
                None => desktop_config_path()?,
            };
            merge_server(&path, &entry.json())?;
            println!("Registered the aurora server in {path}");
            println!("Restart Claude Desktop to see the AURORA tools");
        }
        Client::ClaudeCode => {
            if args.scope == Scope::Project || args.config.is_some() {
                let path = match &args.config {
                    Some(path) => path.clone(),
                    None => Utf8PathBuf::from(".mcp.json"),
                };
                merge_server(&path, &entry.json())?;
                println!("Registered the aurora server in {path} (shared with the folder)");
                return Ok(());
            }
            let add = entry.claude_add_args(args.scope);
            match claude_command() {
                Some(mut command) => {
                    let status = command
                        .args(&add)
                        .status()
                        .context("Running claude mcp add")?;
                    if !status.success() {
                        bail!(
                            "claude mcp add exited with {}",
                            status
                                .code()
                                .map_or_else(|| "?".to_owned(), |c| c.to_string())
                        );
                    }
                    println!(
                        "Registered the aurora server in Claude Code ({} scope)",
                        args.scope.as_str()
                    );
                }
                None => {
                    println!("The claude command is not in the PATH; run this yourself:");
                    println!("  claude {}", shell_join(&add));
                }
            }
        }
    }
    Ok(())
}

/// Mescla `mcpServers.aurora` em `path`, mantendo o resto do arquivo. Cria a
/// pasta e o arquivo se não existem.
pub(crate) fn merge_server(path: &Utf8Path, entry: &Value) -> anyhow::Result<()> {
    let mut root: Value = match std::fs::read_to_string(path) {
        Ok(text) if !text.trim().is_empty() => serde_json::from_str(&text)
            .with_context(|| format!("{path} is not valid JSON; fix it or remove it first"))?,
        Ok(_) => json!({}),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(error).with_context(|| format!("Reading {path}")),
    };
    if !root.is_object() {
        bail!("{path} does not hold a JSON object");
    }
    let servers = root
        .as_object_mut()
        .expect("checked")
        .entry("mcpServers")
        .or_insert_with(|| json!({}));
    if !servers.is_object() {
        bail!("mcpServers in {path} is not an object");
    }
    servers["aurora"] = entry.clone();
    if let Some(parent) = path.parent().filter(|p| !p.as_str().is_empty()) {
        std::fs::create_dir_all(parent).with_context(|| format!("Creating {parent}"))?;
    }
    let text = serde_json::to_string_pretty(&root)? + "\n";
    std::fs::write(path, text).with_context(|| format!("Writing {path}"))?;
    Ok(())
}

/// O `claude_desktop_config.json` desta plataforma.
fn desktop_config_path() -> anyhow::Result<Utf8PathBuf> {
    let base = if cfg!(windows) {
        std::env::var("APPDATA")
            .ok()
            .map(Utf8PathBuf::from)
            .context("APPDATA is not set")?
    } else if cfg!(target_os = "macos") {
        home()?.join("Library/Application Support")
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|v| !v.is_empty())
            .map(Utf8PathBuf::from)
            .unwrap_or(home()?.join(".config"))
    };
    Ok(base.join("Claude").join("claude_desktop_config.json"))
}

fn home() -> anyhow::Result<Utf8PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(Utf8PathBuf::from)
        .context("HOME is not set")
}

/// O `claude` do `PATH`, pronto para receber argumentos: no Windows, o
/// `claude.exe` ou, pelo `cmd`, o `claude.cmd` do npm.
fn claude_command() -> Option<Command> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if cfg!(windows) {
            let exe = dir.join("claude.exe");
            if exe.is_file() {
                return Some(Command::new(exe));
            }
            let cmd = dir.join("claude.cmd");
            if cmd.is_file() {
                let mut command = Command::new("cmd");
                command.arg("/c").arg(cmd);
                return Some(command);
            }
        } else {
            let bin = dir.join("claude");
            if bin.is_file() {
                return Some(Command::new(bin));
            }
        }
    }
    None
}

fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.chars().any(char::is_whitespace) {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_has_the_command_the_args_and_the_env_only_when_needed() {
        let plain = ServerEntry::new("/opt/aurora/bin/aurora".into(), None, false, None);
        assert_eq!(
            plain.json(),
            json!({ "command": "/opt/aurora/bin/aurora", "args": ["mcp"] })
        );
        let full = ServerEntry::new(
            "C:/dev/aurora.exe".into(),
            Some(Utf8Path::new("C:/bundle")),
            true,
            Some(Utf8Path::new("C:/p/soma.spf")),
        );
        assert_eq!(
            full.json(),
            json!({
                "command": "C:/dev/aurora.exe",
                "args": ["-C", "C:/p/soma.spf", "mcp", "--allow-update"],
                "env": { "AURORA_TOOLCHAIN": "C:/bundle" }
            })
        );
        assert_eq!(
            full.claude_add_args(Scope::User),
            [
                "mcp",
                "add",
                "aurora",
                "--scope",
                "user",
                "-e",
                "AURORA_TOOLCHAIN=C:/bundle",
                "--",
                "C:/dev/aurora.exe",
                "-C",
                "C:/p/soma.spf",
                "mcp",
                "--allow-update"
            ]
        );
        assert!(plain.snippet().contains("\"mcpServers\""));
    }

    #[test]
    fn merging_keeps_the_rest_of_the_file_and_creates_it_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path =
            Utf8PathBuf::from_path_buf(dir.path().join("Claude").join("config.json")).unwrap();
        let entry = json!({ "command": "aurora", "args": ["mcp"] });
        merge_server(&path, &entry).unwrap();
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written, json!({ "mcpServers": { "aurora": entry } }));

        std::fs::write(
            &path,
            r#"{"preferences": {"theme": "dark"}, "mcpServers": {"other": {"command": "x"}}}"#,
        )
        .unwrap();
        let newer = json!({ "command": "lace2", "args": ["mcp", "--allow-update"] });
        merge_server(&path, &newer).unwrap();
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["preferences"]["theme"], "dark");
        assert_eq!(written["mcpServers"]["other"]["command"], "x");
        assert_eq!(written["mcpServers"]["aurora"], newer);

        std::fs::write(&path, "not json").unwrap();
        assert!(merge_server(&path, &entry).is_err());
    }
}
