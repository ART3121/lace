//! Acrescentar aplicativos do bundle a uma instalação, sem tocar no `aurora`.
//!
//! Só os pedaços dos componentes novos são extraídos, numa pasta provisória
//! dentro da instalação, e mesclados em `toolchain/`: um arquivo que já existe
//! (uma biblioteca que o componente divide com outro instalado) fica como
//! está. Depois a AURORA confere os hashes de todos os executáveis do bundle.
//! Se algo falha no meio, os arquivos acrescentados saem, e a instalação fica
//! como estava. O pedaço sempre instalado (o `aurora` e o cabeçalho do bundle)
//! não entra. É o que `aurora install` usa.
//!
//! De onde vêm os pedaços é um [`ChunkSource`]: o payload de um instalador no
//! disco ([`LocalPayload`]), ou os pedaços que a release publica, que a CLI
//! baixa.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
use camino::{Utf8Path, Utf8PathBuf};
use aurora_core::{CancelToken, Platform, Toolchain};
use schemars::JsonSchema;
use serde::Serialize;

use crate::error::{Error, Result, from_anyhow};
use crate::files::{FILES_FILE, FilesManifest};
use crate::install::{Event, RECEIPT_FILE, Receipt};
use crate::payload::{self, Chunk, Index};
use crate::plan::{self, Selection};
use crate::release::{Network, Sources};

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
    /// O atalho da AURORA IDE no menu de aplicativos, se o Studio entrou e
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
             to change bundles, update AURORA",
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
/// eles exigem; ver [`crate::plan::adding`]. Sem cancelamento: ver
/// [`add_with`].
pub fn add(
    source: &dyn ChunkSource,
    index: &Index,
    prefix: &Path,
    new: &Selection,
    on: impl FnMut(Event),
) -> anyhow::Result<AddReport> {
    add_with(source, index, prefix, new, on, &CancelToken::new())
}

/// [`add`] conferindo `cancel` antes de cada pedaço: cancelado, o que entrou
/// sai e a instalação fica como estava ([`crate::Error::Cancelled`] dentro
/// do `anyhow`).
pub fn add_with(
    source: &dyn ChunkSource,
    index: &Index,
    prefix: &Path,
    new: &Selection,
    mut on: impl FnMut(Event),
    cancel: &CancelToken,
) -> anyhow::Result<AddReport> {
    let _lock = crate::update::lock(prefix)?;
    let toolchain = prefix.join("toolchain");
    let before = installed(index, &toolchain)?;
    if let Some(c) = new.iter().find(|c| before.contains(*c)) {
        bail!("{c} is already installed");
    }
    // Só os pedaços dos componentes novos; o sempre instalado (o `aurora`)
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
        cancel,
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
    // `aurora uninstall`. No Windows não há recibo: quem sabe é o bundle.
    if let Some(mut receipt) = Receipt::load(prefix) {
        receipt.components = components.clone();
        let path = prefix.join(RECEIPT_FILE);
        fs::write(&path, serde_json::to_string_pretty(&receipt)? + "\n")
            .with_context(|| format!("Writing {}", path.display()))?;
    }
    let shortcut = if new.contains(aurora_core::component::STUDIO) {
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
    cancel: &CancelToken,
) -> anyhow::Result<usize> {
    let mut done = 0;
    for (i, chunk) in chunks.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(anyhow::Error::new(Error::Cancelled));
        }
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

/// `aurora install` (`docs/schema/install.json`).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct InstallReport {
    /// A pasta da instalação.
    #[schemars(with = "String")]
    pub prefix: Utf8PathBuf,
    /// Os aplicativos do bundle instalados agora.
    pub components: Vec<String>,
    /// Os que entraram nesta chamada (com os que eles exigem).
    pub added: Vec<String>,
}

/// O payload de um instalador no disco: a pasta e o começo dos nomes dos
/// arquivos (vazio, ou `aurora-<versão>-<plataforma>-` com os nomes que a
/// release publica).
#[derive(Debug, Clone)]
pub struct DiskPayload {
    dir: Utf8PathBuf,
    prefix: String,
    cancel: CancelToken,
}

impl DiskPayload {
    /// O payload em `path`: a pasta do instalador, a pasta `payload/` dele,
    /// o `.tar.gz` da release (extraído em `work`), ou uma pasta com os
    /// arquivos da release desta plataforma.
    pub fn find(path: &Utf8Path, work: &Utf8Path, cancel: CancelToken) -> Result<DiskPayload> {
        let plain = |dir: Utf8PathBuf| DiskPayload {
            dir,
            prefix: String::new(),
            cancel: cancel.clone(),
        };
        if path
            .join(payload::PAYLOAD_DIR)
            .join(payload::INDEX_FILE)
            .is_file()
        {
            return Ok(plain(path.join(payload::PAYLOAD_DIR)));
        }
        if path.join(payload::INDEX_FILE).is_file() {
            return Ok(plain(path.to_owned()));
        }
        if path.is_file() {
            let file = fs::File::open(path).with_context(|| format!("Opening {path}"))?;
            tar::Archive::new(flate2::read::GzDecoder::new(file))
                .unpack(work)
                .with_context(|| format!("Extracting {path}"))?;
            return Ok(find_payload(work)
                .map(plain)
                .with_context(|| format!("{path} does not contain an AURORA installer"))?);
        }
        if path.is_dir()
            && let Some(prefix) = release_prefix(path)?
        {
            return Ok(DiskPayload {
                dir: path.to_owned(),
                prefix,
                cancel,
            });
        }
        Err(anyhow::anyhow!(
            "{path} is not an AURORA installer (its folder, its payload/ folder, the release .tar.gz, \
             or a folder with the release files of this platform)"
        )
        .into())
    }

    /// Um arquivo do payload.
    pub fn path(&self, file: &str) -> Utf8PathBuf {
        self.dir.join(format!("{}{file}", self.prefix))
    }

    /// O índice.
    pub fn index(&self) -> Result<Index> {
        let path = self.path(payload::INDEX_FILE);
        let text = fs::read_to_string(&path).with_context(|| format!("Reading {path}"))?;
        Ok(Index::parse(&text).with_context(|| format!("Invalid payload index: {path}"))?)
    }

    /// O manifesto de arquivos; `None` num payload anterior à 0.7.0.
    pub fn files(&self) -> Result<Option<FilesManifest>> {
        let path = self.path(FILES_FILE);
        if !path.is_file() {
            return Ok(None);
        }
        Ok(FilesManifest::load(path.as_std_path()).map(Some)?)
    }
}

impl ChunkSource for DiskPayload {
    fn fetch(&self, chunk: &Chunk, downloaded: &mut dyn FnMut(u64)) -> anyhow::Result<PathBuf> {
        // Cada pedaço vem antes de a instalação mudar: o cancelamento para aqui.
        if self.cancel.is_cancelled() {
            return Err(anyhow::Error::new(Error::Cancelled));
        }
        if self.prefix.is_empty() {
            return LocalPayload(self.dir.clone().into_std_path_buf()).fetch(chunk, downloaded);
        }
        let path = self.path(&chunk.file);
        if !path.is_file() {
            bail!("The payload has no chunk {path}");
        }
        Ok(path.into_std_path_buf())
    }
}

/// O começo dos nomes dos arquivos da release desta plataforma numa pasta
/// (`aurora-0.7.0-linux-x64-`), se ela tem o índice de uma versão só.
fn release_prefix(dir: &Utf8Path) -> Result<Option<String>> {
    let platform = Platform::current()
        .context("No bundle apps for this platform")?
        .as_str();
    let suffix = format!("-{platform}-{}", payload::INDEX_FILE);
    let mut found: Vec<String> = dir
        .read_dir_utf8()
        .with_context(|| format!("Reading {dir}"))?
        .filter_map(std::result::Result::ok)
        .filter_map(|e| {
            let name = e.file_name();
            (name.starts_with("aurora-") && name.ends_with(&suffix))
                .then(|| name[..name.len() - payload::INDEX_FILE.len()].to_owned())
        })
        .collect();
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.pop()),
        _ => {
            Err(anyhow::anyhow!("{dir} has the release files of more than one AURORA version").into())
        }
    }
}

fn find_payload(dir: &Utf8Path) -> Option<Utf8PathBuf> {
    dir.read_dir_utf8()
        .ok()?
        .filter_map(std::result::Result::ok)
        .find_map(|e| {
            let payload = e.path().join(payload::PAYLOAD_DIR);
            payload
                .join(payload::INDEX_FILE)
                .is_file()
                .then_some(payload)
        })
}

/// Os pedaços publicados na release, baixados um a um quando são extraídos,
/// para `dir` (que guarda também o `SHA256SUMS` dela).
pub struct ReleasePayload {
    sources: Arc<dyn Sources>,
    version: String,
    platform: String,
    dir: Utf8PathBuf,
    cancel: CancelToken,
}

impl ReleasePayload {
    /// Os pedaços da release `v<version>` para `platform`, em `dir`.
    pub fn new(
        sources: Arc<dyn Sources>,
        version: String,
        platform: String,
        dir: Utf8PathBuf,
        cancel: CancelToken,
    ) -> ReleasePayload {
        ReleasePayload {
            sources,
            version,
            platform,
            dir,
            cancel,
        }
    }
}

impl std::fmt::Debug for ReleasePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReleasePayload")
            .field("version", &self.version)
            .field("platform", &self.platform)
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

impl ChunkSource for ReleasePayload {
    fn fetch(&self, chunk: &Chunk, downloaded: &mut dyn FnMut(u64)) -> anyhow::Result<PathBuf> {
        // Cada pedaço vem antes de a instalação mudar: o cancelamento para aqui.
        if self.cancel.is_cancelled() {
            return Err(anyhow::Error::new(Error::Cancelled));
        }
        let file = payload::release_asset(&self.version, &self.platform, &chunk.file);
        let path = self
            .sources
            .release_file(&self.version, &file, &self.dir, downloaded, &self.cancel)
            .map_err(anyhow::Error::new)?;
        Ok(path.into_std_path_buf())
    }
}

/// De onde vêm os pedaços do `aurora install`.
#[derive(Debug, Clone, Copy)]
pub enum Payload<'a> {
    /// A release da AURORA desta versão, pela rede.
    Release {
        /// A versão desta AURORA.
        version: &'a str,
    },
    /// Um instalador no disco.
    Disk(&'a Utf8Path),
}

/// O que o `aurora install` precisa antes de escolher: o índice, a fonte dos
/// pedaços e o que já está instalado.
pub struct AddPrepared {
    /// O índice do payload.
    pub index: Index,
    /// De onde vêm os pedaços.
    pub source: Arc<dyn ChunkSource>,
    /// Os componentes instalados.
    pub installed: Selection,
    _work: tempfile::TempDir,
}

impl std::fmt::Debug for AddPrepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AddPrepared")
            .field("installed", &self.installed)
            .finish_non_exhaustive()
    }
}

/// Lê o índice do payload e os componentes instalados em `prefix`. Da
/// release, só o índice é baixado agora; cada pedaço vem quando é extraído.
pub fn prepare(
    sources: Arc<dyn Sources>,
    network: &Network,
    prefix: &Utf8Path,
    payload: Payload<'_>,
    cancel: &CancelToken,
) -> Result<AddPrepared> {
    let work = network.work_dir()?;
    let work_dir = Utf8Path::from_path(work.path())
        .context("Temporary folder is not UTF-8")?
        .to_owned();
    let (index, source): (Index, Arc<dyn ChunkSource>) = match payload {
        Payload::Disk(path) => {
            let disk = DiskPayload::find(path, &work_dir, cancel.clone())?;
            (disk.index()?, Arc::new(disk))
        }
        Payload::Release { version } => {
            let platform = Platform::current()
                .context("No bundle apps for this platform")?
                .as_str()
                .to_owned();
            let file = payload::release_asset(version, &platform, payload::INDEX_FILE);
            let path = sources
                .release_file(version, &file, &work_dir, &mut |_| {}, cancel)
                .map_err(|error| match error {
                    Error::Cancelled => Error::Cancelled,
                    error => Error::Other(anyhow::Error::new(error).context(format!(
                        "Release v{version} does not publish the bundle apps separately (that \
                         started after 0.1.0); use an installer of this version: aurora install \
                         --from <folder|file.tar.gz>"
                    ))),
                })?;
            let text = fs::read_to_string(&path).with_context(|| format!("Reading {path}"))?;
            let index =
                Index::parse(&text).with_context(|| format!("Invalid {file} in the release"))?;
            let source = ReleasePayload::new(
                sources,
                version.to_owned(),
                platform,
                work_dir,
                cancel.clone(),
            );
            (index, Arc::new(source))
        }
    };
    let toolchain = prefix.join("toolchain");
    let installed = installed(&index, toolchain.as_std_path())?;
    Ok(AddPrepared {
        index,
        source,
        installed,
        _work: work,
    })
}

/// Instala `components` (com o que eles exigem) na instalação em `prefix`.
/// Com todos já instalados, não faz nada e `added` sai vazio.
pub fn add_components(
    prepared: &AddPrepared,
    prefix: &Utf8Path,
    components: &[String],
    on: &mut dyn FnMut(Event),
    cancel: &CancelToken,
) -> Result<InstallReport> {
    let installed_list: Vec<String> = prepared.installed.iter().cloned().collect();
    let (_, new) = plan::adding(&prepared.index, &installed_list, components)?;
    if new.is_empty() {
        return Ok(InstallReport {
            prefix: prefix.to_owned(),
            components: installed_list,
            added: Vec::new(),
        });
    }
    let new: Selection = new.into_iter().collect();
    let report = add_with(
        &*prepared.source,
        &prepared.index,
        prefix.as_std_path(),
        &new,
        &mut *on,
        cancel,
    )
    .map_err(from_anyhow)?;
    Ok(InstallReport {
        prefix: prefix.to_owned(),
        components: report.components,
        added: report.added,
    })
}

/// O que `aurora setup` fez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupReport {
    /// Os rótulos dos componentes instalados, na ordem do índice.
    pub labels: Vec<String>,
    /// Quantos executáveis tiveram o hash conferido.
    pub verified: usize,
}

/// `aurora setup`, que o assistente do Windows que baixa só os aplicativos
/// escolhidos roda antes de copiar o `aurora.exe`: o bundle de `components` em
/// `prefix/toolchain`, a partir dos arquivos da release em `payload`
/// ([`crate::install::install_bundle`]).
pub fn setup(
    network: &Network,
    payload: &Utf8Path,
    components: &[String],
    prefix: &Utf8Path,
    on: &mut dyn FnMut(Event),
) -> Result<SetupReport> {
    let work = network.work_dir()?;
    let work_dir = Utf8Path::from_path(work.path())
        .context("Temporary folder is not UTF-8")?
        .to_owned();
    let disk = DiskPayload::find(payload, &work_dir, CancelToken::new())?;
    let index = disk.index()?;
    let (selection, _) = plan::from_names(&index, components)?;
    let verified =
        crate::install::install_bundle(&disk, &index, &selection, prefix.as_std_path(), &mut *on)?;
    let labels = index
        .components
        .iter()
        .filter(|c| selection.contains(&c.name))
        .map(|c| c.label.clone())
        .collect();
    Ok(SetupReport { labels, verified })
}
