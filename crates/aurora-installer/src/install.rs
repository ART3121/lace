//! Instalar o payload num prefixo.
//!
//! ```text
//! <prefixo>/
//!   bin/aurora
//!   toolchain/          o bundle, só com os componentes escolhidos
//!   install.json        o que foi instalado (o instalador lê numa próxima vez)
//!   uninstall.sh        remove tudo isto, o atalho, o do Studio no menu e
//!                       as sobras de uma instalação interrompida
//!                       (`.instalando-*`, `.antigo-*`)
//! ```
//!
//! Com o componente `studio`, o Studio ganha um atalho no menu de aplicativos
//! ([`crate::desktop`]).
//!
//! A instalação extrai os pedaços num diretório provisório dentro do
//! prefixo, confere os hashes dos executáveis com a própria AURORA
//! ([`aurora_core::Toolchain::verify`]) e só então troca a instalação
//! anterior, se houver. Um erro no meio deixa a instalação anterior intacta.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use aurora_core::{Platform, Toolchain};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::payload::Index;
use crate::plan::Selection;
use crate::update::{JOURNAL_FILE, LOCK_FILE};

/// O recibo da instalação, no prefixo.
pub const RECEIPT_FILE: &str = "install.json";
/// O desinstalador, no prefixo.
pub const UNINSTALL_SCRIPT: &str = "uninstall.sh";

/// O que está instalado num prefixo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// Versão da AURORA.
    pub aurora_version: String,
    /// Identificador do bundle.
    pub bundle: String,
    /// Plataforma.
    pub platform: String,
    /// Os componentes instalados.
    pub components: Vec<String>,
    /// O atalho criado, se houver.
    #[serde(default)]
    pub link: Option<PathBuf>,
}

impl Receipt {
    /// O recibo em `prefix`, se lá houver uma instalação da AURORA.
    pub fn load(prefix: &Path) -> Option<Receipt> {
        let text = fs::read_to_string(prefix.join(RECEIPT_FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }
}

/// Onde instalar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// O prefixo (absoluto).
    pub prefix: PathBuf,
    /// Onde criar o atalho `aurora` (um symlink para `<prefixo>/bin/aurora`).
    pub link: Option<PathBuf>,
}

impl Target {
    /// O padrão: para o usuário, `~/.local/share/aurora` com o atalho em
    /// `~/.local/bin`; como root, `/opt/aurora` com o atalho em
    /// `/usr/local/bin`.
    pub fn default_for_user() -> Target {
        if is_root() {
            return Target {
                prefix: "/opt/aurora".into(),
                link: Some("/usr/local/bin/aurora".into()),
            };
        }
        let home = home();
        Target {
            prefix: home.join(".local/share/aurora"),
            link: Some(home.join(".local/bin/aurora")),
        }
    }
}

pub(crate) fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub(crate) fn is_root() -> bool {
    std::env::var("USER").is_ok_and(|u| u == "root")
        || std::env::var("HOME").is_ok_and(|h| h == "/root" || h == "/var/root")
}

/// `~/x` com o `HOME` expandido; o resto como veio.
pub fn expand_home(input: &str) -> PathBuf {
    let input = input.trim();
    match input.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if input == "~" => home(),
        None => PathBuf::from(input),
    }
}

/// Confere se dá para instalar em `prefix`: caminho absoluto, e uma pasta
/// nova, vazia ou com uma instalação da AURORA (devolvida). O instalador não
/// escreve numa pasta com outras coisas.
pub fn check_prefix(prefix: &Path) -> anyhow::Result<Option<Receipt>> {
    if !prefix.is_absolute() {
        bail!(
            "Use an absolute path for the installation: {}",
            prefix.display()
        );
    }
    match fs::symlink_metadata(prefix) {
        Err(_) => Ok(None),
        Ok(meta) if !meta.is_dir() => bail!("{} exists and is not a folder", prefix.display()),
        Ok(_) => {
            if let Some(receipt) = Receipt::load(prefix) {
                return Ok(Some(receipt));
            }
            let empty = fs::read_dir(prefix)
                .with_context(|| format!("Reading {}", prefix.display()))?
                .next()
                .is_none();
            if empty {
                Ok(None)
            } else {
                bail!(
                    "{} already has files and is not an AURORA installation: choose a new or empty folder",
                    prefix.display()
                )
            }
        }
    }
}

/// O que a instalação, o `aurora install` e o `aurora update` vão fazendo, para
/// a barra de progresso e para o console de quem chamou. Em JSON, etiquetado
/// por `event` (`{"event": "chunk", ...}`): é a notificação `install` do
/// `aurora serve` e do Studio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Event {
    /// Começou: `total` bytes a extrair (ou a baixar, no `aurora update`).
    Start {
        /// Bytes a extrair.
        total: u64,
    },
    /// Extraindo o pedaço `index` (a partir de 1) de `count`.
    Chunk {
        /// Posição do pedaço.
        index: usize,
        /// Quantos pedaços.
        count: usize,
        /// Os componentes que usam o pedaço (vazio: a AURORA).
        components: Vec<String>,
        /// Bytes do `.tar.zst` (zero: desconhecido), para a barra do download.
        download: u64,
    },
    /// `done` bytes extraídos até agora.
    Progress {
        /// Bytes extraídos.
        done: u64,
    },
    /// Baixando o pedaço atual (ou o instalador inteiro): `bytes` já
    /// baixados.
    Downloading {
        /// Bytes baixados.
        bytes: u64,
    },
    /// Trocando `files` arquivos da instalação pelos da versão nova (o
    /// `aurora update` por componentes).
    Replacing {
        /// Quantos arquivos entram ou saem.
        files: usize,
    },
    /// Conferindo os hashes dos executáveis.
    Verifying,
    /// A atualização por componentes não deu, e o instalador inteiro da
    /// versão nova vai no lugar: por quê.
    Fallback {
        /// A frase, pronta para mostrar.
        reason: String,
    },
    /// Baixando o instalador inteiro da release.
    FallbackDownload {
        /// O arquivo.
        file: String,
        /// De onde.
        url: String,
    },
    /// O assistente do Windows foi aberto com os componentes instalados
    /// marcados (`selected`), ou sem a lista, com um aviso.
    WizardSelection {
        /// A lista foi passada ao assistente.
        selected: bool,
        /// Por que não, quando não.
        warning: Option<String>,
    },
    /// Uma linha que o instalador inteiro escreveu (Linux, macOS), quando
    /// quem chamou pediu a saída capturada.
    InstallerOutput {
        /// A linha.
        line: String,
    },
    /// Um aviso que não impede a operação (uma troca interrompida que não
    /// pôde ser desfeita, na busca).
    Warning {
        /// A frase, pronta para mostrar.
        message: String,
    },
}

impl Event {
    /// A linha do console para o evento, com os nomes dos componentes que
    /// `label` dá; `None` para o que é só barra de progresso (`start`,
    /// `progress`, `downloading`). É a linha que a notificação `install` do
    /// `aurora serve` leva e que o Studio mostra no console.
    pub fn console_line(&self, label: &dyn Fn(&str) -> String) -> Option<String> {
        Some(match self {
            Event::Chunk {
                index,
                count,
                components,
                download,
            } => {
                let what = if components.is_empty() {
                    "AURORA".to_owned()
                } else {
                    components
                        .iter()
                        .map(|c| label(c))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                if *download > 0 {
                    format!("  [{index}/{count}] {what} ({})", crate::mib(*download))
                } else {
                    format!("  [{index}/{count}] {what}")
                }
            }
            Event::Replacing { files } => format!("  Replacing {files} files"),
            Event::Verifying => "  Verifying the executables".to_owned(),
            Event::Fallback { reason } | Event::Warning { message: reason } => reason.clone(),
            Event::FallbackDownload { url, .. } => format!("Downloading {url}"),
            Event::WizardSelection {
                warning: Some(warning),
                ..
            } => format!("Warning: {warning}"),
            Event::InstallerOutput { line } => line.clone(),
            _ => return None,
        })
    }
}

/// O nome de um componente para mostrar: `AURORA` para a própria AURORA, o
/// rótulo do índice da versão, ou o nome, quando o índice não o tem ou ainda
/// não se sabe.
pub fn label(index: Option<&Index>, name: &str) -> String {
    if name == crate::update::AURORA {
        return "AURORA".to_owned();
    }
    index
        .and_then(|i| i.component(name))
        .map_or_else(|| name.to_owned(), |c| c.label.clone())
}

/// O atalho `aurora`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkOutcome {
    /// Não foi pedido.
    NotRequested,
    /// Criado (ou atualizado).
    Created(PathBuf),
    /// Já existe um arquivo que não é symlink nesse lugar; não foi mexido.
    Blocked(PathBuf),
}

/// O resultado de uma instalação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// O prefixo.
    pub prefix: PathBuf,
    /// O executável instalado.
    pub aurora: PathBuf,
    /// O atalho.
    pub link: LinkOutcome,
    /// Os componentes instalados.
    pub components: Vec<String>,
    /// O atalho da AURORA IDE no menu de aplicativos, se foi criado.
    pub shortcut: Option<PathBuf>,
    /// Quantos executáveis tiveram o hash conferido.
    pub verified: usize,
    /// A instalação que foi substituída, se houve.
    pub replaced: Option<Receipt>,
}

impl Report {
    /// O diretório do atalho está no `PATH` do usuário?
    pub fn link_on_path(&self) -> bool {
        let LinkOutcome::Created(link) = &self.link else {
            return false;
        };
        let Some(dir) = link.parent() else {
            return false;
        };
        std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|p| p == dir))
    }
}

/// Instala `selection` do payload em `payload_dir` no `target`, chamando
/// `on` a cada passo.
pub fn install(
    payload_dir: &Path,
    index: &Index,
    selection: &Selection,
    target: &Target,
    mut on: impl FnMut(Event),
) -> anyhow::Result<Report> {
    let current = Platform::current().map(Platform::as_str);
    if current != Some(index.platform.as_str()) {
        bail!(
            "This installer is for {}, and this machine is {}",
            index.platform,
            current.unwrap_or("an unsupported platform")
        );
    }
    let prefix = &target.prefix;
    let replaced = check_prefix(prefix)?;
    fs::create_dir_all(prefix).with_context(|| format!("Creating {}", prefix.display()))?;
    let _lock = prepare(prefix)?;

    let pid = std::process::id();
    let staging = prefix.join(format!(".instalando-{pid}"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).with_context(|| format!("Creating {}", staging.display()))?;
    let source = crate::add::LocalPayload(payload_dir.to_owned());
    let verified = match extract_and_verify(&source, index, selection, &staging, &mut on) {
        Ok(v) => v,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };

    // Troca: o que existia vai para um diretório que é apagado no fim.
    let old = prefix.join(format!(".antigo-{pid}"));
    let aurora_entry = crate::pack::aurora_entry(&index.platform);
    for entry in ["toolchain", aurora_entry] {
        let dest = prefix.join(entry);
        if fs::symlink_metadata(&dest).is_ok() {
            let aside = old.join(entry);
            fs::create_dir_all(aside.parent().expect("tem pai"))?;
            fs::rename(&dest, &aside).with_context(|| format!("Moving {}", dest.display()))?;
        }
        fs::create_dir_all(dest.parent().expect("tem pai"))?;
        fs::rename(staging.join(entry), &dest)
            .with_context(|| format!("Installing {}", dest.display()))?;
    }
    let _ = fs::remove_dir_all(&old);
    let _ = fs::remove_dir_all(&staging);
    crate::update::forget_journal(prefix);

    let aurora = prefix.join(aurora_entry);
    // Um atalho de uma instalação anterior, em outro lugar, sai.
    if let Some(previous) = replaced.as_ref().and_then(|r| r.link.clone())
        && Some(&previous) != target.link.as_ref()
    {
        remove_link_to(&previous, &aurora);
    }
    let link = match &target.link {
        Some(path) => make_link(path, &aurora)?,
        None => LinkOutcome::NotRequested,
    };

    let components: Vec<String> = selection.iter().cloned().collect();
    let receipt = Receipt {
        aurora_version: index.aurora_version.clone(),
        bundle: index.bundle.clone(),
        platform: index.platform.clone(),
        components: components.clone(),
        link: match &link {
            LinkOutcome::Created(p) => Some(p.clone()),
            _ => None,
        },
    };
    fs::write(
        prefix.join(RECEIPT_FILE),
        serde_json::to_string_pretty(&receipt)? + "\n",
    )?;
    write_uninstaller(prefix, receipt.link.as_deref())?;
    let shortcut = crate::desktop::sync(prefix)?;

    Ok(Report {
        prefix: prefix.clone(),
        aurora,
        link,
        components,
        shortcut,
        verified,
        replaced,
    })
}

fn extract_and_verify(
    source: &dyn crate::add::ChunkSource,
    index: &Index,
    selection: &Selection,
    staging: &Path,
    on: &mut impl FnMut(Event),
) -> anyhow::Result<usize> {
    let chunks: Vec<_> = index.chunks_for(selection).collect();
    let total = chunks.iter().map(|c| c.size).sum();
    on(Event::Start { total });
    let mut done = 0;
    for (i, chunk) in chunks.iter().enumerate() {
        on(Event::Chunk {
            index: i + 1,
            count: chunks.len(),
            components: chunk.components.clone(),
            download: chunk.download,
        });
        let path = source.fetch(chunk, &mut |bytes| on(Event::Downloading { bytes }))?;
        extract_chunk(&path, staging, &mut |size| {
            done += size;
            on(Event::Progress { done });
        })?;
    }

    on(Event::Verifying);
    let toolchain = Toolchain::open(
        staging
            .join("toolchain")
            .to_str()
            .context("Path is not UTF-8")?,
    )
    .context("Could not open the extracted bundle")?;
    let installed: Selection = toolchain
        .manifest()
        .components
        .iter()
        .map(|c| c.name.clone())
        .collect();
    if &installed != selection {
        bail!(
            "The payload does not match the selection: extracted {:?}, requested {:?}",
            installed,
            selection
        );
    }
    let mismatches = toolchain.verify()?;
    if let Some(m) = mismatches.first() {
        bail!(
            "Executables that do not match the manifest: {} (the first: {}); the installer is corrupted",
            mismatches.len(),
            m.path
        );
    }
    Ok(toolchain.verified_files())
}

/// Instala só o bundle (`toolchain/`) de `selection` em `prefix`, com os
/// pedaços de `source`: extrai numa pasta provisória, confere os executáveis
/// e troca o `toolchain/` que houver, guardado até a troca dar certo. O
/// `aurora` e o resto da pasta ficam como estão. É o `aurora setup`, que o
/// assistente do Windows que baixa os aplicativos roda antes de copiar o
/// `bin/aurora.exe` dele. Devolve quantos executáveis tiveram o hash conferido.
pub fn install_bundle(
    source: &dyn crate::add::ChunkSource,
    index: &Index,
    selection: &Selection,
    prefix: &Path,
    mut on: impl FnMut(Event),
) -> anyhow::Result<usize> {
    let current = Platform::current().map(Platform::as_str);
    if current != Some(index.platform.as_str()) {
        bail!(
            "These apps are for {}, and this machine is {}",
            index.platform,
            current.unwrap_or("an unsupported platform")
        );
    }
    let dest = prefix.join("toolchain");
    // A pasta escolhida no assistente pode ter uma `toolchain` de outra
    // ferramenta, e ela iria inteira embora no fim, sem passar pela lixeira.
    if fs::symlink_metadata(&dest).is_ok() && !is_aurora_installation(prefix) {
        bail!(
            "{} already exists and is not the bundle of an AURORA installation; \
             choose another folder, or move it away first",
            dest.display()
        );
    }
    fs::create_dir_all(prefix).with_context(|| format!("Creating {}", prefix.display()))?;
    let _lock = prepare(prefix)?;
    let pid = std::process::id();
    let staging = prefix.join(format!(".instalando-{pid}"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).with_context(|| format!("Creating {}", staging.display()))?;
    let verified = match extract_and_verify(source, index, selection, &staging, &mut on) {
        Ok(verified) => verified,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    let old = prefix.join(format!(".antigo-{pid}"));
    let aside = old.join("toolchain");
    if fs::symlink_metadata(&dest).is_ok() {
        fs::create_dir_all(&old).with_context(|| format!("Creating {}", old.display()))?;
        if let Err(e) = fs::rename(&dest, &aside) {
            let _ = fs::remove_dir_all(&staging);
            return Err(e).with_context(|| format!("Moving {} aside", dest.display()));
        }
    }
    if let Err(e) = fs::rename(staging.join("toolchain"), &dest) {
        let _ = fs::rename(&aside, &dest);
        let _ = fs::remove_dir_all(&staging);
        return Err(e).with_context(|| format!("Installing {}", dest.display()));
    }
    let _ = fs::remove_dir_all(&old);
    let _ = fs::remove_dir_all(&staging);
    crate::update::forget_journal(prefix);
    Ok(verified)
}

/// `prefix` tem uma instalação da AURORA: o cabeçalho do bundle
/// (`toolchain/bundle.json`) ou o `aurora` em `bin/`.
pub fn is_aurora_installation(prefix: &Path) -> bool {
    let aurora = format!("bin/aurora{}", std::env::consts::EXE_SUFFIX);
    prefix
        .join("toolchain")
        .join(aurora_core::MANIFEST_FILE)
        .is_file()
        || prefix.join(aurora).is_file()
}

/// Antes de trocar a instalação em `prefix`: desfaz a troca que uma
/// atualização interrompida deixou pela metade ([`crate::update::recover`]),
/// para a troca inteira partir de uma instalação coerente, e trava a pasta
/// ([`crate::update::lock`]). Uma troca que não pôde ser desfeita não impede
/// a instalação, que troca tudo, e o diário dela sai no fim.
fn prepare(prefix: &Path) -> anyhow::Result<crate::update::InstallLock> {
    if let Err(error) = crate::update::recover(prefix) {
        tracing::warn!("{error:#}");
    }
    crate::update::lock(prefix)
}

/// Extrai um pedaço (`.tar.zst`) em `dest`, avisando o tamanho de cada
/// arquivo extraído. Recusa um caminho que sairia de `dest`.
pub(crate) fn extract_chunk(
    path: &Path,
    dest: &Path,
    extracted: &mut dyn FnMut(u64),
) -> anyhow::Result<()> {
    let file =
        fs::File::open(path).with_context(|| format!("Opening the chunk {}", path.display()))?;
    let decoder = zstd::Decoder::new(file)?;
    let mut archive = tar::Archive::new(decoder);
    archive.set_preserve_permissions(true);
    archive.set_overwrite(true);
    for entry in archive
        .entries()
        .with_context(|| format!("Reading {}", path.display()))?
    {
        let mut entry = entry.with_context(|| format!("Reading {}", path.display()))?;
        let size = entry.size();
        if !entry
            .unpack_in(dest)
            .with_context(|| format!("Extracting {}", path.display()))?
        {
            bail!("{} has a path outside the installation", path.display());
        }
        extracted(size);
    }
    Ok(())
}

/// Extrai de um pedaço só os arquivos de `wanted` (os nomes dentro dele,
/// com `/`), em `dest`. Um nome de `wanted` que o pedaço não tem é erro.
pub(crate) fn extract_entries(
    path: &Path,
    dest: &Path,
    wanted: &std::collections::BTreeSet<&str>,
) -> anyhow::Result<()> {
    let file =
        fs::File::open(path).with_context(|| format!("Opening the chunk {}", path.display()))?;
    let decoder = zstd::Decoder::new(file)?;
    let mut archive = tar::Archive::new(decoder);
    archive.set_preserve_permissions(true);
    archive.set_overwrite(true);
    let mut found = 0;
    for entry in archive
        .entries()
        .with_context(|| format!("Reading {}", path.display()))?
    {
        let mut entry = entry.with_context(|| format!("Reading {}", path.display()))?;
        let name = entry.path()?.to_string_lossy().replace('\\', "/");
        if !wanted.contains(name.as_str()) {
            continue;
        }
        if !entry
            .unpack_in(dest)
            .with_context(|| format!("Extracting {name} from {}", path.display()))?
        {
            bail!("{} has a path outside the installation", path.display());
        }
        found += 1;
    }
    if found != wanted.len() {
        bail!(
            "{} lacks {} of the files the update needs",
            path.display(),
            wanted.len() - found
        );
    }
    Ok(())
}

#[cfg(unix)]
fn make_link(link: &Path, target: &Path) -> anyhow::Result<LinkOutcome> {
    if let Ok(meta) = fs::symlink_metadata(link) {
        if !meta.file_type().is_symlink() {
            return Ok(LinkOutcome::Blocked(link.to_owned()));
        }
        fs::remove_file(link).with_context(|| format!("Replacing {}", link.display()))?;
    }
    if let Some(dir) = link.parent() {
        fs::create_dir_all(dir).with_context(|| format!("Creating {}", dir.display()))?;
    }
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("Creating the link {}", link.display()))?;
    Ok(LinkOutcome::Created(link.to_owned()))
}

#[cfg(not(unix))]
fn make_link(_link: &Path, _target: &Path) -> anyhow::Result<LinkOutcome> {
    Ok(LinkOutcome::NotRequested)
}

/// Remove `link` se ele for um symlink para `target`.
fn remove_link_to(link: &Path, target: &Path) {
    if fs::read_link(link).is_ok_and(|t| t == target) {
        let _ = fs::remove_file(link);
    }
}

/// Aspas simples de shell.
fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

fn write_uninstaller(prefix: &Path, link: Option<&Path>) -> anyhow::Result<()> {
    let link_part = match link {
        Some(link) => format!(
            "link={}\nif [ -L \"$link\" ] && [ \"$(readlink \"$link\")\" = \"$prefix/bin/aurora\" ]; then\n  rm -f \"$link\"\nfi\n",
            sh_quote(link)
        ),
        None => String::new(),
    };
    let shortcut_part = crate::desktop::uninstall_lines(sh_quote);
    let script = format!(
        "#!/bin/sh\n\
         # Remove a AURORA instalada nesta pasta (gerado pelo instalador).\n\
         set -e\n\
         prefix={prefix}\n\
         {link_part}\
         {shortcut_part}\
         rm -rf \"$prefix/toolchain\" \"$prefix/bin/aurora\" \"$prefix/{RECEIPT_FILE}\"\n\
         # Sobras de uma instalação interrompida (`install`, acima), o diário\n\
         # de uma atualização e a trava delas (`update`).\n\
         rm -rf \"$prefix\"/.instalando-* \"$prefix\"/.antigo-*\n\
         rm -f \"$prefix/{JOURNAL_FILE}\" \"$prefix/{JOURNAL_FILE}.tmp\" \"$prefix/{LOCK_FILE}\"\n\
         rmdir \"$prefix/bin\" 2>/dev/null || true\n\
         rm -f \"$prefix/{UNINSTALL_SCRIPT}\"\n\
         rmdir \"$prefix\" 2>/dev/null || true\n\
         echo \"Removed AURORA from $prefix\"\n",
        prefix = sh_quote(prefix),
    );
    let path = prefix.join(UNINSTALL_SCRIPT);
    fs::write(&path, script).with_context(|| format!("Writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}
