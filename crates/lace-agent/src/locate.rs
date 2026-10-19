//! Onde está o Claude Code. Quem chama passa o caminho das preferências, se
//! houver; senão a busca é pelo `PATH` e pelas pastas em que os
//! instaladores o põem: `~/.local/bin` (o instalador oficial),
//! `%APPDATA%\npm` e `~/.npm-global/bin` (o `npm install -g`),
//! `/usr/local/bin` e `/opt/homebrew/bin`.

use std::process::Stdio;

use camino::{Utf8Path, Utf8PathBuf};

/// Um executável do Claude Code e como rodá-lo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claude {
    /// O programa a iniciar.
    pub program: Utf8PathBuf,
    /// Argumentos antes dos do Claude Code: vazio para o executável em si;
    /// o `cli.js` quando o que há é o `claude.cmd` do npm, que roda pelo
    /// `node`, sem o `cmd.exe` no meio (aspas em JSON não passam por ele).
    pub prefix: Vec<String>,
}

impl Claude {
    /// O caminho para mostrar.
    pub fn display(&self) -> String {
        self.prefix
            .last()
            .cloned()
            .unwrap_or_else(|| self.program.to_string())
    }
}

/// Os nomes que o executável pode ter nesta plataforma.
fn names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["claude.exe", "claude.cmd", "claude"]
    } else {
        &["claude"]
    }
}

/// Acha o Claude Code: `configured` (um executável, ou a pasta dele) vale
/// primeiro; depois o `PATH` e as pastas dos instaladores. `None` quando
/// não há.
pub fn find_claude(configured: Option<&Utf8Path>) -> Option<Claude> {
    if let Some(given) = configured.filter(|p| !p.as_str().trim().is_empty()) {
        if given.is_file() {
            return Some(wrap(given.to_owned()));
        }
        if given.is_dir() {
            for name in names() {
                let candidate = given.join(name);
                if candidate.is_file() {
                    return Some(wrap(candidate));
                }
            }
        }
        // Declarado e ausente: não cai para o PATH, como o `--toolchain`.
        return None;
    }
    let mut dirs: Vec<Utf8PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(
            std::env::split_paths(&path).filter_map(|p| Utf8PathBuf::from_path_buf(p).ok()),
        );
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .and_then(|h| Utf8PathBuf::from_path_buf(h.into()).ok());
    if let Some(home) = &home {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".npm-global").join("bin"));
    }
    if cfg!(windows) {
        if let Some(appdata) =
            std::env::var_os("APPDATA").and_then(|a| Utf8PathBuf::from_path_buf(a.into()).ok())
        {
            dirs.push(appdata.join("npm"));
        }
    } else {
        dirs.push("/usr/local/bin".into());
        dirs.push("/opt/homebrew/bin".into());
    }
    for dir in dirs {
        for name in names() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(wrap(candidate));
            }
        }
    }
    None
}

/// O executável como programa: o `claude.cmd` do npm vira `node cli.js`,
/// se o `cli.js` está ao lado e há `node`; senão fica como está, e o
/// sistema o roda pelo `cmd.exe`.
fn wrap(path: Utf8PathBuf) -> Claude {
    let is_cmd = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    if is_cmd && let Some(dir) = path.parent() {
        let cli = dir
            .join("node_modules")
            .join("@anthropic-ai")
            .join("claude-code")
            .join("cli.js");
        if cli.is_file()
            && let Some(node) = find_node()
        {
            return Claude {
                program: node,
                prefix: vec![cli.to_string()],
            };
        }
    }
    Claude {
        program: path,
        prefix: Vec::new(),
    }
}

fn find_node() -> Option<Utf8PathBuf> {
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter_map(|p| Utf8PathBuf::from_path_buf(p).ok())
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// A versão do Claude Code (`claude --version`), como `2.1.0 (Claude
/// Code)`; `None` se ele não rodou.
pub fn version(claude: &Claude) -> Option<String> {
    let mut command = std::process::Command::new(claude.program.as_std_path());
    command
        .args(&claude.prefix)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    lace_core::hide_console(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declared_folder_or_file_is_used_and_a_missing_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8PathBuf::from_path_buf(dir.path().to_owned()).unwrap();
        assert_eq!(find_claude(Some(&dir)), None);
        let exe = dir.join(names()[0]);
        std::fs::write(&exe, "").unwrap();
        assert_eq!(find_claude(Some(&dir)).unwrap().program, exe);
        assert_eq!(find_claude(Some(&exe)).unwrap().program, exe);
        assert_eq!(find_claude(Some(&dir.join("nao"))), None);
    }

    #[test]
    fn an_npm_wrapper_without_node_beside_stays_a_wrapper() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8PathBuf::from_path_buf(dir.path().to_owned()).unwrap();
        let cmd = dir.join("claude.cmd");
        std::fs::write(&cmd, "").unwrap();
        let claude = wrap(cmd.clone());
        assert_eq!(claude.program, cmd);
        assert!(claude.prefix.is_empty());
        assert_eq!(claude.display(), cmd.as_str());
    }
}
