//! Acrescentar aplicativos do bundle a uma instalação, sem tocar no `lace`.
//!
//! Só os pedaços dos componentes novos são extraídos, numa pasta provisória
//! dentro da instalação, e mesclados em `toolchain/`: um arquivo que já existe
//! (uma biblioteca que o componente divide com outro instalado) fica como
//! está. Depois o Lace confere os hashes de todos os executáveis do bundle.
//! Se algo falha no meio, os arquivos acrescentados saem, e a instalação fica
//! como estava. O pedaço sempre instalado (o `lace` e o cabeçalho do bundle)
//! não entra. É o que `lace install` usa.
//!
//! De onde vêm os pedaços é um [`ChunkSource`]: o payload de um instalador no
//! disco ([`LocalPayload`]), ou os pedaços que a release publica, que a CLI
//! baixa.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use lace_core::Toolchain;

use crate::install::{Event, RECEIPT_FILE, Receipt};
use crate::payload::{Chunk, Index};
use crate::plan::Selection;

/// De onde vêm os pedaços do payload.
pub trait ChunkSource: Send + Sync {
    /// O arquivo do pedaço no disco. Quem baixa avisa os bytes já baixados
    /// por `downloaded`.
    fn fetch(&self, chunk: &Chunk, downloaded: &mut dyn FnMut(u64)) -> anyhow::Result<PathBuf>;
}

/// O payload de um instalador no disco: `<instalador>/payload/`.
#[derive(Debug, Clone)]
pub struct LocalPayload(pub PathBuf);

impl ChunkSource for LocalPayload {
    fn fetch(&self, chunk: &Chunk, _downloaded: &mut dyn FnMut(u64)) -> anyhow::Result<PathBuf> {
        let path = self.0.join(&chunk.file);
        if !path.is_file() {
            bail!("The payload has no chunk {}", path.display());
        }
        Ok(path)
    }
}

/// O que [`add`] fez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddReport {
    /// A pasta do bundle (`<instalação>/toolchain`).
    pub toolchain: PathBuf,
    /// Os componentes que entraram, na ordem do índice.
    pub added: Vec<String>,
    /// Todos os componentes instalados agora.
    pub components: Vec<String>,
    /// O atalho do Lace Studio no menu de aplicativos, se o Studio entrou e
    /// o atalho foi criado ([`crate::desktop`]).
    pub shortcut: Option<PathBuf>,
    /// Quantos executáveis tiveram o hash conferido.
    pub verified: usize,
}

/// Os componentes instalados em `toolchain`, se o bundle de lá é o mesmo do
/// índice. Recusa outro bundle ou outra plataforma: acrescentar misturaria
/// ferramentas de versões diferentes, e isso é atualizar.
pub fn installed(index: &Index, toolchain: &Path) -> anyhow::Result<Selection> {
    let dir = toolchain.to_str().context("Path is not UTF-8")?;
    let tc = Toolchain::open(dir).with_context(|| {
        format!(
            "Could not open the bundle installed in {}",
            toolchain.display()
        )
    })?;
    let manifest = tc.manifest();
    if manifest.bundle != index.bundle || manifest.platform != index.platform {
        bail!(
            "The installed bundle is {} ({}), and the apps offered are from bundle {} ({}); \
             to change bundles, update Lace",
            manifest.bundle,
            manifest.platform,
            index.bundle,
            index.platform
        );
    }
    Ok(manifest.components.iter().map(|c| c.name.clone()).collect())
}

/// Acrescenta `new` à instalação em `prefix` (com o bundle em
/// `prefix/toolchain`). `new` são só os componentes que entram, com o que
/// eles exigem; ver [`crate::plan::adding`].
pub fn add(
    source: &dyn ChunkSource,
    index: &Index,
    prefix: &Path,
    new: &Selection,
    mut on: impl FnMut(Event),
) -> anyhow::Result<AddReport> {
    let toolchain = prefix.join("toolchain");
    let before = installed(index, &toolchain)?;
    if let Some(c) = new.iter().find(|c| before.contains(*c)) {
        bail!("{c} is already installed");
    }
    // Só os pedaços dos componentes novos; o sempre instalado (o `lace`)
    // fica de fora.
    let chunks: Vec<&Chunk> = index
        .chunks
        .iter()
        .filter(|c| c.components.iter().any(|n| new.contains(n)))
        .collect();
    on(Event::Start {
        total: chunks.iter().map(|c| c.size).sum(),
    });

    let staging = prefix.join(format!(".instalando-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).with_context(|| format!("Creating {}", staging.display()))?;
    let mut added_files = Vec::new();
    let result = extract_merge_verify(
        source,
        &chunks,
        &staging,
        &toolchain,
        &mut added_files,
        &mut on,
    );
    let _ = fs::remove_dir_all(&staging);
    let verified = match result {
        Ok(verified) => verified,
        Err(e) => {
            // A instalação volta a ser o que era: o que entrou sai, os
            // arquivos antes das pastas que os contêm.
            for path in added_files.iter().rev() {
                if path.is_dir() {
                    let _ = fs::remove_dir(path);
                } else {
                    let _ = fs::remove_file(path);
                }
            }
            return Err(e);
        }
    };

    let mut components: Selection = before;
    components.extend(new.iter().cloned());
    let components: Vec<String> = components.into_iter().collect();
    // O recibo (Linux, macOS) passa a listar os novos, para o instalador e o
    // `lace uninstall`. No Windows não há recibo: quem sabe é o bundle.
    if let Some(mut receipt) = Receipt::load(prefix) {
        receipt.components = components.clone();
        let path = prefix.join(RECEIPT_FILE);
        fs::write(&path, serde_json::to_string_pretty(&receipt)? + "\n")
            .with_context(|| format!("Writing {}", path.display()))?;
    }
    let shortcut = if new.contains(lace_core::component::STUDIO) {
        crate::desktop::sync(prefix)?
    } else {
        None
    };
    Ok(AddReport {
        toolchain,
        added: index
            .components
            .iter()
            .filter(|c| new.contains(&c.name))
            .map(|c| c.name.clone())
            .collect(),
        components,
        shortcut,
        verified,
    })
}

fn extract_merge_verify(
    source: &dyn ChunkSource,
    chunks: &[&Chunk],
    staging: &Path,
    toolchain: &Path,
    added_files: &mut Vec<PathBuf>,
    on: &mut impl FnMut(Event),
) -> anyhow::Result<usize> {
    let mut done = 0;
    for (i, chunk) in chunks.iter().enumerate() {
        on(Event::Chunk {
            index: i + 1,
            count: chunks.len(),
            components: chunk.components.clone(),
            download: chunk.download,
        });
        let path = source.fetch(chunk, &mut |bytes| on(Event::Downloading { bytes }))?;
        crate::install::extract_chunk(&path, staging, &mut |size| {
            done += size;
            on(Event::Progress { done });
        })?;
    }
    merge(&staging.join("toolchain"), toolchain, added_files)?;

    on(Event::Verifying);
    let dir = toolchain.to_str().context("Path is not UTF-8")?;
    let tc = Toolchain::open(dir).context("Could not open the bundle after adding")?;
    let mismatches = tc.verify()?;
    if let Some(m) = mismatches.first() {
        bail!(
            "Executables that do not match the manifest: {} (the first: {}); nothing was added",
            mismatches.len(),
            m.path
        );
    }
    Ok(tc.verified_files())
}

/// Move para `to` o que há em `from` e ainda não existe lá, guardando em
/// `added` cada arquivo e cada pasta novos, na ordem em que entraram.
fn merge(from: &Path, to: &Path, added: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !from.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(from).with_context(|| format!("Reading {}", from.display()))? {
        let entry = entry?;
        let source = entry.path();
        let dest = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if fs::symlink_metadata(&dest).is_err() {
                fs::create_dir(&dest).with_context(|| format!("Creating {}", dest.display()))?;
                added.push(dest.clone());
            }
            merge(&source, &dest, added)?;
        } else if fs::symlink_metadata(&dest).is_err() {
            fs::rename(&source, &dest).with_context(|| format!("Installing {}", dest.display()))?;
            added.push(dest);
        }
    }
    Ok(())
}
