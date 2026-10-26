//! `lace wave signals|select|unselect`: a escolha dos sinais que a onda do
//! testbench do projeto grava e que o layout mostra
//! ([`lace_core::write_selection`]).
//!
//! A árvore vem da elaboração do testbench pelo Icarus
//! ([`lace_core::wave_signals`]); `select` e `unselect` conferem os caminhos
//! nela antes de gravar `wave/<testbench>.json`.

use anstream::println;
use lace_core::flows::{WaveChoice, choose_signals};
use lace_core::{Control, SignalScope, Status, WaveSignals};

use crate::output::Output;
use crate::report::WaveChoiceReport;
use crate::{Cli, WaveCommand};

pub fn run(
    cli: &Cli,
    command: &WaveCommand,
    out: &Output,
    control: &Control,
) -> anyhow::Result<bool> {
    let project = crate::commands::open(cli)?;
    let toolchain = cli.toolchain.resolve()?;
    match command {
        WaveCommand::Signals => {
            let tree = lace_core::wave_signals(&toolchain, &project, control)?;
            if out.is_text() {
                print_tree(&tree, out);
            }
            out.json(&tree)?;
            // Com a elaboração quebrada, o resultado já diz (o status e os
            // diagnósticos): um erro a mais seria um segundo objeto no
            // --json. Sai com 1, a operação que rodou e falhou.
            if out.is_text() {
                match tree.status {
                    Status::Succeeded => {}
                    Status::Cancelled => anstream::eprintln!("Cancelled with Ctrl+C"),
                    _ => anstream::eprintln!(
                        "The project testbench did not elaborate: fix the errors above"
                    ),
                }
            }
            Ok(tree.status == Status::Succeeded)
        }
        WaveCommand::Select { paths, all } => {
            let choice = if *all {
                WaveChoice::All
            } else {
                WaveChoice::Add(paths.clone())
            };
            let outcome = choose_signals(&toolchain, &project, &choice, control)?;
            write(&outcome, out)?;
            Ok(true)
        }
        WaveCommand::Unselect { paths } => {
            let choice = WaveChoice::Remove(paths.clone());
            let outcome = choose_signals(&toolchain, &project, &choice, control)?;
            write(&outcome, out)?;
            Ok(true)
        }
    }
}

/// Conta o que ficou e escreve o relatório.
fn write(outcome: &lace_core::flows::WaveChoiceOutcome, out: &Output) -> anyhow::Result<()> {
    let module = &outcome.testbench;
    let selection = &outcome.selection;
    if out.is_text() {
        match &outcome.file {
            Some(file) => {
                println!(
                    "The waveform of {module} records {} chosen {} ({file})",
                    selection.len(),
                    if selection.len() == 1 { "item" } else { "items" }
                );
                for item in selection {
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
        testbench: module.clone(),
        file: outcome.file.clone(),
        selection: selection.clone(),
    })
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
