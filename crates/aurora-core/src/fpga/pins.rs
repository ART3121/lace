//! A conferência do que o Quartus fez com os pinos, depois do Fitter: o
//! `<revisão>.pin` diz onde cada sinal ficou, em que direção e com que padrão
//! de I/O. O Lace compara com a placa ([`super::Board`]) antes de gerar o
//! arquivo de gravação.
//!
//! É a proteção da placa contra o que o `.qsf` não garante sozinho: uma
//! atribuição que o Quartus não entendeu e ignorou deixa o Fitter escolher o
//! pino, e uma saída num pino que a placa liga a uma chave, a um botão ou a
//! outro chip é um curto quando os dois lados divergem.
//!
//! O formato foi conferido no `.pin` do Quartus Prime 25.1 Lite:
//!
//! ```text
//! Pin Name/Usage               : Location  : Dir.   : I/O Standard      : Voltage : I/O Bank  : User Assignment
//! SW[15]                       : AA22      : input  : 2.5 V             :         : 5         : Y
//! ```

use std::collections::BTreeMap;

use super::board::SignalDirection;
use super::config::Resolved;

/// Um pino como a placa o define e como o `.pin` o mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pin {
    location: String,
    direction: &'static str,
    standard: String,
}

/// Os pinos que o topo da placa usa, por nome do bit (`SW[3]`, `CLOCK_50`).
fn expected(resolved: &Resolved) -> BTreeMap<String, Pin> {
    let board = &resolved.board;
    let used = resolved
        .board_inputs
        .iter()
        .map(String::as_str)
        .chain(resolved.board_outputs.iter().map(|d| d.signal.as_str()));
    let mut pins = BTreeMap::new();
    for name in used {
        let Some(signal) = board.signal(name) else {
            continue;
        };
        let direction = match signal.direction {
            SignalDirection::Input => "input",
            _ => "output",
        };
        for (bit, pin) in signal.pins.iter().enumerate() {
            let key = if signal.pins.len() > 1 {
                format!("{name}[{bit}]")
            } else {
                name.to_owned()
            };
            pins.insert(
                key,
                Pin {
                    location: pin.trim_start_matches("PIN_").to_owned(),
                    direction,
                    standard: signal.io_standard.of(bit).unwrap_or_default().to_owned(),
                },
            );
        }
    }
    pins
}

/// As linhas de sinal do `.pin`: nome, pino, direção, padrão e se a posição
/// veio de uma atribuição (`Y`) ou foi escolhida pelo Fitter.
fn placed(report: &str) -> BTreeMap<String, (Pin, bool)> {
    let mut rows = BTreeMap::new();
    let mut table = false;
    for line in report.lines() {
        let cells: Vec<&str> = line.split(':').map(str::trim).collect();
        if cells.first() == Some(&"Pin Name/Usage") {
            table = true;
            continue;
        }
        if !table || cells.len() < 7 {
            continue;
        }
        let direction = match cells[2] {
            "input" => "input",
            "output" => "output",
            "bidir" => "bidir",
            _ => continue,
        };
        rows.insert(
            cells[0].to_owned(),
            (
                Pin {
                    location: cells[1].to_owned(),
                    direction,
                    standard: cells[3].to_owned(),
                },
                cells[6] == "Y",
            ),
        );
    }
    rows
}

/// Os problemas do `.pin` contra a placa, um por linha; vazio, os pinos
/// estão como a placa pede. Cada sinal do topo da placa precisa estar no
/// pino do manual, na direção da placa, com o padrão de I/O dela e por
/// atribuição (não escolhido pelo Fitter).
pub(crate) fn check(report: &str, resolved: &Resolved) -> Vec<String> {
    let placed = placed(report);
    if placed.is_empty() {
        return vec![
            "The pin report of Quartus has no pins: the placement could not be checked".into(),
        ];
    }
    let mut problems = Vec::new();
    for (name, want) in expected(resolved) {
        let Some((got, assigned)) = placed.get(&name) else {
            problems.push(format!("{name} is not in the pin report of Quartus"));
            continue;
        };
        if got.location != want.location {
            problems.push(format!(
                "{name} went to pin {}, but the board has it on {}",
                got.location, want.location
            ));
        } else if !assigned {
            problems.push(format!(
                "{name} is on pin {}, but chosen by the Fitter, not by the assignment",
                got.location
            ));
        }
        if got.direction != want.direction {
            problems.push(format!(
                "{name} is an {} of the FPGA, but an {} on the board",
                got.direction, want.direction
            ));
        }
        if got.standard != want.standard {
            problems.push(format!(
                "{name} uses the I/O standard {}, but the board needs {}",
                got.standard, want.standard
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::super::config::tests::{sapho_top, test_board};
    use super::super::config::{FpgaConfig, Link};
    use super::*;

    fn resolved() -> Resolved {
        let mut config = FpgaConfig::new("teste");
        config.connect = [
            ("clk", "CLOCK_50"),
            ("rst", "!KEY[0]"),
            ("out[1:0]", "LED[1:0]"),
        ]
        .iter()
        .map(|(p, s)| Link::new(*p, *s))
        .collect();
        config.resolve(&test_board(), &sapho_top()).unwrap()
    }

    /// Um `.pin` no formato do Quartus 25.1, com os pinos da placa de teste.
    fn report(rows: &[(&str, &str, &str, &str, &str)]) -> String {
        let mut text = String::from(
            "CHIP  \"lace_board_top\"  ASSIGNED TO AN: EP4CE115F29C7\n\n\
             Pin Name/Usage               : Location  : Dir.   : I/O Standard      : Voltage : I/O Bank  : User Assignment\n\
             -------------------------------------------------------------------------------------------------------------\n\
             GND                          : A1        : gnd    :                   :         :           :\n\
             RESERVED_INPUT_WITH_WEAK_PULLUP : A3     : input  : 2.5 V             :         : 8         : N\n",
        );
        for (name, location, direction, standard, user) in rows {
            text.push_str(&format!(
                "{name:<28} : {location:<9} : {direction:<6} : {standard:<17} :         : 5         : {user}\n"
            ));
        }
        text
    }

    fn good() -> Vec<(
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
    )> {
        let lvttl = "3.3-V LVTTL";
        let mut rows = vec![
            ("CLOCK_50", "A1", "input", lvttl, "Y"),
            ("KEY[0]", "B1", "input", lvttl, "Y"),
            ("KEY[1]", "B2", "input", lvttl, "Y"),
        ];
        for (bit, pin) in ["D1", "D2", "D3", "D4", "D5", "D6"].iter().enumerate() {
            rows.push((
                ["LED[0]", "LED[1]", "LED[2]", "LED[3]", "LED[4]", "LED[5]"][bit],
                pin,
                "output",
                lvttl,
                "Y",
            ));
        }
        rows
    }

    #[test]
    fn the_pins_as_the_board_defines_them_pass() {
        assert_eq!(check(&report(&good()), &resolved()), Vec::<String>::new());
    }

    #[test]
    fn a_moved_pin_a_wrong_direction_and_a_fitter_choice_are_refused() {
        let mut rows = good();
        rows[1].1 = "C9"; // KEY[0] noutro pino
        rows[2].4 = "N"; // KEY[1] escolhido pelo Fitter
        rows[3].2 = "input"; // LED[0] como entrada
        rows[4].3 = "2.5 V"; // LED[1] com outro padrão
        rows.pop(); // LED[5] some
        let problems = check(&report(&rows), &resolved());
        assert_eq!(problems.len(), 5, "{problems:#?}");
        assert!(problems[0].contains("KEY[0] went to pin C9, but the board has it on B1"));
        assert!(
            problems
                .iter()
                .any(|p| p.contains("KEY[1]") && p.contains("chosen by the Fitter"))
        );
        assert!(problems.iter().any(|p| p.contains("LED[0] is an input")));
        assert!(
            problems
                .iter()
                .any(|p| p.contains("LED[1] uses the I/O standard 2.5 V"))
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("LED[5] is not in the pin report"))
        );
        assert!(!check("", &resolved()).is_empty());
    }
}
