//! Instalar o payload num prefixo.
//!
//! ```text
//! <prefixo>/
//!   bin/solar
//!   toolchain/          o bundle, só com os componentes escolhidos
//!   install.json        o que foi instalado (o instalador lê numa próxima vez)
//!   uninstall.sh        remove tudo isto e o atalho
//! ```
//!
//! A instalação extrai os pedaços num diretório provisório dentro do
//! prefixo, confere os hashes dos executáveis com o próprio Solar
//! ([`solar_core::Toolchain::verify`]) e só então troca a instalação
//! anterior, se houver. Um erro no meio deixa a instalação anterior intacta.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use solar_core::{Platform, Toolchain};

use crate::payload::Index;
use crate::plan::Selection;

/// O recibo da instalação, no prefixo.
pub const RECEIPT_FILE: &str = "install.json";
/// O desinstalador, no prefixo.
pub const UNINSTALL_SCRIPT: &str = "uninstall.sh";

/// O que está instalado num prefixo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// Versão do Solar.
    pub solar_version: String,
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
    /// O recibo em `prefix`, se lá houver uma instalação do Solar.
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
    /// Onde criar o atalho `solar` (um symlink para `<prefixo>/bin/solar`).
    pub link: Option<PathBuf>,
}

impl Target {
    /// O padrão: para o usuário, `~/.local/share/solar` com o atalho em
    /// `~/.local/bin`; como root, `/opt/solar` com o atalho em
    /// `/usr/local/bin`.
    pub fn default_for_user() -> Target {
        if is_root() {
            return Target {
                prefix: "/opt/solar".into(),
                link: Some("/usr/local/bin/solar".into()),
            };
        }
        let home = home();
        Target {
            prefix: home.join(".local/share/solar"),
            link: Some(home.join(".local/bin/solar")),
        }
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn is_root() -> bool {
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
/// nova, vazia ou com uma instalação do Solar (devolvida). O instalador não
/// escreve numa pasta com outras coisas.
pub fn check_prefix(prefix: &Path) -> anyhow::Result<Option<Receipt>> {
    if !prefix.is_absolute() {
        bail!(
            "use um caminho absoluto para a instalação: {}",
            prefix.display()
        );
    }
    match fs::symlink_metadata(prefix) {
        Err(_) => Ok(None),
        Ok(meta) if !meta.is_dir() => bail!("{} existe e não é uma pasta", prefix.display()),
        Ok(_) => {
            if let Some(receipt) = Receipt::load(prefix) {
                return Ok(Some(receipt));
            }
            let empty = fs::read_dir(prefix)
                .with_context(|| format!("lendo {}", prefix.display()))?
                .next()
                .is_none();
            if empty {
                Ok(None)
            } else {
                bail!(
                    "{} já tem arquivos e não é uma instalação do Solar: escolha uma pasta nova ou vazia",
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
        /// Os componentes que usam o pedaço (vazio: o Solar).
        components: Vec<String>,
    },
    /// `done` bytes extraídos até agora.
    Progress {
        /// Bytes extraídos.
        done: u64,
    },
    /// Conferindo os hashes dos executáveis.
    Verifying,
}

/// O atalho `solar`.
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
    pub solar: PathBuf,
    /// O atalho.
    pub link: LinkOutcome,
    /// Os componentes instalados.
    pub components: Vec<String>,
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
            "este instalador é para {}, e esta máquina é {}",
            index.platform,
            current.unwrap_or("uma plataforma sem suporte")
        );
    }
    let prefix = &target.prefix;
    let replaced = check_prefix(prefix)?;
    fs::create_dir_all(prefix).with_context(|| format!("criando {}", prefix.display()))?;

    let pid = std::process::id();
    let staging = prefix.join(format!(".instalando-{pid}"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).with_context(|| format!("criando {}", staging.display()))?;
    let verified = match extract_and_verify(payload_dir, index, selection, &staging, &mut on) {
        Ok(v) => v,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };

    // Troca: o que existia vai para um diretório que é apagado no fim.
    let old = prefix.join(format!(".antigo-{pid}"));
    for entry in ["toolchain", "bin/solar"] {
        let dest = prefix.join(entry);
        if fs::symlink_metadata(&dest).is_ok() {
            let aside = old.join(entry);
            fs::create_dir_all(aside.parent().expect("tem pai"))?;
            fs::rename(&dest, &aside).with_context(|| format!("movendo {}", dest.display()))?;
        }
        fs::create_dir_all(dest.parent().expect("tem pai"))?;
        fs::rename(staging.join(entry), &dest)
            .with_context(|| format!("instalando {}", dest.display()))?;
    }
    let _ = fs::remove_dir_all(&old);
    let _ = fs::remove_dir_all(&staging);

    let solar = prefix.join("bin/solar");
    // Um atalho de uma instalação anterior, em outro lugar, sai.
    if let Some(previous) = replaced.as_ref().and_then(|r| r.link.clone())
        && Some(&previous) != target.link.as_ref()
    {
        remove_link_to(&previous, &solar);
    }
    let link = match &target.link {
        Some(path) => make_link(path, &solar)?,
        None => LinkOutcome::NotRequested,
    };

    let components: Vec<String> = selection.iter().cloned().collect();
    let receipt = Receipt {
        solar_version: index.solar_version.clone(),
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

    Ok(Report {
        prefix: prefix.clone(),
        solar,
        link,
        components,
        verified,
        replaced,
    })
}

fn extract_and_verify(
    payload_dir: &Path,
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
        });
        let path = payload_dir.join(&chunk.file);
        let file = fs::File::open(&path)
            .with_context(|| format!("abrindo o pedaço {}", path.display()))?;
        let decoder = zstd::Decoder::new(file)?;
        let mut archive = tar::Archive::new(decoder);
        archive.set_preserve_permissions(true);
        archive.set_overwrite(true);
        for entry in archive
            .entries()
            .with_context(|| format!("lendo {}", path.display()))?
        {
            let mut entry = entry.with_context(|| format!("lendo {}", path.display()))?;
            let size = entry.size();
            if !entry
                .unpack_in(staging)
                .with_context(|| format!("extraindo {}", path.display()))?
            {
                bail!("{} tem um caminho fora da instalação", path.display());
            }
            done += size;
            on(Event::Progress { done });
        }
    }

    on(Event::Verifying);
    let toolchain = Toolchain::open(
        staging
            .join("toolchain")
            .to_str()
            .context("caminho não é UTF-8")?,
    )
    .context("o bundle extraído não abre")?;
    let installed: Selection = toolchain
        .manifest()
        .components
        .iter()
        .map(|c| c.name.clone())
        .collect();
    if &installed != selection {
        bail!(
            "o payload não bate com a seleção: extraídos {:?}, pedidos {:?}",
            installed,
            selection
        );
    }
    let mismatches = toolchain.verify()?;
    if let Some(m) = mismatches.first() {
        bail!(
            "{} executáveis não conferem com o manifesto (o primeiro: {}); o instalador está corrompido",
            mismatches.len(),
            m.path
        );
    }
    Ok(toolchain.verified_files())
}

#[cfg(unix)]
fn make_link(link: &Path, target: &Path) -> anyhow::Result<LinkOutcome> {
    if let Ok(meta) = fs::symlink_metadata(link) {
        if !meta.file_type().is_symlink() {
            return Ok(LinkOutcome::Blocked(link.to_owned()));
        }
        fs::remove_file(link).with_context(|| format!("trocando {}", link.display()))?;
    }
    if let Some(dir) = link.parent() {
        fs::create_dir_all(dir).with_context(|| format!("criando {}", dir.display()))?;
    }
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("criando o atalho {}", link.display()))?;
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
            "link={}\nif [ -L \"$link\" ] && [ \"$(readlink \"$link\")\" = \"$prefix/bin/solar\" ]; then\n  rm -f \"$link\"\nfi\n",
            sh_quote(link)
        ),
        None => String::new(),
    };
    let script = format!(
        "#!/bin/sh\n\
         # Remove o Solar instalado nesta pasta (gerado pelo instalador).\n\
         set -e\n\
         prefix={prefix}\n\
         {link_part}\
         rm -rf \"$prefix/toolchain\" \"$prefix/bin/solar\" \"$prefix/{RECEIPT_FILE}\"\n\
         rmdir \"$prefix/bin\" 2>/dev/null || true\n\
         rm -f \"$prefix/{UNINSTALL_SCRIPT}\"\n\
         rmdir \"$prefix\" 2>/dev/null || true\n\
         echo \"Solar removido de $prefix\"\n",
        prefix = sh_quote(prefix),
    );
    let path = prefix.join(UNINSTALL_SCRIPT);
    fs::write(&path, script).with_context(|| format!("gravando {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}
