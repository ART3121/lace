//! `lace wave signals|select|unselect`: a escolha dos sinais que a onda do
//! testbench do projeto grava e que o layout mostra
//! ([`lace_core::write_selection`]).
//!
//! A árvore vem da elaboração do testbench pelo Icarus
//! ([`lace_core::wave_signals`]); `select` e `unselect` conferem os caminhos
//! nela antes de gravar `wave/<testbench>.json`.

use anstream::println;
use anyhow::{Context, bail};
use lace_core::{Control, Project, SignalScope, Status, WaveSignals};

use crate::output::Output;
use crate::report::WaveChoiceReport;
use crate::{Cli, WaveCommand};

pub fn run(cli: &Cli, command: &WaveCommand, out: &Output) -> anyhow::Result<()> {
    let project = Project::discover(&cli.project)?;
    let toolchain = cli.toolchain.resolve()?;
    let control = Control::default();
    match command {
        WaveCommand::Signals => {
            let tree = lace_core::wave_signals(&toolchain, &project, &control)?;
            if out.is_text() {
                print_tree(&tree, out);
            }
            out.json(&tree)?;
            if tree.status != Status::Succeeded {
                bail!("The project testbench did not elaborate: fix the errors above");
            }
            Ok(())
        }
        WaveCommand::Select { paths, all } => {
            let module = lace_core::wave_testbench(&project)?;
            let selection = if *all {
                Vec::new()
            } else {
                let tree = elaborated(&toolchain, &project, &control)?;
                let root = tree.root.as_ref().expect("conferido em elaborated");
                for path in paths {
                    if !root.contains(path) {
                        bail!("{}", not_found(root, path));
                    }
                }
                let mut selection = tree.selection.clone();
                selection.extend(paths.iter().cloned());
                selection
            };
            write(&project, &module, &selection, out)
        }
        WaveCommand::Unselect { paths } => {
            let module = lace_core::wave_testbench(&project)?;
            let tree = elaborated(&toolchain, &project, &control)?;
            let root = tree.root.as_ref().expect("conferido em elaborated");
            let mut selection = tree.selection.clone();
            for path in paths {
                selection = match root.without(&selection, path) {
                    Some(rest) => rest,
                    // Um item que o design não tem mais também sai.
                    None if selection.contains(path) => {
                        selection.into_iter().filter(|s| s != path).collect()
                    }
                    None => bail!("{path} is not in the signal choice of {module}"),
                };
            }
            write(&project, &module, &selection, out)
        }
    }
}

/// A árvore, exigindo que a elaboração tenha dado certo.
fn elaborated(
    toolchain: &lace_core::Toolchain,
    project: &Project,
    control: &Control,
) -> anyhow::Result<WaveSignals> {
    let tree = lace_core::wave_signals(toolchain, project, control)?;
    if tree.root.is_none() {
        let first = tree
            .diagnostics
            .iter()
            .find(|d| d.severity == lace_core::Severity::Error)
            .map(|d| format!(": {}", d.message))
            .unwrap_or_default();
        bail!("The project testbench did not elaborate{first}; see lace check");
    }
    Ok(tree)
}

/// Grava a escolha e conta o que ficou.
fn write(
    project: &Project,
    module: &str,
    selection: &[String],
    out: &Output,
) -> anyhow::Result<()> {
    let file = lace_core::write_selection(project, module, selection)
        .with_context(|| format!("Writing the signal choice of {module}"))?;
    let selection = lace_core::read_selection(project, module)?;
    if out.is_text() {
        match &file {
            Some(file) => {
                println!(
                    "The waveform of {module} records {} chosen {} ({file})",
                    selection.len(),
                    if selection.len() == 1 { "item" } else { "items" }
                );
                for item in &selection {
                    println!("  {item}");
                }
                println!("Simulate again to record them: lace sim");
            }
            None => println!(
                "The waveform of {module} records every signal again; simulate again: lace sim"
            ),
        }
    }
    out.json(&WaveChoiceReport {
        testbench: module.to_owned(),
        file,
        selection,
    })
}

/// O erro de um caminho que a árvore não tem, com os caminhos parecidos.
fn not_found(root: &SignalScope, path: &str) -> String {
    let name = path.rsplit('.').next().unwrap_or(path);
    let mut similar = Vec::new();
    collect_named(root, name, &mut similar);
    let mut message = format!("{path} is not a signal or scope of {}", root.path);
    if !similar.is_empty() {
        similar.truncate(5);
        message.push_str(&format!("; did you mean {}?", similar.join(", ")));
    } else {
        message.push_str("; list them with: lace wave signals");
    }
    message
}

fn collect_named(scope: &SignalScope, name: &str, out: &mut Vec<String>) {
    if scope.name == name {
        out.push(scope.path.clone());
    }
    out.extend(
        scope
            .signals
            .iter()
            .filter(|s| s.name == name)
            .map(|s| s.path.clone()),
    );
    for child in &scope.scopes {
        collect_named(child, name, out);
    }
}

/// A árvore em texto: cada escopo numa linha e os sinais dele embaixo,
/// com `*` no que a onda grava por escolha.
fn print_tree(tree: &WaveSignals, out: &Output) {
    let file = &tree.selection_file;
    if tree.selection.is_empty() {
        println!(
            "Testbench {}: the waveform records every signal (no choice in {file})",
            tree.module
        );
    } else {
        println!(
            "Testbench {}: the waveform records the {} chosen items, marked with * ({file})",
            tree.module,
            tree.selection.len()
        );
    }
    for item in &tree.unknown {
        out.warning(&format!(
            "{item} is in the choice, but not in the design anymore"
        ));
    }
    let Some(root) = &tree.root else {
        for d in &tree.diagnostics {
            println!("  {}", d.message);
        }
        return;
    };
    println!();
    print_scope(root, &tree.selection, 0, false);
    println!();
    println!(
        "Choose with lace wave select <path>...; take out with lace wave unselect <path>...; \
         all signals again with lace wave select --all"
    );
}

fn print_scope(scope: &SignalScope, selection: &[String], depth: usize, inherited: bool) {
    let chosen = inherited || selection.iter().any(|s| s == &scope.path);
    let indent = "  ".repeat(depth);
    let mut line = format!("{indent}{}{}", if chosen { "*" } else { "" }, scope.name);
    if let Some(module) = &scope.module
        && *module != scope.name
    {
        line.push_str(&format!(" ({module})"));
    }
    if scope.processor {
        line.push_str(" [SAPHO processor: its own signals are always recorded]");
    }
    println!("{line}");
    if !scope.signals.is_empty() {
        let names: Vec<String> = scope
            .signals
            .iter()
            .map(|s| {
                let mark = if chosen || selection.iter().any(|item| item == &s.path) {
                    "*"
                } else {
                    ""
                };
                if s.width > 1 {
                    format!("{mark}{} ({})", s.name, s.width)
                } else {
                    format!("{mark}{}", s.name)
                }
            })
            .collect();
        for chunk in wrap(&names, 96usize.saturating_sub(indent.len())) {
            println!("{indent}  {chunk}");
        }
    }
    for child in &scope.scopes {
        print_scope(child, selection, depth + 1, chosen);
    }
}

/// Os nomes em linhas de até `width` caracteres.
fn wrap(names: &[String], width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for name in names {
        if !line.is_empty() && line.len() + 2 + name.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push_str("  ");
        }
        line.push_str(name);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
