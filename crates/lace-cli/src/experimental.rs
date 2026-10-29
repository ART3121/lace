//! `lace experimental`: os recursos experimentais do Lace, desligados até o
//! usuário ligar (`lace_core::experimental`). O que a CLI liga vale também
//! para o Lace Studio: os dois leem o mesmo `experimental.json`.

use anyhow::anyhow;
use clap::builder::PossibleValuesParser;
use lace_core::experimental::{self, Feature};

use crate::output::{BOLD, DIM, OK, Output, paint};
use crate::report::ExperimentalReport;
use crate::{ExperimentalArgs, ExperimentalCommand};

/// Os identificadores que `enable` e `disable` aceitam: os do Core.
pub fn component_parser() -> PossibleValuesParser {
    PossibleValuesParser::new(Feature::ALL.iter().map(|feature| feature.id()))
}

/// Sem subcomando, a lista; com `enable` ou `disable`, a mudança e depois a
/// lista no JSON.
pub fn run(args: &ExperimentalArgs, out: &Output) -> anyhow::Result<()> {
    let change = match &args.command {
        None => None,
        Some(ExperimentalCommand::Enable { component }) => Some((feature(component)?, true)),
        Some(ExperimentalCommand::Disable { component }) => Some((feature(component)?, false)),
    };
    if let Some((feature, enabled)) = change {
        experimental::set_enabled(feature, enabled)?;
    }
    let info = experimental::info();
    let report = ExperimentalReport {
        path: info.path,
        components: info.components,
    };
    if out.is_text() {
        match change {
            Some((feature, true)) => println(&format!(
                "{} {feature} ({}). It is experimental: it may change or break between releases.",
                paint(OK, "Enabled"),
                feature.name()
            )),
            Some((feature, false)) => {
                println(&format!("Disabled {feature} ({})", feature.name()));
            }
            None => list(&report),
        }
    }
    out.json(&report)?;
    Ok(())
}

fn feature(id: &str) -> anyhow::Result<Feature> {
    Feature::from_id(id).ok_or_else(|| anyhow!("Unknown experimental component: {id}"))
}

/// `lace experimental`: cada recurso, ligado ou não, e como ligar.
fn list(report: &ExperimentalReport) {
    println(&paint(
        BOLD,
        "Experimental components, off until enabled here or in AURORA IDE (Settings > Experimental)",
    ));
    let width = report
        .components
        .iter()
        .map(|state| state.feature.id().len())
        .max()
        .unwrap_or(0);
    for state in &report.components {
        let status = if state.enabled {
            paint(OK, "on ")
        } else {
            paint(DIM, "off")
        };
        println(&format!(
            "  {:<width$}  {status}  {}",
            state.feature.id(),
            state.description
        ));
    }
    println("Enable one with: lace experimental enable <component>");
    if let Some(path) = &report.path {
        println(&paint(DIM, format!("Settings: {path}")));
    }
}

fn println(text: &str) {
    anstream::println!("{text}");
}
