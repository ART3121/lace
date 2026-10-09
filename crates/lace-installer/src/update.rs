//! Atualizar uma instalação para a versão nova trocando só os arquivos que
//! mudaram. É o `lace update` quando a instalação tem o manifesto de arquivos
//! da versão dela (`toolchain/files.json`, [`crate::files`]) e a release nova
//! publica o seu.
//!
//! [`plan`] compara os dois manifestos para os componentes instalados, mais o
//! que a versão nova exige deles. Entram os arquivos novos, os de outro
//! SHA-256 e os que sumiram do disco; saem os que a versão nova não tem. Os
//! pedaços a baixar são só os que trazem algum arquivo que entra.
//!
//! [`apply`] baixa esses pedaços e extrai só os arquivos que entram numa pasta
//! provisória (`.instalando-<pid>`), conferindo cada um com o manifesto novo.
//! Depois troca os arquivos, guardando os antigos em `.antigo-<pid>`, grava o
//! manifesto novo, abre o bundle e confere os executáveis. Uma falha em
//! qualquer passo devolve os antigos, e a instalação fica como estava.
//!
//! O `lace` que roda a atualização é um dos arquivos trocados. O Windows
//! deixa renomear um executável em uso, mas não apagá-lo: lá, a pasta
//! `.antigo-<pid>` fica para trás com ele e sai na próxima vez
//! ([`clean_leftovers`]).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, bail};
use lace_core::{Platform, Toolchain};

use crate::add::ChunkSource;
use crate::files::{self, FilesManifest};
use crate::install::{Event, RECEIPT_FILE, Receipt};
use crate::payload::{Chunk, Index};
use crate::plan::Selection;

/// O nome que [`Plan::changed`] dá aos arquivos sempre instalados: o `lace`
/// e o cabeçalho do bundle.
pub const LACE: &str = "lace";

/// Um pedaço que a atualização baixa, com os arquivos que entram dele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedChunk {
    /// O pedaço, como o índice novo o descreve.
    pub chunk: Chunk,
    /// Os arquivos que entram, com o valor do manifesto novo.
    pub files: BTreeMap<String, String>,
}

/// O que a atualização vai fazer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// A versão instalada.
    pub from: String,
    /// A versão nova.
    pub to: String,
    /// O bundle da versão nova.
    pub bundle: String,
    /// Os componentes depois da atualização, na ordem do índice novo.
    pub components: Vec<String>,
    /// Os que entram porque a versão nova os exige de um instalado.
    pub added: Vec<String>,
    /// Os instalados que a versão nova não tem mais: os arquivos deles saem.
    pub removed: Vec<String>,
    /// Os componentes com algum arquivo que entra ou sai, na ordem do índice,
    /// com [`LACE`] na frente quando o `lace` ou o cabeçalho mudam.
    pub changed: Vec<String>,
    /// Os pedaços a baixar, na ordem do índice.
    pub chunks: Vec<PlannedChunk>,
    /// Os arquivos que saem, relativos à instalação.
    pub stale: Vec<String>,
}

impl Plan {
    /// Bytes a baixar.
    pub fn download(&self) -> u64 {
        self.chunks.iter().map(|c| c.chunk.download).sum()
    }

    /// Quantos arquivos entram.
    pub fn files(&self) -> usize {
        self.chunks.iter().map(|c| c.files.len()).sum()
    }
}

/// O resultado de [`apply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// Arquivos que entraram (novos ou trocados).
    pub replaced: usize,
    /// Arquivos que saíram.
    pub removed: usize,
    /// Quantos executáveis tiveram o hash conferido.
    pub verified: usize,
    /// O atalho do Lace Studio no menu de aplicativos, se há um ([`crate::desktop`]).
    pub shortcut: Option<PathBuf>,
}

/// Um caminho do manifesto, relativo e sem `..`: um manifesto não escreve
/// nem apaga fora da instalação.
fn check_path(rel: &str) -> anyhow::Result<()> {
    let path = Path::new(rel);
    if rel.is_empty() || !path.components().all(|c| matches!(c, Component::Normal(_))) {
        bail!("The file manifest has an invalid path: {rel}");
    }
    Ok(())
}

/// O que atualizar a instalação em `prefix`, com o manifesto `installed`
/// (o dela), para a versão do índice `index` e do manifesto `new`.
pub fn plan(
    prefix: &Path,
    installed: &FilesManifest,
    index: &Index,
    new: &FilesManifest,
) -> anyhow::Result<Plan> {
    let current = Platform::current().map(Platform::as_str);
    if current != Some(index.platform.as_str()) {
        bail!(
            "Release {} is for {}, and this machine is {}",
            index.lace_version,
            index.platform,
            current.unwrap_or("an unsupported platform")
        );
    }
    if installed.platform != index.platform {
        bail!(
            "The installation is for {}, and release {} is for {}",
            installed.platform,
            index.lace_version,
            index.platform
        );
    }
    if new.lace_version != index.lace_version
        || new.bundle != index.bundle
        || new.platform != index.platform
    {
        bail!(
            "The file manifest of release {} does not match its index",
            index.lace_version
        );
    }
    let toolchain = prefix.join("toolchain");
    let tc = Toolchain::open(toolchain.to_str().context("Path is not UTF-8")?).with_context(
        || {
            format!(
                "Could not open the bundle installed in {}",
                toolchain.display()
            )
        },
    )?;
    let before: Selection = tc
        .manifest()
        .components
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let mut after = Selection::new();
    for name in &before {
        if index.component(name).is_some() {
            crate::plan::add_with_requirements(index, &mut after, name);
        }
    }

    let old_files = installed.files_for(&before);
    let new_files = new.files_for(&after);
    let mut changed: BTreeSet<&str> = BTreeSet::new();
    let mut by_chunk: BTreeMap<&str, BTreeMap<String, String>> = BTreeMap::new();
    for (&rel, &(hash, group)) in &new_files {
        check_path(rel)?;
        let same = old_files.get(rel).is_some_and(|&(old, _)| old == hash);
        if same && fs::symlink_metadata(prefix.join(rel)).is_ok() {
            continue;
        }
        by_chunk
            .entry(group.chunk.as_str())
            .or_default()
            .insert(rel.to_owned(), hash.to_owned());
        mark(&mut changed, &group.components);
    }
    let mut stale = Vec::new();
    for (&rel, &(_, group)) in &old_files {
        if !new_files.contains_key(rel) {
            check_path(rel)?;
            stale.push(rel.to_owned());
            mark(&mut changed, &group.components);
        }
    }

    let mut chunks = Vec::new();
    for chunk in &index.chunks {
        if let Some(files) = by_chunk.remove(chunk.file.as_str()) {
            chunks.push(PlannedChunk {
                chunk: chunk.clone(),
                files,
            });
        }
    }
    if let Some(file) = by_chunk.keys().next() {
        bail!(
            "The file manifest of release {} names the chunk {file}, which its index does not have",
            index.lace_version
        );
    }

    let in_order = |set: &dyn Fn(&str) -> bool| -> Vec<String> {
        index
            .components
            .iter()
            .filter(|c| set(&c.name))
            .map(|c| c.name.clone())
            .collect()
    };
    let mut changed_names = Vec::new();
    if changed.contains(LACE) {
        changed_names.push(LACE.to_owned());
    }
    changed_names.extend(in_order(&|n| changed.contains(n)));
    // Um componente que a versão nova não conhece não está no índice dela.
    changed_names.extend(
        changed
            .iter()
            .filter(|n| **n != LACE && index.component(n).is_none())
            .map(|n| (*n).to_owned()),
    );
    Ok(Plan {
        from: installed.lace_version.clone(),
        to: index.lace_version.clone(),
        bundle: index.bundle.clone(),
        components: in_order(&|n| after.contains(n)),
        added: in_order(&|n| after.contains(n) && !before.contains(n)),
        removed: before
            .iter()
            .filter(|n| !after.contains(*n))
            .cloned()
            .collect(),
        changed: changed_names,
        chunks,
        stale,
    })
}

/// Anota os componentes de um grupo em `changed` (os sempre instalados como
/// [`LACE`]).
fn mark<'a>(changed: &mut BTreeSet<&'a str>, components: &'a [String]) {
    if components.is_empty() {
        changed.insert(LACE);
    }
    changed.extend(components.iter().map(String::as_str));
}

/// Faz o [`Plan`] na instalação em `prefix`, com os pedaços de `source` e o
/// manifesto `new` da versão nova, chamando `on` a cada passo.
pub fn apply(
    source: &dyn ChunkSource,
    plan: &Plan,
    new: &FilesManifest,
    prefix: &Path,
    mut on: impl FnMut(Event),
) -> anyhow::Result<Applied> {
    let pid = std::process::id();
    let staging = prefix.join(format!(".instalando-{pid}"));
    let backup = prefix.join(format!(".antigo-{pid}"));
    for dir in [&staging, &backup] {
        let _ = fs::remove_dir_all(dir);
    }
    fs::create_dir_all(&staging).with_context(|| format!("Creating {}", staging.display()))?;
    let result = stage(source, plan, new, &staging, &mut on).and_then(|()| {
        let mut journal = Journal {
            prefix,
            backup: &backup,
            steps: Vec::new(),
        };
        let result = swap(&mut journal, plan, &staging, &mut on);
        if result.is_err() {
            journal.undo();
        }
        result
    });
    let _ = fs::remove_dir_all(&staging);
    let verified = match result {
        Ok(verified) => verified,
        Err(e) => {
            let _ = fs::remove_dir_all(&backup);
            return Err(e);
        }
    };
    remove_empty_dirs(prefix, &plan.stale);
    // No Windows, o `lace.exe` em uso fica, e a pasta com ele.
    remove_what_can_be_removed(&backup);

    // O recibo (Linux, macOS) passa a ser o da versão nova. No Windows não há
    // recibo: quem sabe é o bundle.
    if let Some(mut receipt) = Receipt::load(prefix) {
        receipt.lace_version = plan.to.clone();
        receipt.bundle = plan.bundle.clone();
        receipt.components = plan.components.clone();
        let path = prefix.join(RECEIPT_FILE);
        fs::write(&path, serde_json::to_string_pretty(&receipt)? + "\n")
            .with_context(|| format!("Writing {}", path.display()))?;
    }
    let shortcut = crate::desktop::sync(prefix)?;
    Ok(Applied {
        replaced: plan.files(),
        removed: plan.stale.len(),
        verified,
        shortcut,
    })
}

/// Baixa os pedaços e extrai em `staging` os arquivos que entram, conferidos
/// com o manifesto novo, que vai junto.
fn stage(
    source: &dyn ChunkSource,
    plan: &Plan,
    new: &FilesManifest,
    staging: &Path,
    on: &mut impl FnMut(Event),
) -> anyhow::Result<()> {
    on(Event::Start {
        total: plan.download(),
    });
    let count = plan.chunks.len();
    for (i, planned) in plan.chunks.iter().enumerate() {
        on(Event::Chunk {
            index: i + 1,
            count,
            components: planned.chunk.components.clone(),
            download: planned.chunk.download,
        });
        let path = source.fetch(&planned.chunk, &mut |bytes| {
            on(Event::Downloading { bytes })
        })?;
        let wanted: BTreeSet<&str> = planned.files.keys().map(String::as_str).collect();
        crate::install::extract_entries(&path, staging, &wanted)?;
        for (rel, expected) in &planned.files {
            if &files::digest(&staging.join(rel))? != expected {
                bail!(
                    "{rel} in {} does not match the file manifest of Lace {}",
                    planned.chunk.file,
                    plan.to
                );
            }
        }
    }
    let manifest = staging.join(files::INSTALLED);
    fs::create_dir_all(manifest.parent().expect("tem pai"))?;
    new.save(&manifest)
}

/// Troca os arquivos e confere o bundle. Quem chama desfaz o que o
/// `journal` registrou se der erro.
fn swap(
    journal: &mut Journal,
    plan: &Plan,
    staging: &Path,
    on: &mut impl FnMut(Event),
) -> anyhow::Result<usize> {
    on(Event::Replacing {
        files: plan.files() + plan.stale.len(),
    });
    for planned in &plan.chunks {
        for rel in planned.files.keys() {
            journal.put(rel, &staging.join(rel))?;
        }
    }
    journal.put(files::INSTALLED, &staging.join(files::INSTALLED))?;
    for rel in &plan.stale {
        journal.take(rel)?;
    }

    on(Event::Verifying);
    let toolchain = journal.prefix.join("toolchain");
    let tc = Toolchain::open(toolchain.to_str().context("Path is not UTF-8")?)
        .context("Could not open the bundle after updating")?;
    let installed: Selection = tc
        .manifest()
        .components
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let expected: Selection = plan.components.iter().cloned().collect();
    if installed != expected {
        bail!(
            "The updated bundle has the components {installed:?}, and the update expected {expected:?}"
        );
    }
    let mismatches = tc.verify()?;
    if let Some(m) = mismatches.first() {
        bail!(
            "Executables that do not match the manifest after updating: {} (the first: {})",
            mismatches.len(),
            m.path
        );
    }
    Ok(tc.verified_files())
}

/// Um passo da troca, para desfazer.
enum Step {
    /// Um arquivo novo foi posto aqui.
    Placed(PathBuf),
    /// O arquivo de `from` foi guardado em `to`.
    MovedAside { from: PathBuf, to: PathBuf },
    /// Esta pasta foi criada.
    CreatedDir(PathBuf),
}

/// O que a troca fez, na ordem, para desfazer de trás para frente.
struct Journal<'a> {
    prefix: &'a Path,
    backup: &'a Path,
    steps: Vec<Step>,
}

impl Journal<'_> {
    /// Põe `src` em `rel`, guardando o que havia lá.
    fn put(&mut self, rel: &str, src: &Path) -> anyhow::Result<()> {
        let dest = self.prefix.join(rel);
        self.take(rel)?;
        let mut missing = Vec::new();
        let mut dir = dest.parent();
        while let Some(d) = dir {
            if fs::symlink_metadata(d).is_ok() {
                break;
            }
            missing.push(d.to_owned());
            dir = d.parent();
        }
        for d in missing.into_iter().rev() {
            fs::create_dir(&d).with_context(|| format!("Creating {}", d.display()))?;
            self.steps.push(Step::CreatedDir(d));
        }
        fs::rename(src, &dest).with_context(|| format!("Installing {}", dest.display()))?;
        self.steps.push(Step::Placed(dest));
        Ok(())
    }

    /// Tira `rel` da instalação, guardando-o em `.antigo-<pid>`.
    fn take(&mut self, rel: &str) -> anyhow::Result<()> {
        let from = self.prefix.join(rel);
        if fs::symlink_metadata(&from).is_err() {
            return Ok(());
        }
        let to = self.backup.join(rel);
        fs::create_dir_all(to.parent().expect("tem pai"))
            .with_context(|| format!("Creating {}", self.backup.display()))?;
        fs::rename(&from, &to).with_context(|| format!("Moving {} aside", from.display()))?;
        self.steps.push(Step::MovedAside { from, to });
        Ok(())
    }

    /// Desfaz tudo, do último passo ao primeiro.
    fn undo(&mut self) {
        while let Some(step) = self.steps.pop() {
            match step {
                Step::Placed(path) => {
                    let _ = fs::remove_file(&path);
                }
                Step::MovedAside { from, to } => {
                    let _ = fs::rename(&to, &from);
                }
                Step::CreatedDir(dir) => {
                    let _ = fs::remove_dir(&dir);
                }
            }
        }
    }
}

/// Apaga as pastas que ficaram vazias quando os arquivos de `stale` saíram,
/// dentro de `toolchain/`.
fn remove_empty_dirs(prefix: &Path, stale: &[String]) {
    let toolchain = prefix.join("toolchain");
    for rel in stale {
        let mut dir = prefix.join(rel).parent().map(Path::to_owned);
        while let Some(d) = dir {
            if !d.starts_with(&toolchain) || d == toolchain || fs::remove_dir(&d).is_err() {
                break;
            }
            dir = d.parent().map(Path::to_owned);
        }
    }
}

/// Apaga `dir` com tudo dentro, menos o que o sistema não deixa apagar (no
/// Windows, um executável em uso) e as pastas que o contêm.
fn remove_what_can_be_removed(dir: &Path) {
    if fs::remove_dir_all(dir).is_ok() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            remove_what_can_be_removed(&path);
        } else {
            let _ = fs::remove_file(&path);
        }
    }
    let _ = fs::remove_dir(dir);
}

/// Apaga as pastas `.antigo-<pid>` que uma atualização anterior deixou em
/// `prefix` (no Windows, com o `lace.exe` que estava rodando). A desta
/// execução fica.
pub fn clean_leftovers(prefix: &Path) {
    let own = format!(".antigo-{}", std::process::id());
    let Ok(entries) = fs::read_dir(prefix) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".antigo-") && name != own {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_paths_stay_inside_the_installation() {
        for ok in ["bin/lace", "toolchain/oss/lib/libc.so.6"] {
            assert!(check_path(ok).is_ok(), "{ok}");
        }
        for bad in ["", "../x", "toolchain/../../x", "/etc/passwd", "./x"] {
            assert!(check_path(bad).is_err(), "{bad}");
        }
        #[cfg(windows)]
        assert!(check_path("C:/Windows/x").is_err());
    }
}
