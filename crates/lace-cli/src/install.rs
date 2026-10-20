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

use anyhow::bail;
use camino::Utf8Path;
use lace_core::{CancelToken, component};
use lace_installer::add::{self, AddPrepared, Payload};
use lace_installer::install::Event;
use lace_installer::release::Remote;
use lace_installer::tui;

use crate::installation::{self, lib_error};
use crate::meter::Meter;
use crate::output::Output;

pub fn run(
    out: &Output,
    components: &[String],
    from: Option<&Utf8Path>,
    cancel: &CancelToken,
) -> anyhow::Result<()> {
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
    lace_installer::update::recover_interrupted(&prefix).map_err(lib_error)?;
    lace_installer::update::clean_leftovers(prefix.as_std_path());
    let network = crate::update::network()?;
    let remote: Arc<Remote> = Arc::new(Remote(network.clone()));
    let payload = match from {
        Some(path) => Payload::Disk(path),
        None => Payload::Release {
            version: env!("CARGO_PKG_VERSION"),
        },
    };
    let prepared = add::prepare(remote, &network, &prefix, payload, cancel).map_err(lib_error)?;

    if components.is_empty() {
        return choose(out, &prepared, &prefix);
    }
    let label = |name: &str| {
        prepared
            .index
            .component(name)
            .map_or_else(|| name.to_owned(), |c| c.label.clone())
    };
    let mut meter = Meter::new();
    // O progresso vai para o stderr também com `--json`: o Studio mostrava.
    let report = add::add_components(
        &prepared,
        &prefix,
        components,
        &mut |event| show(&mut meter, &event, &label),
        cancel,
    );
    meter.finish();
    let report = report.map_err(lib_error)?;
    if out.is_text() {
        if report.added.is_empty() {
            println!("Already installed: {}", components.join(", "));
        } else {
            let added = report
                .added
                .iter()
                .map(|c| label(c))
                .collect::<Vec<_>>()
                .join(", ");
            println!("Installed in {}: {added}", prepared.index.bundle);
        }
    }
    out.json(&report)
}

/// A lista no terminal.
fn choose(out: &Output, prepared: &AddPrepared, prefix: &Utf8Path) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("No terminal for the list: name the apps to install, as in lace install verilator");
    }
    let app = tui::App::for_adding(
        prepared.index.clone(),
        prepared.source.clone(),
        prefix.as_std_path().to_owned(),
        lace_core::SystemCompiler::detect(),
        prepared.installed.clone(),
    );
    let app = tui::run(app)?;
    match &app.outcome {
        Some(Ok(tui::Finished::Added(report))) => {
            println!("{}", tui::plain(&tui::added_lines(report, &prepared.index)));
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
/// `prefix/toolchain`, a partir dos arquivos da release em `payload`. Cada
/// pedaço vira uma linha `[i/n] ...` no stderr, que o assistente mostra.
pub fn setup(payload: &Utf8Path, components: &[String], prefix: &Utf8Path) -> anyhow::Result<()> {
    let network = crate::update::network()?;
    let mut meter = Meter::new();
    let plain = |name: &str| name.to_owned();
    let report = add::setup(&network, payload, components, prefix, &mut |event| {
        show(&mut meter, &event, &plain)
    });
    meter.finish();
    let report = report.map_err(lib_error)?;
    println!(
        "Installed in {prefix}: {}; {} executables verified",
        if report.labels.is_empty() {
            "no apps".to_owned()
        } else {
            report.labels.join(", ")
        },
        report.verified
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
        _ => {}
    }
}
