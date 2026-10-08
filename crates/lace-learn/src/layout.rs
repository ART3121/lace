//! O arquivo de comandos do Surfer (`.sucl`) que abre a onda de um
//! exercício já arrumada: o clock e o reset, as entradas, cada saída do aluno
//! ao lado da da referência, o sinal de erro, e o cursor e um marcador no
//! primeiro erro.
//!
//! Os comandos são os do parser do surfer-aurora (`variable_add`,
//! `divider_add`, `cursor_set`, `marker_set_at`, `zoom_fit`); o Lace passa o
//! arquivo com `-c`, e o Studio, na aba da onda.

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::verilog::{ModuleInterface, PortDirection};

use crate::testbench::{MISMATCH_SIGNAL, testbench_module};
use crate::track::{Exercise, Kind};
use crate::workspace::HIDDEN_DIR;

/// O layout, relativo à pasta do exercício.
pub const LAYOUT_FILE: &str = ".lace-learn/wave.sucl";

/// O layout de uma onda de exercício: com a onda em `.lace-learn/` e o
/// `wave.sucl` da última correção ao lado dela. É como o Lace e o Studio
/// abrem a onda de um exercício já arrumada, por qualquer caminho.
pub fn layout_of(waveform: &Utf8Path) -> Option<Utf8PathBuf> {
    let dir = waveform.parent()?;
    if dir.file_name() != Some(HIDDEN_DIR) {
        return None;
    }
    let layout = dir.join("wave.sucl");
    layout.is_file().then_some(layout)
}

/// O nome do marcador do primeiro erro.
pub const FIRST_MISMATCH_MARKER: &str = "mismatch";

/// Os comandos para o testbench gerado do exercício.
pub fn commands(exercise: &Exercise, interface: &ModuleInterface, first_ns: Option<u64>) -> String {
    let tb = testbench_module(&exercise.module);
    let mut lines = Vec::new();
    let control: Vec<String> = match exercise.spec.kind {
        Kind::Sequential => {
            std::iter::once(exercise.spec.clock.clone().unwrap_or_else(|| "clk".into()))
                .chain(exercise.spec.reset.as_ref().map(|r| r.name.clone()))
                .collect()
        }
        _ => Vec::new(),
    };
    for name in &control {
        lines.push(format!("variable_add {tb}.{name}"));
    }
    let inputs: Vec<&str> = interface
        .ports
        .iter()
        .filter(|p| p.direction == PortDirection::Input && !control.contains(&p.name))
        .map(|p| p.name.as_str())
        .collect();
    for name in &inputs {
        lines.push(format!("variable_add {tb}.{name}"));
    }
    lines.push("divider_add".to_owned());
    for port in interface
        .ports
        .iter()
        .filter(|p| p.direction == PortDirection::Output)
    {
        lines.push(format!("variable_add {tb}.{}", port.name));
        lines.push(format!("variable_add {tb}.{}_ref", port.name));
    }
    lines.push("divider_add".to_owned());
    lines.push(format!("variable_add {tb}.{MISMATCH_SIGNAL}"));
    finish(&mut lines, first_ns);
    lines.join("\n") + "\n"
}

/// Os comandos para um testbench escrito à mão: o escopo inteiro dele.
pub fn scope_commands(testbench: &str, first_ns: Option<u64>) -> String {
    let mut lines = vec![format!("scope_add {testbench}")];
    finish(&mut lines, first_ns);
    lines.join("\n") + "\n"
}

fn finish(lines: &mut Vec<String>, first_ns: Option<u64>) {
    lines.push("zoom_fit".to_owned());
    if let Some(ns) = first_ns {
        lines.push(format!("cursor_set {ns}ns"));
        lines.push(format!("marker_set_at {ns}ns {FIRST_MISMATCH_MARKER}"));
    }
}
