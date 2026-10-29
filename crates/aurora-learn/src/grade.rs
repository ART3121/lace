//! A correção de um exercício: o `check` do projeto dele (o Icarus com
//! `-Wall`, para os erros e avisos com arquivo e linha), a simulação no
//! Icarus e o resumo que o testbench escreve
//! ([`testbench`](crate::testbench)).
//!
//! A correção não muda o estado da pasta de exercícios: quem chama marca o
//! exercício como resolvido ([`Workspace::set_solved`]) quando o veredito é
//! [`Verdict::Solved`]. Ela também não grava relatório em `.aurora8/reports`.

use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use aurora_core::{
    CheckOptions, Control, DesignTarget, Diagnostic, Severity, SimulationOptions, Simulator,
    Status, Step, Tool, Toolchain,
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
    /// A síntese achou um latch: o sinal guarda o valor quando nenhum ramo
    /// do `always` o atribui. Num circuito combinacional, faltou um `else`,
    /// um `default` ou um valor antes do `if`. Pode aparecer com o exercício
    /// resolvido: a simulação passa, e o circuito não é o que parece.
    LatchInferred {
        /// O sinal (`modulo.sinal` quando é de um submódulo).
        signal: String,
        /// A linha do `always` que gerou o latch, no arquivo do aluno.
        line: Option<u32>,
    },
}

/// A primeira amostra errada: o que entrou e o que saiu, para o aluno
/// refazer o caso de cabeça. Vem das linhas `first` do resumo; um `tb.v`
/// escrito à mão que não as escreve fica sem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Counterexample {
    /// O instante da amostra, em ns.
    pub time_ns: u64,
    /// No sequencial, o ciclo (a partir de 0).
    pub cycle: Option<u64>,
    /// As entradas nessa amostra, sem o clock.
    pub inputs: Vec<SignalValue>,
    /// Cada saída nessa amostra: a esperada e a do aluno.
    pub outputs: Vec<OutputValue>,
}

/// O valor de uma entrada no contraexemplo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct SignalValue {
    /// A porta.
    pub name: String,
    /// Os bits, do mais significativo ao menos, com `x` e `z`.
    pub bits: String,
    /// O valor para ler: `1`, `4'b0101 (5)`, `16'h00ff (255)`.
    pub text: String,
}

/// Uma saída no contraexemplo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct OutputValue {
    /// A porta.
    pub name: String,
    /// Os bits que a referência deu.
    pub expected_bits: String,
    /// Os bits que o módulo do aluno deu.
    pub actual_bits: String,
    /// O esperado, para ler.
    pub expected: String,
    /// O do aluno, para ler.
    pub actual: String,
    /// Se esta saída errou nessa amostra.
    pub wrong: bool,
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
    /// A primeira amostra errada, com as entradas e as saídas.
    pub counterexample: Option<Counterexample>,
    /// O que se percebeu sobre a causa do erro, e o latch que a síntese
    /// achou (este, também com o exercício resolvido).
    pub findings: Vec<Finding>,
    /// Os erros e avisos dos compiladores sobre o código do aluno, com os
    /// avisos do lint do Verilator quando ele está instalado.
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
/// Os do aurora-core quando nem dá para rodar (ferramenta que falta, projeto
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
        counterexample: None,
        findings: Vec::new(),
        diagnostics: Vec::new(),
        output: Vec::new(),
        waveform: None,
        layout: None,
        duration_ms: 0,
    };

    let check = aurora_core::check(toolchain, &project, &CheckOptions::default(), control)?;
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
        let simulation = aurora_core::simulate_project(toolchain, &project, &options, control)?;
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
        grade.counterexample = summary
            .first
            .as_ref()
            .map(|first| counterexample(first, exercise));
    }

    // Os avisos além do veredito, num código que compila e simula.
    let simulated = matches!(grade.verdict, Verdict::Solved | Verdict::Mismatch);
    if simulated && exercise.spec.advice != Some(false) {
        let file = workspace.student_file(exercise);
        let latches = latches(toolchain, &project, &file, control);
        for d in lint(toolchain, &project, &file, control) {
            // O latch que o Yosys achou já vem explicado nas constatações;
            // o aviso do Verilator sobre ele seria a mesma coisa duas vezes.
            if !latches.is_empty() && d.message.ends_with("[LATCH]") {
                continue;
            }
            push_new(&mut grade.diagnostics, &d);
        }
        grade.findings.extend(latches);
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
                .and_then(|text| aurora_core::verilog::modules_in(&text).into_iter().next())
                .unwrap_or_else(|| testbench_module(&exercise.module));
            layout::scope_commands(&top, first_ns)
        }
    }
}

/// Os diagnósticos que importam ao aluno, sem repetição: os erros, e os
/// avisos sobre os arquivos dele. Ficam de fora as informações do
/// simulador, os avisos sobre o que o `aurora learn` gera e o aviso de
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
    first: Option<First>,
    ended: bool,
    other: Vec<String>,
}

/// As linhas `first` do resumo, como vieram: os valores em binário.
#[derive(Debug, Default, PartialEq, Eq)]
struct First {
    time_ns: u64,
    cycle: Option<u64>,
    /// `(entrada, bits)`.
    inputs: Vec<(String, String)>,
    /// `(saída, bits esperados, bits do aluno)`.
    outputs: Vec<(String, String, String)>,
}

/// O contraexemplo para o aluno: os bits lidos e o texto de cada valor,
/// com o decimal com sinal nas portas `signed` da solução.
fn counterexample(first: &First, exercise: &Exercise) -> Counterexample {
    let signed: Vec<String> = exercise
        .interface()
        .map(|i| {
            i.ports
                .into_iter()
                .filter(|p| p.signed)
                .map(|p| p.name)
                .collect()
        })
        .unwrap_or_default();
    let is_signed = |name: &str| signed.iter().any(|s| s == name);
    Counterexample {
        time_ns: first.time_ns,
        cycle: first.cycle,
        inputs: first
            .inputs
            .iter()
            .map(|(name, bits)| SignalValue {
                name: name.clone(),
                bits: bits.clone(),
                text: value_text(bits, is_signed(name)),
            })
            .collect(),
        outputs: first
            .outputs
            .iter()
            .map(|(name, expected, actual)| OutputValue {
                name: name.clone(),
                expected_bits: expected.clone(),
                actual_bits: actual.clone(),
                expected: value_text(expected, is_signed(name)),
                actual: value_text(actual, is_signed(name)),
                wrong: differs(expected, actual),
            })
            .collect(),
    }
}

/// Um valor em binário para ler: um bit como está; até 8 bits, o binário e
/// o decimal (`4'b0101 (5)`); acima, o hexadecimal e o decimal
/// (`16'h00ff (255)`). Com `x` ou `z`, só o binário. `signed` dá o decimal
/// com sinal.
pub fn value_text(bits: &str, signed: bool) -> String {
    let bits = bits.trim().to_ascii_lowercase();
    let width = bits.len();
    if width <= 1 {
        return bits;
    }
    let known = bits.chars().all(|c| c == '0' || c == '1');
    if !known || width > 128 {
        return format!("{width}'b{bits}");
    }
    let value = u128::from_str_radix(&bits, 2).unwrap_or(0);
    let decimal = if signed && bits.starts_with('1') {
        // O complemento de dois em `width` bits.
        let magnitude = if width == 128 {
            value.wrapping_neg()
        } else {
            (1u128 << width) - value
        };
        format!("-{magnitude}")
    } else {
        value.to_string()
    };
    if width <= 8 {
        format!("{width}'b{bits} ({decimal})")
    } else {
        let digits = width.div_ceil(4);
        format!("{width}'h{value:0digits$x} ({decimal})")
    }
}

/// A saída do aluno difere da esperada, pela regra do testbench: um bit em
/// X na referência aceita qualquer valor, e um bit do aluno em X ou Z com a
/// referência em 0 ou 1 é erro.
fn differs(expected: &str, actual: &str) -> bool {
    let expected = expected.to_ascii_lowercase();
    let actual = actual.to_ascii_lowercase();
    if expected.len() != actual.len() {
        return expected != actual;
    }
    expected
        .chars()
        .zip(actual.chars())
        .any(|(e, a)| e != 'x' && e != a)
}

/// Os avisos do lint do Verilator sobre o arquivo do aluno, sem os de
/// estilo que não ensinam nada a quem começa. O lint não muda o veredito:
/// sem o Verilator, ou se ele falha, fica sem aviso.
fn lint(
    toolchain: &Toolchain,
    project: &aurora_core::Project,
    file: &Utf8Path,
    control: &Control,
) -> Vec<Diagnostic> {
    /// Os códigos que ficam de fora: o nome do arquivo e o do módulo já
    /// batem no exercício, o fim de linha no fim do arquivo é estilo, uma
    /// saída de instância deixada sem ligação (`.cout()`) é de propósito, e
    /// o Verilog estende com zeros um valor mais estreito (`acc + x`, com
    /// `x` de 8 bits e `acc` de 12) sem que isso seja erro. O que corta
    /// bits (`WIDTHTRUNC`) fica.
    const QUIET: [&str; 5] = [
        "[DECLFILENAME]",
        "[EOFNEWLINE]",
        "[UNUSEDPARAM]",
        "[PINCONNECTEMPTY]",
        "[WIDTHEXPAND]",
    ];
    // Bits que o aluno descarta de propósito (`soma[9:2]`, a divisão por
    // 4): o aviso de parte do sinal sem uso fica de fora; o do sinal
    // inteiro sem uso (uma entrada esquecida) fica.
    let partly_unused = |d: &Diagnostic| {
        d.message.starts_with("Bits of signal are not used")
            && d.message.ends_with("[UNUSEDSIGNAL]")
    };
    if toolchain.tool(Tool::Verilator).is_err() {
        return Vec::new();
    }
    let mut options = CheckOptions::default();
    options.lint = true;
    let Ok(result) = aurora_core::check(toolchain, project, &options, control) else {
        return Vec::new();
    };
    result
        .diagnostics
        .into_iter()
        .filter(|d| d.tool == Tool::Verilator && d.severity == Severity::Warning)
        .filter(|d| d.file.as_deref().is_some_and(|f| same_file(f, file)))
        .filter(|d| !QUIET.iter().any(|q| d.message.ends_with(q)) && !partly_unused(d))
        .collect()
}

/// Os latches que a síntese do Yosys achou no design do aluno. Sem o Yosys,
/// ou se a síntese não termina, fica sem.
fn latches(
    toolchain: &Toolchain,
    project: &aurora_core::Project,
    file: &Utf8Path,
    control: &Control,
) -> Vec<Finding> {
    if toolchain.tool(Tool::Yosys).is_err() {
        return Vec::new();
    }
    let Ok(result) = aurora_core::synthesize(toolchain, project, &DesignTarget::TopLevel, control)
    else {
        return Vec::new();
    };
    let Some(netlist) = result
        .netlist
        .filter(|_| result.status == Status::Succeeded)
    else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&netlist) else {
        return Vec::new();
    };
    latches_in(&text, &result.top, file)
}

/// Os latches de um netlist do Yosys (`write_json`): as células `$dlatch`
/// (e as variantes), com o sinal ligado à saída `Q` e a linha do `src`.
fn latches_in(netlist: &str, top: &str, file: &Utf8Path) -> Vec<Finding> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(netlist) else {
        return Vec::new();
    };
    let Some(modules) = json.get("modules").and_then(|m| m.as_object()) else {
        return Vec::new();
    };
    let file_name = file.file_name().unwrap_or_default();
    let mut found = Vec::new();
    for (module, body) in modules {
        let cells = body.get("cells").and_then(|c| c.as_object());
        let netnames = body.get("netnames").and_then(|n| n.as_object());
        for cell in cells.into_iter().flat_map(|c| c.values()) {
            let kind = cell.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if !kind.to_ascii_lowercase().contains("dlatch") {
                continue;
            }
            let q: Vec<u64> = cell
                .pointer("/connections/Q")
                .and_then(|q| q.as_array())
                .map(|bits| bits.iter().filter_map(|b| b.as_u64()).collect())
                .unwrap_or_default();
            // O nome que o aluno deu ao sinal: o fio com nome que tem os
            // bits da saída do latch.
            let name = netnames
                .into_iter()
                .flat_map(|n| n.iter())
                .filter(|(_, net)| net.get("hide_name").and_then(|h| h.as_u64()) != Some(1))
                .find(|(_, net)| {
                    net.get("bits")
                        .and_then(|b| b.as_array())
                        .is_some_and(|bits| {
                            bits.iter()
                                .any(|b| b.as_u64().is_some_and(|b| q.contains(&b)))
                        })
                })
                .map(|(name, _)| name.clone());
            let Some(name) = name else { continue };
            let signal = if module == top {
                name
            } else {
                format!("{module}.{name}")
            };
            // `src`: `arquivo.v:12.5-15.8`, às vezes várias separadas por `|`.
            let line = cell
                .pointer("/attributes/src")
                .and_then(|s| s.as_str())
                .and_then(|src| {
                    src.split('|')
                        .find(|s| s.contains(file_name))
                        .or_else(|| src.split('|').next())
                })
                .and_then(|s| s.rsplit_once(':'))
                .and_then(|(_, pos)| pos.split('.').next())
                .and_then(|n| n.parse::<u32>().ok());
            let finding = Finding::LatchInferred { signal, line };
            if !found.contains(&finding) {
                found.push(finding);
            }
        }
    }
    found
}

/// O mesmo arquivo, com as diferenças de grafia do Windows (`\` e `/`,
/// maiúsculas).
fn same_file(a: &Utf8Path, b: &Utf8Path) -> bool {
    let norm = |p: &Utf8Path| p.as_str().replace('\\', "/").to_ascii_lowercase();
    a == b || (cfg!(windows) && norm(a) == norm(b))
}

fn parse_summary(stdout: &str) -> Summary {
    let mut summary = Summary::default();
    // Um `tb.v` escrito à mão pode escrever mais de um `first`: valem as
    // linhas do primeiro.
    let mut later_first = false;
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
            // O contraexemplo: só o primeiro `first` vale.
            Some("first") if summary.first.is_none() => {
                summary.first = Some(First {
                    time_ns: number(1).unwrap_or(0).max(0) as u64,
                    cycle: number(2).filter(|c| *c >= 0).map(|c| c as u64),
                    ..First::default()
                });
            }
            Some("first") => later_first = true,
            Some("first_in") if words.len() >= 3 => {
                if let Some(first) = summary.first.as_mut().filter(|_| !later_first) {
                    first
                        .inputs
                        .push((words[1].to_owned(), words[2].to_owned()));
                }
            }
            Some("first_out") if words.len() >= 4 => {
                if let Some(first) = summary.first.as_mut().filter(|_| !later_first) {
                    first.outputs.push((
                        words[1].to_owned(),
                        words[2].to_owned(),
                        words[3].to_owned(),
                    ));
                }
            }
            _ => summary.other.push(line.to_owned()),
        }
    }
    summary
}

/// O `vvp` não conseguiu abrir a onda para gravar (`FST Error: ... Unable to
/// open ... for output`).
fn wave_was_locked(simulation: &aurora_core::SimulationResult) -> bool {
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
        let stdout = "FST info: dumpfile .aurora-learn/wave.fst opened for output.\r\nolá do testbench\r\nAURORA-LEARN samples 8\r\nAURORA-LEARN mismatched 4\r\nAURORA-LEARN output y 4 25 1\r\nAURORA-LEARN output z 0 -1 0\r\nAURORA-LEARN reset 2\r\nAURORA-LEARN end\r\nC:/x/tb.v:45: $finish called at 80000 (1ps)\r\n";
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
    fn the_first_mismatch_is_read_once() {
        let stdout = "AURORA-LEARN first 25 -1\nAURORA-LEARN first_in a 1\nAURORA-LEARN first_in b 0101\nAURORA-LEARN first_out y 1 0\nAURORA-LEARN first 35 -1\nAURORA-LEARN first_in a 0\nAURORA-LEARN samples 8\nAURORA-LEARN end\n";
        let summary = parse_summary(stdout);
        let first = summary.first.unwrap();
        assert_eq!(first.time_ns, 25);
        assert_eq!(first.cycle, None);
        assert_eq!(
            first.inputs,
            [("a".into(), "1".into()), ("b".into(), "0101".into())]
        );
        assert_eq!(first.outputs, [("y".into(), "1".into(), "0".into())]);
        assert!(summary.other.is_empty(), "{:?}", summary.other);
        let sequential = parse_summary("AURORA-LEARN first 17 3\n").first.unwrap();
        assert_eq!(sequential.cycle, Some(3));
    }

    #[test]
    fn values_are_written_for_people() {
        assert_eq!(value_text("1", false), "1");
        assert_eq!(value_text("x", false), "x");
        assert_eq!(value_text("0101", false), "4'b0101 (5)");
        assert_eq!(value_text("11111110", true), "8'b11111110 (-2)");
        assert_eq!(value_text("11111110", false), "8'b11111110 (254)");
        assert_eq!(value_text("0000000011111111", false), "16'h00ff (255)");
        assert_eq!(value_text("1000000000000000", true), "16'h8000 (-32768)");
        assert_eq!(value_text("01xz", false), "4'b01xz");
        assert_eq!(value_text("10X1", false), "4'b10x1");
    }

    #[test]
    fn a_reference_x_accepts_anything_and_a_student_x_is_wrong() {
        assert!(!differs("1x0", "100"));
        assert!(!differs("1x0", "1z0"));
        assert!(differs("10", "1x"));
        assert!(differs("10", "11"));
        assert!(!differs("0101", "0101"));
    }

    #[test]
    fn latches_come_from_the_dlatch_cells_of_the_netlist() {
        let netlist = r#"{"modules": {
            "maioria": {
                "cells": {
                    "$procdff$1": {"type": "$dlatch", "attributes": {"src": "C:/x/maioria.v:7.5-12.8"},
                                   "connections": {"D": [5], "EN": [6], "Q": [4]}},
                    "$and$2": {"type": "$and", "connections": {"Y": [7]}}
                },
                "netnames": {
                    "$auto$3": {"hide_name": 1, "bits": [4]},
                    "y": {"hide_name": 0, "bits": [4]},
                    "a": {"hide_name": 0, "bits": [5]}
                }
            },
            "sub": {
                "cells": {"$l": {"type": "$_DLATCH_P_", "attributes": {"src": "sub.v:3.1-4.2"},
                                 "connections": {"Q": [9]}}},
                "netnames": {"q": {"hide_name": 0, "bits": [8, 9]}}
            }
        }}"#;
        let found = latches_in(netlist, "maioria", Utf8Path::new("C:/x/maioria.v"));
        assert_eq!(
            found,
            [
                Finding::LatchInferred {
                    signal: "y".into(),
                    line: Some(7)
                },
                Finding::LatchInferred {
                    signal: "sub.q".into(),
                    line: Some(3)
                },
            ]
        );
        assert!(latches_in("{}", "m", Utf8Path::new("m.v")).is_empty());
    }

    #[test]
    fn a_summary_without_end_is_not_ended() {
        let summary = parse_summary("AURORA-LEARN samples 3\n");
        assert!(!summary.ended);
        assert_eq!(summary.samples, 3);
    }
}
