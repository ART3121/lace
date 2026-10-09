//! O Quartus Prime instalado no sistema, que o Lace roda sem abrir a
//! interface.
//!
//! É a segunda exceção à regra do bundle, depois do compilador C++ do
//! Verilator: o Quartus é proprietário e grande, e não pode ir no bundle. O
//! Core não lê variáveis de ambiente para achá-lo: procura nas pastas padrão
//! do instalador da Intel ([`Quartus::detect`]), e a CLI e o Studio passam
//! uma pasta declarada ([`Quartus::in_dir`]). O Quartus não existe para
//! macOS.

use camino::{Utf8Path, Utf8PathBuf};
use schemars::JsonSchema;
use serde::Serialize;

use crate::error::{LaceError, Result};
use crate::process::Invocation;
use crate::toolchain::Platform;

/// O que os programas do Quartus recebem do ambiente do Lace, além do que
/// todo processo recebe: a pasta e o nome do usuário, onde o Quartus guarda
/// as preferências, e no Linux também o idioma e a pasta temporária.
///
/// Deduzido do que programas de linha de comando costumam ler, e não
/// conferido com o Quartus rodando: o Lace nunca o executou. Se faltar uma
/// variável, o relatório do passo mostra as que foram (`inherit`).
#[cfg(windows)]
const QUARTUS_ENV: &[&str] = &[
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "HOMEDRIVE",
    "HOMEPATH",
    "USERNAME",
    "COMPUTERNAME",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
];
#[cfg(not(windows))]
const QUARTUS_ENV: &[&str] = &["HOME", "USER", "LANG", "TMPDIR"];

/// Uma instalação do Quartus Prime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Quartus {
    /// A pasta `quartus` da instalação (`C:\intelFPGA_lite\22.1std\quartus`).
    #[schemars(with = "String")]
    pub root: Utf8PathBuf,
    /// A pasta dos programas: `bin64` no Windows, `bin` no Linux.
    #[schemars(with = "String")]
    pub bin: Utf8PathBuf,
    /// A versão, pelo nome da pasta da instalação (`22.1std`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// As pastas onde o instalador da Intel (e o da Altera, nas versões novas)
/// põe uma pasta por versão, no Windows.
const WINDOWS_BASES: &[&str] = &[
    "C:/intelFPGA_lite",
    "C:/intelFPGA",
    "C:/altera_lite",
    "C:/altera",
];

/// O mesmo no Linux: dentro da pasta do usuário e em `/opt`.
const LINUX_BASES: &[&str] = &["intelFPGA_lite", "intelFPGA", "altera_lite", "altera"];

impl Quartus {
    /// A instalação numa pasta declarada: a da versão
    /// (`C:\intelFPGA_lite\22.1std`), a `quartus` dentro dela ou a dos
    /// programas. `None` se não houver `quartus_sh` lá.
    pub fn in_dir(dir: &Utf8Path) -> Option<Quartus> {
        let platform = Platform::current()?;
        let bins: &[&str] = match platform {
            Platform::WindowsX64 => &["quartus/bin64", "bin64", ""],
            Platform::LinuxX64 => &["quartus/bin", "bin", ""],
            Platform::MacosArm64 => return None,
        };
        let exe = platform.exe();
        let bin = bins
            .iter()
            .map(|b| {
                if b.is_empty() {
                    dir.to_owned()
                } else {
                    dir.join(b)
                }
            })
            .find(|b| b.join(format!("quartus_sh{exe}")).is_file())?;
        let bin = dunce::canonicalize(&bin)
            .ok()
            .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
            .unwrap_or(bin);
        let root = bin.parent().map_or_else(|| bin.clone(), Utf8Path::to_owned);
        let version = (root.file_name() == Some("quartus"))
            .then(|| root.parent().and_then(Utf8Path::file_name))
            .flatten()
            .map(str::to_owned);
        Some(Quartus { root, bin, version })
    }

    /// Procura nas pastas padrão do instalador e fica com a versão mais
    /// nova. `home` é a pasta do usuário, onde o instalador do Linux põe o
    /// Quartus por padrão; o Core não a lê do ambiente. `None` sempre no
    /// macOS.
    pub fn detect(home: Option<&Utf8Path>) -> Option<Quartus> {
        let bases: Vec<Utf8PathBuf> = match Platform::current()? {
            Platform::WindowsX64 => WINDOWS_BASES.iter().map(Utf8PathBuf::from).collect(),
            Platform::LinuxX64 => home
                .into_iter()
                .flat_map(|h| LINUX_BASES.iter().map(move |b| h.join(b)))
                .chain(LINUX_BASES.iter().map(|b| Utf8Path::new("/opt").join(b)))
                .collect(),
            Platform::MacosArm64 => return None,
        };
        Self::newest_in(&bases)
    }

    /// A versão mais nova entre as pastas de versão de `bases`.
    fn newest_in(bases: &[Utf8PathBuf]) -> Option<Quartus> {
        let mut found: Vec<Quartus> = bases
            .iter()
            .filter_map(|base| base.read_dir_utf8().ok())
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| Self::in_dir(entry.path()))
            .collect();
        found.sort_by_key(|q| q.version.as_deref().map(version_key));
        found.pop()
    }

    /// O caminho de um programa do Quartus (`quartus_sh`, `quartus_pgm`).
    pub fn program(&self, name: &str) -> Utf8PathBuf {
        let exe = Platform::current().map_or("", Platform::exe);
        self.bin.join(format!("{name}{exe}"))
    }

    /// Uma invocação do programa `name` (`quartus_map`, `quartus_sh`) em
    /// `cwd`, com o `PATH` na pasta dos programas e no sistema base, o
    /// `QUARTUS_ROOTDIR` nesta instalação (um de outra versão no ambiente
    /// não se mistura) e as variáveis de `QUARTUS_ENV`.
    ///
    /// # Erros
    ///
    /// [`LaceError::ToolchainIncomplete`] se o programa não está na pasta.
    pub(crate) fn invocation(&self, name: &str, cwd: impl Into<Utf8PathBuf>) -> Result<Invocation> {
        let program = self.program(name);
        if !program.is_file() {
            return Err(LaceError::ToolchainIncomplete {
                what: format!("{name} of Quartus Prime"),
                path: program,
            });
        }
        let mut path = vec![self.bin.clone()];
        if let Some(platform) = Platform::current() {
            path.extend(crate::toolchain::base_path(platform));
        }
        let root = dunce::simplified(self.root.as_std_path())
            .to_string_lossy()
            .into_owned();
        Ok(Invocation::new(program, cwd)
            .env("QUARTUS_ROOTDIR", root)
            .search_path(&path)
            .inherit(QUARTUS_ENV))
    }
}

/// `22.1std` vira `[22, 1]`, para ordenar as versões.
fn version_key(version: &str) -> Vec<u32> {
    version
        .split(|c: char| !c.is_ascii_digit())
        .take_while(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uma instalação de mentira: só o `quartus_sh` que a detecção procura.
    fn fake(base: &Utf8Path, version: &str) -> Utf8PathBuf {
        let exe = Platform::current().map_or("", Platform::exe);
        let bin = if cfg!(windows) {
            "quartus/bin64"
        } else {
            "quartus/bin"
        };
        let dir = base.join(version).join(bin);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("quartus_sh{exe}")), "").unwrap();
        base.join(version)
    }

    #[test]
    #[cfg_attr(target_os = "macos", ignore = "o Quartus não existe para macOS")]
    fn a_declared_folder_can_be_the_install_the_quartus_or_the_bin() {
        let dir = tempfile::tempdir().unwrap();
        let base = Utf8Path::from_path(dir.path()).unwrap();
        let install = fake(base, "22.1std");
        let found = Quartus::in_dir(&install).unwrap();
        assert_eq!(found.version.as_deref(), Some("22.1std"));
        assert_eq!(Quartus::in_dir(&found.root).unwrap(), found);
        assert_eq!(Quartus::in_dir(&found.bin).unwrap(), found);
        assert!(found.program("quartus_pgm").starts_with(&found.bin));
        assert!(Quartus::in_dir(base).is_none());
    }

    #[test]
    #[cfg_attr(target_os = "macos", ignore = "o Quartus não existe para macOS")]
    fn the_newest_version_wins() {
        let dir = tempfile::tempdir().unwrap();
        let base = Utf8Path::from_path(dir.path()).unwrap();
        fake(base, "20.1");
        fake(base, "22.1std");
        fake(base, "21.1");
        std::fs::create_dir_all(base.join("vazia")).unwrap();
        let found = Quartus::newest_in(&[base.to_owned(), base.join("nao-existe")]).unwrap();
        assert_eq!(found.version.as_deref(), Some("22.1std"));
    }

    #[test]
    fn versions_sort_by_number() {
        assert_eq!(version_key("22.1std"), [22, 1]);
        assert_eq!(version_key("25.1std.0"), [25, 1]);
        assert!(version_key("9.1") < version_key("13.0"));
    }
}
