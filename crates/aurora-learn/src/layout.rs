//! O arquivo de comandos do Surfer (`.sucl`) que abre a onda de um
//! exercício já arrumada: o clock e o reset, as entradas, cada saída do aluno
//! ao lado da da referência, o sinal de erro, e o cursor e um marcador no
//! primeiro erro.
//!
//! Os comandos são os do parser do surfer-aurora (`variable_add`,
//! `divider_add`, `cursor_set`, `marker_set_at`, `zoom_fit`); a AURORA passa o
//! arquivo com `-c`, e o Studio, na aba da onda.

use aurora_core::flows::{LayoutPolicy, WaveRequest, WaveSource};
use aurora_core::verilog::{ModuleInterface, PortDirection};
use camino::{Utf8Path, Utf8PathBuf};

use crate::testbench::{MISMATCH_SIGNAL, testbench_module};
use crate::track::{Exercise, Kind};
use crate::workspace::HIDDEN_DIR;

/// O layout, relativo à pasta do exercício.
pub const LAYOUT_FILE: &str = ".aurora-learn/wave.sucl";

/// O layout de uma onda de exercício: com a onda em `.aurora-learn/` e o
/// `wave.sucl` da última correção ao lado dela. É como a AURORA e o Studio
/// abrem a onda de um exercício já arrumada, por qualquer caminho.
pub fn layout_of(waveform: &Utf8Path) -> Option<Utf8PathBuf> {
    let dir = waveform.parent()?;
    if dir.file_name() != Some(HIDDEN_DIR) {
        return None;
    }
    let layout = dir.join("wave.sucl");
    layout.is_file().then_some(layout)
}

/// O pedido de onda com o layout do exercício quando o arquivo é a onda de
/// um exercício ([`layout_of`]) e o pedido quer o layout gerado: o `aurora
/// wave <arquivo>` e o `wave.open` do protocolo abrem a onda da correção
/// arrumada, como o `aurora learn wave`. Os outros pedidos ficam como vieram.
pub fn with_exercise_layout(mut request: WaveRequest) -> WaveRequest {
    if let (WaveSource::File { path }, LayoutPolicy::Generated) = (&request.source, &request.layout)
        && let Some(layout) = layout_of(path)
    {
        request.layout = LayoutPolicy::File { path: layout };
    }
    request
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wave_of_an_exercise_opens_with_its_layout() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let hidden = root.join(HIDDEN_DIR);
        std::fs::create_dir_all(&hidden).unwrap();
        let wave = hidden.join("wave.vcd");
        std::fs::write(&wave, "").unwrap();
        let request = |path: &Utf8Path, layout: LayoutPolicy| WaveRequest {
            source: WaveSource::File {
                path: path.to_owned(),
            },
            layout,
        };
        // Sem o wave.sucl ao lado, a onda fica como veio.
        let plain = request(&wave, LayoutPolicy::Generated);
        assert_eq!(with_exercise_layout(plain.clone()), plain);
        std::fs::write(hidden.join("wave.sucl"), "").unwrap();
        assert_eq!(
            with_exercise_layout(plain).layout,
            LayoutPolicy::File {
                path: hidden.join("wave.sucl")
            }
        );
        // Quem pediu outro layout, ou uma onda fora do .aurora-learn, fica.
        let none = request(&wave, LayoutPolicy::None);
        assert_eq!(with_exercise_layout(none.clone()), none);
        let outside = request(&root.join("wave.vcd"), LayoutPolicy::Generated);
        assert_eq!(with_exercise_layout(outside.clone()), outside);
    }
}
