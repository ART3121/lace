//! `lace update`: compara as versões instaladas com as publicadas e, se há
//! um Lace mais novo, atualiza a instalação de onde este `lace` roda.
//!
//! A regra do que é atualizar mora em `lace_installer::update` (consultar,
//! planejar por componentes, aplicar, cair para o instalador inteiro), e é
//! a mesma do Studio e do `lace serve`. Aqui fica só o que é da linha de
//! comando: a tabela, o plano impresso, as duas perguntas, a barra de
//! progresso, as frases de resumo e o JSON.
//!
//! Com `--json`, o stdout é só do objeto; o progresso e os avisos de
//! fallback vão para o stderr, que o Studio mostrava linha a linha.

use std::cell::RefCell;
use std::io::{BufRead, IsTerminal, Write};
use std::rc::Rc;

use anyhow::bail;
use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{CancelToken, Toolchain, component};
use lace_installer::install::Event;
use lace_installer::payload::Index;
use lace_installer::release::{Bar, Network, Remote};
use lace_installer::update::{
    AppliedUpdate, ChildOutput, Hooks, LaceVersions, Plan, Prepared, Reopen, RunOptions,
    UpdateAction, UpdateMethod, UpdateReport, upstream_news,
};

use crate::installation;
use crate::meter::Meter;
use crate::output::Output;

pub use lace_installer::release::is_newer;

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
    /// `--reopen-studio`: quem pede é o Lace Studio, que fecha para a
    /// atualização. No Windows, o assistente de instalação o abre de novo
    /// quando termina.
    pub reopen_studio: bool,
}

/// O que a CLI mostra e pergunta no meio da atualização.
struct CliHooks<'a> {
    out: &'a Output,
    yes: bool,
    /// Com `--from`, o plano é o que se mostra; sem ele, a tabela e o
    /// resumo.
    from_disk: bool,
    /// O índice da versão nova, para os rótulos da barra de progresso.
    index: Rc<RefCell<Option<Index>>>,
}

impl Hooks for CliHooks<'_> {
    fn report(&mut self, report: &UpdateReport) {
        crate::update_check::record(&report.lace.latest);
        if self.out.is_text() {
            print_table(report);
        }
    }

    fn finished_without_update(&mut self, report: &UpdateReport) {
        if self.out.is_text() && !self.from_disk {
            print_summary(report);
        }
    }

    fn plan(&mut self, prepared: &Prepared) {
        *self.index.borrow_mut() = Some(prepared.index.clone());
        if self.out.is_text() {
            print_plan(&prepared.plan, &prepared.index, prepared.local);
        }
    }

    fn confirm_plan(
        &mut self,
        prepared: &Prepared,
        prefix: &Utf8Path,
    ) -> lace_installer::error::Result<()> {
        if self.yes {
            return Ok(());
        }
        confirm_plan(&prepared.plan, prefix).map_err(lace_installer::Error::Other)
    }

    fn confirm_installer(
        &mut self,
        lace: &LaceVersions,
        prefix: &Utf8Path,
    ) -> lace_installer::error::Result<()> {
        if self.yes {
            return Ok(());
        }
        confirm(lace, prefix).map_err(lace_installer::Error::Other)
    }

    fn applied(&mut self, prepared: &Prepared, applied: &AppliedUpdate, prefix: &Utf8Path) {
        if !self.out.is_text() {
            return;
        }
        if let Some(warning) = &applied.registry_warning {
            println!("{warning}");
        }
        let plan = &prepared.plan;
        let downloaded = if prepared.local {
            String::new()
        } else {
            format!("{} downloaded, ", lace_installer::mib(plan.download()))
        };
        println!(
            "Updated Lace to {} in {prefix}: {downloaded}{} files in, {} out, {} executables verified",
            plan.to, applied.applied.replaced, applied.applied.removed, applied.applied.verified
        );
        if cfg!(windows) && plan.changed.iter().any(|c| c == component::STUDIO) {
            println!("If AURORA IDE is open, close it and open it again to use the new version");
        }
    }
}

pub fn run(
    out: &Output,
    toolchain: Option<&Toolchain>,
    options: Options,
    cancel: &CancelToken,
) -> anyhow::Result<()> {
    let remote = std::sync::Arc::new(Remote(network()?));
    let prefix = installation::prefix_result();
    let text = out.is_text();
    let index: Rc<RefCell<Option<Index>>> = Rc::new(RefCell::new(None));
    let mut hooks = CliHooks {
        out,
        yes: options.yes,
        from_disk: options.from.is_some(),
        index: Rc::clone(&index),
    };
    let mut meter = Meter::new();
    let mut selected = false;
    let mut on = |event: Event| match event {
        // No stderr também com `--json`: o Studio mostra.
        Event::Fallback { reason } | Event::Warning { message: reason } => eprintln!("{reason}"),
        Event::FallbackDownload { url, .. } => {
            if text {
                eprintln!("Downloading {url}");
            }
        }
        Event::WizardSelection {
            selected: chosen,
            warning,
        } => {
            selected = chosen;
            if text && let Some(warning) = warning {
                eprintln!("Warning: {warning}");
            }
        }
        Event::InstallerOutput { line } => eprintln!("{line}"),
        other => {
            let index = index.borrow();
            let label = |name: &str| match &*index {
                Some(index) => label_of(index, name),
                None => name.to_owned(),
            };
            crate::install::show(&mut meter, &other, &label);
        }
    };
    let reg = reg_exe();
    let run_options = RunOptions {
        check: options.check,
        full: options.full,
        from: options.from,
        reopen: options.reopen_studio.then_some(Reopen::Studio),
        bar: if text { Bar::Curl } else { Bar::Quiet },
        child_output: if text {
            ChildOutput::Inherit
        } else {
            ChildOutput::ToStderr
        },
        reg: reg.as_deref(),
    };
    let result = lace_installer::update::run(
        &remote,
        env!("CARGO_PKG_VERSION"),
        toolchain.map(Toolchain::manifest),
        prefix,
        &run_options,
        &mut hooks,
        &mut on,
        cancel,
    );
    meter.finish();
    let report = result.map_err(crate::installation::lib_error)?;
    if text && report.method == Some(UpdateMethod::Installer) {
        let version = &report.lace.latest;
        match report.action {
            UpdateAction::Updated => {
                let prefix = report.prefix.as_deref().unwrap_or(Utf8Path::new(""));
                println!("Updated Lace to {version} in {prefix}");
            }
            UpdateAction::WizardOpened if selected => println!(
                "Opened the Lace {version} setup wizard, with the installed components selected"
            ),
            UpdateAction::WizardOpened => println!("Opened the Lace {version} setup wizard"),
            UpdateAction::Checked | UpdateAction::UpToDate => {}
        }
    }
    out.json(&report)
}

/// A rede como as variáveis de ambiente a descrevem (`LACE_REPO`,
/// `LACE_RELEASE_URL`).
pub(crate) fn network() -> anyhow::Result<Network> {
    Network::from_env().map_err(crate::installation::lib_error)
}

/// O `reg.exe` do Windows, pelo `SystemRoot`.
pub(crate) fn reg_exe() -> Option<Utf8PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    Some(
        std::env::var("SystemRoot")
            .map(|root| Utf8PathBuf::from(format!("{root}\\System32\\reg.exe")))
            .unwrap_or_else(|_| Utf8PathBuf::from("reg.exe")),
    )
}

/// O nome de um componente para mostrar, com os rótulos do índice novo.
fn label_of(index: &Index, name: &str) -> String {
    lace_installer::install::label(Some(index), name)
}

/// O que a atualização por componentes vai fazer, antes de perguntar.
fn print_plan(plan: &Plan, index: &Index, local: bool) {
    println!();
    if plan.chunks.is_empty() && plan.stale.is_empty() {
        println!(
            "The installation already has every file of Lace {}",
            plan.to
        );
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
        if local {
            "from the installer"
        } else {
            "to download"
        },
        lace_installer::mib(plan.download()),
        plan.files()
    );
    if !plan.stale.is_empty() {
        detail.push_str(&format!(", {} out", plan.stale.len()));
    }
    println!("{detail}");
    if !plan.added.is_empty() {
        let added: Vec<String> = plan.added.iter().map(|n| label_of(index, n)).collect();
        println!(
            "Comes in, required by the new version: {}",
            added.join(", ")
        );
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
    crate::cancel::check()?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        bail!("Nothing was updated")
    }
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
    crate::cancel::check()?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        bail!("Nothing was updated")
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8Path;
    use lace_core::BundleComponent;
    use lace_installer::release::Sources;
    use serde_json::json;

    use super::*;

    /// O `bundle/versions.json` de uma release de exemplo.
    const VERSIONS: &str = r#"{
  "bundle": "2026.10.15",
  "packages": {
    "oss-cad-suite": { "version": "2026-10-15" },
    "yanc": { "version": "v5.7" },
    "surfer-aurora": { "version": "v0.7.0-nips.10" },
    "graphviz": { "version": "16.1.0" },
    "msys": { "version": "ucrt64-v2" }
  }
}"#;

    /// Fontes sem rede, para montar um relatório.
    struct Fake;

    impl Sources for Fake {
        fn latest_lace(&self, _cancel: &CancelToken) -> lace_installer::error::Result<String> {
            Ok("0.2.0".to_owned())
        }

        fn release_versions(
            &self,
            _version: &str,
            _cancel: &CancelToken,
        ) -> lace_installer::error::Result<String> {
            Ok(VERSIONS.to_owned())
        }

        fn upstream(
            &self,
            package: &str,
            _cancel: &CancelToken,
        ) -> lace_installer::error::Result<String> {
            match package {
                "yanc" => Ok("v5.8".to_owned()),
                _ => Err(lace_installer::Error::NoLatestVersion),
            }
        }

        fn release_file(
            &self,
            _version: &str,
            _file: &str,
            _dir: &Utf8Path,
            _downloaded: &mut dyn FnMut(u64),
            _cancel: &CancelToken,
        ) -> lace_installer::error::Result<Utf8PathBuf> {
            Err(lace_installer::Error::NoLatestVersion)
        }
    }

    fn manifest(components: &[(&str, &str)]) -> lace_core::BundleManifest {
        let mut manifest: lace_core::BundleManifest = serde_json::from_value(json!({
            "schema": 2,
            "bundle": "2026.09.29",
            "platform": "linux-x64",
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
        let installed = manifest(&[("yanc", "v5.6"), ("icarus", "2026-09-29")]);
        let mut report =
            lace_installer::update::check(&Fake, "0.1.0", Some(&installed), &CancelToken::new())
                .unwrap();
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
    fn the_summary_lists_upstream_news_by_package() {
        let installed = manifest(&[("yanc", "v5.6")]);
        let report =
            lace_installer::update::check(&Fake, "0.2.0", Some(&installed), &CancelToken::new())
                .unwrap();
        let news = upstream_news(&report.components);
        assert_eq!(news, [("yanc", "v5.8", vec!["yanc"])]);
        // Só imprime; não pode entrar em pânico com colunas vazias.
        print_table(&report);
        print_summary(&report);
    }
}
