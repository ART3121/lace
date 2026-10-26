//! Os fluxos do Studio: o pedido da interface vira um pedido ao servidor do
//! Lace ([`crate::server`]), um fluxo do core (`flow.run`) ou a correção de
//! um exercício do lace-learn (`learn.check`), e o resultado volta com o
//! que só o Studio acrescenta: a onda numa aba e a correção.
//!
//! A composição em si (compilar antes, parar no primeiro build que falhar,
//! registrar o Verilog gerado, simular, ler as portas, gravar o relatório,
//! abrir a onda) é a do core, a mesma da CLI, e roda no servidor. Aqui
//! ficam só as decisões da interface: o texto `lace-studio ...` do
//! histórico, e se a onda abre em janela ou numa aba (preferência
//! `wave_viewer`).

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::delivery::trim_step_output;
use lace_core::flows::{self as core, FlowOptions, WavePolicy};
use lace_core::{Simulator, Status};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{IpcResult, codes};
use crate::jobs::{JobMessage, Operation};
use crate::settings::Settings;
use crate::toolchain;

/// Quanto do `stdout` e do `stderr` de cada passo vai no resultado final. A
/// interface já recebeu cada linha pelos eventos; o resultado leva o fim,
/// para o resumo, sem mandar megabytes de uma vez.
const MAX_STEP_OUTPUT: usize = 256 * 1024;

/// O que a interface pede: os fluxos do core, no formato deles, mais a
/// correção de um exercício.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "flow", rename_all = "snake_case")]
pub enum FlowRequest {
    /// `lace build [-p NOME]...`.
    Build {
        /// Os processadores; vazio é todos.
        #[serde(default)]
        processors: Vec<String>,
    },
    /// `lace check [ARQUIVO] [-p NOME] [--lint]`.
    Check {
        /// Só os módulos deste arquivo.
        #[serde(default)]
        file: Option<Utf8PathBuf>,
        /// Só este processador.
        #[serde(default)]
        processor: Option<String>,
        /// Acrescenta o `verilator --lint-only`.
        #[serde(default)]
        lint: bool,
    },
    /// `lace hierarchy [-p NOME]`.
    Hierarchy {
        /// Só este processador.
        #[serde(default)]
        processor: Option<String>,
    },
    /// `lace sim [TESTBENCH] [-p NOME] [--verilator] [--fast] [--timeout S] [--open]`.
    Simulate {
        /// Simula este processador; `None`, a simulação do projeto.
        #[serde(default)]
        processor: Option<String>,
        /// Troca o testbench da simulação do projeto antes de simular.
        #[serde(default)]
        testbench: Option<Utf8PathBuf>,
        /// Icarus ou Verilator.
        simulator: Simulator,
        /// A simulação rápida: sem onda, com o testbench Verilog no Verilator.
        #[serde(default)]
        fast: bool,
        /// Prazo do passo `simulate`, em segundos.
        #[serde(default)]
        timeout_s: Option<u64>,
        /// Abrir a onda se a simulação der certo: em janela ou, com a
        /// preferência `wave_viewer` em `tab`, numa aba (`wave_tab`).
        #[serde(default)]
        open_wave: bool,
    },
    /// `lace synth [-p NOME] [--svg] [--module M]`.
    Synthesize {
        /// Sintetiza só este processador; `None`, o topo do projeto.
        #[serde(default)]
        processor: Option<String>,
        /// Desenha o esquemático depois.
        #[serde(default)]
        schematic: bool,
        /// O módulo do esquemático; `None`, o topo da síntese.
        #[serde(default)]
        module: Option<String>,
        /// Grava o HTML do RTL viewer ao lado do netlist (o Studio desenha o
        /// próprio esquemático, então a interface não pede isso).
        #[serde(default)]
        open_viewer: bool,
    },
    /// Desenha outro módulo de um netlist que já existe. Sem relatório.
    Schematic {
        /// O `hierarchy.json` da síntese.
        netlist: Utf8PathBuf,
        /// O módulo.
        module: String,
        /// Escrever a largura dos barramentos nas arestas.
        #[serde(default = "yes")]
        bus_widths: bool,
    },
    /// `lace fpga build`.
    FpgaBuild,
    /// `lace fpga program [--cable NOME]`.
    FpgaProgram {
        /// O cabo; `None`, o primeiro que o Quartus lista.
        #[serde(default)]
        cable: Option<String>,
    },
    /// `lace learn check NOME`: corrige um exercício da pasta de exercícios e
    /// grava se ele está resolvido. Não grava relatório.
    Learn {
        /// A pasta de exercícios (ou uma pasta dentro dela).
        root: Utf8PathBuf,
        /// O exercício.
        exercise: String,
    },
}

fn yes() -> bool {
    true
}

impl FlowRequest {
    /// O nome curto do fluxo, como vai no resultado.
    pub fn name(&self) -> &'static str {
        match self.core() {
            Some(request) => request.name(),
            None => "learn",
        }
    }

    /// O comando equivalente da CLI, para o relatório e para o console. O
    /// prefixo é `lace-studio` porque quem rodou foi o Studio, não a CLI.
    pub fn command_line(&self) -> String {
        match self {
            FlowRequest::Learn { exercise, .. } => {
                format!("lace-studio learn check {}", quoted(exercise))
            }
            other => other
                .core()
                .expect("todo pedido que não é learn é do core")
                .command_line("lace-studio"),
        }
    }

    /// O pedido do core, para tudo que não é a correção de um exercício.
    fn core(&self) -> Option<core::FlowRequest> {
        Some(match self {
            FlowRequest::Build { processors } => core::FlowRequest::Build {
                processors: processors.clone(),
            },
            FlowRequest::Check {
                file,
                processor,
                lint,
            } => core::FlowRequest::Check {
                file: file.clone(),
                processor: processor.clone(),
                lint: *lint,
            },
            FlowRequest::Hierarchy { processor } => core::FlowRequest::Hierarchy {
                processor: processor.clone(),
            },
            FlowRequest::Simulate {
                processor,
                testbench,
                simulator,
                fast,
                timeout_s,
                open_wave,
            } => core::FlowRequest::Simulate {
                processor: processor.clone(),
                testbench: testbench.clone(),
                simulator: *simulator,
                fast: *fast,
                timeout_s: *timeout_s,
                open_wave: *open_wave,
            },
            FlowRequest::Synthesize {
                processor,
                schematic,
                module,
                open_viewer,
            } => core::FlowRequest::Synthesize {
                processor: processor.clone(),
                schematic: *schematic,
                module: module.clone(),
                open_viewer: *open_viewer,
            },
            FlowRequest::Schematic {
                netlist,
                module,
                bus_widths,
            } => core::FlowRequest::Schematic {
                netlist: netlist.clone(),
                module: module.clone(),
                bus_widths: *bus_widths,
            },
            FlowRequest::FpgaBuild => core::FlowRequest::FpgaBuild,
            FlowRequest::FpgaProgram { cable } => core::FlowRequest::FpgaProgram {
                cable: cable.clone(),
            },
            FlowRequest::Learn { .. } => return None,
        })
    }
}

fn quoted(text: &str) -> String {
    if text.is_empty() || text.contains(char::is_whitespace) {
        format!("\"{text}\"")
    } else {
        text.to_owned()
    }
}

/// O que fazer com a resposta do servidor no fim de um fluxo.
pub struct Finisher {
    name: &'static str,
    command: String,
    learn: bool,
    wave_tab: bool,
}

impl FlowRequest {
    /// O pedido ao servidor para este fluxo, com as decisões da interface,
    /// e o que fazer com a resposta.
    pub fn plan(&self, settings: &Settings, spf: &Utf8Path) -> (Operation, Finisher) {
        let command = self.command_line();
        let name = self.name();
        // O exercício é um projeto Lace, mas a correção é a do lace-learn:
        // não depende do projeto aberto.
        if let FlowRequest::Learn { root, exercise } = self {
            let operation = Operation {
                command: command.clone(),
                method: "learn.check",
                params: json!({ "root": root, "exercise": exercise }),
            };
            let finisher = Finisher {
                name,
                command,
                learn: true,
                wave_tab: false,
            };
            return (operation, finisher);
        }
        let request = self.core().expect("não é learn");
        // A onda numa aba é do Studio: o core não a abre, e a interface
        // recebe o caminho em `wave_tab`.
        let tab = settings.wave_viewer == "tab"
            && toolchain::require(settings).is_ok_and(|t| t.surfer_web_dir().is_ok());
        let mut options = FlowOptions::new(command.clone());
        options.wave = if tab {
            WavePolicy::None
        } else {
            WavePolicy::Window
        };
        let wave_tab = tab
            && matches!(
                request,
                core::FlowRequest::Simulate {
                    open_wave: true,
                    ..
                }
            );
        let operation = Operation {
            command: command.clone(),
            method: "flow.run",
            params: json!({
                "project": spf,
                "request": request,
                "options": options,
                "output": true,
            }),
        };
        let finisher = Finisher {
            name,
            command,
            learn: false,
            wave_tab,
        };
        (operation, finisher)
    }
}

impl Finisher {
    /// O fluxo é a correção de um exercício.
    pub fn is_learn(&self) -> bool {
        self.learn
    }

    /// A mensagem final da interface: o `FlowOutcome` do core, com
    /// `wave_tab` (a onda que a interface abre numa aba, no lugar da janela)
    /// e `learn` (a correção do exercício). `failed` é o Lace não conseguir
    /// rodar; um fluxo que rodou e falhou é `finished` com `succeeded:
    /// false`, e um cancelado antes de começar também.
    pub fn finish(self, reply: IpcResult<Value>) -> JobMessage {
        let (mut outcome, learn) = match reply {
            Ok(report) if self.learn => {
                let grade = report["results"].get(0).cloned().unwrap_or(Value::Null);
                let mut outcome = core::FlowOutcome::new(self.name, &self.command);
                match grade.get("verdict").and_then(Value::as_str) {
                    None | Some("solved") => {}
                    Some("cancelled") => outcome.worsen(Status::Cancelled),
                    Some(_) => outcome.worsen(Status::Failed),
                }
                (json!(outcome), grade)
            }
            Ok(outcome) => (outcome, Value::Null),
            Err(error) if error.code == codes::CANCELLED => {
                let mut outcome = core::FlowOutcome::new(self.name, &self.command);
                outcome.worsen(Status::Cancelled);
                (json!(outcome), Value::Null)
            }
            Err(error) => return JobMessage::Failed { error },
        };
        trim_step_output(&mut outcome, MAX_STEP_OUTPUT);
        let wave_tab = if self.wave_tab {
            outcome
                .pointer("/simulation/waveform/path")
                .cloned()
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        if let Some(fields) = outcome.as_object_mut() {
            fields.insert("wave_tab".to_owned(), wave_tab);
            fields.insert("learn".to_owned(), learn);
        }
        JobMessage::Finished { outcome }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finisher(learn: bool, wave_tab: bool) -> Finisher {
        Finisher {
            name: if learn { "learn" } else { "simulate" },
            command: "lace-studio sim".to_owned(),
            learn,
            wave_tab,
        }
    }

    #[test]
    fn the_outcome_gets_the_wave_tab_and_the_grade() {
        let outcome = json!({
            "flow": "simulate", "status": "succeeded", "succeeded": true,
            "simulation": { "waveform": { "path": "/p/tb.fst" } },
        });
        let JobMessage::Finished { outcome } = finisher(false, true).finish(Ok(outcome)) else {
            panic!("finished");
        };
        assert_eq!(outcome["wave_tab"], "/p/tb.fst");
        assert!(outcome["learn"].is_null());

        let report = json!({ "results": [{ "exercise": "e1", "verdict": "mismatch" }] });
        let JobMessage::Finished { outcome } = finisher(true, false).finish(Ok(report)) else {
            panic!("finished");
        };
        assert_eq!(outcome["flow"], "learn");
        assert_eq!(outcome["status"], "failed");
        assert_eq!(outcome["learn"]["exercise"], "e1");
    }

    #[test]
    fn a_flow_cancelled_before_it_started_finishes_cancelled() {
        let reply = Err(crate::error::IpcError::new(codes::CANCELLED, "Cancelled"));
        let JobMessage::Finished { outcome } = finisher(false, false).finish(reply) else {
            panic!("finished");
        };
        assert_eq!(outcome["status"], "cancelled");
        let reply = Err(crate::error::IpcError::new(
            "toolchain_missing",
            "No bundle",
        ));
        assert!(matches!(
            finisher(false, false).finish(reply),
            JobMessage::Failed { .. }
        ));
    }
}
