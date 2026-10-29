//! O YANC que o Lace sabe usar: o 7.1 e os seguintes.
//!
//! O YANC 7.1 mudou a linha de comando, os compiladores e as pastas. O fonte é
//! o argumento sem opção, o processador se chama como o arquivo (o `#PRNAME`
//! virou erro), cada ferramenta acha as pastas da biblioteca (`SAPHO/`,
//! `Macros/`, `Header/`) ao lado do próprio executável, não há `appcomp` (o
//! `asmcomp` faz as duas passadas) nem `cpppp` (o `cppcomp` pré-processa), e
//! tudo o que é gerado vai para uma pasta só, a de saída (`-o`), com os
//! arquivos citados pelo caminho relativo a ela. O projeto do Lace segue a
//! mesma estrutura (ver o módulo `project`).
//!
//! Até a 6.x a interface era outra (`-p`, `-t`, `-n`, `appcomp`, `cpppp`) e
//! as pastas também (`Software/`, `Hardware/`, `Simulation/`). Um bundle com
//! um YANC assim é recusado antes de compilar, com o erro que diz para
//! reinstalar ([`require`]).

use crate::error::{AuroraError, Result};
use crate::toolchain::{Toolchain, component};

/// A menor versão do YANC que o Lace usa.
pub(crate) const MINIMUM: (u32, u32) = (7, 1);

/// A versão do YANC do bundle, lida do manifesto do componente (`v7.1`,
/// `v7.1.2`): `(maior, menor)`. `None` sem o componente ou com uma versão
/// que não começa por número (um bundle de desenvolvimento), que o Lace
/// trata como atual.
fn parse_version(version: &str) -> Option<(u32, u32)> {
    let mut numbers = version.trim_start_matches(['v', 'V']).split(['.', '-']);
    let major = numbers.next()?.parse().ok()?;
    let minor = numbers
        .next()
        .and_then(|n| {
            n.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .ok()
        })
        .unwrap_or(0);
    Some((major, minor))
}

/// Confere que o YANC do bundle é o 7.1 ou mais novo.
///
/// # Erros
///
/// [`AuroraError::YancTooOld`] com um YANC até a 6.x, que tem outra linha de
/// comando e outra estrutura de pastas.
pub(crate) fn require(toolchain: &Toolchain) -> Result<()> {
    match toolchain.component(component::YANC) {
        Some(c) if parse_version(&c.version).is_some_and(|v| v < MINIMUM) => {
            Err(AuroraError::YancTooOld {
                found: c.version.clone(),
                minimum: format!("v{}.{}", MINIMUM.0, MINIMUM.1),
            })
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v7.1"), Some((7, 1)));
        assert_eq!(parse_version("v7.1-dev"), Some((7, 1)));
        assert_eq!(parse_version("7.10.2"), Some((7, 10)));
        assert_eq!(parse_version("v8"), Some((8, 0)));
        assert_eq!(parse_version("v6.0"), Some((6, 0)));
        assert_eq!(parse_version("main"), None);
        assert!(parse_version("v6.0").unwrap() < MINIMUM);
        assert!(parse_version("v7.0").unwrap() < MINIMUM);
        assert!(parse_version("v7.1").unwrap() >= MINIMUM);
    }
}
