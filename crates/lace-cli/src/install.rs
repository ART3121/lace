//! `lace install`: instala aplicativos do bundle (Verilator, Yosys, ...) na
//! instalação de onde este `lace` roda, sem reinstalar o Lace.
//!
//! Sem nomes, abre no terminal a lista dos aplicativos do bundle, com os
//! instalados marcados e travados (a TUI de `lace-installer`,
//! `App::for_adding`). Com nomes, instala direto, com o que eles exigem.
//!
//! Só os pedaços dos aplicativos novos são baixados e extraídos em
//! `toolchain/` (`lace_installer::add`): o `lace` não muda, e uma falha no
//! meio desfaz o que entrou. Os pedaços vêm:
//!
//! - da release do Lace desta versão no GitHub, que publica o índice e cada
//!   pedaço do payload (`lace-<versão>-<plataforma>-<arquivo>`), conferidos
//!   pelo `SHA256SUMS` dela; ou
//! - com `--from`, de um instalador no disco: a pasta, o `.tar.gz` da release
//!   ou a pasta `payload/` dele, ou uma pasta com os arquivos que a release
//!   publica (`lace-<versão>-<plataforma>-index.json` e os pedaços).
//!
//! O bundle de lá precisa ser o instalado: outro bundle é atualizar, e isso é
//! `lace update`.

use std::io::IsTerminal;
use std::sync::Arc;

use anyhow::{Context, bail};
use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{Platform, component};
use lace_installer::add::{self, ChunkSource, LocalPayload};
use lace_installer::install::Event;
use lace_installer::files::{FILES_FILE, FilesManifest};
use lace_installer::payload::{self, Chunk, Index};
use lace_installer::plan::{self, Selection};
use lace_installer::tui;

use crate::installation;
use crate::meter::Meter;
use crate::output::Output;
use crate::release;
use crate::report::InstallReport;

pub fn run(out: &Output, components: &[String], from: Option<&Utf8Path>) -> anyhow::Result<()> {
    // Um nome errado aparece antes de qualquer download.
    if let Some(unknown) = components
        .iter()
        .find(|c| !component::ALL.contains(&c.as_str()))
    {
        bail!(
            "Unknown app: {unknown} (bundle apps: {})",
            component::ALL.join(", ")
        );
    }
    if components.is_empty() && !out.is_text() {
        bail!("With --json, name the apps to install: lace install <app>");
    }
    let prefix = installation::prefix()?;
    lace_installer::update::clean_leftovers(prefix.as_std_path());
    let work = release::work_dir()?;
    let work_dir = Utf8Path::from_path(work.path())
        .context("Temporary folder is not UTF-8")?
        .to_owned();
    let (index, source): (Index, Arc<dyn ChunkSource>) = match from {
        Some(path) => {
            let payload = DiskPayload::find(path, &work_dir)?;
            (payload.index()?, Arc::new(payload))
        }
        None => remote(&work_dir)?,
    };
    let toolchain = prefix.join("toolchain");
    let installed = add::installed(&index, toolchain.as_std_path())?;

    if components.is_empty() {
        return choose(out, index, source, &prefix, installed);
    }
    let installed_list: Vec<String> = installed.iter().cloned().collect();
    let (_, new) = plan::adding(&index, &installed_list, components)?;
    if new.is_empty() {
        if out.is_text() {
            println!("Already installed: {}", components.join(", "));
        }
        return out.json(&InstallReport {
            prefix,
            components: installed_list,
            added: Vec::new(),
        });
    }
    let new: Selection = new.into_iter().collect();
    let text = out.is_text();
    let label = |name: &str| {
        index
            .component(name)
            .map_or_else(|| name.to_owned(), |c| c.label.clone())
    };
    let mut meter = Meter::new();
    // O progresso vai para o stderr também com `--json`: o Studio mostra.
    let report = add::add(&*source, &index, prefix.as_std_path(), &new, |event| {
        show(&mut meter, &event, &label)
    })?;
    meter.finish();
    if text {
        println!("{}", tui::plain(&tui::added_lines(&report, &index)));
    }
    out.json(&InstallReport {
        prefix,
        components: report.components,
        added: report.added,
    })
}

/// A lista no terminal.
fn choose(
    out: &Output,
    index: Index,
    source: Arc<dyn ChunkSource>,
    prefix: &Utf8Path,
    installed: Selection,
) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("No terminal for the list: name the apps to install, as in lace install verilator");
    }
    let app = tui::App::for_adding(
        index.clone(),
        source,
        prefix.as_std_path().to_owned(),
        lace_core::SystemCompiler::detect(),
        installed,
    );
    let app = tui::run(app)?;
    match &app.outcome {
        Some(Ok(tui::Finished::Added(report))) => {
            println!("{}", tui::plain(&tui::added_lines(report, &index)));
        }
        Some(Ok(tui::Finished::Installed(_))) => {}
        Some(Err(e)) => bail!("{e}"),
        None if out.is_text() => println!("Nothing was installed"),
        None => {}
    }
    Ok(())
}

/// `lace setup`, que o assistente do Windows que baixa só os aplicativos
/// escolhidos roda antes de copiar o `lace.exe`: o bundle de `components` em
/// `prefix/toolchain`, a partir dos arquivos da release em `payload`
/// (`install_bundle`). Cada pedaço vira uma linha `[i/n] ...` no stderr, que
/// o assistente mostra.
pub fn setup(payload: &Utf8Path, components: &[String], prefix: &Utf8Path) -> anyhow::Result<()> {
    let work = release::work_dir()?;
    let work_dir = Utf8Path::from_path(work.path())
        .context("Temporary folder is not UTF-8")?
        .to_owned();
    let disk = DiskPayload::find(payload, &work_dir)?;
    let index = disk.index()?;
    let (selection, _) = plan::from_names(&index, components)?;
    let label = |name: &str| {
        index
            .component(name)
            .map_or_else(|| name.to_owned(), |c| c.label.clone())
    };
    let mut meter = Meter::new();
    let verified = lace_installer::install::install_bundle(
        &disk,
        &index,
        &selection,
        prefix.as_std_path(),
        |event| show(&mut meter, &event, &label),
    )?;
    meter.finish();
    let names: Vec<String> = selection.iter().map(|c| label(c)).collect();
    println!(
        "Installed in {prefix}: {}; {verified} executables verified",
        if names.is_empty() {
            "no apps".to_owned()
        } else {
            names.join(", ")
        }
    );
    Ok(())
}

/// Uma linha do progresso de `lace install` e do `lace update`: cada pedaço
/// com a barra do download, e os passos depois.
pub(crate) fn show(meter: &mut Meter, event: &Event, label: &dyn Fn(&str) -> String) {
    match event {
        Event::Chunk {
            index,
            count,
            components,
            download,
        } => {
            let what = if components.is_empty() {
                "Lace".to_owned()
            } else {
                components
                    .iter()
                    .map(|c| label(c))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            meter.start(format!("[{index}/{count}] {what}"), *download);
        }
        Event::Downloading { bytes } => meter.bytes(*bytes),
        Event::Replacing { files } => meter.note(&format!("Replacing {files} files")),
        Event::Verifying => meter.note("Verifying the executables"),
        Event::Start { .. } | Event::Progress { .. } => {}
    }
}

/// O payload de um instalador no disco: a pasta e o começo dos nomes dos
/// arquivos (vazio, ou `lace-<versão>-<plataforma>-` com os nomes que a
/// release publica).
pub(crate) struct DiskPayload {
    dir: Utf8PathBuf,
    prefix: String,
}

impl DiskPayload {
    /// O payload em `path`: a pasta do instalador, a pasta `payload/` dele,
    /// o `.tar.gz` da release (extraído em `work`), ou uma pasta com os
    /// arquivos da release desta plataforma.
    pub(crate) fn find(path: &Utf8Path, work: &Utf8Path) -> anyhow::Result<DiskPayload> {
        let plain = |dir: Utf8PathBuf| DiskPayload {
            dir,
            prefix: String::new(),
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
            let file = std::fs::File::open(path).with_context(|| format!("Opening {path}"))?;
            tar::Archive::new(flate2::read::GzDecoder::new(file))
                .unpack(work)
                .with_context(|| format!("Extracting {path}"))?;
            return find_payload(work)
                .map(plain)
                .with_context(|| format!("{path} does not contain a Lace installer"));
        }
        if path.is_dir()
            && let Some(prefix) = release_prefix(path)?
        {
            return Ok(DiskPayload {
                dir: path.to_owned(),
                prefix,
            });
        }
        bail!(
            "{path} is not a Lace installer (its folder, its payload/ folder, the release .tar.gz, \
             or a folder with the release files of this platform)"
        )
    }

    /// Um arquivo do payload.
    pub(crate) fn path(&self, file: &str) -> Utf8PathBuf {
        self.dir.join(format!("{}{file}", self.prefix))
    }

    /// O índice.
    pub(crate) fn index(&self) -> anyhow::Result<Index> {
        let path = self.path(payload::INDEX_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| format!("Reading {path}"))?;
        Index::parse(&text).with_context(|| format!("Invalid payload index: {path}"))
    }

    /// O manifesto de arquivos; `None` num payload anterior à 0.7.0.
    pub(crate) fn files(&self) -> anyhow::Result<Option<FilesManifest>> {
        let path = self.path(FILES_FILE);
        if !path.is_file() {
            return Ok(None);
        }
        FilesManifest::load(path.as_std_path()).map(Some)
    }
}

impl ChunkSource for DiskPayload {
    fn fetch(
        &self,
        chunk: &Chunk,
        downloaded: &mut dyn FnMut(u64),
    ) -> anyhow::Result<std::path::PathBuf> {
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
/// (`lace-0.7.0-linux-x64-`), se ela tem o índice de uma versão só.
fn release_prefix(dir: &Utf8Path) -> anyhow::Result<Option<String>> {
    let platform = Platform::current()
        .context("No bundle apps for this platform")?
        .as_str();
    let suffix = format!("-{platform}-{}", payload::INDEX_FILE);
    let mut found: Vec<String> = dir
        .read_dir_utf8()
        .with_context(|| format!("Reading {dir}"))?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name();
            (name.starts_with("lace-") && name.ends_with(&suffix))
                .then(|| name[..name.len() - payload::INDEX_FILE.len()].to_owned())
        })
        .collect();
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.pop()),
        _ => bail!("{dir} has the release files of more than one Lace version"),
    }
}

fn find_payload(dir: &Utf8Path) -> Option<Utf8PathBuf> {
    dir.read_dir_utf8()
        .ok()?
        .filter_map(Result::ok)
        .find_map(|e| {
            let payload = e.path().join(payload::PAYLOAD_DIR);
            payload
                .join(payload::INDEX_FILE)
                .is_file()
                .then_some(payload)
        })
}

/// Os pedaços da release desta versão: o índice agora, cada pedaço quando
/// for extraído.
fn remote(work: &Utf8Path) -> anyhow::Result<(Index, Arc<dyn ChunkSource>)> {
    let version = env!("CARGO_PKG_VERSION").to_owned();
    let platform = Platform::current()
        .context("No bundle apps for this platform")?
        .as_str()
        .to_owned();
    let file = payload::release_asset(&version, &platform, payload::INDEX_FILE);
    let path =
        release::download_checked_quiet(&version, &file, work, &mut |_| {}).map_err(|e| {
            e.context(format!(
            "Release v{version} does not publish the bundle apps separately (that started after \
             0.1.0); use an installer of this version: lace install --from <folder|file.tar.gz>"
        ))
        })?;
    let text = std::fs::read_to_string(&path).with_context(|| format!("Reading {path}"))?;
    let index: Index =
        serde_json::from_str(&text).with_context(|| format!("Invalid {file} in the release"))?;
    let source = ReleasePayload {
        version,
        platform,
        dir: work.to_owned(),
    };
    Ok((index, Arc::new(source)))
}

/// Os pedaços publicados na release, baixados um a um quando são extraídos,
/// para `dir` (que guarda também o `SHA256SUMS` dela).
pub(crate) struct ReleasePayload {
    pub(crate) version: String,
    pub(crate) platform: String,
    pub(crate) dir: Utf8PathBuf,
}

impl ChunkSource for ReleasePayload {
    fn fetch(
        &self,
        chunk: &Chunk,
        downloaded: &mut dyn FnMut(u64),
    ) -> anyhow::Result<std::path::PathBuf> {
        let file = payload::release_asset(&self.version, &self.platform, &chunk.file);
        let path = release::download_checked_quiet(&self.version, &file, &self.dir, downloaded)?;
        Ok(path.into_std_path_buf())
    }
}
