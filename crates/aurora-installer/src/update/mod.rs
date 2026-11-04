//! Atualizar uma instalação para a versão nova trocando só os arquivos que
//! mudaram. É o `aurora update` quando a instalação tem o manifesto de arquivos
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
//! O `aurora` que roda a atualização é um dos arquivos trocados. O Windows
//! deixa renomear um executável em uso, mas não apagá-lo: lá, a pasta
//! `.antigo-<pid>` fica para trás com ele e sai na próxima vez
//! ([`clean_leftovers`]).
//!
//! Um processo que morre no meio da troca (o Studio fechado, um Ctrl+C
//! duplo, falta de energia) não chega a desfazê-la. Antes de trocar, o
//! [`apply`] grava o que vai fazer num diário (`.atualizando.json`), e a
//! próxima atualização ou instalação desfaz o que ficou feito ([`recover`]),
//! com o diário e as pastas provisórias que ele cita. Uma trava
//! (`.instalacao.lock`, [`lock`]) impede duas operações na mesma instalação ao
//! mesmo tempo, e que uma desfaça a troca de outra que ainda está rodando.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, bail};
use aurora_core::{CancelToken, Platform, Toolchain};
use serde::{Deserialize, Serialize};

use crate::add::ChunkSource;
use crate::files::{self, FilesManifest};
use crate::install::{Event, RECEIPT_FILE, Receipt};
use crate::payload::{Chunk, Index};
use crate::plan::Selection;

mod flow;

pub use flow::*;

/// O nome que [`Plan::changed`] dá aos arquivos sempre instalados: o `aurora`
/// e o cabeçalho do bundle.
pub const AURORA: &str = "aurora";

/// O diário da troca em andamento, na raiz da instalação.
pub const JOURNAL_FILE: &str = ".atualizando.json";
/// A trava das instalações e atualizações, na raiz da instalação.
pub const LOCK_FILE: &str = ".instalacao.lock";

/// O que [`apply`] grava antes de trocar os arquivos, para [`recover`]
/// desfazer a troca se o processo morrer no meio dela.
#[derive(Debug, Serialize, Deserialize)]
struct SavedJournal {
    /// A versão para a qual a troca ia.
    to: String,
    /// A pasta dos arquivos novos (`.instalando-<pid>`), na raiz.
    staging: String,
    /// A pasta dos antigos (`.antigo-<pid>`), na raiz.
    backup: String,
    /// Os arquivos que entram, na ordem da troca.
    put: Vec<String>,
    /// Os que saem, depois deles.
    take: Vec<String>,
}

/// A trava de uma instalação ou atualização, solta quando sai de escopo (e
/// pelo sistema, se o processo morrer).
#[derive(Debug)]
pub struct InstallLock(#[allow(dead_code)] fs::File);

/// Trava a instalação em `prefix` para instalar ou atualizar, sem esperar:
/// com outra operação rodando nela (outro `aurora`, o Studio), um erro que diz
/// isso.
pub fn lock(prefix: &Path) -> anyhow::Result<InstallLock> {
    // O erro tipado por dentro do `anyhow`: quem recebe na fronteira
    // ([`crate::error::from_anyhow`]) o recupera com o código dele.
    try_lock(prefix)?.ok_or_else(|| {
        anyhow::Error::new(crate::Error::InstallInProgress {
            prefix: camino::Utf8PathBuf::from(prefix.to_string_lossy().into_owned()),
        })
    })
}

/// A trava, ou `None` se outra operação a segura.
fn try_lock(prefix: &Path) -> anyhow::Result<Option<InstallLock>> {
    let path = prefix.join(LOCK_FILE);
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("Creating {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(InstallLock(file))),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(e)) => {
            Err(e).with_context(|| format!("Locking {}", path.display()))
        }
    }
}

/// Grava o diário da troca de `plan`, por inteiro ou nada: um arquivo
/// provisório, que só ganha o nome do diário depois de gravado no disco.
fn save_journal(prefix: &Path, saved: &SavedJournal) -> anyhow::Result<()> {
    use std::io::Write;
    let path = prefix.join(JOURNAL_FILE);
    let temporary = prefix.join(format!("{JOURNAL_FILE}.tmp"));
    let write = || -> std::io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(serde_json::to_string(saved)?.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &path)
    };
    write().with_context(|| format!("Writing {}", path.display()))
}

/// Tira o diário: a troca terminou, ou foi desfeita, ou a instalação inteira
/// foi trocada depois dela.
pub fn forget_journal(prefix: &Path) {
    let _ = fs::remove_file(prefix.join(JOURNAL_FILE));
    let _ = fs::remove_file(prefix.join(format!("{JOURNAL_FILE}.tmp")));
}

/// Desfaz a troca que um processo morto deixou pela metade em `prefix`, se
/// o diário dela ([`JOURNAL_FILE`]) está lá: cada arquivo antigo volta para o
/// lugar, e os novos saem, como numa falha do [`apply`]. Devolve a versão
/// para a qual a troca ia, ou `None` se não havia o que desfazer.
///
/// Não mexe em nada com outra operação rodando na instalação ([`lock`]): a
/// troca dela não foi interrompida. Nem sem a pasta dos arquivos novos, que
/// diz quais já entraram: sem ela (um assistente de instalação que rodou
/// depois a apaga, e troca a instalação inteira), o diário não vale mais, e
/// só ele sai.
///
/// # Erros
///
/// Um diário ilegível, ou arquivos que não voltaram para o lugar. Nos dois
/// casos o diário fica, para a próxima vez.
pub fn recover(prefix: &Path) -> anyhow::Result<Option<String>> {
    let path = prefix.join(JOURNAL_FILE);
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    let Some(_lock) = try_lock(prefix)? else {
        return Ok(None);
    };
    let saved: SavedJournal =
        serde_json::from_str(&text).with_context(|| format!("Reading {}", path.display()))?;
    let own_dir = |name: &str, start: &str| {
        name.starts_with(start) && !name.contains(['/', '\\']) && !name.contains("..")
    };
    if !own_dir(&saved.staging, ".instalando-") || !own_dir(&saved.backup, ".antigo-") {
        bail!("{} names folders that are not its own", path.display());
    }
    for rel in saved.put.iter().chain(&saved.take) {
        check_path(rel)?;
    }
    let staging = prefix.join(&saved.staging);
    if !staging.is_dir() {
        forget_journal(prefix);
        return Ok(None);
    }
    let backup = prefix.join(&saved.backup);
    // Os novos saem para cá, e não para o nada: no Windows um executável em
    // uso (o `aurora` novo, que pode ser este) só pode ser renomeado.
    let discarded = backup.join(".desfeitos");
    let put: BTreeSet<&str> = saved.put.iter().map(String::as_str).collect();
    let exists = |p: &Path| fs::symlink_metadata(p).is_ok();
    let mut failed = Vec::new();
    for rel in saved.put.iter().chain(&saved.take).rev() {
        let current = prefix.join(rel);
        let old = backup.join(rel);
        let restore = exists(&old);
        // Um arquivo que entrou sem antigo no lugar: não está mais na pasta
        // dos novos.
        let placed = !restore && put.contains(rel.as_str()) && !exists(&staging.join(rel));
        if (restore || placed) && exists(&current) {
            let aside = discarded.join(rel);
            let moved = fs::create_dir_all(aside.parent().expect("tem pai"))
                .and_then(|()| fs::rename(&current, &aside));
            if let Err(e) = moved {
                failed.push(format!("{rel}: {e}"));
                continue;
            }
        }
        if restore {
            let moved = fs::create_dir_all(current.parent().expect("tem pai"))
                .and_then(|()| fs::rename(&old, &current));
            if let Err(e) = moved {
                failed.push(format!("{rel}: {e}"));
            }
        }
    }
    if let Some(first) = failed.first() {
        bail!(
            "An update to AURORA {} was interrupted, and {} of its files could not be put back \
             (the first: {first}); the next aurora update tries again",
            saved.to,
            failed.len()
        );
    }
    remove_empty_dirs(prefix, &saved.put);
    let _ = fs::remove_dir_all(&staging);
    remove_what_can_be_removed(&backup);
    forget_journal(prefix);
    Ok(Some(saved.to))
}

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
    /// com [`AURORA`] na frente quando o `aurora` ou o cabeçalho mudam.
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
    /// O atalho da AURORA IDE no menu de aplicativos, se há um ([`crate::desktop`]).
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
            index.aurora_version,
            index.platform,
            current.unwrap_or("an unsupported platform")
        );
    }
    if installed.platform != index.platform {
        bail!(
            "The installation is for {}, and release {} is for {}",
            installed.platform,
            index.aurora_version,
            index.platform
        );
    }
    if new.aurora_version != index.aurora_version
        || new.bundle != index.bundle
        || new.platform != index.platform
    {
        bail!(
            "The file manifest of release {} does not match its index",
            index.aurora_version
        );
    }
    let toolchain = prefix.join("toolchain");
    let tc =
        Toolchain::open(toolchain.to_str().context("Path is not UTF-8")?).with_context(|| {
            format!(
                "Could not open the bundle installed in {}",
                toolchain.display()
            )
        })?;
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
            index.aurora_version
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
    if changed.contains(AURORA) {
        changed_names.push(AURORA.to_owned());
    }
    changed_names.extend(in_order(&|n| changed.contains(n)));
    // Um componente que a versão nova não conhece não está no índice dela.
    changed_names.extend(
        changed
            .iter()
            .filter(|n| **n != AURORA && index.component(n).is_none())
            .map(|n| (*n).to_owned()),
    );
    Ok(Plan {
        from: installed.aurora_version.clone(),
        to: index.aurora_version.clone(),
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
/// [`AURORA`]).
fn mark<'a>(changed: &mut BTreeSet<&'a str>, components: &'a [String]) {
    if components.is_empty() {
        changed.insert(AURORA);
    }
    changed.extend(components.iter().map(String::as_str));
}

/// Faz o [`Plan`] na instalação em `prefix`, com os pedaços de `source` e o
/// manifesto `new` da versão nova, chamando `on` a cada passo. Sem
/// cancelamento: ver [`apply_with`].
pub fn apply(
    source: &dyn ChunkSource,
    plan: &Plan,
    new: &FilesManifest,
    prefix: &Path,
    on: impl FnMut(Event),
) -> anyhow::Result<Applied> {
    apply_with(source, plan, new, prefix, on, &CancelToken::new())
}

/// [`apply`] conferindo `cancel` antes de cada pedaço e antes da troca dos
/// arquivos. Durante a troca o pedido é ignorado: o diário já cobre um
/// processo que morre no meio, e parar ali só repetiria o que ele desfaz.
/// Cancelado, devolve [`crate::Error::Cancelled`] dentro do `anyhow`.
pub fn apply_with(
    source: &dyn ChunkSource,
    plan: &Plan,
    new: &FilesManifest,
    prefix: &Path,
    mut on: impl FnMut(Event),
    cancel: &CancelToken,
) -> anyhow::Result<Applied> {
    let _lock = lock(prefix)?;
    let pid = std::process::id();
    let staging_name = format!(".instalando-{pid}");
    let backup_name = format!(".antigo-{pid}");
    let staging = prefix.join(&staging_name);
    let backup = prefix.join(&backup_name);
    for dir in [&staging, &backup] {
        let _ = fs::remove_dir_all(dir);
    }
    fs::create_dir_all(&staging).with_context(|| format!("Creating {}", staging.display()))?;
    let mut left_to_recover = false;
    let result = stage(source, plan, new, &staging, &mut on, cancel).and_then(|()| {
        if cancel.is_cancelled() {
            return Err(anyhow::Error::new(crate::Error::Cancelled));
        }
        let mut put: Vec<String> = plan
            .chunks
            .iter()
            .flat_map(|c| c.files.keys().cloned())
            .collect();
        put.push(files::INSTALLED.to_owned());
        save_journal(
            prefix,
            &SavedJournal {
                to: plan.to.clone(),
                staging: staging_name,
                backup: backup_name,
                put,
                take: plan.stale.clone(),
            },
        )?;
        let mut journal = Journal {
            prefix,
            backup: &backup,
            steps: Vec::new(),
        };
        let result = swap(&mut journal, plan, &staging, &mut on);
        if result.is_err() && !journal.undo() {
            // Sem desfazer tudo, ficam o diário e as duas pastas: a próxima
            // atualização termina de desfazer ([`recover`]).
            left_to_recover = true;
            return result.context(
                "Some files could not be put back; the next aurora update finishes undoing it",
            );
        }
        forget_journal(prefix);
        result
    });
    if left_to_recover {
        return Err(result.expect_err("the swap failed"));
    }
    let _ = fs::remove_dir_all(&staging);
    let verified = match result {
        Ok(verified) => verified,
        Err(e) => {
            let _ = fs::remove_dir_all(&backup);
            return Err(e);
        }
    };
    remove_empty_dirs(prefix, &plan.stale);
    // No Windows, o `aurora.exe` em uso fica, e a pasta com ele.
    remove_what_can_be_removed(&backup);

    // O recibo (Linux, macOS) passa a ser o da versão nova. No Windows não há
    // recibo: quem sabe é o bundle.
    if let Some(mut receipt) = Receipt::load(prefix) {
        receipt.aurora_version = plan.to.clone();
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
    cancel: &CancelToken,
) -> anyhow::Result<()> {
    on(Event::Start {
        total: plan.download(),
    });
    let count = plan.chunks.len();
    for (i, planned) in plan.chunks.iter().enumerate() {
        // Cada pedaço vem antes de a instalação mudar: o cancelamento para
        // aqui, sem nada a desfazer.
        if cancel.is_cancelled() {
            return Err(anyhow::Error::new(crate::Error::Cancelled));
        }
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
                    "{rel} in {} does not match the file manifest of AURORA {}",
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

    /// Desfaz tudo, do último passo ao primeiro. Diz se todo arquivo antigo
    /// voltou para o lugar e todo novo saiu (uma pasta que ficou não conta).
    fn undo(&mut self) -> bool {
        let mut complete = true;
        while let Some(step) = self.steps.pop() {
            match step {
                Step::Placed(path) => {
                    complete &= fs::remove_file(&path).is_ok();
                }
                Step::MovedAside { from, to } => {
                    complete &= fs::rename(&to, &from).is_ok();
                }
                Step::CreatedDir(dir) => {
                    let _ = fs::remove_dir(&dir);
                }
            }
        }
        complete
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

/// Apaga as pastas `.antigo-<pid>` e `.instalando-<pid>` que uma operação
/// anterior deixou em `prefix` (no Windows, a `.antigo-<pid>` fica com o
/// `aurora.exe` que estava rodando). As desta execução ficam. Com outra
/// operação rodando, ou com uma troca a desfazer ([`recover`]), nada sai: as
/// pastas são dela.
pub fn clean_leftovers(prefix: &Path) {
    if prefix.join(JOURNAL_FILE).exists() {
        return;
    }
    let Ok(Some(_lock)) = try_lock(prefix) else {
        return;
    };
    let pid = std::process::id();
    let own = [format!(".antigo-{pid}"), format!(".instalando-{pid}")];
    let Ok(entries) = fs::read_dir(prefix) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let leftover = name.starts_with(".antigo-") || name.starts_with(".instalando-");
        if leftover && !own.iter().any(|o| *o == name) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_paths_stay_inside_the_installation() {
        for ok in ["bin/aurora", "toolchain/oss/lib/libc.so.6"] {
            assert!(check_path(ok).is_ok(), "{ok}");
        }
        for bad in ["", "../x", "toolchain/../../x", "/etc/passwd", "./x"] {
            assert!(check_path(bad).is_err(), "{bad}");
        }
        #[cfg(windows)]
        assert!(check_path("C:/Windows/x").is_err());
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(path: &Path) -> Option<String> {
        fs::read_to_string(path).ok()
    }

    /// Uma troca que morreu no meio: o `aurora` já trocado, o `a` tirado sem o
    /// novo no lugar, o `novo` (sem antigo) já posto, e o resto por fazer.
    fn interrupted(prefix: &Path) {
        let staging = prefix.join(".instalando-1");
        let backup = prefix.join(".antigo-1");
        write(&backup.join("bin/aurora"), "aurora velho");
        write(&prefix.join("bin/aurora"), "aurora novo");
        write(&backup.join("toolchain/a"), "a velho");
        write(&staging.join("toolchain/a"), "a novo");
        write(&prefix.join("toolchain/sub/novo"), "novo");
        write(&staging.join("toolchain/depois"), "depois");
        write(&prefix.join("toolchain/files.json"), "manifesto velho");
        write(&staging.join("toolchain/files.json"), "manifesto novo");
        write(&prefix.join("toolchain/sai"), "sai");
        let put = [
            "bin/aurora",
            "toolchain/a",
            "toolchain/sub/novo",
            "toolchain/depois",
        ];
        save_journal(
            prefix,
            &SavedJournal {
                to: "9.9.9".into(),
                staging: ".instalando-1".into(),
                backup: ".antigo-1".into(),
                put: put
                    .iter()
                    .map(|s| (*s).to_owned())
                    .chain([files::INSTALLED.to_owned()])
                    .collect(),
                take: vec!["toolchain/sai".into()],
            },
        )
        .unwrap();
    }

    #[test]
    fn an_interrupted_swap_is_undone_on_the_next_run() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path();
        interrupted(prefix);
        assert_eq!(recover(prefix).unwrap().as_deref(), Some("9.9.9"));
        assert_eq!(
            read(&prefix.join("bin/aurora")).as_deref(),
            Some("aurora velho")
        );
        assert_eq!(
            read(&prefix.join("toolchain/a")).as_deref(),
            Some("a velho")
        );
        assert_eq!(
            read(&prefix.join("toolchain/files.json")).as_deref(),
            Some("manifesto velho")
        );
        assert_eq!(read(&prefix.join("toolchain/sai")).as_deref(), Some("sai"));
        // O que entrou sem antigo sai, com a pasta que a troca criou.
        assert!(!prefix.join("toolchain/sub").exists());
        assert!(!prefix.join("toolchain/depois").exists());
        for gone in [JOURNAL_FILE, ".instalando-1", ".antigo-1"] {
            assert!(!prefix.join(gone).exists(), "{gone}");
        }
        // Sem diário, nada a desfazer.
        assert_eq!(recover(prefix).unwrap(), None);
    }

    #[test]
    fn a_journal_without_its_staging_folder_only_goes_away() {
        // Um assistente de instalação rodou depois e apagou as pastas: a
        // instalação é a dele, e nada volta.
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path();
        interrupted(prefix);
        fs::remove_dir_all(prefix.join(".instalando-1")).unwrap();
        assert_eq!(recover(prefix).unwrap(), None);
        assert_eq!(
            read(&prefix.join("bin/aurora")).as_deref(),
            Some("aurora novo")
        );
        assert!(!prefix.join(JOURNAL_FILE).exists());
    }

    #[test]
    fn a_swap_that_is_still_running_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path();
        interrupted(prefix);
        let running = lock(prefix).unwrap();
        assert_eq!(recover(prefix).unwrap(), None);
        assert!(lock(prefix).is_err(), "a trava é de uma operação só");
        // As pastas dela ficam também.
        clean_leftovers(prefix);
        assert!(prefix.join(".antigo-1").exists());
        drop(running);
        assert_eq!(recover(prefix).unwrap().as_deref(), Some("9.9.9"));
    }

    #[test]
    fn a_journal_cannot_point_outside_the_installation() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path();
        interrupted(prefix);
        let path = prefix.join(JOURNAL_FILE);
        let text = read(&path).unwrap().replace(".antigo-1", "../fora");
        fs::write(&path, text).unwrap();
        assert!(recover(prefix).is_err());
        assert_eq!(
            read(&prefix.join("bin/aurora")).as_deref(),
            Some("aurora novo")
        );
    }
}
