//! O `main` em C++ da placa virtual: instancia o modelo do topo da placa
//! ([`crate::fpga::wrapper`]), liga cada sinal dele a um periférico do
//! runtime (`runtime/aurora_board.hpp`) e chama o laço de simulação.
//!
//! As portas do topo da placa são os sinais da placa usados pelas ligações,
//! com a largura do sinal; o Verilator as expõe como membros do modelo com o
//! mesmo nome.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::fpga::wrapper::TOP_MODULE;
use crate::fpga::{Peripheral, Resolved};

/// O meio período do relógio do teclado PS/2, em kHz: 12,5 kHz fica no meio
/// da faixa da especificação (10 a 16,7 kHz).
const PS2_KHZ: f64 = 12.5;

/// O `board_main.cpp` do topo da placa conferido.
pub(super) fn generate(resolved: &Resolved) -> String {
    let board = &resolved.board;
    // Os sinais que são portas do topo da placa, com a largura.
    let input = |name: &str| resolved.board_inputs.iter().any(|s| s == name);
    let output = |name: &str| resolved.board_outputs.iter().any(|d| d.signal == name);
    let pin = |name: &str| -> String {
        let width = board.signal(name).map_or(1, |s| s.width());
        format!("Pin(\"{name}\", {width}, &top.{name})")
    };
    let active_low = |name: &str| board.signal(name).is_some_and(|s| s.active_low);
    let bool_cpp = |b: bool| if b { "true" } else { "false" };

    let mut body = String::new();
    let mut covered = BTreeSet::new();
    // Os relógios, um por sinal da placa.
    let mut clocks = BTreeSet::new();
    for clock in &resolved.clocks {
        if clocks.insert(clock.signal.as_str()) && input(&clock.signal) {
            let _ = writeln!(
                body,
                "    board.clock({}, {:.1});",
                pin(&clock.signal),
                clock.mhz * 1e6
            );
            covered.insert(clock.signal.clone());
        }
    }

    for peripheral in &board.peripherals {
        let id = peripheral
            .signals()
            .first()
            .map(|(name, _, _)| (*name).to_owned())
            .unwrap_or_default();
        match peripheral {
            Peripheral::Switches { signal } | Peripheral::Buttons { signal } if input(signal) => {
                let buttons = matches!(peripheral, Peripheral::Buttons { .. });
                let _ = writeln!(
                    body,
                    "    board.add(new Inputs(\"{id}\", {}, {}, {}));",
                    pin(signal),
                    bool_cpp(active_low(signal)),
                    bool_cpp(buttons)
                );
                covered.insert(signal.clone());
            }
            Peripheral::Leds { signal, .. } if output(signal) => {
                let _ = writeln!(
                    body,
                    "    board.add(new Lights(\"{signal}\", {}, {}, \"leds\"));",
                    pin(signal),
                    bool_cpp(active_low(signal))
                );
            }
            Peripheral::SevenSegment { digits } => {
                for digit in digits.iter().filter(|d| output(d)) {
                    let _ = writeln!(
                        body,
                        "    board.add(new Lights(\"{digit}\", {}, {}, \"seven_segment\"));",
                        pin(digit),
                        bool_cpp(active_low(digit))
                    );
                }
            }
            Peripheral::Vga {
                red,
                green,
                blue,
                hsync,
                vsync,
                blank,
                clock,
            } if [red, green, blue, hsync, vsync]
                .into_iter()
                .all(|s| output(s)) =>
            {
                let optional = |s: &Option<String>| match s {
                    Some(name) if output(name) => pin(name),
                    _ => "Pin()".to_owned(),
                };
                let _ = writeln!(
                    body,
                    "    board.add(new Vga(\"{id}\", {}, {}, {}, {}, {}, {}, {}, board.main_clock()));",
                    pin(red),
                    pin(green),
                    pin(blue),
                    pin(hsync),
                    pin(vsync),
                    optional(blank),
                    optional(clock)
                );
            }
            Peripheral::Ps2 { clock, data } if input(clock) && input(data) => {
                let _ = writeln!(
                    body,
                    "    board.add(new Ps2Keyboard(\"{id}\", {}, {}, {PS2_KHZ:.1}));",
                    pin(clock),
                    pin(data)
                );
                covered.insert(clock.clone());
                covered.insert(data.clone());
            }
            Peripheral::Uart { tx, rx, baud } if output(tx) => {
                let rx_pin = match rx {
                    Some(name) if input(name) => {
                        covered.insert(name.clone());
                        pin(name)
                    }
                    _ => "Pin()".to_owned(),
                };
                let _ = writeln!(
                    body,
                    "    board.add(new Uart(\"{id}\", {}, {rx_pin}, {baud}.0));",
                    pin(tx)
                );
            }
            _ => {}
        }
    }

    // As entradas usadas que nenhum periférico dirige ficam no nível
    // inativo: 0, ou 1 nos sinais ativos em baixo (um botão de reset solto
    // não segura o projeto em reset).
    let mut idle = String::new();
    for name in &resolved.board_inputs {
        if covered.contains(name) {
            continue;
        }
        let value = if active_low(name) { "~0ULL" } else { "0" };
        let _ = writeln!(idle, "    {}.set({value});", pin(name));
    }

    format!(
        "// Generated by AURORA for the virtual board of {board_name}: the board\n\
         // top ({TOP_MODULE}) with each board signal on a simulated peripheral.\n\
         // AURORA writes it again on every build: do not edit.\n\
         #include \"V{TOP_MODULE}.h\"\n\
         #include \"verilated.h\"\n\
         #include \"aurora_board.hpp\"\n\
         \n\
         double sc_time_stamp() {{ return 0; }}\n\
         \n\
         int main(int argc, char** argv) {{\n\
         \x20   using namespace aurora_board;\n\
         \x20   VerilatedContext context;\n\
         \x20   context.commandArgs(argc, argv);\n\
         \x20   V{TOP_MODULE} top{{&context, \"top\"}};\n\
         \x20   Board board;\n\
         {body}{idle}\
         \x20   int code = board.run([&] {{ top.eval(); }}, [&] {{ return context.gotFinish(); }});\n\
         \x20   top.final();\n\
         \x20   return code;\n\
         }}\n",
        board_name = board.name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpga::FpgaConfig;
    use crate::verilog::{ModuleInterface, Port, PortDirection};

    fn port(name: &str, direction: PortDirection, width: u32) -> Port {
        Port {
            name: name.into(),
            direction,
            width,
            signed: false,
        }
    }

    #[test]
    fn each_used_signal_goes_to_its_peripheral() {
        let board = crate::fpga::board("de2-115").unwrap();
        let top = ModuleInterface {
            name: "placa".into(),
            file: "placa.v".into(),
            ports: vec![
                port("CLOCK_50", PortDirection::Input, 1),
                port("SW", PortDirection::Input, 10),
                port("KEY", PortDirection::Input, 2),
                port("LEDR", PortDirection::Output, 10),
                port("HEX0", PortDirection::Output, 7),
                port("outro", PortDirection::Output, 1),
            ],
        };
        let resolved = FpgaConfig::by_name(&board, &top)
            .resolve(&board, &top)
            .unwrap();
        let main = generate(&resolved);
        assert!(
            main.contains("board.clock(Pin(\"CLOCK_50\", 1, &top.CLOCK_50), 50000000.0);"),
            "{main}"
        );
        assert!(
            main.contains("new Inputs(\"SW\", Pin(\"SW\", 18, &top.SW), false, false)"),
            "{main}"
        );
        assert!(
            main.contains("new Inputs(\"KEY\", Pin(\"KEY\", 4, &top.KEY), true, true)"),
            "{main}"
        );
        assert!(
            main.contains("new Lights(\"LEDR\", Pin(\"LEDR\", 18, &top.LEDR), false, \"leds\")"),
            "{main}"
        );
        assert!(
            main.contains(
                "new Lights(\"HEX0\", Pin(\"HEX0\", 7, &top.HEX0), true, \"seven_segment\")"
            ),
            "{main}"
        );
        // O que o projeto não usa não vira porta, nem periférico.
        assert!(!main.contains("LEDG"), "{main}");
        assert!(!main.contains("HEX1"), "{main}");
    }
}
