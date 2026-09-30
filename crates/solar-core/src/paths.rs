//! Utilitários de caminho compartilhados pelo Core.

use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};

use crate::error::{Result, SolarError};

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
/// O Solar confere cada arquivo que um compilador vai abrir ou criar (fonte,
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
    let canonical =
        dunce::canonicalize(path).map_err(SolarError::io("resolvendo caminho", path))?;
    to_utf8(canonical)
}

pub(crate) fn to_utf8(path: impl AsRef<Path>) -> Result<Utf8PathBuf> {
    let path = path.as_ref();
    Utf8PathBuf::from_path_buf(path.to_owned())
        .map_err(|p| SolarError::NonUtf8Path(p.display().to_string()))
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
        return Err(SolarError::PathTooLong {
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
