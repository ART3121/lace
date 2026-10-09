//! O manifesto de arquivos (`files.json`): o SHA-256 de cada arquivo que o
//! instalador põe na instalação, agrupado como nos pedaços do payload.
//!
//! O `lace-pack` grava o mesmo manifesto no `payload/` (que a release publica
//! como `lace-<versão>-<plataforma>-files.json`), no pedaço sempre instalado
//! e no estágio do Inno Setup. Toda instalação fica com o da versão dela em
//! `toolchain/files.json` ([`INSTALLED`]), e o `lace update` compara os dois
//! para baixar só os pedaços com arquivos que mudaram ([`crate::update`]).
//!
//! ```json
//! { "schema": 1, "lace_version": "0.7.0", "bundle": "2026.10.07", "platform": "linux-x64",
//!   "groups": [
//!     { "chunk": "lace.tar.zst", "components": [],
//!       "files": { "bin/lace": "<sha256>", "toolchain/bundle.json": "<sha256>" } },
//!     { "chunk": "c01.tar.zst", "components": ["icarus", "yosys"],
//!       "files": { "toolchain/oss-cad-suite/lib/libc.so.6": "<sha256>",
//!                  "toolchain/oss-cad-suite/lib/libc.so": "symlink:libc.so.6" } } ] }
//! ```
//!
//! Os caminhos são relativos à pasta da instalação, com `/`, como os nomes
//! dentro dos pedaços. Um link simbólico vale `symlink:<destino>`. O próprio
//! `toolchain/files.json` não entra.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::plan::Selection;

/// Nome do manifesto no payload.
pub const FILES_FILE: &str = "files.json";
/// Versão do formato.
pub const FILES_SCHEMA: u32 = 1;
/// Onde a instalação guarda o manifesto da versão dela, relativo à pasta da
/// instalação.
pub const INSTALLED: &str = "toolchain/files.json";

/// O manifesto de arquivos de uma versão do instalador.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesManifest {
    /// [`FILES_SCHEMA`].
    pub schema: u32,
    /// Versão do Lace.
    pub lace_version: String,
    /// Identificador do bundle.
    pub bundle: String,
    /// Plataforma.
    pub platform: String,
    /// Os arquivos, um grupo por pedaço do payload, na ordem do índice.
    pub groups: Vec<FileGroup>,
}

/// Os arquivos de um pedaço.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileGroup {
    /// O pedaço (`lace.tar.zst`, `c01.tar.zst`).
    pub chunk: String,
    /// Os componentes que usam estes arquivos. Vazio: sempre instalados.
    pub components: Vec<String>,
    /// Caminho relativo à instalação, e o SHA-256 do arquivo (ou
    /// `symlink:<destino>`).
    pub files: BTreeMap<String, String>,
}

impl FileGroup {
    /// Este grupo vai para a instalação com `selection`?
    pub fn needed_by(&self, selection: &Selection) -> bool {
        self.components.is_empty() || self.components.iter().any(|c| selection.contains(c))
    }
}

impl FilesManifest {
    /// Lê um manifesto.
    pub fn load(path: &Path) -> anyhow::Result<FilesManifest> {
        let text =
            fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("Invalid file manifest: {}", path.display()))
    }

    /// Lê um manifesto do texto.
    pub fn parse(text: &str) -> anyhow::Result<FilesManifest> {
        let manifest: FilesManifest = serde_json::from_str(text)?;
        if manifest.schema != FILES_SCHEMA {
            bail!(
                "the file manifest has format {}; this Lace reads format {FILES_SCHEMA}",
                manifest.schema
            );
        }
        Ok(manifest)
    }

    /// Grava o manifesto.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        fs::write(path, serde_json::to_string_pretty(self)? + "\n")
            .with_context(|| format!("Writing {}", path.display()))
    }

    /// Os arquivos que `selection` instala, com o hash e o grupo de cada um.
    pub fn files_for<'a>(
        &'a self,
        selection: &Selection,
    ) -> BTreeMap<&'a str, (&'a str, &'a FileGroup)> {
        self.groups
            .iter()
            .filter(|g| g.needed_by(selection))
            .flat_map(|g| g.files.iter().map(move |(p, h)| (p.as_str(), (h.as_str(), g))))
            .collect()
    }
}

/// O valor de um arquivo no manifesto: o SHA-256 do conteúdo, ou
/// `symlink:<destino>` para um link simbólico (que não é seguido).
pub fn digest(path: &Path) -> anyhow::Result<String> {
    let meta = fs::symlink_metadata(path).with_context(|| format!("Reading {}", path.display()))?;
    if meta.file_type().is_symlink() {
        let target = fs::read_link(path).with_context(|| format!("Reading {}", path.display()))?;
        return Ok(format!(
            "symlink:{}",
            target.to_string_lossy().replace('\\', "/")
        ));
    }
    let mut file = fs::File::open(path).with_context(|| format!("Opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 16];
    loop {
        let n = file
            .read(&mut buffer)
            .with_context(|| format!("Reading {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_of_a_file_and_a_selection_of_groups() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("abc");
        fs::write(&file, "abc").unwrap();
        assert_eq!(
            digest(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        let group = |chunk: &str, components: &[&str], files: &[(&str, &str)]| FileGroup {
            chunk: chunk.into(),
            components: components.iter().map(|s| s.to_string()).collect(),
            files: files
                .iter()
                .map(|(p, h)| (p.to_string(), h.to_string()))
                .collect(),
        };
        let manifest = FilesManifest {
            schema: FILES_SCHEMA,
            lace_version: "0.7.0".into(),
            bundle: "b".into(),
            platform: "linux-x64".into(),
            groups: vec![
                group("lace.tar.zst", &[], &[("bin/lace", "1")]),
                group("c01.tar.zst", &["icarus", "yosys"], &[("toolchain/lib", "2")]),
                group("c02.tar.zst", &["yosys"], &[("toolchain/yosys", "3")]),
            ],
        };
        let text = serde_json::to_string(&manifest).unwrap();
        assert_eq!(FilesManifest::parse(&text).unwrap(), manifest);
        let icarus: Selection = ["icarus".to_owned()].into();
        let files = manifest.files_for(&icarus);
        assert_eq!(
            files.keys().copied().collect::<Vec<_>>(),
            ["bin/lace", "toolchain/lib"]
        );
        assert_eq!(files["toolchain/lib"].1.chunk, "c01.tar.zst");
        assert!(
            FilesManifest::parse(&text.replace("\"schema\":1", "\"schema\":2"))
                .unwrap_err()
                .to_string()
                .contains("format 2")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_its_target() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("libc.so.6", dir.path().join("libc.so")).unwrap();
        assert_eq!(
            digest(&dir.path().join("libc.so")).unwrap(),
            "symlink:libc.so.6"
        );
    }
}
