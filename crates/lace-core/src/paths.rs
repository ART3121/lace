//! Utilitários de caminho compartilhados pelo Core.

use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};

use crate::error::{LaceError, Result};

/// Maior caminho completo que é seguro entregar ao YANC, em bytes.
///
/// Dois limites valem ao mesmo tempo, e o menor manda:
///
/// - **Buffers do YANC.** Os compiladores montam caminhos com `snprintf` em
///   buffers fixos; o menor tem 1001 bytes (`appcomp`, `eval.c`). Estourar
///   trunca o caminho em silêncio e o compilador abre o arquivo errado.
/// - **MAX_PATH do Windows.** Os `.exe` do YANC usam o runtime C do mingw, que
///   não é "long path aware": `fopen` falha acima de 259 caracteres. É o
///   comportamento documentado do Windows para programas sem o manifesto
///   `longPathAware`.
///
/// O Lace confere cada arquivo que um compilador vai abrir ou criar (fonte,
/// `.asm`, Verilog, memórias, testbench, intermediários), não só os
/// diretórios.
#[cfg(windows)]
pub const YANC_PATH_LIMIT: usize = 259;
/// Maior caminho completo que é seguro entregar ao YANC, em bytes (ver a
/// versão do Windows, onde o limite é menor).
#[cfg(not(windows))]
pub const YANC_PATH_LIMIT: usize = 1000;

/// Folga para arquivos da toolchain que o YANC abre por nome dentro de um
/// diretório (`HDL/instr_dec.v`, `Macros/float_sin.asm`): o diretório
/// precisa caber com um nome desse tamanho.
pub(crate) const TOOLCHAIN_FILE_MARGIN: usize = 40;

/// `canonicalize` sem o prefixo `\\?\` no Windows (os compiladores C não o
/// entendem) e com conversão para UTF-8.
pub(crate) fn canonicalize(path: &Utf8Path) -> Result<Utf8PathBuf> {
    let canonical = dunce::canonicalize(path).map_err(LaceError::io("Resolving path", path))?;
    to_utf8(canonical)
}

pub(crate) fn to_utf8(path: impl AsRef<Path>) -> Result<Utf8PathBuf> {
    let path = path.as_ref();
    Utf8PathBuf::from_path_buf(path.to_owned())
        .map_err(|p| LaceError::NonUtf8Path(p.display().to_string()))
}

/// A pasta de cache do Lace, só do usuário (o `/tmp` do Linux é de todos):
/// `$XDG_CACHE_HOME/lace` ou `~/.cache/lace` no Linux,
/// `~/Library/Caches/lace` no macOS, `%TEMP%\lace` no Windows. Não é
/// criada aqui.
pub(crate) fn user_cache_dir() -> Result<Utf8PathBuf> {
    let home = || std::env::var_os("HOME").filter(|h| !h.is_empty());
    let base = if cfg!(windows) {
        None
    } else if cfg!(target_os = "macos") {
        home().map(|h| std::path::PathBuf::from(h).join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home().map(|h| std::path::PathBuf::from(h).join(".cache")))
    };
    let base = base.unwrap_or_else(std::env::temp_dir);
    to_utf8(base.join("lace"))
}

/// Recusa um caminho com caractere fora do ASCII onde o simulador abre
/// arquivo por nome. O YANC grava no Verilog e no testbench o caminho
/// absoluto das memórias (`<proc>/Hardware/<proc>_inst.mif`) e das entradas
/// (`<proc>/Simulation/input_<n>.txt`), e o `vvp` do Icarus recusa nome de
/// arquivo com byte fora do ASCII ("contains non-printable characters"): o
/// processador rodaria sem programa e sem entradas, sem erro.
pub(crate) fn check_simulator_path(path: &Utf8Path) -> Result<()> {
    if path.as_str().is_ascii() {
        Ok(())
    } else {
        Err(LaceError::NonAsciiPath {
            path: path.to_owned(),
        })
    }
}

/// Recusa caminhos que estourariam os buffers do YANC ou o MAX_PATH.
pub(crate) fn check_yanc_limit(path: &Utf8Path) -> Result<()> {
    check_yanc_limit_with_margin(path, 0)
}

/// Como [`check_yanc_limit`], deixando `margin` bytes para um nome de
/// arquivo que o YANC acrescenta ao diretório.
pub(crate) fn check_yanc_limit_with_margin(path: &Utf8Path, margin: usize) -> Result<()> {
    let len = path.as_str().len() + margin;
    if len > YANC_PATH_LIMIT {
        return Err(LaceError::PathTooLong {
            path: path.to_owned(),
            len,
            limit: YANC_PATH_LIMIT,
        });
    }
    Ok(())
}

/// Caminho que não depende do CWD: absoluto no sistema atual, absoluto do
/// Windows (`C:\x`), ou começando por `/` ou `\` (no Windows, relativo à raiz
/// do drive atual, mas nunca ao CWD).
pub(crate) fn is_rooted(path: &str) -> bool {
    Utf8Path::new(path).is_absolute() || is_windows_absolute(path) || path.starts_with(['/', '\\'])
}

/// `C:\x` ou `C:/x`, reconhecido em qualquer sistema (um `.spf` gravado no
/// Windows pode ser aberto no Linux).
pub(crate) fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() > 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

/// Um caminho que uma ferramenta escreveu, com o separador do sistema: no
/// Windows, `/` vira `\`. O Icarus recebe os fontes com `/`
/// ([`icarus_path`](crate::process::icarus_path)) e os devolve assim nas
/// mensagens e no `.vvp`. Com o separador nativo, cada arquivo tem um só nome
/// nos resultados, igual ao das outras operações (o Studio abre o arquivo de
/// um diagnóstico pelo caminho, e um nome com `/` viraria outra aba).
pub(crate) fn native_separators(path: &str) -> Utf8PathBuf {
    if cfg!(windows) {
        Utf8PathBuf::from(path.replace('/', "\\"))
    } else {
        Utf8PathBuf::from(path)
    }
}

/// Caminho relativo que não sai do diretório base: sem raiz, sem prefixo de
/// drive e sem `..`.
pub(crate) fn is_contained(path: &Utf8Path) -> bool {
    !is_rooted(path.as_str())
        && path.components().all(|c| {
            matches!(
                c,
                camino::Utf8Component::Normal(_) | camino::Utf8Component::CurDir
            )
        })
}

/// Uma trava de execução, solta quando sai de escopo.
pub(crate) struct RunLock(#[allow(dead_code)] std::fs::File);

/// Trava `<base>.lock` (a pasta temporária de um processador, ou a do
/// projeto) para um build ou uma simulação. Sem esperar: se outra operação
/// do Lace a segura, de outro processo ou desta mesma (o Studio e a CLI),
/// [`LaceError::OperationInProgress`] com `name`.
pub(crate) fn run_lock(base: &Utf8Path, name: &str) -> Result<RunLock> {
    let path = base.with_extension("lock");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(LaceError::io("Creating directory", parent))?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(LaceError::io("Locking", &path))?;
    match file.try_lock() {
        Ok(()) => Ok(RunLock(file)),
        Err(std::fs::TryLockError::WouldBlock) => Err(LaceError::OperationInProgress {
            name: name.to_owned(),
        }),
        Err(std::fs::TryLockError::Error(e)) => Err(LaceError::io("Locking", &path)(e)),
    }
}

/// A pasta de topo do repositório git que contém `dir`: a primeira, de `dir`
/// para cima, com `.git` (pasta, ou arquivo num worktree ou submódulo).
pub(crate) fn repository_top(dir: &Utf8Path) -> Option<&Utf8Path> {
    dir.ancestors().find(|d| d.join(".git").exists())
}

/// `path` relativo a `root` com `..` (`../../rtl/x.v`, com `/`), quando os
/// dois estão no mesmo repositório git; `None` fora de repositório ou em
/// repositórios diferentes. Os dois caminhos são absolutos e canônicos.
///
/// É a regra do `.spf` para arquivo de fora da pasta do projeto:
/// dentro do mesmo repositório, o caminho relativo vale em qualquer clone.
pub(crate) fn relative_in_repository(root: &Utf8Path, path: &Utf8Path) -> Option<String> {
    let top = repository_top(root)?;
    let inside = path.strip_prefix(top).ok()?;
    let up = root.strip_prefix(top).ok()?.components().count();
    let parts: Vec<&str> = std::iter::repeat_n("..", up)
        .chain(inside.components().map(|c| c.as_str()))
        .collect();
    Some(parts.join("/"))
}

/// O caminho de hoje para um absoluto gravado em outra máquina, como a
/// AURORA (`resgatarPelaCauda`, `js/project/caminho_de_projeto.ts`): tenta,
/// dentro de `root`, as caudas de `recorded`, da mais longa para a mais curta
/// (`C:\velho\proj\rtl\x.v` tenta `velho/proj/rtl/x.v`, `proj/rtl/x.v`,
/// `rtl/x.v` e `x.v`). O primeiro arquivo que existir ganha. Nunca tenta o
/// caminho inteiro, que começa pela letra do drive ou pela raiz.
pub(crate) fn rescue_by_tail(root: &Utf8Path, recorded: &str) -> Option<Utf8PathBuf> {
    let parts: Vec<&str> = recorded
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if parts.contains(&"..") {
        return None;
    }
    (1..parts.len())
        .map(|i| {
            parts[i..]
                .iter()
                .fold(root.to_owned(), |acc, p| acc.join(p))
        })
        .find(|candidate| candidate.is_file())
}

/// Tira `.` e resolve `..` sem tocar no disco: `a/../b.v` vira `b.v`, para o
/// mesmo arquivo não entrar duas vezes no projeto.
pub(crate) fn normalize(path: &Utf8Path) -> Utf8PathBuf {
    use camino::Utf8Component;
    let mut out = Utf8PathBuf::new();
    for component in path.components() {
        match component {
            Utf8Component::CurDir => {}
            Utf8Component::ParentDir => {
                let last_is_normal =
                    matches!(out.components().next_back(), Some(Utf8Component::Normal(_)));
                if last_is_normal {
                    out.pop();
                } else if !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_only_inside_the_same_repository() {
        let dir = tempfile::tempdir().unwrap();
        let base = canonicalize(Utf8Path::from_path(dir.path()).unwrap()).unwrap();
        let repo = base.join("hits");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("projects/sim")).unwrap();
        std::fs::create_dir_all(repo.join("rtl/noise")).unwrap();
        let root = repo.join("projects/sim");
        assert_eq!(
            relative_in_repository(&root, &repo.join("rtl/noise/n.v")).as_deref(),
            Some("../../rtl/noise/n.v")
        );
        // Fora do repositório, e sem repositório nenhum.
        assert_eq!(relative_in_repository(&root, &base.join("x.v")), None);
        assert_eq!(relative_in_repository(&base, &base.join("y/x.v")), None);
    }

    #[test]
    fn rescue_tries_the_longest_tail_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = canonicalize(Utf8Path::from_path(dir.path()).unwrap()).unwrap();
        std::fs::create_dir_all(root.join("rtl")).unwrap();
        std::fs::write(root.join("rtl/x.v"), "").unwrap();
        std::fs::write(root.join("x.v"), "").unwrap();
        assert_eq!(
            rescue_by_tail(&root, "C:\\Users\\a\\proj\\rtl\\x.v"),
            Some(root.join("rtl/x.v"))
        );
        assert_eq!(
            rescue_by_tail(&root, "/home/b/proj/outra/x.v"),
            Some(root.join("x.v"))
        );
        assert_eq!(rescue_by_tail(&root, "C:\\a\\nada.v"), None);
        assert_eq!(rescue_by_tail(&root, "/a/../x.v"), None);
    }

    #[test]
    fn simulator_paths_must_be_ascii() {
        assert!(check_simulator_path(Utf8Path::new("/home/u/projetos/soma")).is_ok());
        let err = check_simulator_path(Utf8Path::new("/home/u/Área de Trabalho/soma")).unwrap_err();
        assert_eq!(err.code(), "non_ascii_path");
    }
}
