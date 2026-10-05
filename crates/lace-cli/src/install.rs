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
//!   ou a pasta `payload/` dele.
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
use lace_installer::payload::{self, Chunk, Index};
use lace_installer::plan::{self, Selection};
use lace_installer::tui;

use crate::installation;
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
    let work = release::work_dir()?;
    let work_dir = Utf8Path::from_path(work.path())
        .context("Temporary folder is not UTF-8")?
        .to_owned();
    let (index, source) = match from {
        Some(path) => local(path, &work_dir)?,
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
    let report = add::add(&*source, &index, prefix.as_std_path(), &new, |event| {
        if !text {
            return;
        }
        match event {
            Event::Chunk {
                index: i,
                count,
                components,
            } => {
                let what: Vec<String> = components.iter().map(|c| label(c)).collect();
                println!("  [{i}/{count}] {}", what.join(", "));
            }
            Event::Verifying => println!("  Verifying the executables"),
            _ => {}
        }
    })?;
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

/// Os pedaços de um instalador no disco: a pasta dele, a pasta `payload/`
/// dele, ou o `.tar.gz` da release (extraído em `work`).
fn local(path: &Utf8Path, work: &Utf8Path) -> anyhow::Result<(Index, Arc<dyn ChunkSource>)> {
    let payload_dir = if path
        .join(payload::PAYLOAD_DIR)
        .join(payload::INDEX_FILE)
        .is_file()
    {
        path.join(payload::PAYLOAD_DIR)
    } else if path.join(payload::INDEX_FILE).is_file() {
        path.to_owned()
    } else if path.is_file() {
        let file = std::fs::File::open(path).with_context(|| format!("Opening {path}"))?;
        tar::Archive::new(flate2::read::GzDecoder::new(file))
            .unpack(work)
            .with_context(|| format!("Extracting {path}"))?;
        find_payload(work).with_context(|| format!("{path} does not contain a Lace installer"))?
    } else {
        bail!("{path} is not a Lace installer (neither its folder nor the release .tar.gz)");
    };
    let index = Index::load(payload_dir.as_std_path())?;
    Ok((
        index,
        Arc::new(LocalPayload(payload_dir.into_std_path_buf())),
    ))
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

/// Os pedaços publicados na release, baixados um a um quando são extraídos.
struct ReleasePayload {
    version: String,
    platform: String,
    dir: Utf8PathBuf,
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
