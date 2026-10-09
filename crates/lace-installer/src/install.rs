//! Instalar o payload num prefixo.
//!
//! ```text
//! <prefixo>/
//!   bin/lace
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
//! prefixo, confere os hashes dos executáveis com o próprio Lace
//! ([`lace_core::Toolchain::verify`]) e só então troca a instalação
//! anterior, se houver. Um erro no meio deixa a instalação anterior intacta.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use lace_core::{Platform, Toolchain};
use serde::{Deserialize, Serialize};

use crate::payload::Index;
use crate::plan::Selection;

/// O recibo da instalação, no prefixo.
pub const RECEIPT_FILE: &str = "install.json";
/// O desinstalador, no prefixo.
pub const UNINSTALL_SCRIPT: &str = "uninstall.sh";

/// O que está instalado num prefixo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// Versão do Lace.
    pub lace_version: String,
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
    /// O recibo em `prefix`, se lá houver uma instalação do Lace.
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
    /// Onde criar o atalho `lace` (um symlink para `<prefixo>/bin/lace`).
    pub link: Option<PathBuf>,
}

impl Target {
    /// O padrão: para o usuário, `~/.local/share/lace` com o atalho em
    /// `~/.local/bin`; como root, `/opt/lace` com o atalho em
    /// `/usr/local/bin`.
    pub fn default_for_user() -> Target {
        if is_root() {
            return Target {
                prefix: "/opt/lace".into(),
                link: Some("/usr/local/bin/lace".into()),
            };
        }
        let home = home();
        Target {
            prefix: home.join(".local/share/lace"),
            link: Some(home.join(".local/bin/lace")),
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
/// nova, vazia ou com uma instalação do Lace (devolvida). O instalador não
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
                    "{} already has files and is not a Lace installation: choose a new or empty folder",
                    prefix.display()
                )
            }
        }
    }
}

/// O que a instalação vai fazendo, para a barra de progresso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Começou: `total` bytes a extrair.
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
        /// Os componentes que usam o pedaço (vazio: o Lace).
        components: Vec<String>,
        /// Bytes do `.tar.zst` (zero: desconhecido), para a barra do download.
        download: u64,
    },
    /// `done` bytes extraídos até agora.
    Progress {
        /// Bytes extraídos.
        done: u64,
    },
    /// Baixando o pedaço atual: `bytes` já baixados.
    Downloading {
        /// Bytes baixados do pedaço.
        bytes: u64,
    },
    /// Trocando `files` arquivos da instalação pelos da versão nova (o
    /// `lace update` por componentes).
    Replacing {
        /// Quantos arquivos entram ou saem.
        files: usize,
    },
    /// Conferindo os hashes dos executáveis.
    Verifying,
}

/// O atalho `lace`.
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
    pub lace: PathBuf,
    /// O atalho.
    pub link: LinkOutcome,
    /// Os componentes instalados.
    pub components: Vec<String>,
    /// O atalho do Lace Studio no menu de aplicativos, se foi criado.
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
    let lace_entry = crate::pack::lace_entry(&index.platform);
    for entry in ["toolchain", lace_entry] {
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

    let lace = prefix.join(lace_entry);
    // Um atalho de uma instalação anterior, em outro lugar, sai.
    if let Some(previous) = replaced.as_ref().and_then(|r| r.link.clone())
        && Some(&previous) != target.link.as_ref()
    {
        remove_link_to(&previous, &lace);
    }
    let link = match &target.link {
        Some(path) => make_link(path, &lace)?,
        None => LinkOutcome::NotRequested,
    };

    let components: Vec<String> = selection.iter().cloned().collect();
    let receipt = Receipt {
        lace_version: index.lace_version.clone(),
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
        lace,
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
/// `lace` e o resto da pasta ficam como estão. É o `lace setup`, que o
/// assistente do Windows que baixa os aplicativos roda antes de copiar o
/// `bin/lace.exe` dele. Devolve quantos executáveis tiveram o hash conferido.
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
    fs::create_dir_all(prefix).with_context(|| format!("Creating {}", prefix.display()))?;
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
    let dest = prefix.join("toolchain");
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
    Ok(verified)
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
            "link={}\nif [ -L \"$link\" ] && [ \"$(readlink \"$link\")\" = \"$prefix/bin/lace\" ]; then\n  rm -f \"$link\"\nfi\n",
            sh_quote(link)
        ),
        None => String::new(),
    };
    let shortcut_part = crate::desktop::uninstall_lines(sh_quote);
    let script = format!(
        "#!/bin/sh\n\
         # Remove o Lace instalado nesta pasta (gerado pelo instalador).\n\
         set -e\n\
         prefix={prefix}\n\
         {link_part}\
         {shortcut_part}\
         rm -rf \"$prefix/toolchain\" \"$prefix/bin/lace\" \"$prefix/{RECEIPT_FILE}\"\n\
         # Sobras de uma instalação interrompida (`install`, acima).\n\
         rm -rf \"$prefix\"/.instalando-* \"$prefix\"/.antigo-*\n\
         rmdir \"$prefix/bin\" 2>/dev/null || true\n\
         rm -f \"$prefix/{UNINSTALL_SCRIPT}\"\n\
         rmdir \"$prefix\" 2>/dev/null || true\n\
         echo \"Removed Lace from $prefix\"\n",
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
