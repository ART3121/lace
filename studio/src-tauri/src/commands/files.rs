//! Comandos de arquivo: a árvore, ler e gravar texto, criar, renomear,
//! mandar para a lixeira, listar e buscar.
//!
//! Ler vale para qualquer caminho, porque o usuário pode abrir um arquivo de
//! fora do projeto. Criar, renomear e apagar só valem dentro da pasta do
//! projeto aberto (erro `outside_project`), como defesa contra um caminho
//! errado vindo da interface. Apagar é mandar para a lixeira do sistema,
//! como a AURORA, nunca apagar de vez.

use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use camino::{Utf8Path, Utf8PathBuf};
use regex::RegexBuilder;
use serde::Serialize;
use tauri::AppHandle;

use crate::error::{IpcError, IpcResult, codes};
use crate::state::{AppState, blocking};

/// Maior arquivo que o editor abre, em bytes.
const MAX_TEXT_BYTES: u64 = 16 * 1024 * 1024;
/// Maior arquivo em que a busca procura, em bytes.
const MAX_SEARCH_BYTES: u64 = 2 * 1024 * 1024;
/// Quantos arquivos a lista do "abrir rápido" traz, no máximo.
const MAX_LISTED_FILES: usize = 20_000;
/// Profundidade máxima das varreduras.
const MAX_DEPTH: usize = 16;

/// Pastas que as varreduras (lista e busca) pulam, além das ocultas.
const SKIPPED_DIRS: &[&str] = &["node_modules", "target", "obj_dir", "__pycache__"];

/// Uma entrada de pasta.
#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    /// O nome.
    pub name: String,
    /// O caminho absoluto.
    pub path: Utf8PathBuf,
    /// É pasta.
    pub is_dir: bool,
    /// Começa com `.`.
    pub hidden: bool,
    /// Arquivo com nome de testbench (`tb_<nome>.v`, `<nome>_tb.v`), que
    /// não pode ser o topo (`verilog::is_testbench_name` do Core).
    pub testbench_name: bool,
}

/// O conteúdo de uma pasta: pastas primeiro, depois arquivos, cada grupo em
/// ordem alfabética sem distinguir maiúsculas.
#[tauri::command]
pub async fn fs_read_dir(app: AppHandle, path: String) -> IpcResult<Vec<DirEntry>> {
    blocking(app, move |_, _| {
        let dir = Utf8PathBuf::from(path);
        let mut entries: Vec<DirEntry> = dir
            .read_dir_utf8()
            .map_err(|e| IpcError::io(&dir, e))?
            .filter_map(Result::ok)
            .map(|entry| {
                let name = entry.file_name().to_owned();
                let is_dir = entry.path().is_dir();
                DirEntry {
                    hidden: name.starts_with('.'),
                    testbench_name: !is_dir && lace_core::verilog::is_testbench_name(&name),
                    is_dir,
                    path: entry.path().to_owned(),
                    name,
                }
            })
            .collect();
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    })
    .await
}

/// Um arquivo de texto aberto.
#[derive(Debug, Clone, Serialize)]
pub struct TextFile {
    /// O caminho.
    pub path: Utf8PathBuf,
    /// O conteúdo. Vazio quando `binary` ou `too_large`.
    pub content: String,
    /// Horário de modificação, em ms desde 1970, para detectar mudança por
    /// fora antes de gravar.
    pub modified_ms: u64,
    /// Não é texto UTF-8.
    pub binary: bool,
    /// Passa de 16 MiB.
    pub too_large: bool,
}

/// Lê um arquivo de texto.
#[tauri::command]
pub async fn fs_read_text(app: AppHandle, path: String) -> IpcResult<TextFile> {
    blocking(app, move |_, _| {
        let path = Utf8PathBuf::from(path);
        let meta = std::fs::metadata(&path).map_err(|e| IpcError::io(&path, e))?;
        let modified_ms = modified_ms(&meta);
        if meta.len() > MAX_TEXT_BYTES {
            return Ok(TextFile {
                path,
                content: String::new(),
                modified_ms,
                binary: false,
                too_large: true,
            });
        }
        let bytes = std::fs::read(&path).map_err(|e| IpcError::io(&path, e))?;
        let probe = &bytes[..bytes.len().min(8192)];
        let (content, binary) = if probe.contains(&0) {
            (String::new(), true)
        } else {
            match String::from_utf8(bytes) {
                Ok(text) => (text, false),
                Err(_) => (String::new(), true),
            }
        };
        Ok(TextFile {
            path,
            content,
            modified_ms,
            binary,
            too_large: false,
        })
    })
    .await
}

/// Grava um arquivo de texto e devolve o novo horário de modificação.
///
/// Com `expected_modified_ms`, recusa (`conflict`) se o arquivo mudou no
/// disco desde que foi lido: alguém o alterou por fora, e gravar perderia
/// essa mudança. A interface pergunta e chama de novo sem o campo.
#[tauri::command]
pub async fn fs_write_text(
    app: AppHandle,
    path: String,
    content: String,
    expected_modified_ms: Option<u64>,
) -> IpcResult<u64> {
    blocking(app, move |_, _| {
        let path = Utf8PathBuf::from(path);
        if let (Some(expected), Ok(meta)) = (expected_modified_ms, std::fs::metadata(&path))
            && modified_ms(&meta) != expected
        {
            return Err(IpcError::new(
                codes::CONFLICT,
                format!("{path} changed on disk since it was opened"),
            ));
        }
        std::fs::write(&path, content).map_err(|e| IpcError::io(&path, e))?;
        let meta = std::fs::metadata(&path).map_err(|e| IpcError::io(&path, e))?;
        Ok(modified_ms(&meta))
    })
    .await
}

/// Situação de um caminho no disco.
#[derive(Debug, Clone, Serialize)]
pub struct FileStat {
    /// É pasta.
    pub is_dir: bool,
    /// Horário de modificação, em ms desde 1970.
    pub modified_ms: u64,
    /// Tamanho, em bytes.
    pub size: u64,
}

/// A situação de um caminho, ou `None` se ele não existe.
#[tauri::command]
pub async fn fs_stat(app: AppHandle, path: String) -> IpcResult<Option<FileStat>> {
    blocking(app, move |_, _| {
        Ok(std::fs::metadata(&path).ok().map(|meta| FileStat {
            is_dir: meta.is_dir(),
            modified_ms: modified_ms(&meta),
            size: meta.len(),
        }))
    })
    .await
}

/// Cria um arquivo, vazio ou com `content`. Recusa se ele já existir.
#[tauri::command]
pub async fn fs_create_file(
    app: AppHandle,
    path: String,
    content: Option<String>,
) -> IpcResult<()> {
    blocking(app, move |_, state| {
        let path = Utf8PathBuf::from(path);
        ensure_inside(state, &path)?;
        if path.exists() {
            return Err(IpcError::new(
                codes::EXISTS,
                format!("{path} already exists"),
            ));
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| IpcError::io(dir, e))?;
        }
        std::fs::write(&path, content.unwrap_or_default()).map_err(|e| IpcError::io(&path, e))
    })
    .await
}

/// Cria uma pasta (e as de cima que faltarem).
#[tauri::command]
pub async fn fs_create_dir(app: AppHandle, path: String) -> IpcResult<()> {
    blocking(app, move |_, state| {
        let path = Utf8PathBuf::from(path);
        ensure_inside(state, &path)?;
        if path.exists() {
            return Err(IpcError::new(
                codes::EXISTS,
                format!("{path} already exists"),
            ));
        }
        std::fs::create_dir_all(&path).map_err(|e| IpcError::io(&path, e))
    })
    .await
}

/// Renomeia ou move. Recusa se o destino existir.
#[tauri::command]
pub async fn fs_rename(app: AppHandle, from: String, to: String) -> IpcResult<()> {
    blocking(app, move |_, state| {
        let (from, to) = (Utf8PathBuf::from(from), Utf8PathBuf::from(to));
        ensure_inside(state, &from)?;
        ensure_inside(state, &to)?;
        if to.exists() {
            return Err(IpcError::new(codes::EXISTS, format!("{to} already exists")));
        }
        std::fs::rename(&from, &to).map_err(|e| IpcError::io(&from, e))
    })
    .await
}

/// Manda um arquivo ou pasta para a lixeira do sistema.
#[tauri::command]
pub async fn fs_trash(app: AppHandle, path: String) -> IpcResult<()> {
    blocking(app, move |_, state| {
        let path = Utf8PathBuf::from(path);
        ensure_inside(state, &path)?;
        trash::delete(&path).map_err(|e| IpcError::new(codes::IO, format!("{path}: {e}")))
    })
    .await
}

/// Copia `from` (arquivo ou pasta, de qualquer lugar) para dentro de
/// `to_dir`, que precisa estar no projeto: o que arrastar do gerenciador de
/// arquivos do sistema faz. Com o mesmo nome já no destino, o erro é
/// `exists`; com `overwrite`, o que estava lá vai para a lixeira antes.
/// Devolve o caminho novo.
#[tauri::command]
pub async fn fs_copy(
    app: AppHandle,
    from: String,
    to_dir: String,
    overwrite: bool,
) -> IpcResult<Utf8PathBuf> {
    blocking(app, move |_, state| {
        let (from, to_dir) = (Utf8PathBuf::from(from), Utf8PathBuf::from(to_dir));
        ensure_within(state, &to_dir, true)?;
        let name = from.file_name().ok_or_else(|| {
            IpcError::new(codes::INVALID_ARGUMENT, format!("{from} has no file name"))
        })?;
        let to = to_dir.join(name);
        // Soltar um arquivo na pasta onde ele já está não faz nada (com
        // `overwrite`, mandaria o próprio original para a lixeira).
        if canonical(&to) == canonical(&from) {
            return Ok(to);
        }
        if from.is_dir() && canonical(&to_dir).starts_with(canonical(&from)) {
            return Err(IpcError::new(
                codes::INVALID_ARGUMENT,
                format!("{from} cannot be copied into itself"),
            ));
        }
        replace_target(&to, overwrite)?;
        copy_recursive(&from, &to)?;
        Ok(to)
    })
    .await
}

/// Abre caminho para `to`: erro `exists` se ele existe, ou, com
/// `overwrite`, manda o que está lá para a lixeira.
pub(crate) fn replace_target(to: &Utf8Path, overwrite: bool) -> IpcResult<()> {
    if !to.exists() {
        return Ok(());
    }
    if !overwrite {
        return Err(IpcError::new(codes::EXISTS, format!("{to} already exists")));
    }
    trash::delete(to).map_err(|e| IpcError::new(codes::IO, format!("{to}: {e}")))
}

/// Copia um arquivo, ou uma pasta com tudo dentro.
pub(crate) fn copy_recursive(from: &Utf8Path, to: &Utf8Path) -> IpcResult<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to).map_err(|e| IpcError::io(to, e))?;
        for entry in from.read_dir_utf8().map_err(|e| IpcError::io(from, e))? {
            let entry = entry.map_err(|e| IpcError::io(from, e))?;
            copy_recursive(entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to)
            .map(|_| ())
            .map_err(|e| IpcError::io(from, e))
    }
}

/// Todos os arquivos do projeto, para o "abrir rápido" (Ctrl+P). Pula as
/// pastas ocultas (inclusive `.lace/`) e as de [`SKIPPED_DIRS`].
#[tauri::command]
pub async fn fs_list_files(app: AppHandle) -> IpcResult<Vec<Utf8PathBuf>> {
    blocking(app, |_, state| {
        let root = project_root(state)?;
        let mut files = Vec::new();
        walk(&root, 0, &mut |path| {
            files.push(path.to_owned());
            files.len() < MAX_LISTED_FILES
        });
        Ok(files)
    })
    .await
}

/// Uma ocorrência da busca.
#[derive(Debug, Clone, Serialize)]
pub struct SearchMatch {
    /// O arquivo.
    pub path: Utf8PathBuf,
    /// A linha, a partir de 1.
    pub line: u32,
    /// A coluna do começo da ocorrência, a partir de 1, em caracteres.
    pub column: u32,
    /// O comprimento da ocorrência, em caracteres.
    pub length: u32,
    /// O texto da linha (cortado em 400 caracteres).
    pub text: String,
}

/// Busca um texto (ou expressão regular) em todos os arquivos de texto do
/// projeto. Para em `max_results` ocorrências.
#[tauri::command]
pub async fn fs_search(
    app: AppHandle,
    query: String,
    regex: bool,
    case_sensitive: bool,
    max_results: Option<usize>,
) -> IpcResult<Vec<SearchMatch>> {
    blocking(app, move |_, state| {
        let root = project_root(state)?;
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let pattern = if regex { query } else { regex::escape(&query) };
        let matcher = RegexBuilder::new(&pattern)
            .case_insensitive(!case_sensitive)
            .build()
            .map_err(|e| IpcError::new(codes::INVALID_ARGUMENT, e.to_string()))?;
        let limit = max_results.unwrap_or(2000);
        let mut found = Vec::new();
        walk(&root, 0, &mut |path| {
            let small = std::fs::metadata(path).is_ok_and(|m| m.len() <= MAX_SEARCH_BYTES);
            if let Some(text) = small.then(|| std::fs::read_to_string(path).ok()).flatten() {
                for (index, line) in text.lines().enumerate() {
                    for m in matcher.find_iter(line) {
                        found.push(SearchMatch {
                            path: path.to_owned(),
                            line: index as u32 + 1,
                            column: line[..m.start()].chars().count() as u32 + 1,
                            length: m.as_str().chars().count() as u32,
                            text: line.chars().take(400).collect(),
                        });
                        if found.len() >= limit {
                            return false;
                        }
                    }
                }
            }
            true
        });
        Ok(found)
    })
    .await
}

/// Visita os arquivos de `dir`, recursivamente, até `visit` devolver
/// `false`. Devolve `false` quando parou.
fn walk(dir: &Utf8Path, depth: usize, visit: &mut dyn FnMut(&Utf8Path) -> bool) -> bool {
    if depth > MAX_DEPTH {
        return true;
    }
    let Ok(entries) = dir.read_dir_utf8() else {
        return true;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by(|a, b| a.file_name().cmp(b.file_name()));
    for entry in entries {
        let name = entry.file_name();
        let path = entry.path();
        if path.is_dir() {
            if name.starts_with('.') || SKIPPED_DIRS.contains(&name) {
                continue;
            }
            if !walk(path, depth + 1, visit) {
                return false;
            }
        } else if !visit(path) {
            return false;
        }
    }
    true
}

/// A pasta do projeto aberto.
fn project_root(state: &AppState) -> IpcResult<Utf8PathBuf> {
    let spf = state.spf()?;
    Ok(spf.parent().map(Utf8Path::to_owned).unwrap_or(spf))
}

/// Recusa um caminho fora da pasta do projeto (ou a própria pasta).
pub(crate) fn ensure_inside(state: &AppState, path: &Utf8Path) -> IpcResult<()> {
    ensure_within(state, path, false)
}

/// Recusa um caminho fora da pasta do projeto; a própria pasta vale com
/// `allow_root` (o destino de arrastar para a raiz).
pub(crate) fn ensure_within(state: &AppState, path: &Utf8Path, allow_root: bool) -> IpcResult<()> {
    let root = canonical(&project_root(state)?);
    if is_within(&root, path, allow_root) {
        Ok(())
    } else {
        Err(IpcError::new(
            codes::OUTSIDE_PROJECT,
            format!("{path} is outside the project folder"),
        ))
    }
}

/// `path` fica dentro de `root` (já canônica)? A pasta de `path` é resolvida
/// e o nome não: um link do projeto que aponta para fora ainda pode ser
/// renomeado ou ir para a lixeira. A pasta pode ainda não existir ("Novo
/// arquivo" em `rtl/modulo.v` num projeto sem `rtl/`).
fn is_within(root: &Path, path: &Utf8Path, allow_root: bool) -> bool {
    let path = lexical(path.as_std_path());
    let target = match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => canonical_path(parent).map(|p| p.join(name)),
        _ => canonical_path(&path),
    };
    target.is_some_and(|t| t.starts_with(root) && (allow_root || t != root))
}

/// [`canonical_path`] de `path`; se nada dele existe, `path` sem `.` nem
/// `..`.
fn canonical(path: &Utf8Path) -> PathBuf {
    let path = lexical(path.as_std_path());
    canonical_path(&path).unwrap_or(path)
}

/// O caminho canônico, mesmo que o fim dele ainda não exista: o ancestral
/// mais fundo que existe é resolvido (links, e no Windows a forma `\\?\` e a
/// caixa do disco) e o resto vem como está. Com raiz e alvo na mesma forma,
/// `starts_with` compara os dois; antes, uma pasta nova saía sem o `\\?\` da
/// raiz e parecia fora do projeto. `None` se nada do caminho existe.
fn canonical_path(path: &Path) -> Option<PathBuf> {
    let mut missing = Vec::new();
    let mut existing = path;
    loop {
        if let Ok(found) = std::fs::canonicalize(existing) {
            return Some(missing.iter().rev().fold(found, |acc, name| acc.join(name)));
        }
        missing.push(existing.file_name()?);
        existing = existing.parent()?;
    }
}

/// Tira `.` e resolve `..` sem tocar no disco (`proj/x/../a.v` vira
/// `proj/a.v`), como o Core faz com os caminhos do projeto. Sem isso, um
/// `proj/x/../../fora/a.v` com `x` inexistente passava por dentro do
/// projeto. Um `..` acima da raiz do disco fica na raiz.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else if !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// O horário de modificação em ms desde 1970 (0 se o sistema não informa).
fn modified_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf8(path: &Path) -> &Utf8Path {
        Utf8Path::from_path(path).expect("caminho UTF-8")
    }

    #[test]
    fn a_new_folder_inside_the_project_is_inside() {
        let dir = tempfile::tempdir().unwrap();
        let base = utf8(dir.path());
        let root = canonical(base);
        // "Novo arquivo" em rtl/modulo.v num projeto sem rtl/: no Windows a
        // raiz canônica tem a forma \?\, e a pasta nova saía sem ela.
        assert!(is_within(&root, &base.join("rtl").join("modulo.v"), false));
        assert!(is_within(
            &root,
            &base.join("a").join("b").join("c.v"),
            false
        ));
        assert!(is_within(
            &root,
            &base.join("novo").join("..").join("a.v"),
            false
        ));
        // A própria pasta só vale com `allow_root`.
        assert!(!is_within(&root, base, false));
        assert!(is_within(&root, base, true));
    }

    #[test]
    fn dot_dot_through_a_missing_folder_stays_outside() {
        let dir = tempfile::tempdir().unwrap();
        let base = utf8(dir.path());
        let project = base.join("proj");
        std::fs::create_dir(&project).unwrap();
        let root = canonical(&project);
        let escape = project
            .join("x")
            .join("..")
            .join("..")
            .join("fora")
            .join("a.v");
        assert!(!is_within(&root, &escape, false));
        assert!(!is_within(&root, &base.join("fora.v"), false));
        assert!(is_within(&root, &project.join("x").join("a.v"), false));
    }

    #[test]
    fn lexical_resolves_dots_without_the_disk() {
        assert_eq!(lexical(Path::new("a/./b/../c.v")), Path::new("a/c.v"));
        assert_eq!(lexical(Path::new("../a")), Path::new("../a"));
        assert_eq!(lexical(Path::new("/../a")), Path::new("/a"));
    }
}
