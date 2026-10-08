//! O testbench gerado do `exercise.json`: o mesmo estímulo no módulo do
//! aluno (`dut`) e na referência (`reference`), a comparação das saídas a
//! cada amostra e o resumo que a [correção](mod@crate::grade) lê.
//!
//! A comparação é `(ref ^ dut ^ ref) !== ref`: um bit em X na referência
//! aceita qualquer valor, e um bit em X ou Z no aluno com a referência em 0
//! ou 1 é erro. O resumo sai no stdout, uma linha por fato:
//!
//! ```text
//! LACE-LEARN samples 16
//! LACE-LEARN mismatched 4         amostras com alguma saída errada
//! LACE-LEARN output y 4 25 0      erros, primeiro erro em ns (-1: nenhum), amostras em X
//! LACE-LEARN reset 2              só com reset: erros com o reset ativo
//! LACE-LEARN end
//! ```
//!
//! Um `tb.v` escrito à mão segue o mesmo contrato: instancia `<módulo>` e
//! `<módulo>_ref` (incluindo `<módulo>_ref.v`), grava a onda em
//! [`WAVE_FILE`] e escreve o resumo.

use lace_core::verilog::{ModuleInterface, Port, PortDirection};

use crate::error::{LearnError, Result};
use crate::track::{Exercise, Kind, Stimulus};

/// Até quantos bits de entrada o combinacional testa todas as combinações
/// quando o `exercise.json` não escolhe o estímulo.
pub const EXHAUSTIVE_BITS: u32 = 12;
/// O máximo de bits de entrada com o estímulo exaustivo pedido.
pub const MAX_EXHAUSTIVE_BITS: u32 = 16;
/// Amostras aleatórias, sem `samples`.
pub const DEFAULT_SAMPLES: u32 = 200;
/// Ciclos do sequencial, sem `cycles`.
pub const DEFAULT_CYCLES: u32 = 200;
/// A cada quantos ciclos, em média, o reset volta, sem `every`.
pub const DEFAULT_RESET_EVERY: u32 = 16;
/// O período do clock, em ns.
pub const CLOCK_PERIOD_NS: u32 = 10;
/// O prefixo das linhas do resumo.
pub const PROTOCOL: &str = "LACE-LEARN";
/// A onda, relativa à pasta do exercício (o CWD da simulação).
pub const WAVE_FILE: &str = ".lace-learn/wave.fst";
/// O sinal que marca a amostra com erro.
pub const MISMATCH_SIGNAL: &str = "mismatch";

/// O nome do módulo do testbench.
pub fn testbench_module(module: &str) -> String {
    format!("tb_{module}")
}

/// O nome da referência.
pub fn reference_module(module: &str) -> String {
    format!("{module}_ref")
}

/// O cabeçalho dos arquivos gerados.
pub(crate) const GENERATED: &str = "// Gerado pelo lace learn, que grava de novo este arquivo quando a trilha\n// muda: não edite.\n";

/// Escreve o testbench do exercício para as portas `interface` (as da
/// solução).
///
/// # Erros
///
/// [`LearnError::InvalidTrack`] se as portas não servem para o testbench
/// gerado: `inout`, clock ou reset que não existe ou não é uma entrada de 1
/// bit, entradas demais para o estímulo exaustivo, ou um nome que colide
/// com os do testbench.
pub fn generate(exercise: &Exercise, interface: &ModuleInterface) -> Result<String> {
    let spec = &exercise.spec;
    let invalid =
        |reason: String| LearnError::track(&exercise.source.join("exercise.json"), reason);
    if let Some(port) = interface
        .ports
        .iter()
        .find(|p| p.direction == PortDirection::Inout)
    {
        return Err(invalid(format!(
            "port '{}' is inout; the generated testbench needs a tb.v for it",
            port.name
        )));
    }
    let inputs: Vec<&Port> = interface
        .ports
        .iter()
        .filter(|p| p.direction == PortDirection::Input)
        .collect();
    let outputs: Vec<&Port> = interface
        .ports
        .iter()
        .filter(|p| p.direction == PortDirection::Output)
        .collect();
    if outputs.is_empty() {
        return Err(invalid(format!(
            "module '{}' has no outputs",
            interface.name
        )));
    }
    let names: Vec<&str> = interface.ports.iter().map(|p| p.name.as_str()).collect();
    for name in &names {
        let clash = *name == MISMATCH_SIGNAL
            || name.starts_with("lace_")
            || names.contains(&format!("{name}_ref").as_str());
        if clash {
            return Err(invalid(format!(
                "port '{name}' clashes with a name of the generated testbench"
            )));
        }
    }

    let module = &exercise.module;
    let tb = testbench_module(module);
    let mut v = String::new();
    v.push_str(&format!(
        "// O testbench do exercício {}.\n{GENERATED}\n`timescale 1ns / 1ps\n`default_nettype wire\n`include \"{}.v\"\n\n",
        exercise.name,
        reference_module(module)
    ));
    v.push_str(&format!("module {tb};\n"));

    // Os sinais.
    let (clock, reset) = match spec.kind {
        Kind::Combinational => (None, None),
        Kind::Sequential => {
            let clock = spec.clock.clone().unwrap_or_else(|| "clk".to_owned());
            check_control_input(&inputs, &clock, "clock").map_err(invalid)?;
            if let Some(reset) = &spec.reset {
                check_control_input(&inputs, &reset.name, "reset").map_err(invalid)?;
                if reset.active > 1 {
                    return Err(invalid("reset.active must be 0 or 1".into()));
                }
            }
            (Some(clock), spec.reset.clone())
        }
    };
    let data_inputs: Vec<&Port> = inputs
        .iter()
        .copied()
        .filter(|p| {
            Some(&p.name) != clock.as_ref() && Some(&p.name) != reset.as_ref().map(|r| &r.name)
        })
        .collect();
    if let Some(clock) = &clock {
        v.push_str(&format!("    reg {clock} = 0;\n"));
    }
    if let Some(reset) = &reset {
        v.push_str(&format!("    reg {} = 1'b{};\n", reset.name, reset.active));
    }
    for port in &data_inputs {
        v.push_str(&format!("    reg {}{} = 0;\n", range(port), port.name));
    }
    for port in &outputs {
        v.push_str(&format!("    wire {}{};\n", range(port), port.name));
        v.push_str(&format!("    wire {}{}_ref;\n", range(port), port.name));
    }
    v.push_str(&format!("    reg {MISMATCH_SIGNAL} = 0;\n\n"));

    // As duas instâncias.
    let connections = |suffix: &str| -> String {
        interface
            .ports
            .iter()
            .map(|p| match p.direction {
                PortDirection::Output => format!(".{0}({0}{suffix})", p.name),
                _ => format!(".{0}({0})", p.name),
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    v.push_str(&format!("    {module} dut ({});\n", connections("")));
    v.push_str(&format!(
        "    {} reference ({});\n\n",
        reference_module(module),
        connections("_ref")
    ));

    // A contagem.
    v.push_str("    integer lace_samples = 0;\n    integer lace_mismatched = 0;\n");
    for port in &outputs {
        let n = &port.name;
        v.push_str(&format!(
            "    integer lace_{n}_errors = 0;\n    integer lace_{n}_first = -1;\n    integer lace_{n}_unknown = 0;\n"
        ));
    }
    if reset.is_some() {
        v.push_str("    integer lace_reset_errors = 0;\n");
    }
    v.push_str(&format!(
        "    integer lace_seed = {};\n    integer lace_i;\n\n",
        spec.seed.unwrap_or(1)
    ));

    // A comparação.
    v.push_str("    task compare;\n        begin\n            lace_samples = lace_samples + 1;\n");
    v.push_str(&format!("            {MISMATCH_SIGNAL} = 0;\n"));
    for port in &outputs {
        let n = &port.name;
        v.push_str(&format!(
            "            if (({n}_ref ^ {n} ^ {n}_ref) !== {n}_ref) begin\n                lace_{n}_errors = lace_{n}_errors + 1;\n                if (lace_{n}_first < 0) lace_{n}_first = $time;\n                if ((^{n}) === 1'bx) lace_{n}_unknown = lace_{n}_unknown + 1;\n                {MISMATCH_SIGNAL} = 1;\n            end\n"
        ));
    }
    v.push_str(&format!(
        "            if ({MISMATCH_SIGNAL}) lace_mismatched = lace_mismatched + 1;\n"
    ));
    if let Some(reset) = &reset {
        v.push_str(&format!(
            "            if ({MISMATCH_SIGNAL} && {} === 1'b{}) lace_reset_errors = lace_reset_errors + 1;\n",
            reset.name, reset.active
        ));
    }
    v.push_str("        end\n    endtask\n\n");

    if let Some(clock) = &clock {
        v.push_str(&format!(
            "    always #{} {clock} = ~{clock};\n\n",
            CLOCK_PERIOD_NS / 2
        ));
    }

    // O estímulo.
    v.push_str(&format!(
        "    initial begin\n        $dumpfile(\"{WAVE_FILE}\");\n        $dumpvars(0, {tb});\n"
    ));
    match spec.kind {
        Kind::Combinational => combinational(&mut v, exercise, &data_inputs).map_err(invalid)?,
        Kind::Sequential => sequential(
            &mut v,
            exercise,
            clock.as_deref().unwrap_or("clk"),
            reset.as_ref(),
            &data_inputs,
        ),
    }

    // O resumo.
    v.push_str(&format!(
        "        $display(\"{PROTOCOL} samples %0d\", lace_samples);\n        $display(\"{PROTOCOL} mismatched %0d\", lace_mismatched);\n"
    ));
    for port in &outputs {
        let n = &port.name;
        v.push_str(&format!(
            "        $display(\"{PROTOCOL} output {n} %0d %0d %0d\", lace_{n}_errors, lace_{n}_first, lace_{n}_unknown);\n"
        ));
    }
    if reset.is_some() {
        v.push_str(&format!(
            "        $display(\"{PROTOCOL} reset %0d\", lace_reset_errors);\n"
        ));
    }
    v.push_str(&format!(
        "        $display(\"{PROTOCOL} end\");\n        $finish;\n    end\nendmodule\n"
    ));
    Ok(v)
}

/// `[7:0] ` para uma porta de 8 bits, `signed ` antes quando for; nada para
/// 1 bit sem sinal.
fn range(port: &Port) -> String {
    let signed = if port.signed { "signed " } else { "" };
    if port.width > 1 {
        format!("{signed}[{}:0] ", port.width - 1)
    } else {
        signed.to_owned()
    }
}

fn check_control_input(
    inputs: &[&Port],
    name: &str,
    what: &str,
) -> std::result::Result<(), String> {
    match inputs.iter().find(|p| p.name == name) {
        Some(port) if port.width == 1 => Ok(()),
        Some(_) => Err(format!("the {what} '{name}' must be a 1-bit input")),
        None => Err(format!("the module has no input '{name}' for the {what}")),
    }
}

/// O valor aleatório de uma entrada: `$random` basta até 32 bits; acima,
/// a concatenação de quantos forem precisos.
fn random_value(port: &Port) -> String {
    let words = port.width.div_ceil(32).max(1);
    if words == 1 {
        "$random(lace_seed)".to_owned()
    } else {
        let parts = vec!["$random(lace_seed)"; words as usize].join(", ");
        format!("{{{parts}}}")
    }
}

/// Uma amostra combinacional: entradas aplicadas, 5 ns, compara, 5 ns.
const SAMPLE: &str = "            #5 compare;\n            #5;\n";

fn combinational(
    v: &mut String,
    exercise: &Exercise,
    inputs: &[&Port],
) -> std::result::Result<(), String> {
    let spec = &exercise.spec;
    let bits: u32 = inputs.iter().map(|p| p.width).sum();
    let all = || {
        inputs
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let exhaustive = match spec.stimulus {
        Some(Stimulus::Exhaustive) => {
            if bits > MAX_EXHAUSTIVE_BITS {
                return Err(format!(
                    "{bits} input bits are too many for the exhaustive stimulus (at most {MAX_EXHAUSTIVE_BITS})"
                ));
            }
            true
        }
        Some(Stimulus::Random) => false,
        None => bits <= EXHAUSTIVE_BITS,
    };
    if inputs.is_empty() {
        // Sem entradas: uma amostra basta.
        v.push_str("        begin\n");
        v.push_str(SAMPLE);
        v.push_str("        end\n");
    } else if exhaustive {
        v.push_str(&format!(
            "        for (lace_i = 0; lace_i < {}; lace_i = lace_i + 1) begin\n            {{{}}} = lace_i;\n",
            1u64 << bits,
            all()
        ));
        v.push_str(SAMPLE);
        v.push_str("        end\n");
    } else {
        // Tudo 0, tudo 1 e as amostras aleatórias.
        v.push_str(&format!("        {{{}}} = 0;\n", all()));
        v.push_str(&SAMPLE.replace("            ", "        "));
        v.push_str(&format!("        {{{}}} = {{{bits}{{1'b1}}}};\n", all()));
        v.push_str(&SAMPLE.replace("            ", "        "));
        v.push_str(&format!(
            "        for (lace_i = 0; lace_i < {}; lace_i = lace_i + 1) begin\n",
            spec.samples.unwrap_or(DEFAULT_SAMPLES)
        ));
        for port in inputs {
            v.push_str(&format!(
                "            {} = {};\n",
                port.name,
                random_value(port)
            ));
        }
        v.push_str(SAMPLE);
        v.push_str("        end\n");
    }
    Ok(())
}

/// O sequencial: o clock sobe em 5 ns e a cada 10 ns. Cada ciclo compara 2
/// ns depois da subida (o estado novo com as entradas do ciclo), troca as
/// entradas na descida e compara de novo 2 ns antes da próxima subida (as
/// saídas com as entradas novas: Mealy e reset assíncrono). O reset fica
/// ativo nos dois primeiros ciclos e depois volta ao acaso.
fn sequential(
    v: &mut String,
    exercise: &Exercise,
    clock: &str,
    reset: Option<&crate::track::Reset>,
    inputs: &[&Port],
) {
    let spec = &exercise.spec;
    v.push_str(&format!(
        "        for (lace_i = 0; lace_i < {}; lace_i = lace_i + 1) begin\n            @(posedge {clock});\n            #2 compare;\n            #3;\n",
        spec.cycles.unwrap_or(DEFAULT_CYCLES)
    ));
    for port in inputs {
        v.push_str(&format!(
            "            {} = {};\n",
            port.name,
            random_value(port)
        ));
    }
    if let Some(reset) = reset {
        let active = format!("1'b{}", reset.active);
        let inactive = format!("1'b{}", 1 - reset.active);
        let every = reset.every.unwrap_or(DEFAULT_RESET_EVERY);
        if every == 0 {
            v.push_str(&format!(
                "            {} = (lace_i < 1) ? {active} : {inactive};\n",
                reset.name
            ));
        } else {
            v.push_str(&format!(
                "            {} = (lace_i < 1 || ($unsigned($random(lace_seed)) % {every}) == 0) ? {active} : {inactive};\n",
                reset.name
            ));
        }
    }
    v.push_str("            #3 compare;\n        end\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{Reset, Spec};
    use camino::Utf8PathBuf;

    /// As portas de um módulo escrito no teste, pelo mesmo leitor que a
    /// trilha usa (os tipos do lace-core não se constroem fora dele).
    fn interface(text: &str) -> ModuleInterface {
        let tmp = tempfile::tempdir().unwrap();
        let file = Utf8PathBuf::from_path_buf(tmp.path().join("m.v")).unwrap();
        std::fs::write(&file, text).unwrap();
        lace_core::verilog::read_interfaces(None, &[file])
            .unwrap()
            .remove(0)
    }

    fn exercise(module: &str, spec: Spec) -> Exercise {
        Exercise {
            name: module.to_owned(),
            chapter: "01_teste".into(),
            source: "trilha/01_teste/01_x".into(),
            title: "T".into(),
            prompt: String::new(),
            hints: Vec::new(),
            module: module.to_owned(),
            spec,
            custom_testbench: false,
            given: Vec::new(),
        }
    }

    fn spec(json: &str) -> Spec {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn small_combinational_circuits_get_every_combination() {
        let iface = interface("module mux2(input a, input b, input sel, output y);\nendmodule\n");
        let tb = generate(
            &exercise("mux2", spec(r#"{"kind": "combinational"}"#)),
            &iface,
        )
        .unwrap();
        insta::assert_snapshot!(tb);
    }

    #[test]
    fn wide_inputs_get_random_samples_after_the_corners() {
        let iface =
            interface("module soma(input [15:0] a, input [15:0] b, output [16:0] s);\nendmodule\n");
        let tb = generate(
            &exercise(
                "soma",
                spec(r#"{"kind": "combinational", "samples": 50, "seed": 7}"#),
            ),
            &iface,
        )
        .unwrap();
        assert!(tb.contains("{a, b} = 0;"), "{tb}");
        assert!(tb.contains("{a, b} = {32{1'b1}};"), "{tb}");
        assert!(tb.contains("lace_i < 50"), "{tb}");
        assert!(tb.contains("integer lace_seed = 7;"), "{tb}");
        assert!(tb.contains("wire [16:0] s_ref;"), "{tb}");
    }

    #[test]
    fn inputs_wider_than_32_bits_concatenate_random_words() {
        let iface = interface("module w(input [39:0] a, output [39:0] y);\nendmodule\n");
        let tb = generate(&exercise("w", spec(r#"{"kind": "combinational"}"#)), &iface).unwrap();
        assert!(
            tb.contains("a = {$random(lace_seed), $random(lace_seed)};"),
            "{tb}"
        );
    }

    #[test]
    fn sequential_circuits_get_clock_reset_and_two_samples_per_cycle() {
        let iface = interface(
            "module conta(input clk, input reset, input en, output reg [3:0] q);\nendmodule\n",
        );
        let mut s = spec(r#"{"kind": "sequential", "cycles": 40}"#);
        s.reset = Some(Reset {
            name: "reset".into(),
            active: 1,
            every: Some(8),
        });
        let tb = generate(&exercise("conta", s), &iface).unwrap();
        insta::assert_snapshot!(tb);
    }

    #[test]
    fn ports_that_the_generated_testbench_cannot_drive_are_refused() {
        let inout = interface("module m(inout a, output y);\nendmodule\n");
        assert!(generate(&exercise("m", spec(r#"{"kind": "combinational"}"#)), &inout).is_err());

        let no_clock = interface("module m(input a, output y);\nendmodule\n");
        let error = generate(&exercise("m", spec(r#"{"kind": "sequential"}"#)), &no_clock)
            .unwrap_err()
            .to_string();
        assert!(error.contains("no input 'clk'"), "{error}");

        let clash = interface("module m(input a, output y, output y_ref);\nendmodule\n");
        assert!(generate(&exercise("m", spec(r#"{"kind": "combinational"}"#)), &clash).is_err());

        let too_wide = interface("module m(input [16:0] a, output y);\nendmodule\n");
        let error = generate(
            &exercise(
                "m",
                spec(r#"{"kind": "combinational", "stimulus": "exhaustive"}"#),
            ),
            &too_wide,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("too many"), "{error}");
    }
}
