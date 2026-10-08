//! A correção de um exercício: o `check` do projeto dele (o Icarus com
//! `-Wall`, para os erros e avisos com arquivo e linha), a simulação no
//! Icarus e o resumo que o testbench escreve
//! ([`testbench`](crate::testbench)).
//!
//! A correção não muda o estado da pasta de exercícios: quem chama marca o
//! exercício como resolvido ([`Workspace::set_solved`]) quando o veredito é
//! [`Verdict::Solved`]. Ela também não grava relatório em `.lace/reports`.

use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{
    CheckOptions, Control, Diagnostic, Severity, SimulationOptions, Simulator, Status, Step,
    Toolchain,
};
use schemars::JsonSchema;
use serde::Serialize;

use crate::error::Result;
use crate::layout;
use crate::testbench::{PROTOCOL, testbench_module};
use crate::track::Exercise;
use crate::workspace::{HIDDEN_DIR, Workspace};

/// Quanto a simulação pode durar sem `timeout_s` no `exercise.json`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Quantas vezes simular de novo quando o `vvp` não consegue abrir a onda.
/// No Windows, com várias simulações ao mesmo tempo, a onda gravada há
/// pouco às vezes ainda está presa por outro processo (o antivírus, pelo que
/// se viu: com a proteção em tempo real do Defender ligada, uma em 32
/// correções paralelas falhou assim, e nenhuma em 25 seguidas), e a
/// simulação para antes do resumo.
const WAVE_RETRIES: u32 = 3;
/// A espera antes de simular de novo, que cresce a cada tentativa.
const WAVE_RETRY_DELAY: Duration = Duration::from_millis(200);

/// Como terminou a correção.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Verdict {
    /// Compilou e nenhuma saída errou.
    Solved,
    /// O Verilog não compila: os erros estão em `diagnostics`.
    CompileError,
    /// Simulou, e alguma saída diferiu da referência.
    Mismatch,
    /// A simulação terminou sem o resumo do testbench, ou parou com erro.
    Incomplete,
    /// A simulação passou do prazo: um laço sem fim, ou um laço
    /// combinacional que não estabiliza.
    TimedOut,
    /// A correção foi cancelada.
    Cancelled,
}

/// O resultado de uma saída do módulo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct OutputCheck {
    /// O nome da porta.
    pub name: String,
    /// Em quantas amostras ela diferiu da referência.
    pub mismatches: u64,
    /// O instante do primeiro erro, em ns.
    pub first_ns: Option<u64>,
    /// Das amostras erradas, quantas tinham bit em X ou Z.
    pub unknown: u64,
}

/// O que a correção percebeu sobre a causa do erro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[non_exhaustive]
pub enum Finding {
    /// O erro de compilação é no testbench: o nome do módulo ou o de uma
    /// porta mudou.
    InterfaceChanged,
    /// Todas as amostras erradas desta saída estão em X ou Z: ela não é
    /// atribuída, ou depende de algo que não é.
    UndrivenOutput {
        /// A saída.
        output: String,
    },
    /// Só erra com o reset ativo: o reset não faz o que o enunciado pede
    /// (síncrono ou assíncrono, o valor depois dele).
    ResetOnly,
}

/// O resultado da correção.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Grade {
    /// O exercício.
    pub exercise: String,
    /// Como terminou.
    pub verdict: Verdict,
    /// Quantas amostras o testbench comparou.
    pub samples: u64,
    /// Em quantas amostras alguma saída errou.
    pub mismatched: u64,
    /// Cada saída, na ordem das portas.
    pub outputs: Vec<OutputCheck>,
    /// Com reset: quantas amostras erradas foram com ele ativo.
    pub reset_mismatches: Option<u64>,
    /// O que se percebeu sobre a causa do erro.
    pub findings: Vec<Finding>,
    /// Os erros e avisos dos compiladores sobre o código do aluno.
    pub diagnostics: Vec<Diagnostic>,
    /// O que o testbench escreveu, fora o resumo (um `tb.v` escrito à mão
    /// pode explicar o erro aqui).
    pub output: Vec<String>,
    /// A onda da simulação.
    #[schemars(with = "Option<String>")]
    pub waveform: Option<Utf8PathBuf>,
    /// O arquivo de comandos do Surfer que mostra a onda com as entradas, as
    /// saídas lado a lado com as da referência e o primeiro erro marcado.
    #[schemars(with = "Option<String>")]
    pub layout: Option<Utf8PathBuf>,
    /// Quanto a correção levou, em milissegundos.
    pub duration_ms: u64,
}

impl Grade {
    /// Resolvido.
    pub fn solved(&self) -> bool {
        self.verdict == Verdict::Solved
    }

    /// O primeiro erro, em ns, entre todas as saídas.
    pub fn first_mismatch_ns(&self) -> Option<u64> {
        self.outputs.iter().filter_map(|o| o.first_ns).min()
    }
}

/// Corrige o exercício na pasta de exercícios: verifica, simula e lê o
/// resumo.
///
/// # Erros
///
/// Os do lace-core quando nem dá para rodar (ferramenta que falta, projeto
/// que não abre). Código errado do aluno não é erro: é o [`Grade`].
pub fn grade(
    toolchain: &Toolchain,
    workspace: &Workspace,
    exercise: &Exercise,
    control: &Control,
) -> Result<Grade> {
    let started = Instant::now();
    let project = workspace.project(exercise)?;
    let dir = workspace.exercise_dir(exercise);
    let mut grade = Grade {
        exercise: exercise.name.clone(),
        verdict: Verdict::Incomplete,
        samples: 0,
        mismatched: 0,
        outputs: Vec::new(),
        reset_mismatches: None,
        findings: Vec::new(),
        diagnostics: Vec::new(),
        output: Vec::new(),
        waveform: None,
        layout: None,
        duration_ms: 0,
    };

    let check = lace_core::check(toolchain, &project, &CheckOptions::default(), control)?;
    let mut diagnostics = check.diagnostics.clone();
    if check.status != Status::Succeeded {
        grade.verdict = match check.status {
            Status::Cancelled => Verdict::Cancelled,
            _ => Verdict::CompileError,
        };
        grade.diagnostics = for_student(&diagnostics, &dir, true);
        grade.findings = compile_findings(&grade.diagnostics, &dir);
        grade.duration_ms = elapsed(started);
        return Ok(grade);
    }

    let mut options = SimulationOptions::new(Simulator::Icarus);
    options.timeout = Some(
        exercise
            .spec
            .timeout_s
            .map_or(DEFAULT_TIMEOUT, Duration::from_secs),
    );
    let wave = dir.join(crate::testbench::WAVE_FILE);
    let mut attempt = 0;
    let simulation = loop {
        // A onda da correção anterior sai antes: o vvp cria uma nova.
        let _ = std::fs::remove_file(&wave);
        let simulation = lace_core::simulate_project(toolchain, &project, &options, control)?;
        if attempt < WAVE_RETRIES && wave_was_locked(&simulation) {
            attempt += 1;
            tracing::debug!(attempt, %wave, "Waveform locked, simulating again");
            std::thread::sleep(WAVE_RETRY_DELAY * attempt);
            continue;
        }
        break simulation;
    };
    diagnostics.extend(simulation.diagnostics.iter().cloned());
    let failed_to_compile = simulation.failed_step == Some(Step::Elaborate);
    grade.diagnostics = for_student(&diagnostics, &dir, failed_to_compile);
    grade.waveform = simulation
        .waveform
        .as_ref()
        .map(|w| w.path.clone())
        .filter(|p| p.is_file());
    let stdout = simulation
        .steps
        .iter()
        .filter(|s| s.step == Step::Simulate)
        .map(|s| s.stdout.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let summary = parse_summary(&stdout);
    grade.output = summary.other;
    grade.samples = summary.samples;
    grade.mismatched = summary.mismatched;
    grade.outputs = summary.outputs;
    grade.reset_mismatches = summary.reset;

    grade.verdict = match simulation.status {
        Status::Cancelled => Verdict::Cancelled,
        Status::TimedOut => Verdict::TimedOut,
        _ if failed_to_compile => Verdict::CompileError,
        Status::Succeeded if summary.ended && summary.samples > 0 => {
            if grade.outputs.iter().any(|o| o.mismatches > 0) {
                Verdict::Mismatch
            } else {
                Verdict::Solved
            }
        }
        _ => Verdict::Incomplete,
    };
    if grade.verdict == Verdict::CompileError {
        grade.findings = compile_findings(&grade.diagnostics, &dir);
    }
    if grade.verdict == Verdict::Mismatch {
        for output in &grade.outputs {
            if output.mismatches > 0 && output.unknown == output.mismatches {
                grade.findings.push(Finding::UndrivenOutput {
                    output: output.name.clone(),
                });
            }
        }
        if grade
            .reset_mismatches
            .is_some_and(|r| r > 0 && r == grade.mismatched)
        {
            grade.findings.push(Finding::ResetOnly);
        }
    }

    if let Some(wave) = &grade.waveform {
        let text = layout_for(exercise, &dir, grade.first_mismatch_ns());
        let path = dir.join(layout::LAYOUT_FILE);
        if std::fs::write(&path, text).is_ok() {
            grade.layout = Some(path);
        }
        tracing::debug!(%wave, "Exercise waveform");
    }
    grade.duration_ms = elapsed(started);
    Ok(grade)
}

/// O layout da onda: o do testbench gerado, com as portas; com `tb.v`
/// escrito à mão, o escopo do testbench inteiro.
fn layout_for(exercise: &Exercise, dir: &Utf8Path, first_ns: Option<u64>) -> String {
    match exercise.interface() {
        Ok(interface) if !exercise.custom_testbench => {
            layout::commands(exercise, &interface, first_ns)
        }
        _ => {
            let testbench = dir
                .join(HIDDEN_DIR)
                .join(format!("{}.v", testbench_module(&exercise.module)));
            let top = std::fs::read_to_string(&testbench)
                .ok()
                .and_then(|text| lace_core::verilog::modules_in(&text).into_iter().next())
                .unwrap_or_else(|| testbench_module(&exercise.module));
            layout::scope_commands(&top, first_ns)
        }
    }
}

/// Os diagnósticos que importam ao aluno, sem repetição: os erros, e os
/// avisos sobre os arquivos dele. Ficam de fora as informações do
/// simulador, os avisos sobre o que o `lace learn` gera e o aviso de
/// `timescale` (o testbench declara um, e o arquivo do aluno não precisa).
/// Com `failed`, sem nenhum erro reconhecido, vale tudo o que não é
/// informação: o aluno precisa ver por que não compilou.
fn for_student(all: &[Diagnostic], dir: &Utf8Path, failed: bool) -> Vec<Diagnostic> {
    let hidden = dir.join(HIDDEN_DIR);
    let mut kept: Vec<Diagnostic> = Vec::new();
    for d in all {
        let ours = d.file.as_deref().is_some_and(|f| f.starts_with(&hidden));
        let keep = match d.severity {
            Severity::Error => true,
            Severity::Warning => !ours && !d.message.to_ascii_lowercase().contains("timescale"),
            _ => false,
        };
        if keep {
            push_new(&mut kept, d);
        }
    }
    if failed && !kept.iter().any(|d| d.severity == Severity::Error) {
        for d in all {
            if !matches!(d.severity, Severity::Info) {
                push_new(&mut kept, d);
            }
        }
    }
    kept
}

/// Acrescenta o diagnóstico se ainda não houver um igual (o `check` elabora
/// o design sozinho e com o testbench, e repete os avisos).
fn push_new(kept: &mut Vec<Diagnostic>, d: &Diagnostic) {
    let seen = kept
        .iter()
        .any(|k| k.file == d.file && k.line == d.line && k.message == d.message);
    if !seen {
        kept.push(d.clone());
    }
}

/// Um erro no testbench gerado quer dizer que ele não achou o módulo ou uma
/// porta do aluno.
fn compile_findings(diagnostics: &[Diagnostic], dir: &Utf8Path) -> Vec<Finding> {
    let hidden = dir.join(HIDDEN_DIR);
    let in_testbench = diagnostics.iter().any(|d| {
        d.severity == Severity::Error && d.file.as_deref().is_some_and(|f| f.starts_with(&hidden))
    });
    if in_testbench {
        vec![Finding::InterfaceChanged]
    } else {
        Vec::new()
    }
}

/// O resumo lido do stdout do testbench.
#[derive(Debug, Default, PartialEq, Eq)]
struct Summary {
    samples: u64,
    mismatched: u64,
    outputs: Vec<OutputCheck>,
    reset: Option<u64>,
    ended: bool,
    other: Vec<String>,
}

fn parse_summary(stdout: &str) -> Summary {
    let mut summary = Summary::default();
    for line in stdout.lines() {
        let line = line.trim_end_matches('\r');
        let Some(rest) = line.trim().strip_prefix(PROTOCOL) else {
            if !is_simulator_noise(line) && !line.trim().is_empty() {
                summary.other.push(line.to_owned());
            }
            continue;
        };
        let words: Vec<&str> = rest.split_whitespace().collect();
        let number = |i: usize| words.get(i).and_then(|w| w.parse::<i64>().ok());
        match words.first().copied() {
            Some("samples") => summary.samples = number(1).unwrap_or(0).max(0) as u64,
            Some("mismatched") => summary.mismatched = number(1).unwrap_or(0).max(0) as u64,
            Some("reset") => summary.reset = Some(number(1).unwrap_or(0).max(0) as u64),
            Some("end") => summary.ended = true,
            Some("output") if words.len() >= 5 => summary.outputs.push(OutputCheck {
                name: words[1].to_owned(),
                mismatches: number(2).unwrap_or(0).max(0) as u64,
                first_ns: number(3).filter(|t| *t >= 0).map(|t| t as u64),
                unknown: number(4).unwrap_or(0).max(0) as u64,
            }),
            _ => summary.other.push(line.to_owned()),
        }
    }
    summary
}

/// O `vvp` não conseguiu abrir a onda para gravar (`FST Error: ... Unable to
/// open ... for output`).
fn wave_was_locked(simulation: &lace_core::SimulationResult) -> bool {
    simulation
        .steps
        .iter()
        .filter(|s| s.step == Step::Simulate)
        .any(|s| {
            s.stdout
                .lines()
                .chain(s.stderr.lines())
                .any(|l| l.contains("Unable to open") && l.contains("for output"))
        })
}

/// As linhas que o `vvp` escreve sozinho: a onda aberta e o `$finish`.
fn is_simulator_noise(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("FST info:")
        || line.starts_with("VCD info:")
        || line.starts_with("LXT2 info:")
        || line.contains("$finish called at")
}

fn elapsed(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_is_read_and_the_rest_is_testbench_output() {
        let stdout = "FST info: dumpfile .lace-learn/wave.fst opened for output.\r\nolá do testbench\r\nLACE-LEARN samples 8\r\nLACE-LEARN mismatched 4\r\nLACE-LEARN output y 4 25 1\r\nLACE-LEARN output z 0 -1 0\r\nLACE-LEARN reset 2\r\nLACE-LEARN end\r\nC:/x/tb.v:45: $finish called at 80000 (1ps)\r\n";
        let summary = parse_summary(stdout);
        assert_eq!(summary.samples, 8);
        assert_eq!(summary.mismatched, 4);
        assert_eq!(summary.reset, Some(2));
        assert!(summary.ended);
        assert_eq!(summary.other, ["olá do testbench"]);
        assert_eq!(
            summary.outputs,
            [
                OutputCheck {
                    name: "y".into(),
                    mismatches: 4,
                    first_ns: Some(25),
                    unknown: 1
                },
                OutputCheck {
                    name: "z".into(),
                    mismatches: 0,
                    first_ns: None,
                    unknown: 0
                },
            ]
        );
    }

    #[test]
    fn a_summary_without_end_is_not_ended() {
        let summary = parse_summary("LACE-LEARN samples 3\n");
        assert!(!summary.ended);
        assert_eq!(summary.samples, 3);
    }
}
