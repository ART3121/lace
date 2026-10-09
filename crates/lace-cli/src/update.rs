//! `lace update`: compara as versões instaladas com as publicadas e, se há
//! um Lace mais novo, atualiza a instalação de onde este `lace` roda.
//!
//! As ferramentas de terceiros vêm no bundle, fechado e versionado por
//! release do Lace (`bundle/versions.json` fixa a versão e o hash de cada
//! pacote). Uma versão nova de uma ferramenta só chega ao usuário num bundle
//! novo, publicado numa release nova do Lace: o empacotamento compila o YANC
//! e o surfer-aurora do fonte e divide por ferramenta o OSS CAD Suite e, no
//! Windows, o bloco MSYS2 do lace-toolchain (`scripts/bundle.py`). Por isso
//! este comando não instala nada direto do upstream: ele só mostra, para
//! cada componente, a versão instalada, a do bundle da última release e a
//! última upstream.
//!
//! De onde vêm as versões:
//!
//! - o Lace: a última release no GitHub (`release::latest_lace_version`);
//! - o bundle da última release: o `bundle/versions.json` da tag dela;
//! - upstream: as releases do OSS CAD Suite, do lace-toolchain e do YANC no
//!   GitHub, as tags do surfer-aurora e as releases do Graphviz no GitLab.
//!   Uma fonte que falha deixa só a coluna dela vazia.
//!
//! A atualização vai por componentes quando dá: a instalação guarda o
//! manifesto de arquivos da versão dela (`toolchain/files.json`, desde a
//! 0.7.0), a release nova publica o seu, e só os pedaços com arquivos que
//! mudaram são baixados, conferidos pelo `SHA256SUMS` dela, e trocados no
//! lugar (`lace_installer::update`). Uma falha no meio devolve a instalação
//! ao que era. `--from` faz o mesmo com um instalador no disco, sem rede.
//!
//! Sem o manifesto (uma instalação anterior à 0.7.0), com `--full` ou se a
//! atualização por componentes falha, quem atualiza é o instalador inteiro
//! da versão nova, baixado da release e conferido pelo `SHA256SUMS` dela:
//!
//! - Linux e macOS: `install --yes --components <os instalados> --prefix
//!   <instalação>`, com o atalho do recibo. Ele troca o `bin/lace` e o
//!   `toolchain/` inteiro.
//! - Windows: o assistente de instalação, que lembra a pasta, com os
//!   componentes instalados marcados (`/COMPONENTS=`): ele só lembra a
//!   própria seleção, e refaz o `toolchain\` com ela, então o que o
//!   `lace install` acrescentou sairia. Ele troca o `lace.exe`, então roda
//!   depois que este `lace` sai.

use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Write};
use std::process::Stdio;

use anyhow::{Context, bail};
use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{BundleManifest, Platform, Toolchain, component};
use lace_installer::add::ChunkSource;
use lace_installer::files::{self, FilesManifest};
use lace_installer::pack;
use lace_installer::payload::{self, Index};
use lace_installer::update::{self as by_parts, Plan};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::install::{DiskPayload, ReleasePayload};
use crate::installation::{self, Receipt};
use crate::meter::Meter;
use crate::output::Output;
use crate::release;

/// Os pacotes de `bundle/versions.json`, de onde saem os componentes.
mod package {
    pub const OSS_CAD_SUITE: &str = "oss-cad-suite";
    pub const YANC: &str = "yanc";
    pub const SURFER_AURORA: &str = "surfer-aurora";
    pub const GRAPHVIZ: &str = "graphviz";
    pub const MSYS: &str = "msys";
    /// O Lace Studio, do próprio repositório: a versão é a do Lace.
    pub const STUDIO: &str = "studio";
    /// As trilhas do `lace learn`, do próprio repositório: a versão é a do
    /// Lace.
    pub const LACE_LEARN: &str = "lace-learn";
}

/// O repositório do bloco de Windows (o pacote `msys`).
const LACE_TOOLCHAIN: &str = "ART3121/lace-toolchain";

/// As tags do surfer-aurora no GitLab.
const SURFER_AURORA_TAGS: &str =
    "https://gitlab.com/api/v4/projects/nips-cern%2Fsurfer-aurora/repository/tags?per_page=20";
/// A última release do Graphviz no GitLab (o projeto `graphviz/graphviz`).
const GRAPHVIZ_RELEASES: &str = "https://gitlab.com/api/v4/projects/4207231/releases?per_page=1";

/// `lace update`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UpdateReport {
    /// O Lace: este e o da última release.
    pub lace: LaceVersions,
    /// O bundle de ferramentas: o instalado e o da última release.
    pub bundle: BundleVersions,
    /// Cada componente instalado, na ordem do manifesto do bundle. Vazio sem
    /// bundle.
    pub components: Vec<ComponentVersions>,
    /// A pasta da instalação; `null` se este `lace` não foi instalado pelo
    /// instalador (um build em `target/`, por exemplo).
    #[schemars(with = "Option<String>")]
    pub prefix: Option<Utf8PathBuf>,
    /// O que o comando fez.
    pub action: UpdateAction,
    /// Como atualizou; `null` quando não atualizou.
    pub method: Option<UpdateMethod>,
    /// O que a atualização por componentes trocou, ou vai trocar com
    /// `--check --from`; `null` quando ela não rodou.
    pub changes: Option<UpdateChanges>,
}

/// Como `lace update` atualizou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpdateMethod {
    /// Só os arquivos que mudaram, dos pedaços que a release publica.
    Components,
    /// O instalador inteiro da versão nova.
    Installer,
}

/// O que a atualização por componentes trocou.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct UpdateChanges {
    /// Os componentes com arquivos que entraram ou saíram, na ordem do
    /// bundle; `lace` é o próprio Lace e o cabeçalho do bundle.
    pub components: Vec<String>,
    /// Bytes baixados.
    pub download_bytes: u64,
    /// Arquivos que entraram (novos ou trocados).
    pub files: usize,
    /// Arquivos que saíram.
    pub removed: usize,
}

impl UpdateChanges {
    fn of(plan: &Plan) -> UpdateChanges {
        UpdateChanges {
            components: plan.changed.clone(),
            download_bytes: plan.download(),
            files: plan.files(),
            removed: plan.stale.len(),
        }
    }
}

/// As versões do Lace em `lace update`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct LaceVersions {
    /// A versão deste `lace`.
    pub installed: String,
    /// A versão da última release no GitHub.
    pub latest: String,
    /// `latest` é mais nova que `installed`.
    pub newer: bool,
}

/// As versões do bundle em `lace update`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BundleVersions {
    /// O bundle instalado; `null` sem bundle.
    pub installed: Option<String>,
    /// O bundle da última release; `null` se o `bundle/versions.json` dela
    /// não pôde ser lido.
    pub latest: Option<String>,
    /// `latest` é mais novo que `installed`.
    pub newer: bool,
}

/// Um componente instalado em `lace update`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ComponentVersions {
    /// O componente (`yanc`, `icarus`, `verilator`, `yosys`, `graphviz`,
    /// `surfer-aurora`).
    pub name: String,
    /// O pacote de `bundle/versions.json` de onde ele sai (`oss-cad-suite`,
    /// `yanc`, `surfer-aurora`, `graphviz`); `null` para um componente que
    /// este Lace não conhece.
    pub package: Option<String>,
    /// A versão instalada.
    pub installed: String,
    /// A versão no bundle da última release; `null` se não pôde ser lida.
    pub release: Option<String>,
    /// `release` é mais nova que `installed`: chega atualizando o Lace.
    pub release_newer: bool,
    /// A última versão upstream; `null` se a consulta falhou.
    pub upstream: Option<String>,
    /// `upstream` é mais nova que `installed` e que `release`: só chega num
    /// bundle novo, numa release nova do Lace. Sem `release`, `false` quando
    /// há Lace mais novo, cujo bundle pode já trazê-la.
    pub upstream_newer: bool,
}

/// O que `lace update` fez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAction {
    /// `--check`: só comparou as versões.
    Checked,
    /// Não há Lace mais novo publicado: nada mudou.
    UpToDate,
    /// O instalador da versão nova trocou o Lace e o bundle (Linux, macOS).
    Updated,
    /// O assistente de instalação do Windows foi aberto e termina depois que
    /// o `lace` sai.
    WizardOpened,
}

/// As opções de `lace update`.
pub struct Options<'a> {
    /// `--check`: só compara.
    pub check: bool,
    /// `--yes`: não pergunta.
    pub yes: bool,
    /// `--full`: o instalador inteiro, sem tentar por componentes.
    pub full: bool,
    /// `--from`: um instalador no disco, sem rede.
    pub from: Option<&'a Utf8Path>,
}

pub fn run(out: &Output, toolchain: Option<&Toolchain>, options: Options) -> anyhow::Result<()> {
    if let Some(path) = options.from {
        return from_disk(out, toolchain, &options, path);
    }
    let mut report = gather(
        &Network,
        env!("CARGO_PKG_VERSION"),
        toolchain.map(Toolchain::manifest),
    )?;
    let installation = installation::prefix();
    report.prefix = installation.as_ref().ok().cloned();
    if out.is_text() {
        print_table(&report);
    }
    if options.check || !report.lace.newer {
        report.action = if options.check {
            UpdateAction::Checked
        } else {
            UpdateAction::UpToDate
        };
        if out.is_text() {
            print_summary(&report);
        }
        return out.json(&report);
    }

    let prefix = installation?;
    by_parts::clean_leftovers(prefix.as_std_path());
    let version = report.lace.latest.clone();
    let mut asked = false;
    if !options.full {
        let prepared = remote_plan(&version, &prefix).unwrap_or_else(|error| {
            // No stderr também com `--json`: o Studio mostra.
            eprintln!(
                "Could not prepare the update by components ({error:#}); \
                 updating with the installer of Lace {version}"
            );
            None
        });
        if let Some(prepared) = prepared {
            if out.is_text() {
                print_plan(&prepared.plan, &prepared.index, false);
            }
            if !options.yes {
                confirm_plan(&prepared.plan, &prefix)?;
                asked = true;
            }
            match by_components(out, &prefix, &prepared) {
                Ok(changes) => {
                    report.action = UpdateAction::Updated;
                    report.method = Some(UpdateMethod::Components);
                    report.changes = Some(changes);
                    return out.json(&report);
                }
                Err(error) => {
                    eprintln!(
                        "Could not update by components ({error:#}); the installation was \
                         left as it was. Updating with the installer of Lace {version}"
                    );
                }
            }
        }
    }

    if !options.yes && !asked {
        confirm(&report.lace, &prefix)?;
    }
    report.method = Some(UpdateMethod::Installer);
    report.action = if cfg!(windows) {
        let installed: Vec<String> = toolchain
            .map(|t| {
                t.manifest()
                    .components
                    .iter()
                    .map(|c| c.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        windows(out, &report.lace.latest, &installed)?
    } else {
        unix(out, &prefix, &report.lace.latest)?
    };
    out.json(&report)
}

/// De onde vêm as versões: a rede em [`Network`], um substituto nos testes.
trait Sources: Sync {
    /// A última versão do Lace publicada (a tag sem o `v`).
    fn latest_lace(&self) -> anyhow::Result<String>;
    /// O `bundle/versions.json` da release `v<version>` do Lace.
    fn release_versions(&self, version: &str) -> anyhow::Result<String>;
    /// A última versão upstream de um pacote de `bundle/versions.json`.
    fn upstream(&self, package: &str) -> anyhow::Result<String>;
}

/// As fontes reais, pela internet, com o `curl` de `release`.
struct Network;

impl Sources for Network {
    fn latest_lace(&self) -> anyhow::Result<String> {
        release::latest_lace_version()
    }

    fn release_versions(&self, version: &str) -> anyhow::Result<String> {
        release::fetch_text(&format!(
            "https://raw.githubusercontent.com/{}/v{version}/bundle/versions.json",
            release::repo()
        ))
    }

    fn upstream(&self, name: &str) -> anyhow::Result<String> {
        match name {
            package::OSS_CAD_SUITE => release::latest_github_tag("YosysHQ/oss-cad-suite-build"),
            package::MSYS => release::latest_github_tag(LACE_TOOLCHAIN),
            package::YANC => release::latest_github_tag("nipscernlab/yanc"),
            // O Studio e as trilhas saem com o Lace: o mais novo é o da
            // última release.
            package::STUDIO | package::LACE_LEARN => release::latest_lace_version(),
            package::SURFER_AURORA => {
                let text = release::fetch_text(SURFER_AURORA_TAGS)?;
                let tags: Vec<GitlabTag> = serde_json::from_str(&text)
                    .with_context(|| format!("Invalid response from {SURFER_AURORA_TAGS}"))?;
                newest(tags.into_iter().map(|t| t.name))
                    .with_context(|| format!("No versioned tag in {SURFER_AURORA_TAGS}"))
            }
            package::GRAPHVIZ => {
                let text = release::fetch_text(GRAPHVIZ_RELEASES)?;
                let releases: Vec<GitlabRelease> = serde_json::from_str(&text)
                    .with_context(|| format!("Invalid response from {GRAPHVIZ_RELEASES}"))?;
                releases
                    .into_iter()
                    .next()
                    .map(|r| r.tag_name)
                    .with_context(|| format!("No release in {GRAPHVIZ_RELEASES}"))
            }
            _ => bail!("No upstream source known for {name}"),
        }
    }
}

/// Uma tag na API do GitLab.
#[derive(Deserialize)]
struct GitlabTag {
    name: String,
}

/// Uma release na API do GitLab.
#[derive(Deserialize)]
struct GitlabRelease {
    tag_name: String,
}

/// O que interessa do `bundle/versions.json` de uma release: o bundle e a
/// versão de cada pacote.
#[derive(Deserialize)]
struct ReleaseBundle {
    bundle: String,
    packages: BTreeMap<String, ReleasePackage>,
}

#[derive(Deserialize)]
struct ReleasePackage {
    version: String,
}

/// O pacote de `bundle/versions.json` de onde sai um componente, como em
/// `bundle/components.json`: no Windows, Icarus, Verilator e cocotb vêm do
/// bloco MSYS2 do lace-toolchain e o Graphviz do pacote oficial dele; o
/// resto, e tudo isso no Linux e no macOS, do OSS CAD Suite. O YANC, o
/// surfer-aurora, o Studio e as trilhas do `lace learn` têm pacote próprio.
fn package_of(name: &str, platform: &str) -> Option<&'static str> {
    let windows = platform == "windows-x64";
    match name {
        component::YANC => Some(package::YANC),
        component::SURFER_AURORA => Some(package::SURFER_AURORA),
        component::STUDIO => Some(package::STUDIO),
        component::LACE_LEARN => Some(package::LACE_LEARN),
        // O cocotb do Windows sai do bloco MSYS2, com o Icarus e o Verilator.
        component::ICARUS | component::VERILATOR | component::COCOTB if windows => {
            Some(package::MSYS)
        }
        component::GRAPHVIZ if windows => Some(package::GRAPHVIZ),
        component::ICARUS
        | component::VERILATOR
        | component::COCOTB
        | component::YOSYS
        | component::GRAPHVIZ
        | component::OPENFPGALOADER => Some(package::OSS_CAD_SUITE),
        _ => None,
    }
}

/// Consulta as versões e monta o relatório, com `action` em `Checked` e sem
/// `prefix`. Sem a última versão do Lace é erro; sem o bundle da release ou
/// sem uma versão upstream, a coluna fica vazia.
fn gather(
    sources: &impl Sources,
    installed: &str,
    manifest: Option<&BundleManifest>,
) -> anyhow::Result<UpdateReport> {
    let components = manifest.map_or(&[][..], |m| m.components.as_slice());
    let platform = manifest.map_or("", |m| m.platform.as_str());
    let packages: Vec<Option<&'static str>> = components
        .iter()
        .map(|c| package_of(&c.name, platform))
        .collect();
    let mut queried: Vec<&'static str> = packages.iter().flatten().copied().collect();
    queried.sort_unstable();
    queried.dedup();

    // Cada consulta upstream é um `curl`, e elas não dependem umas das
    // outras: rodam juntas, enquanto esta thread consulta o GitHub.
    let (latest, upstream) = std::thread::scope(|scope| {
        let handles: Vec<_> = queried
            .iter()
            .map(|&p| (p, scope.spawn(move || sources.upstream(p))))
            .collect();
        let latest = sources.latest_lace();
        let upstream: BTreeMap<&str, Option<String>> = handles
            .into_iter()
            .map(|(p, handle)| {
                let version = match handle.join() {
                    Ok(Ok(version)) => Some(version),
                    Ok(Err(error)) => {
                        tracing::info!("No upstream version for {p}: {error:#}");
                        None
                    }
                    Err(_) => None,
                };
                (p, version)
            })
            .collect();
        (latest, upstream)
    });
    let latest =
        latest.context("Could not get the latest Lace version (no network, or GitHub is down)")?;

    let release = sources
        .release_versions(&latest)
        .and_then(|text| {
            serde_json::from_str::<ReleaseBundle>(&text).context("Invalid bundle/versions.json")
        })
        .inspect_err(|error| tracing::info!("No bundle for release v{latest}: {error:#}"))
        .ok();

    let lace_newer = is_newer(&latest, installed);
    let components = components
        .iter()
        .zip(packages)
        .map(|(c, package)| {
            let in_release = package
                .and_then(|p| release.as_ref()?.packages.get(p))
                .map(|p| p.version.clone());
            let upstream = package.and_then(|p| upstream.get(p).cloned().flatten());
            let release_newer = in_release
                .as_deref()
                .is_some_and(|r| is_newer(r, &c.version));
            // Sem o bundle da release, só dá para dizer quando não há Lace
            // mais novo: o bundle dele pode já trazer a versão upstream.
            let upstream_newer = upstream.as_deref().is_some_and(|u| {
                is_newer(u, &c.version)
                    && match in_release.as_deref() {
                        Some(r) => is_newer(u, r),
                        None => !lace_newer,
                    }
            });
            ComponentVersions {
                name: c.name.clone(),
                package: package.map(str::to_owned),
                installed: c.version.clone(),
                release: in_release,
                release_newer,
                upstream,
                upstream_newer,
            }
        })
        .collect();

    let installed_bundle = manifest.map(|m| m.bundle.clone());
    let latest_bundle = release.map(|r| r.bundle);
    let bundle_newer = match (&latest_bundle, &installed_bundle) {
        (Some(latest), Some(installed)) => is_newer(latest, installed),
        _ => false,
    };
    Ok(UpdateReport {
        lace: LaceVersions {
            installed: installed.to_owned(),
            newer: lace_newer,
            latest,
        },
        bundle: BundleVersions {
            installed: installed_bundle,
            latest: latest_bundle,
            newer: bundle_newer,
        },
        components,
        prefix: None,
        action: UpdateAction::Checked,
        method: None,
        changes: None,
    })
}

/// A atualização por componentes, preparada: o índice e o manifesto da
/// versão nova, o plano, e de onde vêm os pedaços.
struct Prepared {
    index: Index,
    new: FilesManifest,
    plan: Plan,
    source: Box<dyn ChunkSource>,
    /// Os pedaços vêm de um instalador no disco (`--from`), não da rede.
    local: bool,
    /// A pasta dos downloads, apagada no fim.
    _work: Option<tempfile::TempDir>,
}

/// O manifesto de arquivos da instalação; `None` numa instalação anterior à
/// 0.7.0, que não o tem.
fn installed_manifest(prefix: &Utf8Path) -> anyhow::Result<Option<FilesManifest>> {
    let path = prefix.join(files::INSTALLED);
    if !path.is_file() {
        tracing::info!("{path} does not exist: the installation is older than Lace 0.7.0");
        return Ok(None);
    }
    FilesManifest::load(path.as_std_path()).map(Some)
}

/// Prepara a atualização por componentes para a release `v<version>`:
/// baixa o índice e o manifesto de arquivos dela e compara com a
/// instalação. `None` quando não dá (a instalação ou a release sem o
/// manifesto): atualiza o instalador inteiro.
fn remote_plan(version: &str, prefix: &Utf8Path) -> anyhow::Result<Option<Prepared>> {
    let Some(installed) = installed_manifest(prefix)? else {
        return Ok(None);
    };
    let platform = Platform::current()
        .context("No Lace release for this platform")?
        .as_str();
    let work = release::work_dir()?;
    let dir = Utf8Path::from_path(work.path())
        .context("Temporary folder path is not UTF-8")?
        .to_owned();
    let get = |file: &str| -> anyhow::Result<String> {
        let name = payload::release_asset(version, platform, file);
        let path = release::download_checked_quiet(version, &name, &dir, &mut |_| {})?;
        std::fs::read_to_string(&path).with_context(|| format!("Reading {path}"))
    };
    let index = Index::parse(&get(payload::INDEX_FILE)?)
        .with_context(|| format!("Invalid index in release v{version}"))?;
    if index.lace_version != version {
        bail!(
            "The index of release v{version} is for Lace {}",
            index.lace_version
        );
    }
    let new = match get(files::FILES_FILE) {
        Ok(text) => FilesManifest::parse(&text)
            .with_context(|| format!("Invalid file manifest in release v{version}"))?,
        Err(error) => {
            tracing::info!("Release v{version} has no file manifest: {error:#}");
            return Ok(None);
        }
    };
    let plan = by_parts::plan(prefix.as_std_path(), &installed, &index, &new)?;
    Ok(Some(Prepared {
        index,
        new,
        plan,
        source: Box::new(ReleasePayload {
            version: version.to_owned(),
            platform: platform.to_owned(),
            dir,
        }),
        local: false,
        _work: Some(work),
    }))
}

/// O nome de um componente para mostrar, com os rótulos do índice novo.
fn label_of(index: &Index, name: &str) -> String {
    if name == by_parts::LACE {
        return "Lace".to_owned();
    }
    index
        .component(name)
        .map_or_else(|| name.to_owned(), |c| c.label.clone())
}

/// O que a atualização por componentes vai fazer, antes de perguntar.
fn print_plan(plan: &Plan, index: &Index, local: bool) {
    println!();
    if plan.chunks.is_empty() && plan.stale.is_empty() {
        println!("The installation already has every file of Lace {}", plan.to);
        return;
    }
    let what: Vec<String> = plan.changed.iter().map(|n| label_of(index, n)).collect();
    println!(
        "Lace {} to {} by components: {}",
        plan.from,
        plan.to,
        what.join(", ")
    );
    let mut detail = format!(
        "{} of {} chunks {} ({}), {} files in",
        plan.chunks.len(),
        index.chunks.len(),
        if local { "from the installer" } else { "to download" },
        lace_installer::mib(plan.download()),
        plan.files()
    );
    if !plan.stale.is_empty() {
        detail.push_str(&format!(", {} out", plan.stale.len()));
    }
    println!("{detail}");
    if !plan.added.is_empty() {
        let added: Vec<String> = plan.added.iter().map(|n| label_of(index, n)).collect();
        println!("Comes in, required by the new version: {}", added.join(", "));
    }
    if !plan.removed.is_empty() {
        println!(
            "Goes out, no longer in the bundle: {}",
            plan.removed.join(", ")
        );
    }
}

/// Pergunta antes de atualizar por componentes, com o tamanho do download.
fn confirm_plan(plan: &Plan, prefix: &Utf8Path) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("No terminal to confirm: use lace update --yes");
    }
    eprint!(
        "Update Lace {} to {} in {prefix}? [y/N] ",
        plan.from, plan.to
    );
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        bail!("Nothing was updated")
    }
}

/// Faz a atualização por componentes, com a barra de cada pedaço.
fn by_components(
    out: &Output,
    prefix: &Utf8Path,
    prepared: &Prepared,
) -> anyhow::Result<UpdateChanges> {
    let plan = &prepared.plan;
    let text = out.is_text();
    let mut meter = Meter::new();
    let label = |name: &str| label_of(&prepared.index, name);
    let applied = by_parts::apply(
        &*prepared.source,
        plan,
        &prepared.new,
        prefix.as_std_path(),
        |event| crate::install::show(&mut meter, &event, &label),
    )?;
    meter.finish();
    match installation::register_version(prefix, &plan.to) {
        Ok(true) => {}
        Ok(false) => {
            tracing::info!("No entry of this installation in the list of installed apps")
        }
        Err(error) => {
            tracing::info!("The list of installed apps: {error:#}");
            if text {
                println!(
                    "The list of installed apps still shows the previous version ({error:#}); \
                     Lace itself is updated"
                );
            }
        }
    }
    if text {
        let downloaded = if prepared.local {
            String::new()
        } else {
            format!("{} downloaded, ", lace_installer::mib(plan.download()))
        };
        println!(
            "Updated Lace to {} in {prefix}: {downloaded}{} files in, {} out, {} executables verified",
            plan.to, applied.replaced, applied.removed, applied.verified
        );
        if cfg!(windows) && plan.changed.iter().any(|c| c == component::STUDIO) {
            println!("If Lace Studio is open, close it and open it again to use the new version");
        }
    }
    Ok(UpdateChanges::of(plan))
}

/// `lace update --from`: a versão do instalador no disco, por componentes e
/// sem rede.
fn from_disk(
    out: &Output,
    toolchain: Option<&Toolchain>,
    options: &Options,
    path: &Utf8Path,
) -> anyhow::Result<()> {
    let prefix = installation::prefix()?;
    by_parts::clean_leftovers(prefix.as_std_path());
    let work = release::work_dir()?;
    let work_dir = Utf8Path::from_path(work.path())
        .context("Temporary folder path is not UTF-8")?
        .to_owned();
    let disk = DiskPayload::find(path, &work_dir)?;
    let index = disk.index()?;
    let new = disk.files()?.with_context(|| {
        format!(
            "The installer in {path} has no file manifest (files.json): it is older than Lace \
             0.7.0, and updates only with the installer itself"
        )
    })?;
    let installed = installed_manifest(&prefix)?.with_context(|| {
        format!(
            "The installation in {prefix} has no file manifest (it is older than Lace 0.7.0): \
             update it with the installer, or with lace update --full"
        )
    })?;
    let installed_version = env!("CARGO_PKG_VERSION");
    if is_newer(installed_version, &index.lace_version) {
        bail!(
            "The installer in {path} is Lace {}, older than this Lace ({installed_version})",
            index.lace_version
        );
    }
    let plan = by_parts::plan(prefix.as_std_path(), &installed, &index, &new)?;

    let manifest = toolchain.map(Toolchain::manifest);
    let mut report = UpdateReport {
        lace: LaceVersions {
            installed: installed_version.to_owned(),
            latest: index.lace_version.clone(),
            newer: is_newer(&index.lace_version, installed_version),
        },
        bundle: BundleVersions {
            installed: manifest.map(|m| m.bundle.clone()),
            latest: Some(index.bundle.clone()),
            newer: manifest.is_some_and(|m| is_newer(&index.bundle, &m.bundle)),
        },
        components: manifest
            .map_or(&[][..], |m| m.components.as_slice())
            .iter()
            .map(|c| {
                let release = index.component(&c.name).map(|i| i.version.clone());
                ComponentVersions {
                    name: c.name.clone(),
                    package: package_of(&c.name, &index.platform).map(str::to_owned),
                    installed: c.version.clone(),
                    release_newer: release.as_deref().is_some_and(|r| is_newer(r, &c.version)),
                    release,
                    upstream: None,
                    upstream_newer: false,
                }
            })
            .collect(),
        prefix: Some(prefix.clone()),
        action: UpdateAction::Checked,
        method: None,
        changes: Some(UpdateChanges::of(&plan)),
    };
    if out.is_text() {
        print_plan(&plan, &index, true);
    }
    if plan.chunks.is_empty() && plan.stale.is_empty() {
        report.action = if options.check {
            UpdateAction::Checked
        } else {
            UpdateAction::UpToDate
        };
        report.changes = None;
        return out.json(&report);
    }
    if options.check {
        return out.json(&report);
    }
    if !options.yes {
        confirm_plan(&plan, &prefix)?;
    }
    let prepared = Prepared {
        index,
        new,
        plan,
        source: Box::new(disk),
        local: true,
        _work: Some(work),
    };
    report.changes = Some(by_components(out, &prefix, &prepared)?);
    report.action = UpdateAction::Updated;
    report.method = Some(UpdateMethod::Components);
    out.json(&report)
}

/// A chave de ordem de uma versão: os grupos de dígitos, como números, na
/// ordem em que aparecem. Serve aos formatos do bundle (`2026-09-29`, `v5.6`,
/// `v0.7.0-nips.10`, `16.1.0`) e ao do Lace (`0.1.0`). Um sufixo com dígitos
/// fica depois da versão sem ele (`v0.7.0` < `v0.7.0-nips.1`), como nas tags
/// do surfer-aurora; numa pré-versão no estilo do semver (`0.2.0-rc.1`), essa
/// seria a ordem errada.
fn version_key(version: &str) -> Vec<u64> {
    version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|group| !group.is_empty())
        .map(|group| group.parse().unwrap_or(u64::MAX))
        .collect()
}

/// `candidate` é mais nova que `current`. Uma versão sem dígitos (a tag
/// `test`) não se compara: nunca é mais nova, nem mais velha.
fn is_newer(candidate: &str, current: &str) -> bool {
    let (candidate, current) = (version_key(candidate), version_key(current));
    !candidate.is_empty() && !current.is_empty() && candidate > current
}

/// A maior versão de uma lista de tags; as sem dígitos ficam de fora.
fn newest(tags: impl IntoIterator<Item = String>) -> Option<String> {
    tags.into_iter()
        .map(|tag| (version_key(&tag), tag))
        .filter(|(key, _)| !key.is_empty())
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, tag)| tag)
}

/// A tabela: uma linha para o Lace, uma para o bundle e uma por componente,
/// com a versão instalada, a da última release e a upstream. `(new)` marca a
/// da release mais nova que a instalada e a upstream mais nova que a do
/// bundle; `?`, a que não deu para consultar; `-`, o que não está instalado.
fn print_table(report: &UpdateReport) {
    let cell = |version: Option<&str>, newer: bool| match version {
        Some(v) if newer => format!("{v} (new)"),
        Some(v) => v.to_owned(),
        None => "?".to_owned(),
    };
    let mut rows: Vec<[String; 4]> = vec![
        [
            String::new(),
            "Installed".to_owned(),
            format!("Release {}", report.lace.latest),
            "Upstream".to_owned(),
        ],
        [
            "lace".to_owned(),
            report.lace.installed.clone(),
            cell(Some(&report.lace.latest), report.lace.newer),
            String::new(),
        ],
        [
            "bundle".to_owned(),
            report
                .bundle
                .installed
                .clone()
                .unwrap_or_else(|| "-".to_owned()),
            cell(report.bundle.latest.as_deref(), report.bundle.newer),
            String::new(),
        ],
    ];
    for c in &report.components {
        rows.push([
            c.name.clone(),
            c.installed.clone(),
            cell(c.release.as_deref(), c.release_newer),
            cell(c.upstream.as_deref(), c.upstream_newer),
        ]);
    }
    let mut widths = [0; 3];
    for row in &rows {
        for (width, text) in widths.iter_mut().zip(row) {
            *width = (*width).max(text.chars().count());
        }
    }
    for [name, installed, release, upstream] in &rows {
        let line = format!(
            "{name:<w0$}  {installed:<w1$}  {release:<w2$}  {upstream}",
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2],
        );
        println!("{}", line.trim_end());
    }
}

/// O que fazer depois da tabela, quando não há atualização a instalar agora.
fn print_summary(report: &UpdateReport) {
    let lace = &report.lace;
    println!();
    if lace.newer {
        println!("Lace {} is available: update with lace update", lace.latest);
    } else if is_newer(&lace.installed, &lace.latest) {
        println!(
            "This Lace ({}) is newer than the latest release ({})",
            lace.installed, lace.latest
        );
    } else {
        println!("Lace is up to date ({})", lace.latest);
    }
    if report.bundle.installed.is_none() {
        println!("No bundle: no components to compare");
    }
    let news = upstream_news(&report.components);
    if !news.is_empty() {
        let list = news
            .iter()
            .map(|(package, version, names)| {
                if names.len() == 1 && names[0] == *package {
                    format!("{package} {version}")
                } else {
                    format!("{package} {version} ({})", names.join(", "))
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        let what = if news.len() == 1 {
            "Upstream version newer than the bundle's"
        } else {
            "Upstream versions newer than the bundle's"
        };
        println!("{what}: {list}");
        println!(
            "Lace does not install tools straight from upstream: they come in a new bundle, \
             with a new Lace release, which lace update installs once it is out."
        );
    }
    let unknown =
        report.components.iter().any(|c| c.upstream.is_none()) || report.bundle.latest.is_none();
    if unknown {
        println!("?: Could not be checked (run with -v to see why)");
    }
}

/// As versões upstream mais novas que as do bundle, uma por pacote, com os
/// componentes instalados que saem dele, na ordem do manifesto.
fn upstream_news(components: &[ComponentVersions]) -> Vec<(&str, &str, Vec<&str>)> {
    let mut news: Vec<(&str, &str, Vec<&str>)> = Vec::new();
    for c in components.iter().filter(|c| c.upstream_newer) {
        let (Some(package), Some(version)) = (c.package.as_deref(), c.upstream.as_deref()) else {
            continue;
        };
        match news.iter_mut().find(|(p, _, _)| *p == package) {
            Some((_, _, names)) => names.push(&c.name),
            None => news.push((package, version, vec![&c.name])),
        }
    }
    news
}

/// Pergunta no terminal. Sem terminal (um script, uma extensão), exige
/// `--yes`, como o `lace uninstall`: a atualização troca o Lace e o bundle
/// inteiro.
fn confirm(lace: &LaceVersions, prefix: &Utf8Path) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("No terminal to confirm: use lace update --yes");
    }
    eprint!(
        "Update Lace {} to {} in {prefix}, with its bundle? [y/N] ",
        lace.installed, lace.latest
    );
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        bail!("Nothing was updated")
    }
}

/// No Linux e no macOS, o instalador da versão nova, sem perguntar, com os
/// componentes, a pasta e o atalho do recibo.
///
/// Com `--json`, o que o instalador escreve vai para o stderr: o stdout é só
/// do objeto JSON.
fn unix(out: &Output, prefix: &Utf8Path, version: &str) -> anyhow::Result<UpdateAction> {
    let receipt = Receipt::load(prefix)?;
    // `--components` não aceita uma lista vazia, e sem ele o instalador
    // instalaria os recomendados.
    if receipt.components.is_empty() {
        bail!(
            "The installation in {prefix} has no components, and the installer does not reinstall \
             without any: download the installer of release v{version} and run ./install --prefix {prefix}"
        );
    }
    let work = release::work_dir()?;
    let work_path =
        Utf8Path::from_path(work.path()).context("Temporary folder path is not UTF-8")?;
    let file = format!("lace-{version}-{}.tar.gz", receipt.platform);
    if out.is_text() {
        eprintln!("Downloading {}", release::asset_url(version, &file));
    }
    let archive = release::download_checked(version, &file, work_path, out.is_text())?;
    let installer = extract(&archive, work_path)?;
    let program = installer.join("install");

    let mut command = std::process::Command::new(program.as_std_path());
    command
        .arg("--yes")
        .arg("--components")
        .arg(receipt.components.join(","))
        .arg("--prefix")
        .arg(prefix.as_std_path());
    match &receipt.link {
        Some(link) => command.arg("--link").arg(link.as_std_path()),
        None => command.arg("--no-link"),
    };
    if !out.is_text() {
        command.stdout(Stdio::from(std::io::stderr()));
    }
    let status = command
        .status()
        .with_context(|| format!("Running {program}"))?;
    if !status.success() {
        bail!(
            "The installer exited with code {}; if the installation is system-wide (/opt/lace), run: sudo lace update",
            status
                .code()
                .map_or_else(|| "?".to_owned(), |c| c.to_string())
        );
    }
    if out.is_text() {
        println!("Updated Lace to {version} in {prefix}");
    }
    Ok(UpdateAction::Updated)
}

/// No Windows, o assistente de instalação da versão nova, que lembra a
/// pasta, com os componentes `installed` marcados. Ele troca o `lace.exe`,
/// então roda depois que este `lace` sai.
fn windows(out: &Output, version: &str, installed: &[String]) -> anyhow::Result<UpdateAction> {
    // Fica depois que o `lace` sai: o assistente roda dela.
    let work = release::work_dir()?.keep();
    let work = Utf8PathBuf::from_path_buf(work)
        .map_err(|p| anyhow::anyhow!("Temporary folder path is not UTF-8: {}", p.display()))?;
    let file = format!("lace-{version}-windows-x64-setup.exe");
    if out.is_text() {
        eprintln!("Downloading {}", release::asset_url(version, &file));
    }
    let setup = release::download_checked(version, &file, &work, out.is_text())?;
    let mut command = std::process::Command::new(setup.as_std_path());
    // O assistente só lembra a seleção feita nele, e refaz o `toolchain\`
    // com ela: sem a lista, o que o `lace install` acrescentou sairia.
    let selected = match wizard_components(version, installed, &work) {
        Ok(components) => {
            command
                .arg("/TYPE=avancada")
                .arg(format!("/COMPONENTS={}", components.join(",")));
            true
        }
        Err(error) => {
            tracing::warn!("Could not list the components of the new setup: {error:#}");
            if out.is_text() {
                eprintln!(
                    "Warning: could not read the components of release v{version} ({error:#}); \
                     in the setup wizard, check that every app you use is selected"
                );
            }
            false
        }
    };
    installation::open_wizard(&mut command).with_context(|| format!("Opening {setup}"))?;
    if out.is_text() {
        if selected {
            println!(
                "Opened the Lace {version} setup wizard, with the installed components selected"
            );
        } else {
            println!("Opened the Lace {version} setup wizard");
        }
    }
    Ok(UpdateAction::WizardOpened)
}

/// Os nomes que o `/COMPONENTS=` do assistente de `version` aceita para os
/// componentes `installed`, tirados do índice que a release publica
/// (`lace-<versão>-windows-x64-index.json`), com a regra do `lace-pack`
/// ([`pack::inno_component_names`]): `icarus\cocotb`, `surfer_aurora`. Um
/// componente que a versão nova não tem fica de fora; o `lace` vai sempre.
fn wizard_components(
    version: &str,
    installed: &[String],
    work: &Utf8Path,
) -> anyhow::Result<Vec<String>> {
    // Sem a lista (o bundle não abriu), `/COMPONENTS=lace` desmarcaria
    // todas as ferramentas: melhor deixar a seleção que o assistente lembra.
    if installed.is_empty() {
        bail!("the installed components are unknown");
    }
    let platform = Platform::current()
        .context("No Lace installer for this platform")?
        .as_str();
    let file = payload::release_asset(version, platform, payload::INDEX_FILE);
    let path = release::download_checked_quiet(version, &file, work, &mut |_| {})?;
    let text = std::fs::read_to_string(&path).with_context(|| format!("Reading {path}"))?;
    let index: Index =
        serde_json::from_str(&text).with_context(|| format!("Invalid {file} in the release"))?;
    selected_components(&index, installed)
}

/// [`wizard_components`] sobre um índice já lido.
fn selected_components(index: &Index, installed: &[String]) -> anyhow::Result<Vec<String>> {
    let pairs: Vec<(&str, &[String])> = index
        .components
        .iter()
        .map(|c| (c.name.as_str(), c.requires.as_slice()))
        .collect();
    let names = pack::inno_component_names(&pairs)?;
    let mut selected = vec!["lace".to_owned()];
    selected.extend(
        installed
            .iter()
            .filter_map(|c| names.get(c.as_str()).cloned()),
    );
    Ok(selected)
}

/// Extrai o `.tar.gz` da release em `dir` e devolve a pasta extraída que tem
/// o programa `install` (o arquivo traz uma, `lace-<versão>-<plataforma>/`).
fn extract(archive: &Utf8Path, dir: &Utf8Path) -> anyhow::Result<Utf8PathBuf> {
    let file = std::fs::File::open(archive).with_context(|| format!("Opening {archive}"))?;
    tar::Archive::new(flate2::read::GzDecoder::new(file))
        .unpack(dir)
        .with_context(|| format!("Extracting {archive}"))?;
    for entry in dir
        .read_dir_utf8()
        .with_context(|| format!("Reading {dir}"))?
    {
        let path = entry?.path().to_owned();
        if path.join("install").is_file() {
            return Ok(path);
        }
    }
    bail!("{archive} has no Lace installer (a folder with the install program)")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use lace_core::BundleComponent;
    use serde_json::json;

    use super::*;

    /// O `bundle/versions.json` de uma release de exemplo, no formato do
    /// versionado, com o que não interessa ao `update` encurtado.
    const VERSIONS: &str = r#"{
  "bundle": "2026.10.15",
  "packages": {
    "oss-cad-suite": {
      "version": "2026-10-15",
      "assets": {
        "linux-x64": { "url": "https://example.com/oss-cad-suite.tgz", "sha256": "00" }
      }
    },
    "yanc": {
      "version": "v5.7",
      "repository": "https://github.com/nipscernlab/yanc",
      "commit": "0000000000000000000000000000000000000000"
    },
    "surfer-aurora": {
      "version": "v0.7.0-nips.10",
      "repository": "https://gitlab.com/nips-cern/surfer-aurora.git",
      "commit": "0000000000000000000000000000000000000000"
    },
    "graphviz": {
      "version": "16.1.0",
      "platforms": ["windows-x64"],
      "assets": {
        "windows-x64": { "url": "https://example.com/graphviz.zip", "sha256": "00" }
      }
    },
    "msys": {
      "version": "ucrt64-v2",
      "platforms": ["windows-x64"],
      "assets": {
        "windows-x64": { "url": "https://example.com/lace-msys.zip", "sha256": "00" }
      }
    }
  }
}"#;

    /// Fontes sem rede. `None` faz a consulta falhar; `asked` guarda os
    /// pacotes consultados upstream.
    struct Fake {
        lace: Option<&'static str>,
        versions: Option<&'static str>,
        upstream: &'static [(&'static str, &'static str)],
        asked: Mutex<Vec<String>>,
    }

    impl Fake {
        fn new(
            lace: Option<&'static str>,
            versions: Option<&'static str>,
            upstream: &'static [(&'static str, &'static str)],
        ) -> Fake {
            Fake {
                lace,
                versions,
                upstream,
                asked: Mutex::new(Vec::new()),
            }
        }

        fn asked(&self) -> Vec<String> {
            let mut asked = self.asked.lock().unwrap().clone();
            asked.sort();
            asked
        }
    }

    impl Sources for Fake {
        fn latest_lace(&self) -> anyhow::Result<String> {
            self.lace.map(str::to_owned).context("sem rede")
        }

        fn release_versions(&self, version: &str) -> anyhow::Result<String> {
            assert_eq!(Some(version), self.lace, "o bundle é o da última release");
            self.versions.map(str::to_owned).context("sem rede")
        }

        fn upstream(&self, package: &str) -> anyhow::Result<String> {
            self.asked.lock().unwrap().push(package.to_owned());
            self.upstream
                .iter()
                .find(|(p, _)| *p == package)
                .map(|(_, v)| (*v).to_owned())
                .context("fora do ar")
        }
    }

    /// Um manifesto instalado, como o que `Toolchain::open` lê.
    fn manifest(platform: &str, components: &[(&str, &str)]) -> BundleManifest {
        let mut manifest: BundleManifest = serde_json::from_value(json!({
            "schema": 2,
            "bundle": "2026.09.29",
            "platform": platform,
        }))
        .unwrap();
        manifest.components = components
            .iter()
            .map(|(name, version)| {
                serde_json::from_value::<BundleComponent>(json!({
                    "name": name,
                    "version": version,
                    "dir": name,
                    "source": "https://example.com",
                }))
                .unwrap()
            })
            .collect();
        manifest
    }

    fn row<'a>(report: &'a UpdateReport, name: &str) -> &'a ComponentVersions {
        report
            .components
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("{name} não está no relatório"))
    }

    #[test]
    fn versions_compare_by_their_digit_groups() {
        for (older, newer) in [
            ("2026-09-29", "2026-10-01"),
            ("2026-09-29", "2027-01-01"),
            ("v5.6", "v5.7"),
            ("v5.6", "v5.6.1"),
            ("v5.9", "v5.10"),
            ("v0.7.0-nips.9", "v0.7.0-nips.10"),
            ("v0.7.0", "v0.7.0-nips.1"),
            ("v0.6.0", "v0.7.0"),
            ("16.1.0", "16.10.0"),
            ("0.1.0", "0.2.0"),
            ("2026.09.29", "2026.10.15"),
        ] {
            assert!(is_newer(newer, older), "{newer} > {older}");
            assert!(!is_newer(older, newer), "{older} < {newer}");
        }
        for (a, b) in [("v5.6", "v5.6"), ("0.1.0", "v0.1.0"), ("test", "v5.6")] {
            assert!(!is_newer(a, b) && !is_newer(b, a), "{a} e {b}");
        }
    }

    #[test]
    fn the_newest_tag_wins_whatever_the_order() {
        // As tags do surfer-aurora como a API do GitLab as devolve.
        let tags = [
            "v0.7.0-nips.10",
            "v0.7.0-nips.9",
            "v0.7.0-nips.2",
            "v0.7.0-nips.1",
            "v0.7.0",
            "v0.6.0",
            "0.1.0",
            "v0.1.0",
            "test",
        ];
        let mut shuffled = tags.map(str::to_owned).to_vec();
        shuffled.reverse();
        assert_eq!(newest(shuffled).as_deref(), Some("v0.7.0-nips.10"));
        assert_eq!(newest(["test".to_owned()]), None);
    }

    #[test]
    fn components_come_from_the_packages_of_their_platform() {
        assert_eq!(package_of("icarus", "linux-x64"), Some("oss-cad-suite"));
        assert_eq!(
            package_of("verilator", "darwin-arm64"),
            Some("oss-cad-suite")
        );
        assert_eq!(package_of("icarus", "windows-x64"), Some("msys"));
        assert_eq!(package_of("verilator", "windows-x64"), Some("msys"));
        assert_eq!(package_of("yosys", "windows-x64"), Some("oss-cad-suite"));
        assert_eq!(package_of("cocotb", "linux-x64"), Some("oss-cad-suite"));
        assert_eq!(package_of("cocotb", "darwin-arm64"), Some("oss-cad-suite"));
        assert_eq!(package_of("cocotb", "windows-x64"), Some("msys"));
        assert_eq!(package_of("yosys", "darwin-arm64"), Some("oss-cad-suite"));
        assert_eq!(package_of("graphviz", "linux-x64"), Some("oss-cad-suite"));
        assert_eq!(
            package_of("graphviz", "darwin-arm64"),
            Some("oss-cad-suite")
        );
        assert_eq!(package_of("graphviz", "windows-x64"), Some("graphviz"));
        assert_eq!(package_of("yanc", "linux-x64"), Some("yanc"));
        assert_eq!(
            package_of("surfer-aurora", "windows-x64"),
            Some("surfer-aurora")
        );
        assert_eq!(package_of("studio", "darwin-arm64"), Some("studio"));
        assert_eq!(package_of("lace-learn", "windows-x64"), Some("lace-learn"));
        assert_eq!(package_of("outro", "linux-x64"), None);
        // Todo componente que o Lace conhece sai de algum pacote.
        for name in component::ALL {
            for platform in ["linux-x64", "darwin-arm64", "windows-x64"] {
                assert!(package_of(name, platform).is_some(), "{name} em {platform}");
            }
        }
    }

    #[test]
    fn the_report_compares_installed_release_and_upstream() {
        let sources = Fake::new(
            Some("0.2.0"),
            Some(VERSIONS),
            &[
                ("oss-cad-suite", "2026-10-20"),
                ("yanc", "v5.7"),
                ("surfer-aurora", "v0.7.0-nips.11"),
                ("graphviz", "16.2.0"),
            ],
        );
        let installed = manifest(
            "linux-x64",
            &[
                ("yanc", "v5.6"),
                ("icarus", "2026-09-29"),
                ("graphviz", "2026-09-29"),
                ("surfer-aurora", "v0.7.0-nips.9"),
            ],
        );
        let report = gather(&sources, "0.1.0", Some(&installed)).unwrap();

        assert_eq!(report.lace.installed, "0.1.0");
        assert_eq!(report.lace.latest, "0.2.0");
        assert!(report.lace.newer);
        assert_eq!(report.bundle.installed.as_deref(), Some("2026.09.29"));
        assert_eq!(report.bundle.latest.as_deref(), Some("2026.10.15"));
        assert!(report.bundle.newer);
        assert_eq!(report.action, UpdateAction::Checked);
        assert_eq!(report.prefix, None);
        let names: Vec<&str> = report.components.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["yanc", "icarus", "graphviz", "surfer-aurora"]);

        // A release traz um YANC mais novo, e é o último upstream.
        let yanc = row(&report, "yanc");
        assert_eq!(yanc.package.as_deref(), Some("yanc"));
        assert_eq!(yanc.release.as_deref(), Some("v5.7"));
        assert!(yanc.release_newer);
        assert_eq!(yanc.upstream.as_deref(), Some("v5.7"));
        assert!(!yanc.upstream_newer);

        // O upstream do OSS CAD Suite é mais novo que o do bundle da release.
        for name in ["icarus", "graphviz"] {
            let c = row(&report, name);
            assert_eq!(c.package.as_deref(), Some("oss-cad-suite"), "{name}");
            assert_eq!(c.installed, "2026-09-29");
            assert_eq!(c.release.as_deref(), Some("2026-10-15"));
            assert!(c.release_newer);
            assert_eq!(c.upstream.as_deref(), Some("2026-10-20"));
            assert!(c.upstream_newer, "{name}");
        }

        let surfer = row(&report, "surfer-aurora");
        assert_eq!(surfer.release.as_deref(), Some("v0.7.0-nips.10"));
        assert!(surfer.release_newer);
        assert!(surfer.upstream_newer);

        // Cada pacote é consultado uma vez, e só os dos componentes
        // instalados: no Linux, o Graphviz vem do OSS CAD Suite.
        assert_eq!(sources.asked(), ["oss-cad-suite", "surfer-aurora", "yanc"]);

        let news = upstream_news(&report.components);
        assert_eq!(
            news,
            [
                ("oss-cad-suite", "2026-10-20", vec!["icarus", "graphviz"]),
                ("surfer-aurora", "v0.7.0-nips.11", vec!["surfer-aurora"]),
            ]
        );
    }

    #[test]
    fn upstream_newer_is_relative_to_the_release_bundle() {
        // A última release é a instalada; o upstream do OSS CAD Suite andou,
        // o do YANC não, e o do Graphviz no Windows é o do bundle.
        let sources = Fake::new(
            Some("0.1.0"),
            Some(VERSIONS),
            &[
                ("oss-cad-suite", "2026-10-15"),
                ("yanc", "v5.7"),
                ("graphviz", "16.1.0"),
            ],
        );
        let installed = manifest(
            "windows-x64",
            &[
                ("yanc", "v5.7"),
                ("yosys", "2026-10-15"),
                ("graphviz", "16.1.0"),
            ],
        );
        let report = gather(&sources, "0.1.0", Some(&installed)).unwrap();
        assert!(!report.lace.newer);
        for c in &report.components {
            assert!(!c.release_newer, "{}", c.name);
            assert!(!c.upstream_newer, "{}", c.name);
        }
        assert_eq!(
            row(&report, "graphviz").package.as_deref(),
            Some("graphviz")
        );
        assert!(upstream_news(&report.components).is_empty());

        // Upstream mais nova que a instalada, mas não que a da release: o
        // que falta chega atualizando o Lace, e não é marcado.
        let sources = Fake::new(
            Some("0.2.0"),
            Some(VERSIONS),
            &[("oss-cad-suite", "2026-10-15")],
        );
        let installed = manifest("linux-x64", &[("yosys", "2026-09-29")]);
        let report = gather(&sources, "0.1.0", Some(&installed)).unwrap();
        let yosys = row(&report, "yosys");
        assert!(yosys.release_newer);
        assert!(!yosys.upstream_newer);
    }

    #[test]
    fn windows_simulators_follow_the_lace_toolchain() {
        let sources = Fake::new(Some("0.1.0"), Some(VERSIONS), &[("msys", "ucrt64-v3")]);
        let installed = manifest(
            "windows-x64",
            &[("icarus", "ucrt64-v1"), ("verilator", "ucrt64-v1")],
        );
        let report = gather(&sources, "0.1.0", Some(&installed)).unwrap();
        for name in ["icarus", "verilator"] {
            let c = row(&report, name);
            assert_eq!(c.package.as_deref(), Some("msys"));
            assert_eq!(c.release.as_deref(), Some("ucrt64-v2"));
            assert_eq!(c.upstream.as_deref(), Some("ucrt64-v3"));
            assert!(c.release_newer && c.upstream_newer, "{name}");
        }
    }

    #[test]
    fn a_failed_source_only_empties_its_column() {
        let sources = Fake::new(Some("0.1.0"), None, &[("yanc", "v5.7")]);
        let installed = manifest("linux-x64", &[("yanc", "v5.6"), ("icarus", "2026-09-29")]);
        let report = gather(&sources, "0.1.0", Some(&installed)).unwrap();
        assert_eq!(report.bundle.installed.as_deref(), Some("2026.09.29"));
        assert_eq!(report.bundle.latest, None);
        assert!(!report.bundle.newer);

        let icarus = row(&report, "icarus");
        assert_eq!(icarus.release, None);
        assert_eq!(icarus.upstream, None);
        assert!(!icarus.release_newer && !icarus.upstream_newer);

        // Sem o bundle da release e sem Lace mais novo, a upstream se
        // compara com a instalada.
        let yanc = row(&report, "yanc");
        assert_eq!(yanc.release, None);
        assert_eq!(yanc.upstream.as_deref(), Some("v5.7"));
        assert!(yanc.upstream_newer);

        // Com Lace mais novo, não dá para saber: o bundle dele pode já
        // trazer o YANC novo.
        let sources = Fake::new(Some("0.2.0"), None, &[("yanc", "v5.7")]);
        let newer = gather(&sources, "0.1.0", Some(&installed)).unwrap();
        assert!(newer.lace.newer);
        assert!(!row(&newer, "yanc").upstream_newer);

        // `null` no JSON, não ausente.
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(value["bundle"]["latest"], serde_json::Value::Null);
        assert_eq!(value["components"][1]["upstream"], serde_json::Value::Null);
        assert_eq!(value["action"], "checked");
    }

    #[test]
    fn without_a_bundle_only_the_lace_is_compared() {
        let sources = Fake::new(Some("0.2.0"), Some(VERSIONS), &[]);
        let report = gather(&sources, "0.1.0", None).unwrap();
        assert!(report.lace.newer);
        assert_eq!(report.bundle.installed, None);
        assert_eq!(report.bundle.latest.as_deref(), Some("2026.10.15"));
        assert!(!report.bundle.newer);
        assert!(report.components.is_empty());
        assert!(sources.asked().is_empty());
    }

    #[test]
    fn without_github_it_is_an_error() {
        let sources = Fake::new(None, Some(VERSIONS), &[("yanc", "v5.7")]);
        let installed = manifest("linux-x64", &[("yanc", "v5.6")]);
        let error = gather(&sources, "0.1.0", Some(&installed)).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("latest Lace version"), "{message}");
        assert!(message.contains("GitHub"), "{message}");
    }

    #[test]
    fn the_bundle_versions_file_of_this_tree_is_read() {
        let text = std::fs::read_to_string(
            Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bundle/versions.json"),
        )
        .unwrap();
        let bundle: ReleaseBundle = serde_json::from_str(&text).unwrap();
        for name in component::ALL {
            for platform in ["linux-x64", "darwin-arm64", "windows-x64"] {
                let package = package_of(name, platform).unwrap();
                assert!(
                    bundle.packages.contains_key(package),
                    "{package} ({name} em {platform}) não está em bundle/versions.json"
                );
            }
        }
    }

    /// O `tests/schema.rs` não roda `lace update`, que precisa de rede: o
    /// relatório montado aqui, com colunas cheias e vazias, confere com
    /// `docs/schema/update.json`.
    #[test]
    fn the_json_matches_its_schema() {
        // Na rodada que regrava `docs/schema/`, o arquivo pode estar no meio
        // da escrita; a rodada seguinte confere.
        if std::env::var_os("LACE_UPDATE_SCHEMA").is_some() {
            return;
        }
        let path = Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/schema/update.json");
        let schema = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let sources = Fake::new(Some("0.2.0"), Some(VERSIONS), &[("yanc", "v5.8")]);
        let installed = manifest("linux-x64", &[("yanc", "v5.6"), ("icarus", "2026-09-29")]);
        let mut report = gather(&sources, "0.1.0", Some(&installed)).unwrap();
        for (action, prefix) in [
            (UpdateAction::Checked, None),
            (UpdateAction::Updated, Some(Utf8PathBuf::from("/opt/lace"))),
        ] {
            report.action = action;
            report.prefix = prefix;
            let value = serde_json::to_value(&report).unwrap();
            let errors: Vec<String> = validator
                .iter_errors(&value)
                .map(|e| e.to_string())
                .collect();
            assert!(errors.is_empty(), "{errors:?}\n{value:#}");
        }
    }

    #[test]
    fn the_wizard_gets_every_installed_component_by_its_inno_name() {
        let component = |name: &str, requires: &[&str]| lace_installer::payload::ComponentInfo {
            name: name.into(),
            label: name.into(),
            description: String::new(),
            recommended: true,
            requires: requires.iter().map(|r| (*r).to_owned()).collect(),
            version: "1".into(),
        };
        let index = Index {
            schema: payload::INDEX_SCHEMA,
            lace_version: "0.3.0".into(),
            bundle: "2026.10.01".into(),
            platform: "windows-x64".into(),
            components: vec![
                component("yanc", &[]),
                component("icarus", &[]),
                component("verilator", &[]),
                component("cocotb", &["icarus"]),
                component("yosys", &[]),
                component("graphviz", &["yosys"]),
                component("surfer-aurora", &[]),
                component("studio", &[]),
            ],
            chunks: Vec::new(),
        };
        // O Verilator e o cocotb entraram depois, pelo `lace install`; um
        // componente que a versão nova não tem mais fica de fora.
        let installed: Vec<String> = [
            "yanc",
            "icarus",
            "verilator",
            "cocotb",
            "surfer-aurora",
            "antigo",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            selected_components(&index, &installed).unwrap(),
            [
                "lace",
                "yanc",
                "icarus",
                "verilator",
                "icarus\\cocotb",
                "surfer_aurora"
            ]
        );
    }
}
