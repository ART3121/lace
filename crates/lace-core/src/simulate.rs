//! Simulação com Icarus Verilog ou Verilator, de um processador ou do projeto.
//!
//! Dois modos:
//!
//! - [`simulate`]: um processador já compilado, com o testbench que o
//!   `asmcomp` gerou e o build copiou para `Simulation/<nome>_tb.v`
//!   ([`Processor::testbench_path`]). Roda no diretório temporário do
//!   processador, onde já estão o `pc_<nome>_mem.txt` que o `.v` lê por nome
//!   relativo e onde o testbench grava a onda.
//! - [`simulate_project`]: o testbench escolhido no `.spf` com os arquivos
//!   sintetizáveis do projeto, como o botão Wave da AURORA
//!   (`js/compilation/icarus_da_onda.ts`, `verilator_da_onda.ts`). Roda na
//!   raiz do projeto, para onde são copiados os `pc_*_mem.txt` de todos os
//!   processadores e os arquivos que o testbench lê por nome relativo.
//!
//! Comandos, com `H` = biblioteca SAPHO (`yanc/SAPHO`), que só entra quando
//! o projeto tem processadores:
//!
//! ```text
//! Icarus:    iverilog [-y H] -s <topo> -o <trabalho>/<topo>.vvp <arquivos...> <testbench>
//!            vvp -n <trabalho>/<topo>.vvp [-fst]
//! Verilator: verilator --binary --main --trace -j 0 ... --top-module <topo>
//!                      -Mdir <trabalho>/obj_dir_<topo> [-y H] <arquivos...> <testbench>
//!            <trabalho>/obj_dir_<topo>/V<topo>
//! ```
//!
//! A biblioteca entra por `-y`: o simulador só carrega o módulo que faltar,
//! então `myFIFO.v` só entra quando alguém o instancia. As memórias (`.mif`)
//! e os arquivos de `Simulation/` são abertos por caminho absoluto, embutido
//! pelo `asmcomp`, e não precisam ser copiados.
//!
//! A onda sai com o nome do `$dumpfile` do testbench e a extensão do formato
//! que o simulador grava: FST no Icarus (`vvp -fst`), o padrão; VCD no
//! Verilator, porque o FST dele exige a lz4, que o bloco MSYS2 do bundle não
//! traz no Windows e que no Linux e no macOS fica fora da exceção do
//! compilador. Um `$dumpfile` com outra extensão, como o `.vcd` do
//! testbench que o YANC gera simulado no Icarus, é trocado numa cópia do
//! testbench; o arquivo dele não muda. Sem `$dumpfile`, o Lace injeta
//! `<topo>.fst` (`<topo>.vcd` no Verilator) com todos os sinais.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::cocotb::TestReport;
use crate::control::Control;
use crate::diagnostics::Diagnostic;
use crate::error::{LaceError, Result};
use crate::pipeline::{
    Artifact, ArtifactKind, ArtifactTracker, PlannedStep, Runner, Status, Step, StepReport,
    final_status,
};
use crate::project::{Processor, Project};
use crate::source::strip_comments;
use crate::toolchain::{Tool, Toolchain};

/// Qual simulador usar. Em JSON: `"icarus"` ou `"verilator"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Simulator {
    /// Icarus Verilog (`iverilog` + `vvp`): compila em segundos, simula
    /// devagar. O padrão.
    Icarus,
    /// Verilator: compila o modelo para C++ (dezenas de segundos na primeira
    /// vez; o `obj_dir` é reaproveitado depois), simula rápido.
    Verilator,
}

/// O formato do conteúdo de uma onda: FST no Icarus, VCD no Verilator. A
/// extensão é a do formato, menos quando o testbench nomeia a onda com uma
/// expressão que o Lace não resolve: aí o Icarus grava VCD no nome que o
/// testbench der.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WaveformFormat {
    /// Value Change Dump, texto.
    Vcd,
    /// Fast Signal Trace, binário e comprimido.
    Fst,
}

/// Como simular. Construa com [`SimulationOptions::new`] e ajuste os campos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SimulationOptions {
    /// Qual simulador.
    pub simulator: Simulator,
    /// Só Verilator: processos paralelos na compilação C++ do modelo. `None`
    /// usa todos os núcleos (`-j 0`), como a AURORA.
    pub build_jobs: Option<u32>,
    /// Quanto a simulação pode durar: o `vvp` ou o executável do Verilator,
    /// sem contar a elaboração e a compilação. Passou disso, o Lace a
    /// encerra e o resultado vem com
    /// [`Status::TimedOut`](crate::Status::TimedOut) e o que o testbench
    /// escreveu até ali. `None`, o padrão, é sem limite: um testbench sem
    /// `$finish` roda até ser cancelado.
    pub timeout: Option<Duration>,
}

impl SimulationOptions {
    /// `simulator`, com `-j 0` no Verilator.
    ///
    /// ```
    /// use lace_core::{SimulationOptions, Simulator};
    ///
    /// let mut options = SimulationOptions::new(Simulator::Verilator);
    /// options.build_jobs = Some(4);
    /// options.timeout = Some(std::time::Duration::from_secs(60));
    /// ```
    pub fn new(simulator: Simulator) -> Self {
        SimulationOptions {
            simulator,
            build_jobs: None,
            timeout: None,
        }
    }
}

/// A onda da simulação: com `processor`, a do testbench gerado do
/// processador; sem, a do testbench do projeto. É o `$dumpfile` do testbench
/// com a extensão `.fst` (o Icarus) ou `.vcd` (o Verilator), a mais recente
/// das duas; sem `$dumpfile`, `<testbench>.fst`
/// (Icarus) ou `<testbench>.vcd` (Verilator) na raiz do projeto. Sem
/// nenhuma no disco, a do Icarus, o simulador padrão. Com um testbench
/// cocotb (`.py`), `<raiz>/<módulo de teste>.fst` ([`cocotb`](crate::cocotb)).
///
/// # Erros
///
/// [`LaceError::NoTestbench`] sem testbench; [`LaceError::NotBuilt`] se o
/// processador ainda não foi compilado.
pub fn waveform_path(project: &Project, processor: Option<&Processor>) -> Result<Utf8PathBuf> {
    if let Some(processor) = processor {
        let testbench = processor.simulated_testbench();
        if !testbench.is_file() {
            return Err(LaceError::NotBuilt {
                processor: processor.name.clone(),
                missing: testbench,
            });
        }
        let text = read(&testbench)?;
        return Ok(match dump_of(&text, &processor.temp_dir) {
            Dump::Path(wave) => newest_wave(wave),
            _ => newest_wave(
                processor
                    .temp_dir
                    .join(format!("{}_tb.vcd", processor.name)),
            ),
        });
    }
    let testbench = project
        .testbench()
        .ok_or_else(|| LaceError::NoTestbench(project.spf_path().to_owned()))?;
    if crate::cocotb::is_testbench(&testbench) {
        return Ok(crate::cocotb::wave_path(project.root(), &testbench));
    }
    let text = read(&testbench)?;
    match dump_of(&text, project.root()) {
        Dump::Path(wave) => return Ok(newest_wave(wave)),
        Dump::Unknown => {
            return Err(LaceError::InvalidProject {
                path: testbench,
                reason: UNKNOWN_DUMP.into(),
            });
        }
        Dump::Absent => {}
    }
    // Onda injetada pelo Lace: `.fst` do Icarus ou `.vcd` do Verilator; vale
    // a mais recente.
    let top = project
        .testbench_module()?
        .unwrap_or_else(|| testbench.file_stem().unwrap_or_default().to_owned());
    Ok(newest(
        &[Simulator::Icarus, Simulator::Verilator]
            .map(|s| project.root().join(injected_wave(&top, s))),
    ))
}

/// A onda de um `$dumpfile`: a do Icarus (`.fst`) ou a do Verilator
/// (`.vcd`), a mais recente.
fn newest_wave(wave: Utf8PathBuf) -> Utf8PathBuf {
    newest(
        &[Simulator::Icarus, Simulator::Verilator].map(|s| wave.with_extension(wave_extension(s))),
    )
}

/// A extensão da onda que cada simulador grava: FST no Icarus, VCD no
/// Verilator (o FST dele exige a lz4).
fn wave_extension(simulator: Simulator) -> &'static str {
    match simulator {
        Simulator::Icarus => "fst",
        _ => "vcd",
    }
}

/// O candidato modificado por último; sem nenhum no disco, o primeiro.
fn newest(candidates: &[Utf8PathBuf]) -> Utf8PathBuf {
    candidates
        .iter()
        .filter_map(|c| Some((std::fs::metadata(c).ok()?.modified().ok()?, c)))
        .max_by_key(|(time, _)| *time)
        .map_or_else(|| candidates[0].clone(), |(_, c)| c.clone())
}

/// Por que a onda de um testbench não tem caminho conhecido.
const UNKNOWN_DUMP: &str = "The testbench names its waveform ($dumpfile) with an expression Lace cannot read; open the file it writes directly";

/// O nome da onda que o Lace injeta quando o testbench não grava, no formato
/// do simulador ([`wave_extension`]).
fn injected_wave(top: &str, simulator: Simulator) -> String {
    format!("{top}.{}", wave_extension(simulator))
}

/// O resultado de [`simulate`] e de [`simulate_project`].
///
/// Uma simulação que roda até o `$finish` e termina com código 0 é
/// `Succeeded`, mesmo que o circuito esteja errado: conferir o comportamento
/// é olhar `outputs` e a onda. O stdout do testbench (`$display`) está no
/// `stdout` do passo `simulate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SimulationResult {
    /// O módulo de topo simulado (o testbench).
    pub top: String,
    /// O simulador usado.
    pub simulator: Simulator,
    /// Como a simulação terminou.
    pub status: Status,
    /// O passo que falhou, quando for o caso: `elaborate` / `verilate`
    /// (Verilog que não compila) ou `simulate` (a simulação em si).
    pub failed_step: Option<Step>,
    /// Icarus: `elaborate` e `simulate`. Verilator: `verilate` e `simulate`.
    pub steps: Vec<StepReport>,
    /// Mensagens de todos os passos. No passo `simulate`, só as linhas que o
    /// parser reconhece: o resto do stdout é saída do testbench.
    pub diagnostics: Vec<Diagnostic>,
    /// O `.vvp` ou o executável do Verilator, a onda, e cada
    /// `output_<n>.txt` escrito.
    pub artifacts: Vec<Artifact>,
    /// A onda gerada, se a simulação chegou ao fim.
    pub waveform: Option<Waveform>,
    /// `Simulation/output_<n>.txt` escritos nesta simulação.
    #[schemars(with = "Vec<String>")]
    pub outputs: Vec<Utf8PathBuf>,
    /// Arquivos que o testbench tenta ler e que não existem. Não impedem a
    /// simulação (o testbench gerado pelo `asmcomp` confere o `$fopen` antes
    /// de ler), mas a porta correspondente não recebe dado nenhum.
    #[schemars(with = "Vec<String>")]
    pub missing_inputs: Vec<Utf8PathBuf>,
    /// Os testes de um testbench cocotb (`.py`), lidos do `results.xml`;
    /// `null` num testbench Verilog, ou se o cocotb não chegou a gravar o
    /// resultado.
    pub tests: Option<TestReport>,
    /// Quanto a simulação levou, do começo ao fim (preparar, compilar e
    /// rodar), em milissegundos.
    pub duration_ms: u64,
}

impl SimulationResult {
    /// `status == Succeeded`.
    pub fn succeeded(&self) -> bool {
        self.status == Status::Succeeded
    }
}

/// Uma onda gerada por simulação, pronta para
/// [`open_waveform`](crate::open_waveform).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Waveform {
    /// Caminho absoluto, com o nome que o `$dumpfile` do testbench deu.
    #[schemars(with = "String")]
    pub path: Utf8PathBuf,
    /// O formato do conteúdo.
    pub format: WaveformFormat,
}

/// Simula `processor` com o testbench gerado pelo `asmcomp`. O processador
/// precisa ter sido compilado antes com [`build`](crate::build); a duração
/// da simulação (clocks) e a frequência já estão no testbench gerado.
///
/// Roda no diretório temporário do processador. A onda sai em
/// `<temp>/<nome>_tb.fst` (`.vcd` no Verilator) e as saídas em
/// `Simulation/output_<n>.txt`.
///
/// ```no_run
/// use lace_core::*;
/// # let toolchain = Toolchain::open("/opt/lace/toolchain")?;
/// let project = Project::open("/p/soma")?;
/// let soma = project.require_processor("soma")?;
/// soma.write_input(0, "3\n4\n5\n")?;
/// let control = Control::default();
/// build(&toolchain, soma, &BuildOptions::default(), &control)?;
/// let options = SimulationOptions::new(Simulator::Icarus);
/// let result = simulate(&toolchain, soma, &options, &control)?;
/// if result.succeeded() {
///     println!("saída 0: {}", soma.read_output(0)?);
/// }
/// # Ok::<(), lace_core::LaceError>(())
/// ```
///
/// # Erros
///
/// Falha de elaboração ou de simulação volta como `Ok` com `status = Failed`.
/// `Err` quando:
///
/// - [`LaceError::NotBuilt`]: falta o testbench ou o Verilog (rode
///   [`build`](crate::build) antes);
/// - [`LaceError::ComponentMissing`] / [`LaceError::ToolchainIncomplete`]:
///   falta o simulador;
/// - [`LaceError::Spawn`] / [`LaceError::Io`]: o simulador não pôde ser
///   executado, ou um arquivo não pôde ser lido.
pub fn simulate(
    toolchain: &Toolchain,
    processor: &Processor,
    options: &SimulationOptions,
    control: &Control,
) -> Result<SimulationResult> {
    let _span = tracing::info_span!("simulate", processor = %processor.name).entered();
    let started = Instant::now();
    let name = &processor.name;
    crate::paths::check_simulator_path(&processor.dir)?;
    let testbench = processor.simulated_testbench();
    let verilog = processor.hardware_dir().join(format!("{name}.v"));
    for path in [&testbench, &verilog] {
        if !path.is_file() {
            return Err(LaceError::NotBuilt {
                processor: name.clone(),
                missing: path.clone(),
            });
        }
    }
    let text = read(&testbench)?;
    let missing_inputs: Vec<_> = data_files_read(&text)
        .into_iter()
        .map(|f| processor.temp_dir.join(f))
        .chain(absolute_inputs(&text))
        .filter(|p| !p.is_file())
        .collect();
    // Duas simulações ou builds do mesmo processador ao mesmo tempo regravam
    // os mesmos arquivos: a segunda recusa.
    let _lock = crate::paths::run_lock(&processor.temp_dir, name)?;
    let (notes, counts) = check_inputs(processor, &text)?;

    let wave = match dump_of(&text, &processor.temp_dir) {
        Dump::Path(wave) => wave,
        _ => processor.temp_dir.join(format!("{name}_tb.vcd")),
    };
    // No Icarus, o `$dumpfile("<nome>_tb.vcd")` do testbench gerado vira
    // `.fst` na cópia simulada.
    let (wave, renamed) = named_by_format(&text, wave, options.simulator);
    // Uma cópia do testbench que avisa quando o programa lê mais valores do
    // que uma entrada tem; sem o trecho esperado, vai a cópia só com a onda
    // renomeada, ou o testbench como está.
    let copied = watch_inputs(renamed.as_deref().unwrap_or(&text), &counts).or(renamed);
    let (simulated, instrumented) = match copied {
        Some(copied) => {
            let copy = processor.temp_dir.join(format!("instr_{name}_tb.v"));
            std::fs::write(&copy, copied).map_err(LaceError::io("Writing testbench", &copy))?;
            (copy.clone(), Some((copy, testbench)))
        }
        None => (testbench, None),
    };
    let plan = Plan {
        missing_inputs,
        top: format!("{name}_tb"),
        wave: Some(wave),
        wave_required: true,
        files: vec![verilog, simulated],
        cwd: processor.temp_dir.clone(),
        work: processor.temp_dir.clone(),
        output_dirs: vec![processor.simulation_dir()],
        sapho: true,
        notes,
        instrumented,
        cocotb: None,
    };
    let mut result = execute(toolchain, &plan, options, control, started)?;
    // O testbench gerado avisa quando o programa chega ao fim; sem o aviso,
    // os clocks acabaram antes e as saídas param no meio.
    let finished = result
        .steps
        .iter()
        .any(|s| s.step == Step::Simulate && s.stdout.contains(END_OF_PROGRAM));
    if result.succeeded() && !finished {
        result.diagnostics.push(note(
            crate::diagnostics::Severity::Warning,
            format!(
                "The program did not reach its end in the {} simulated clocks: the outputs stop there. Raise the number of clocks to simulate longer.",
                processor.clocks
            ),
            Some(processor.source.clone()),
        ));
    }
    Ok(result)
}

/// O que o testbench do `asmcomp` escreve quando o programa termina
/// (`hdl.c`).
const END_OF_PROGRAM: &str = "end of program";

/// Um aviso do próprio Lace, fora das ferramentas.
fn note(
    severity: crate::diagnostics::Severity,
    message: String,
    file: Option<Utf8PathBuf>,
) -> Diagnostic {
    Diagnostic {
        tool: Tool::Vvp,
        severity,
        message,
        file,
        line: None,
        column: None,
        raw: String::new(),
    }
}

/// Confere as entradas que o testbench do processador lê
/// (`Simulation/input_<n>.txt`) antes de simular: o testbench do YANC lê com
/// `$fscanf` sem olhar o resultado, e uma linha que não é número viraria um
/// valor qualquer, sem aviso.
///
/// - linha que não é inteiro decimal: [`LaceError::InvalidDataFile`], e a
///   simulação não roda;
/// - arquivo vazio, ou valor que não cabe em `#NUBITS` bits (com ou sem
///   sinal): aviso, e a simulação roda.
///
/// Um arquivo com menos valores do que o programa lê só se descobre
/// rodando: [`watch_inputs`] põe o aviso no testbench. Devolve os avisos e
/// quantos valores tem cada entrada.
fn check_inputs(
    processor: &Processor,
    testbench: &str,
) -> Result<(Vec<Diagnostic>, HashMap<Utf8PathBuf, usize>)> {
    let source = std::fs::read_to_string(&processor.source).unwrap_or_default();
    let bits = crate::source::declared_nubits(&source, processor.language);
    let (low, high) = (-(1i64 << (bits - 1)), (1i64 << bits) - 1);
    let mut notes = Vec::new();
    let mut counts = HashMap::new();
    for input in absolute_inputs(testbench)
        .into_iter()
        .filter(|p| p.is_file())
    {
        let values = crate::files::read_data_file(&input)?;
        counts.insert(input.clone(), values.len());
        if values.is_empty() {
            notes.push(note(
                crate::diagnostics::Severity::Warning,
                "The input file is empty: the port reads 0".into(),
                Some(input),
            ));
        } else if let Some(value) = values.iter().find(|v| !(low..=high).contains(*v)) {
            notes.push(note(
                crate::diagnostics::Severity::Warning,
                format!(
                    "{value} does not fit in the {bits}-bit word (#NUBITS): the processor reads it truncated"
                ),
                Some(input),
            ));
        }
    }
    Ok((notes, counts))
}

/// O testbench do `asmcomp` com um aviso para quando o programa lê mais
/// valores do que uma entrada tem. O testbench lê com `scan_result =
/// $fscanf(data_in_<n>, "%d", in_<n>);` e não olha o resultado: sem valor, a
/// porta repete o último em silêncio. A cópia confere o resultado e, na
/// primeira leitura que falha, escreve `WARNING: <entrada>:<linha>: ...`, que
/// o `vvp` passa como aviso com o arquivo e a linha seguinte à última.
///
/// Tudo entra nas linhas que já existem, e os diagnósticos da cópia apontam
/// para o testbench original. `None` se o testbench não tem o trecho
/// esperado (de outro gerador): ele roda como está, sem o aviso.
fn watch_inputs(testbench: &str, counts: &HashMap<Utf8PathBuf, usize>) -> Option<String> {
    const DECLARATION: &str = "integer scan_result;";
    if !testbench.contains(DECLARATION) {
        return None;
    }
    let mut text = testbench.to_owned();
    let mut flags = String::new();
    let mut watched = 0;
    for port in 0..1024u32 {
        let read = format!("scan_result = $fscanf(data_in_{port}, \"%d\", in_{port});");
        if !text.contains(&read) {
            continue;
        }
        // O caminho como o testbench o escreve no `$fopen`, já em texto Verilog.
        let open = format!("data_in_{port} = $fopen(\"");
        let Some(path) = text
            .find(&open)
            .map(|at| &text[at + open.len()..])
            .and_then(|rest| rest.split('"').next())
            .map(str::to_owned)
        else {
            continue;
        };
        let values = counts.get(Utf8Path::new(&path)).copied().unwrap_or(0);
        // No `$display`, `%` abre um formato.
        let shown = path.replace('%', "%%");
        let warning = format!(
            "begin {read} if (scan_result != 1 && !lace_eof_{port}) begin lace_eof_{port} = 1; \
             $display(\"WARNING: {shown}:{}: the program reads more values than this file has ({values}); from here on the port repeats the last one\"); end end",
            values + 1
        );
        text = text.replace(&read, &warning);
        flags.push_str(&format!(" reg lace_eof_{port} = 0;"));
        watched += 1;
    }
    if watched == 0 {
        return None;
    }
    Some(text.replacen(DECLARATION, &format!("{DECLARATION}{flags}"), 1))
}

/// Simula o projeto: o testbench do `.spf` ([`Project::testbench`]) com os
/// arquivos sintetizáveis e o Verilog de cada processador compilado.
///
/// O módulo de topo é o do testbench ([`Project::testbench_module`]: o único
/// do arquivo, ou o que tem o nome dele). A simulação roda na raiz do
/// projeto: a onda sai lá, com o nome do `$dumpfile`, e o `.vvp` ou o
/// `obj_dir` ficam em [`Project::temp_dir`]. Antes de rodar, o Lace copia
/// para a raiz:
///
/// - o `pc_<nome>_mem.txt` de cada processador compilado, que o `.v` dele lê
///   por nome relativo;
/// - os arquivos que o testbench lê por nome relativo (`$readmemb/h("x")`,
///   `$fopen("x", "r")`), a partir da pasta do testbench.
///
/// Se o testbench não grava onda (`$dumpfile`), o Lace simula uma cópia com
/// `$dumpfile("<topo>.fst"); $dumpvars(0, <topo>);` antes do último
/// `endmodule` (`.vcd` no Verilator): todos os sinais, inclusive os do
/// módulo testado. A onda sai com a extensão do formato do simulador (`.fst`
/// no Icarus, `.vcd` no Verilator): um `$dumpfile` com outra é trocado numa
/// cópia. O arquivo do usuário não é alterado.
///
/// O vvp roda com `-n` (um `$stop` termina a simulação), e uma linha
/// `ERROR:` ou `FATAL:` (`$error`, `$fatal`) reprova a simulação, mesmo com
/// o vvp saindo com 0.
///
/// Um testbench cocotb (`.py`) simula, só no Icarus, o módulo da diretiva
/// `# aurora-toplevel:` do arquivo (ou o topo do projeto, com um aviso), com
/// os testes em Python ([`cocotb`](crate::cocotb)). O resultado traz os
/// testes em [`SimulationResult::tests`], cada teste que falha é um
/// diagnóstico de erro no `.py` e reprova a simulação, e a onda vem mesmo
/// assim: é nela que se vê a falha.
///
/// Os processadores não são compilados aqui: chame [`build`](crate::build)
/// para cada um antes, como o botão Wave da AURORA.
///
/// # Erros
///
/// - [`LaceError::NoTestbench`]: o projeto não tem testbench;
/// - [`LaceError::InvalidProject`]: o testbench ou um arquivo sintetizável
///   registrado não existe;
/// - com um testbench cocotb: [`LaceError::CocotbNeedsIcarus`] com o
///   Verilator, [`LaceError::NoCocotbToplevel`] sem diretiva e sem topo,
///   [`LaceError::InvalidName`] com um nome de arquivo que não é
///   identificador do Python, [`LaceError::ComponentMissing`] sem o
///   componente `cocotb` e [`LaceError::CocotbUnavailable`] se o Python dele
///   não carrega o cocotb;
/// - os mesmos de [`simulate`] para ferramenta e I/O.
pub fn simulate_project(
    toolchain: &Toolchain,
    project: &Project,
    options: &SimulationOptions,
    control: &Control,
) -> Result<SimulationResult> {
    let started = Instant::now();
    let testbench = project
        .testbench()
        .ok_or_else(|| LaceError::NoTestbench(project.spf_path().to_owned()))?;
    if !testbench.is_file() {
        return Err(LaceError::InvalidProject {
            path: testbench,
            reason: "The project testbench does not exist".into(),
        });
    }
    // Um testbench cocotb (`.py`) simula o DUT, com os testes em Python.
    let cocotb = crate::cocotb::is_testbench(&testbench);
    if cocotb && options.simulator != Simulator::Icarus {
        return Err(LaceError::CocotbNeedsIcarus(testbench));
    }
    let (top, cocotb_notes) = if cocotb {
        crate::cocotb::dut(project, &testbench)?
    } else {
        let top = project
            .testbench_module()?
            .unwrap_or_else(|| testbench.file_stem().unwrap_or_default().to_owned());
        (top, Vec::new())
    };
    let _span = tracing::info_span!("simulate_project", %top).entered();
    let root = project.root().to_owned();
    let work = project.temp_dir();
    // A simulação do projeto lê o que o build de cada processador grava e
    // copia arquivos para a raiz: nada disso pode correr junto com outra.
    let _locks = std::iter::once(crate::paths::run_lock(
        &work.join(".project"),
        project.name(),
    ))
    .chain(
        project
            .processors()
            .iter()
            .map(|p| crate::paths::run_lock(&p.temp_dir, &p.name)),
    )
    .collect::<Result<Vec<_>>>()?;
    // O Verilog de cada processador abre as memórias por caminho absoluto.
    if !project.processors().is_empty() {
        crate::paths::check_simulator_path(&root)?;
    }
    std::fs::create_dir_all(&work).map_err(LaceError::io("Creating directory", &work))?;

    // Arquivos: sintetizáveis na ordem do .spf, o Verilog de cada processador
    // compilado que ainda não esteja na lista, e o testbench por último.
    let mut files: Vec<Utf8PathBuf> = Vec::new();
    let mut push = |path: Utf8PathBuf| {
        if !files.contains(&path) {
            files.push(path);
        }
    };
    for file in project.files(crate::FileRole::Synthesizable) {
        if !file.path.is_file() {
            return Err(LaceError::InvalidProject {
                path: file.path,
                reason: "Synthesizable file added to the project does not exist".into(),
            });
        }
        push(file.path);
    }
    for processor in project.processors() {
        let verilog = processor
            .hardware_dir()
            .join(format!("{}.v", processor.name));
        if verilog.is_file() {
            push(verilog);
        }
    }

    // O `.v` de cada processador lê `pc_<nome>_mem.txt` por nome relativo ao
    // CWD da simulação, que aqui é a raiz.
    for processor in project.processors() {
        let map = processor
            .temp_dir
            .join(format!("pc_{}_mem.txt", processor.name));
        if map.is_file() {
            copy(&map, &root.join(map.file_name().expect("File has a name")))?;
        }
    }

    if cocotb {
        let Some(run) = crate::cocotb::prepare(toolchain, project, &testbench, &top, control)?
        else {
            return Ok(cancelled(top, options.simulator, started));
        };
        files.push(run.dump_module.clone());
        let plan = Plan {
            missing_inputs: Vec::new(),
            top,
            wave: Some(run.wave.clone()),
            wave_required: false,
            files,
            cwd: root,
            work,
            output_dirs: project
                .processors()
                .iter()
                .map(Processor::simulation_dir)
                .collect(),
            sapho: !project.processors().is_empty(),
            notes: cocotb_notes,
            instrumented: None,
            cocotb: Some(run),
        };
        return execute(toolchain, &plan, options, control, started);
    }

    // Dados que o testbench lê por nome relativo vêm da pasta dele.
    let text = read(&testbench)?;
    let tb_dir = testbench.parent().expect("File has a parent").to_owned();
    let (missing, mut notes) = stage_data_files(&text, &tb_dir, &root, &work)?;
    let mut missing_inputs = missing;
    missing_inputs.extend(absolute_inputs(&text).into_iter().filter(|p| !p.is_file()));

    // Sem `$dumpfile`, o Lace injeta o dump numa cópia do testbench. A onda
    // injetada não é exigida: o testbench que termina no tempo 0 (antes do
    // `initial` injetado) não pediu onda e não reprova por falta dela.
    let original = testbench.clone();
    let dump = dump_of(&text, &root);
    if !matches!(dump, Dump::Absent) && !strip_comments(&text).contains("$dumpvars") {
        notes.push(note(
            crate::diagnostics::Severity::Warning,
            "The testbench calls $dumpfile but not $dumpvars: no signal is recorded, and the waveform is not written".into(),
            Some(original.clone()),
        ));
    }
    let (testbench, wave, wave_required, instrumented) = match dump {
        Dump::Path(wave) => match named_by_format(&text, wave, options.simulator) {
            // A onda sai com a extensão do formato do simulador: a cópia
            // simulada leva o `$dumpfile` com ela, e o arquivo do usuário não
            // muda.
            (wave, Some(renamed)) => {
                let instrumented = work.join(format!(
                    "instr_{}",
                    testbench.file_name().expect("File has a name")
                ));
                std::fs::write(&instrumented, renamed)
                    .map_err(LaceError::io("Writing testbench", &instrumented))?;
                let copy = instrumented.clone();
                (
                    instrumented,
                    Some(wave),
                    true,
                    Some((copy, original.clone())),
                )
            }
            (wave, None) => (testbench, Some(wave), true, None),
        },
        // `$dumpfile(ONDA)` com um nome que o Lace não resolve: o testbench
        // grava onde quiser, e o Lace não injeta outro dump (o `-fst` do
        // injetado mudaria o formato do arquivo dele).
        Dump::Unknown => (testbench, None, false, None),
        Dump::Absent => {
            let instrumented = work.join(format!(
                "instr_{}",
                testbench.file_name().expect("File has a name")
            ));
            let wave = injected_wave(&top, options.simulator);
            let text = with_default_dump(&text, &top, &wave);
            std::fs::write(&instrumented, text)
                .map_err(LaceError::io("Writing testbench", &instrumented))?;
            let copy = instrumented.clone();
            (
                instrumented,
                Some(root.join(wave)),
                false,
                Some((copy, original)),
            )
        }
    };
    // O vvp não grava a onda se a pasta do `$dumpfile` não existe.
    if let Some(parent) = wave.as_deref().and_then(Utf8Path::parent) {
        std::fs::create_dir_all(parent).map_err(LaceError::io("Creating directory", parent))?;
    }
    files.push(testbench);

    let plan = Plan {
        missing_inputs,
        top,
        wave,
        wave_required,
        files,
        cwd: root,
        work,
        output_dirs: project
            .processors()
            .iter()
            .map(Processor::simulation_dir)
            .collect(),
        sapho: !project.processors().is_empty(),
        notes,
        instrumented,
        cocotb: None,
    };
    let mut result = execute(toolchain, &plan, options, control, started)?;
    // O Icarus só grava VCD quando o Lace não consegue trocar o nome do
    // `$dumpfile` (a chamada quebrada em linhas). O FST seria várias vezes
    // menor; o VCD de centenas de MB demora para abrir, e a aba do Studio
    // não abre acima de 256 MB.
    if let Some(wave) = &result.waveform
        && wave.format == WaveformFormat::Vcd
        && options.simulator == Simulator::Icarus
        && let Ok(size) = std::fs::metadata(&wave.path).map(|m| m.len())
        && size > LARGE_VCD
    {
        result.diagnostics.push(note(
            crate::diagnostics::Severity::Warning,
            format!(
                "The waveform has {} MB of VCD: a $dumpfile ending in .fst records the same signals several times smaller",
                size / 1_000_000
            ),
            plan.instrumented
                .as_ref()
                .map(|(_, original)| original.clone())
                .or_else(|| plan.files.last().cloned()),
        ));
    }
    Ok(result)
}

/// A partir de quanto um VCD da simulação do projeto vem com a sugestão de
/// gravar em FST.
const LARGE_VCD: u64 = 100_000_000;

struct Plan {
    missing_inputs: Vec<Utf8PathBuf>,
    top: String,
    /// Onde a onda vai aparecer; `None` quando o testbench a nomeia com uma
    /// expressão que o Lace não resolve.
    wave: Option<Utf8PathBuf>,
    /// A onda é do testbench (`$dumpfile` dele) e faltar é falha. A onda
    /// que o Lace injeta não é exigida.
    wave_required: bool,
    /// Na ordem de compilação, testbench por último.
    files: Vec<Utf8PathBuf>,
    /// CWD da simulação.
    cwd: Utf8PathBuf,
    /// Onde ficam o `.vvp` e o `obj_dir` do Verilator.
    work: Utf8PathBuf,
    /// Onde procurar `output_<n>.txt`.
    output_dirs: Vec<Utf8PathBuf>,
    /// Tem processador SAPHO (a biblioteca SAPHO é obrigatória)?
    sapho: bool,
    /// Avisos do preparo (um dado que não foi copiado), antes dos das
    /// ferramentas.
    notes: Vec<Diagnostic>,
    /// O testbench com o dump injetado e o original: os diagnósticos da
    /// cópia apontam para o original, que tem as mesmas linhas.
    instrumented: Option<(Utf8PathBuf, Utf8PathBuf)>,
    /// Testbench cocotb: a VPI e o ambiente do `vvp`, o `cmds.f` e o módulo
    /// que grava a onda (que já está em `files`).
    cocotb: Option<crate::cocotb::CocotbRun>,
}

/// O resultado de uma simulação cancelada antes do primeiro passo (na sonda
/// do cocotb).
fn cancelled(top: String, simulator: Simulator, started: Instant) -> SimulationResult {
    SimulationResult {
        top,
        simulator,
        status: Status::Cancelled,
        failed_step: Some(Step::Elaborate),
        steps: Vec::new(),
        diagnostics: Vec::new(),
        artifacts: Vec::new(),
        waveform: None,
        outputs: Vec::new(),
        missing_inputs: Vec::new(),
        tests: None,
        duration_ms: crate::pipeline::elapsed_ms(started),
    }
}

fn execute(
    toolchain: &Toolchain,
    plan: &Plan,
    options: &SimulationOptions,
    control: &Control,
    started: Instant,
) -> Result<SimulationResult> {
    let top = &plan.top;
    let hdl = toolchain.sapho_library(plan.sapho)?;
    let mut tracker = ArtifactTracker::new();
    let mut runner = Runner::new(control);
    let outputs_before = output_files(&plan.output_dirs);
    let mut tests = None;
    // O `vvp` do cocotb foi até o fim, com os testes passando ou não.
    let mut tests_ran = false;

    match options.simulator {
        Simulator::Icarus => {
            // O `.vvp` do cocotb fica na pasta dele, com o `cmds.f` e o dump.
            let image = plan
                .cocotb
                .as_ref()
                .map_or_else(|| plan.work.join(format!("{top}.vvp")), |c| c.image.clone());
            tracker.expect(ArtifactKind::IcarusImage, &image, true);
            if let Some(wave) = &plan.wave {
                tracker.expect(ArtifactKind::Waveform, wave, plan.wave_required);
            }

            // A cópia do testbench com o dump injetado fica em `.lace/Temp`:
            // a pasta do original entra nos `include` antes da raiz.
            let original_dir = plan
                .instrumented
                .as_ref()
                .and_then(|(_, original)| original.parent());
            let mut elaborate = crate::synth::include_paths(
                toolchain.invocation(Tool::Iverilog, &plan.cwd)?,
                original_dir,
                &plan.cwd,
            );
            // SystemVerilog só quando há `.sv`, como no `check`: o `-g2012`
            // reserva nomes (`bit`, `logic`) que um Verilog-2001 pode usar.
            if plan.files.iter().any(|f| f.extension() == Some("sv")) {
                elaborate = elaborate.arg("-g2012");
            }
            if let Some(hdl) = &hdl {
                elaborate = elaborate.arg("-y").icarus_path_arg(hdl);
            }
            let mut elaborate = elaborate.arg("-s").arg(top);
            // cocotb: o timescale padrão do runner dele e o módulo que grava a
            // onda, como segunda raiz.
            if let Some(cocotb) = &plan.cocotb {
                elaborate = elaborate
                    .arg("-f")
                    .icarus_path_arg(&cocotb.commands)
                    .arg("-s")
                    .arg(crate::cocotb::DUMP_MODULE);
            }
            let mut elaborate = elaborate.arg("-o").path_arg(&image);
            for file in &plan.files {
                elaborate = elaborate.icarus_path_arg(file);
            }
            if runner.run(PlannedStep::new(Step::Elaborate, Tool::Iverilog, elaborate))? {
                // `-n`: um `$stop` termina em vez de abrir o prompt interativo.
                let mut run = toolchain.invocation(Tool::Vvp, &plan.cwd)?.arg("-n");
                // Num pipe, o vvp guarda o stdout em buffer: o `$display` só
                // sai quando o buffer enche ou no fim, e se perde se o
                // processo for morto. Com alguém acompanhando a saída, `-i` tira o
                // buffer, ao custo de uma escrita por linha; sem, fica o
                // buffer, que é mais rápido (a escolha da AURORA).
                if control.has_sink() {
                    run = run.arg("-i");
                }
                // cocotb: a VPI dele, que sobe o Python e roda os testes.
                if let Some(cocotb) = &plan.cocotb {
                    run = run
                        .arg("-m")
                        .arg(&cocotb.vpi)
                        .append_search_path(std::slice::from_ref(&cocotb.libs));
                    for (key, value) in &cocotb.env {
                        run = run.env(key, value);
                    }
                }
                run = run.path_arg(&image);
                if plan.wave.as_deref().and_then(Utf8Path::extension) == Some("fst") {
                    run = run.arg("-fst");
                }
                runner.run(
                    PlannedStep::new(Step::Simulate, Tool::Vvp, run).timeout(options.timeout),
                )?;
                // cocotb: o resultado dos testes. Cada teste que falhou é um
                // diagnóstico de erro, que reprova a simulação logo abaixo.
                if let Some(cocotb) = &plan.cocotb {
                    tests_ran = runner.steps.last().is_some_and(|s| {
                        s.step == Step::Simulate
                            && matches!(s.termination, crate::process::Termination::Exited(0))
                    });
                    let (report, found) = crate::cocotb::finish(cocotb, tests_ran);
                    runner.diagnostics.extend(found);
                    tests = report;
                }
                // O vvp sai com 0 depois de um `$error`: o erro do testbench
                // reprova a simulação.
                runner.fail_on_error_diagnostics(Tool::Vvp);
            }
        }
        Simulator::Verilator => {
            let obj_dir = plan.work.join(format!("obj_dir_{top}"));
            let model = obj_dir.join(format!("V{top}{}", std::env::consts::EXE_SUFFIX));
            tracker.expect(ArtifactKind::VerilatedModel, &model, true);
            if let Some(wave) = &plan.wave {
                tracker.expect(ArtifactKind::Waveform, wave, plan.wave_required);
            }

            // Flags da AURORA (js/compilation/builders/verilator.ts), sem o
            // arquivo .vlt dos monitores de pilha e ULA, que só servem à
            // interface dela.
            let jobs = options
                .build_jobs
                .map_or_else(|| "0".to_owned(), |j| j.to_string());
            let mut verilate = toolchain
                .invocation(Tool::Verilator, &plan.work)?
                .arg("--binary")
                .arg("--main")
                // VCD sempre: no Linux e no macOS, o FST do Verilator do
                // bundle compila contra lz4 e zlib do sistema, que ficam fora
                // da exceção do compilador (decisão do autor: só compilador,
                // make e Perl). O mesmo formato nas três plataformas.
                .arg("--trace")
                .arg("-j")
                .arg(jobs)
                .arg("-MAKEFLAGS")
                .arg("OBJCACHE=")
                // O Python do bundle, e não o `python3` do sistema.
                .arg("-MAKEFLAGS")
                .arg(format!(
                    "PYTHON3={}",
                    make_path(&toolchain.bundled_python()?)
                ))
                .arg("-Wno-fatal")
                .arg("-Wno-TIMESCALEMOD")
                .arg("-Wno-DECLFILENAME")
                .arg("-Wno-STMTDLY")
                .arg("--timing")
                .arg("--x-assign")
                .arg("fast")
                .arg("--no-trace-top")
                // O `$display` sai na hora, e não em blocos no fim: é o que
                // mostra a saída ao vivo. Sem isso o modelo guarda o stdout
                // em buffer, como o vvp sem `-i`.
                .arg("--autoflush")
                .arg("+define+YANC_TRACE");
            for &flag in verilator_cflags() {
                verilate = verilate.arg("-CFLAGS").arg(flag);
            }
            verilate = verilate
                .arg("--top-module")
                .arg(top)
                .arg("-Mdir")
                .path_arg(&obj_dir);
            if let Some(hdl) = &hdl {
                verilate = verilate.arg("-y").path_arg(hdl);
            }
            for file in &plan.files {
                verilate = verilate.path_arg(file);
            }
            // O PATH (make e compilador; do sistema no Linux e no macOS, do
            // bundle no Windows) e o LC_ALL=C vêm de Toolchain::invocation; o
            // VERILATOR_ROOT fica sem definir, e o script o deduz do próprio
            // caminho.
            if runner.run(PlannedStep::new(Step::Verilate, Tool::Verilator, verilate))? {
                let run = crate::process::Invocation::new(model.clone(), &plan.cwd)
                    .search_path(&toolchain.verilated_model_path());
                runner.run(
                    PlannedStep::new(Step::Simulate, Tool::Verilator, run).timeout(options.timeout),
                )?;
            }
        }
    }

    let mut artifacts = tracker.finish();
    let outputs: Vec<_> = output_files(&plan.output_dirs)
        .into_iter()
        .filter(|entry| !outputs_before.contains(entry))
        .map(|(path, _)| path)
        .collect();
    artifacts.extend(outputs.iter().map(|path| Artifact {
        kind: ArtifactKind::SimulationOutput,
        path: path.clone(),
        required: false,
        fresh: true,
    }));

    let status = final_status(runner.status, &artifacts);
    // A onda de um teste cocotb que falhou também vem: a simulação foi até o
    // fim, e é nela que se vê por que o teste falhou (a AURORA também a abre).
    let fresh = |wave: &Utf8PathBuf| {
        artifacts
            .iter()
            .any(|a| a.kind == ArtifactKind::Waveform && &a.path == wave && a.fresh)
    };
    let waveform = plan
        .wave
        .as_ref()
        .filter(|wave| {
            (status == Status::Succeeded && wave.is_file()) || (tests_ran && fresh(wave))
        })
        .map(|wave| Waveform {
            path: wave.clone(),
            format: if wave.extension() == Some("fst") {
                WaveformFormat::Fst
            } else {
                WaveformFormat::Vcd
            },
        });
    let mut diagnostics = plan.notes.clone();
    diagnostics.extend(runner.diagnostics);
    if let Some((copy, original)) = &plan.instrumented {
        for d in &mut diagnostics {
            if d.file.as_ref() == Some(copy) {
                d.file = Some(original.clone());
            }
        }
    }
    Ok(SimulationResult {
        top: top.clone(),
        simulator: options.simulator,
        status,
        failed_step: runner.failed_step,
        steps: runner.steps,
        diagnostics,
        artifacts,
        waveform,
        outputs,
        missing_inputs: plan.missing_inputs.clone(),
        tests,
        duration_ms: crate::pipeline::elapsed_ms(started),
    })
}

/// Flags do compilador C++ do modelo, as da AURORA. No macOS sai o
/// `-march=native`: o clang da Apple em versões anteriores ao Xcode 15 o
/// recusa no Apple Silicon (inferência a partir de relatos, não testado aqui),
/// e a flag só acelera; não muda o resultado da simulação.
fn verilator_cflags() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["-O3", "-fstrict-aliasing", "-pipe", "-Wno-attributes"]
    } else {
        &[
            "-O3",
            "-march=native",
            "-fstrict-aliasing",
            "-pipe",
            "-Wno-attributes",
        ]
    }
}

fn read(path: &Utf8Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(LaceError::io("Reading testbench", path))
}

fn copy(from: &Utf8Path, to: &Utf8Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(LaceError::io("Creating directory", parent))?;
    }
    std::fs::copy(from, to)
        .map(drop)
        .map_err(LaceError::io("Copying for simulation", to))
}

/// Primeiro argumento literal de cada chamada `name("...")` fora de
/// comentários, com o que vier depois da aspa de fechamento.
fn string_calls<'a>(code: &'a str, name: &str) -> Vec<(&'a str, &'a str)> {
    let pattern = format!("{name}(\"");
    code.match_indices(&pattern)
        .filter_map(|(at, _)| {
            let rest = &code[at + pattern.len()..];
            let end = rest.find('"')?;
            Some((&rest[..end], &rest[end + 1..]))
        })
        .collect()
}

/// Um caminho que o `make` do Verilator repassa ao `/bin/sh`. No Windows
/// esse `sh` é o do MSYS2, para o qual `\` é escape (`D:\a\b` vira `D:ab`):
/// o caminho vai com `/`, que o MSYS2 aceita num caminho do Windows.
fn make_path(path: &Utf8Path) -> String {
    if cfg!(windows) {
        path.as_str().replace('\\', "/")
    } else {
        path.as_str().to_owned()
    }
}

/// A onda com a extensão do formato que o simulador grava
/// ([`wave_extension`]): com um `$dumpfile` de outra extensão, a onda ganha
/// a do formato e vem o texto do testbench com o nome trocado. Com a extensão
/// certa, ou com um `$dumpfile` que o Lace não acha no texto, nada muda.
fn named_by_format(
    testbench: &str,
    wave: Utf8PathBuf,
    simulator: Simulator,
) -> (Utf8PathBuf, Option<String>) {
    let extension = wave_extension(simulator);
    if wave.extension() == Some(extension) {
        return (wave, None);
    }
    let Some(name) = dumpfile_name(testbench) else {
        return (wave, None);
    };
    match with_dumpfile(testbench, &with_extension(&name, extension)) {
        Some(renamed) => (wave.with_extension(extension), Some(renamed)),
        None => (wave, None),
    }
}

/// `name` com `extension` no lugar da que tiver (`soma_tb.vcd` vira
/// `soma_tb.fst`; sem extensão, ganha uma).
fn with_extension(name: &str, extension: &str) -> String {
    let (dir, file) = name.split_at(name.rfind(['/', '\\']).map_or(0, |i| i + 1));
    let stem = match file.rfind('.') {
        Some(dot) if dot > 0 => &file[..dot],
        _ => file,
    };
    format!("{dir}{stem}.{extension}")
}

/// O nome que o último `$dumpfile` dá à onda, como o testbench o escreve: o
/// texto entre aspas ou o de uma constante. `None` sem `$dumpfile` ou com
/// uma expressão.
fn dumpfile_name(testbench: &str) -> Option<String> {
    let code = strip_comments(testbench);
    let at = code.rfind("$dumpfile")?;
    let argument = code[at + "$dumpfile".len()..]
        .trim_start()
        .strip_prefix('(')
        .and_then(|rest| rest.find(')').map(|end| rest[..end].trim()))?;
    let name = if argument.starts_with('"') {
        argument.trim_matches('"').to_owned()
    } else {
        string_constant(&code, argument.trim_start_matches('`'))?
    };
    (!name.is_empty()).then_some(name)
}

/// O testbench com o argumento do último `$dumpfile` (fora de comentário e
/// de texto entre aspas) trocado por `"name"`. As linhas continuam as
/// mesmas. `None` se não houver `$dumpfile(...)`.
fn with_dumpfile(testbench: &str, name: &str) -> Option<String> {
    let at = dumpfile_calls(testbench).pop()?;
    let rest = &testbench[at + "$dumpfile".len()..];
    let open = at + "$dumpfile".len() + rest.find('(')?;
    if !testbench[at + "$dumpfile".len()..open].trim().is_empty() {
        return None;
    }
    let close = open + testbench[open..].find(')')?;
    if testbench[open..close].contains('\n') {
        return None;
    }
    Some(format!(
        "{}(\"{name}\"){}",
        &testbench[..open],
        &testbench[close + 1..]
    ))
}

/// Onde começa cada `$dumpfile` do testbench, fora de comentário e de texto
/// entre aspas.
fn dumpfile_calls(text: &str) -> Vec<usize> {
    const CALL: &[u8] = b"$dumpfile";
    let bytes = text.as_bytes();
    let mut calls = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b'\n' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            b'$' if bytes[i..].starts_with(CALL)
                && bytes
                    .get(i + CALL.len())
                    .is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_' || *c == b'$')) =>
            {
                calls.push(i);
                i += CALL.len();
            }
            _ => i += 1,
        }
    }
    calls
}

/// O `$dumpfile` de um testbench.
#[derive(Debug, PartialEq, Eq)]
enum Dump {
    /// Não tem: o Lace injeta o dele.
    Absent,
    /// A onda vai para este caminho.
    Path(Utf8PathBuf),
    /// Tem, com um nome que o Lace não resolve (uma expressão).
    Unknown,
}

/// Onde a onda vai parar: o `$dumpfile` do testbench, relativo ao CWD. O
/// nome é o texto entre aspas ou um `localparam`, `parameter` ou
/// `` `define `` com texto entre aspas (`$dumpfile(ONDA)`); vale a última
/// chamada.
fn dump_of(testbench: &str, cwd: &Utf8Path) -> Dump {
    if !strip_comments(testbench).contains("$dumpfile") {
        return Dump::Absent;
    }
    match dumpfile_name(testbench) {
        Some(name) if crate::paths::is_rooted(&name) => Dump::Path(Utf8PathBuf::from(name)),
        Some(name) => Dump::Path(cwd.join(name)),
        None => Dump::Unknown,
    }
}

/// O texto de uma constante do Verilog: `` `define NOME "x" `` ou
/// `NOME = "x"` (de um `localparam` ou `parameter`).
fn string_constant(code: &str, name: &str) -> Option<String> {
    let valid = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return None;
    }
    let quoted = |text: &str| {
        text.trim_start()
            .strip_prefix('"')
            .and_then(|rest| rest.split('"').next())
            .map(str::to_owned)
    };
    for line in code.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("`define") {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix(name)
                && value.starts_with(char::is_whitespace)
            {
                return quoted(value);
            }
        }
    }
    code.match_indices(name).find_map(|(at, _)| {
        let before = code[..at].chars().next_back();
        let free = before.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '`'));
        let after = code[at + name.len()..].trim_start();
        free.then(|| after.strip_prefix('='))
            .flatten()
            .and_then(quoted)
    })
}

/// Arquivos que o testbench lê por nome relativo: `$readmemb/h("x")` e
/// `$fopen("x", "r...")`. Caminhos absolutos ficam de fora (o `asmcomp` usa
/// absoluto para `Simulation/input_<n>.txt`; esses são conferidos à parte).
fn data_files_read(testbench: &str) -> Vec<String> {
    let code = strip_comments(testbench);
    let mut names: Vec<String> = Vec::new();
    let reads = string_calls(&code, "$readmemb")
        .into_iter()
        .chain(string_calls(&code, "$readmemh"))
        .map(|(name, _)| name)
        .chain(
            string_calls(&code, "$fopen")
                .into_iter()
                .filter_map(|(name, rest)| {
                    let mode = rest
                        .trim_start_matches([',', ' ', '\t'])
                        .strip_prefix('"')?;
                    mode.starts_with('r').then_some(name)
                }),
        );
    for name in reads {
        let name = name.strip_prefix("./").unwrap_or(name);
        if name.is_empty() || crate::paths::is_rooted(name) || name.contains(['<', '>']) {
            continue;
        }
        names.push(name.to_owned());
    }
    names.sort();
    names.dedup();
    names
}

/// Os `input_<n>.txt` abertos por caminho absoluto no testbench gerado.
fn absolute_inputs(testbench: &str) -> Vec<Utf8PathBuf> {
    let code = strip_comments(testbench);
    string_calls(&code, "$fopen")
        .into_iter()
        .filter_map(|(name, rest)| {
            let mode = rest
                .trim_start_matches([',', ' ', '\t'])
                .strip_prefix('"')?;
            let path = Utf8Path::new(name);
            (mode.starts_with('r') && crate::paths::is_rooted(name)).then(|| path.to_owned())
        })
        .collect()
}

/// O testbench com a gravação de todos os sinais (`$dumpvars(0, ...)`,
/// inclusive os do módulo testado) em `wave`, antes do `endmodule` do módulo
/// `top` (comentário e texto entre aspas não contam). O bloco entra sem
/// quebrar linha, para as linhas do arquivo continuarem as mesmas.
fn with_default_dump(testbench: &str, top: &str, wave: &str) -> String {
    let block = format!(
        " /* Lace: dump padrão, o testbench não grava onda */ initial begin $dumpfile(\"{wave}\"); $dumpvars(0, {top}); end "
    );
    match endmodule_of(testbench, top) {
        Some(at) => format!("{}{block}{}", &testbench[..at], &testbench[at..]),
        None => format!("{testbench}\n{block}\n"),
    }
}

/// Onde começa o `endmodule` do módulo `top`, fora de comentário e de texto
/// entre aspas. Sem `module top`, o último `endmodule`.
fn endmodule_of(text: &str, top: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut words: Vec<(usize, &str)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b'\n' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            c if c.is_ascii_alphabetic() || c == b'_' || c == b'\\' => {
                let start = i;
                if c == b'\\' {
                    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                } else {
                    while i < bytes.len()
                        && (bytes[i].is_ascii_alphanumeric()
                            || bytes[i] == b'_'
                            || bytes[i] == b'$')
                    {
                        i += 1;
                    }
                }
                words.push((start, &text[start..i]));
            }
            _ => i += 1,
        }
    }
    let mut inside = false;
    for pair in words.windows(2) {
        let (_, word) = pair[0];
        if word == "module" && pair[1].1.trim_start_matches('\\') == top {
            inside = true;
        }
        if inside && pair[1].1 == "endmodule" {
            return Some(pair[1].0);
        }
    }
    words
        .iter()
        .rev()
        .find(|(_, w)| *w == "endmodule")
        .map(|(at, _)| *at)
}

/// Copia para a raiz (o CWD da simulação do projeto) os arquivos que o
/// testbench lê por nome relativo e que estão na pasta dele. Não escreve
/// fora da raiz, nem por cima de um arquivo que o usuário tem na raiz: só
/// sobrescreve uma cópia que o próprio Lace fez antes (anotada em
/// `<trabalho>/data_copies.txt`). Devolve os arquivos que faltam e os avisos
/// do que não foi copiado.
fn stage_data_files(
    text: &str,
    tb_dir: &Utf8Path,
    root: &Utf8Path,
    work: &Utf8Path,
) -> Result<(Vec<Utf8PathBuf>, Vec<Diagnostic>)> {
    let ledger = work.join("data_copies.txt");
    let mut copies: Vec<String> = std::fs::read_to_string(&ledger)
        .map(|t| t.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    let copies_before = copies.len();
    let mut missing = Vec::new();
    let mut notes = Vec::new();
    let note = |message: String, file: &Utf8Path| Diagnostic {
        tool: Tool::Vvp,
        severity: crate::diagnostics::Severity::Warning,
        raw: message.clone(),
        message,
        file: Some(file.to_owned()),
        line: None,
        column: None,
    };
    for name in data_files_read(text) {
        let from = crate::paths::normalize(&tb_dir.join(&name));
        let to = crate::paths::normalize(&root.join(&name));
        if !to.starts_with(root) {
            // `../dados/x.hex`: o Icarus lê relativo à raiz, fora do projeto.
            if !to.is_file() {
                missing.push(if from.is_file() { to } else { from });
            }
            continue;
        }
        if !from.is_file() {
            if !to.is_file() {
                missing.push(from);
            }
            continue;
        }
        if from == to {
            continue;
        }
        let relative = to
            .strip_prefix(root)
            .map(|p| p.as_str().replace('\\', "/"))
            .unwrap_or_default();
        let ours = copies.contains(&relative);
        if to.is_file() && !ours && !same_content(&from, &to) {
            notes.push(note(
                format!(
                    "{name} not copied from the testbench folder: the project root already has a different {name}, which the simulation reads"
                ),
                &to,
            ));
            continue;
        }
        copy(&from, &to)?;
        if !ours {
            copies.push(relative);
        }
    }
    if copies.len() != copies_before {
        std::fs::create_dir_all(work).map_err(LaceError::io("Creating directory", work))?;
        std::fs::write(&ledger, copies.join("\n") + "\n")
            .map_err(LaceError::io("Writing", &ledger))?;
    }
    Ok((missing, notes))
}

fn same_content(a: &Utf8Path, b: &Utf8Path) -> bool {
    match (std::fs::read(a), std::fs::read(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn output_files(dirs: &[Utf8PathBuf]) -> Vec<(Utf8PathBuf, Option<std::time::SystemTime>)> {
    let mut files = Vec::new();
    for dir in dirs {
        let Ok(entries) = dir.read_dir_utf8() else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.into_path();
            let is_output = path
                .file_name()
                .is_some_and(|n| n.starts_with("output_") && n.ends_with(".txt"));
            if is_output {
                let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                files.push((path, mtime));
            }
        }
    }
    files.sort();
    files
}

/// Os arquivos de entrada (`Simulation/input_<n>.txt`) que o testbench de um
/// processador vai ler e que ainda não existem. Serve para a interface avisar
/// antes de simular. A mesma lista sai em
/// [`SimulationResult::missing_inputs`] depois.
///
/// # Erros
///
/// [`LaceError::NotBuilt`] se o processador ainda não tem testbench.
pub fn missing_inputs(processor: &Processor) -> Result<Vec<Utf8PathBuf>> {
    let testbench = processor.simulated_testbench();
    if !testbench.is_file() {
        return Err(LaceError::NotBuilt {
            processor: processor.name.clone(),
            missing: testbench,
        });
    }
    Ok(absolute_inputs(&read(&testbench)?)
        .into_iter()
        .filter(|p| !p.is_file())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_paths_have_no_backslash_on_windows() {
        let path = make_path(Utf8Path::new("D:\\a\\_temp\\msys\\ucrt64\\bin/python.exe"));
        if cfg!(windows) {
            assert_eq!(path, "D:/a/_temp/msys/ucrt64/bin/python.exe");
        } else {
            assert_eq!(path, "D:\\a\\_temp\\msys\\ucrt64\\bin/python.exe");
        }
    }

    const TB: &str = r#"module soma_tb();
    data_in_0 = $fopen("/p/soma/Simulation/input_0.txt", "r"); // place your input data in this file
    data_out_0 = $fopen("/p/soma/Simulation/output_0.txt", "w");
    // $readmemb("comentado.txt", x);
    $readmemh("./tabela.hex", mem);
    $dumpfile("soma_tb.vcd");
endmodule
"#;

    #[test]
    fn finds_dump_and_relative_data_files() {
        assert_eq!(
            dump_of(TB, Utf8Path::new("/t")),
            Dump::Path(Utf8PathBuf::from("/t/soma_tb.vcd"))
        );
        assert_eq!(data_files_read(TB), ["tabela.hex"]);
        assert_eq!(
            absolute_inputs(TB),
            [Utf8PathBuf::from("/p/soma/Simulation/input_0.txt")]
        );
    }

    #[test]
    fn dumpfile_named_by_a_constant() {
        let r = Utf8Path::new("/r");
        let local = "module t; localparam ONDA = \"x.vcd\"; initial $dumpfile(ONDA); endmodule";
        assert_eq!(dump_of(local, r), Dump::Path("/r/x.vcd".into()));
        let define = "`define ONDA \"y.fst\"\nmodule t; initial $dumpfile(`ONDA); endmodule";
        assert_eq!(dump_of(define, r), Dump::Path("/r/y.fst".into()));
        let expr = "module t; initial $dumpfile({base, \".vcd\"}); endmodule";
        assert_eq!(dump_of(expr, r), Dump::Unknown);
        // Um nome maior que contém o outro não confunde.
        let longer = "module t; localparam XONDA = \"no.vcd\"; localparam ONDA = \"z.vcd\"; initial $dumpfile(ONDA); endmodule";
        assert_eq!(dump_of(longer, r), Dump::Path("/r/z.vcd".into()));
    }

    #[test]
    fn the_wave_extension_follows_the_simulator_format() {
        assert_eq!(with_extension("soma_tb.vcd", "fst"), "soma_tb.fst");
        assert_eq!(
            with_extension("ondas/x.dump.vcd", "fst"),
            "ondas/x.dump.fst"
        );
        assert_eq!(with_extension("sem_extensao", "fst"), "sem_extensao.fst");
        assert_eq!(with_extension("c:\\a.b\\onda", "vcd"), "c:\\a.b\\onda.vcd");
        let r = Utf8Path::new("/r");
        // O último `$dumpfile` de verdade: o comentado e o do texto ficam.
        let tb = "module t;\n// $dumpfile(\"velho.vcd\");\n  initial $display(\"$dumpfile(x)\");\n  initial $dumpfile( \"soma_tb.vcd\" );\nendmodule\n";
        let (wave, renamed) = named_by_format(tb, r.join("soma_tb.vcd"), Simulator::Icarus);
        let renamed = renamed.unwrap();
        assert_eq!(wave, r.join("soma_tb.fst"));
        assert_eq!(dump_of(&renamed, r), Dump::Path(r.join("soma_tb.fst")));
        assert!(
            renamed.contains("// $dumpfile(\"velho.vcd\");"),
            "{renamed}"
        );
        assert!(renamed.contains("$display(\"$dumpfile(x)\")"), "{renamed}");
        assert_eq!(renamed.lines().count(), tb.lines().count());
        // Pela constante: o nome resolvido entra no lugar dela.
        let local = "module t; localparam ONDA = \"x.vcd\"; initial $dumpfile(ONDA); endmodule";
        let (_, renamed) = named_by_format(local, r.join("x.vcd"), Simulator::Icarus);
        assert_eq!(dump_of(&renamed.unwrap(), r), Dump::Path(r.join("x.fst")));
        // O Verilator grava VCD: o `.fst` do testbench vira `.vcd`. Com a
        // extensão certa, nada muda.
        let fst = "module t; initial $dumpfile(\"y.fst\"); endmodule";
        let (wave, renamed) = named_by_format(fst, r.join("y.fst"), Simulator::Verilator);
        assert_eq!(wave, r.join("y.vcd"));
        assert_eq!(dump_of(&renamed.unwrap(), r), Dump::Path(r.join("y.vcd")));
        assert_eq!(
            named_by_format(fst, r.join("y.fst"), Simulator::Icarus),
            (r.join("y.fst"), None)
        );
        assert_eq!(
            named_by_format(tb, r.join("soma_tb.vcd"), Simulator::Verilator),
            (r.join("soma_tb.vcd"), None)
        );
    }

    #[test]
    fn injects_dump_before_last_endmodule() {
        let tb = "module a; endmodule\nmodule top_tb;\n  initial #10 $finish;\nendmodule\n";
        let out = with_default_dump(tb, "top_tb", "top_tb.fst");
        assert_eq!(dump_of(tb, Utf8Path::new("/r")), Dump::Absent);
        assert_eq!(
            dump_of(&out, Utf8Path::new("/r")),
            Dump::Path(Utf8PathBuf::from("/r/top_tb.fst"))
        );
        assert!(out.trim_end().ends_with("endmodule"));
        // Profundidade 0: todos os sinais, inclusive os do módulo testado.
        assert!(out.find("$dumpvars(0, top_tb)").unwrap() > out.find("module top_tb").unwrap());
        assert_eq!(injected_wave("t", Simulator::Icarus), "t.fst");
        assert_eq!(injected_wave("t", Simulator::Verilator), "t.vcd");
    }

    #[test]
    fn dump_goes_into_the_testbench_module_ignoring_comments() {
        // `endmodule` num comentário e outro módulo depois do testbench.
        let tb = "module ordem_tb;\n  gera g1();\n  initial $finish;\nendmodule // fim do endmodule\nmodule gera;\nendmodule\n";
        let out = with_default_dump(tb, "ordem_tb", "ordem_tb.fst");
        let dump = out.find("$dumpfile").unwrap();
        assert!(dump < out.find("module gera").unwrap(), "{out}");
        assert!(dump > out.find("initial $finish").unwrap(), "{out}");
        // As linhas continuam as mesmas: o bloco entra sem quebra de linha.
        assert_eq!(out.lines().count(), tb.lines().count());
        assert_eq!(
            endmodule_of("module a; // endmodule\n endmodule", "a"),
            Some(24)
        );
        assert_eq!(
            endmodule_of("/* endmodule */ module b; endmodule", "zz"),
            Some(26)
        );
    }

    #[test]
    fn data_files_are_staged_without_overwriting_or_leaving_the_root() {
        let guard = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(guard.path()).unwrap().to_owned();
        let tb_dir = root.join("tb");
        let work = root.join(".lace/Temp");
        std::fs::create_dir_all(&tb_dir).unwrap();
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(tb_dir.join("vetor.hex"), "aa bb\n").unwrap();
        std::fs::write(tb_dir.join("novo.hex"), "11\n").unwrap();
        std::fs::write(root.join("vetor.hex"), "00 00\n").unwrap();
        std::fs::write(root.join("data/outro.hex"), "ff\n").unwrap();
        let tb = r#"initial begin $readmemh("vetor.hex", v); $readmemh("novo.hex", n); $readmemh("../data/outro.hex", w); $readmemh("falta.hex", f); end"#;

        let (missing, notes) = stage_data_files(tb, &tb_dir, &root, &work).unwrap();
        // O vetor.hex do usuário na raiz fica como estava, com aviso.
        assert_eq!(
            std::fs::read_to_string(root.join("vetor.hex")).unwrap(),
            "00 00\n"
        );
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0].message.contains("vetor.hex not copied"),
            "{notes:?}"
        );
        // O novo.hex é copiado e anotado como cópia do Lace.
        assert_eq!(
            std::fs::read_to_string(root.join("novo.hex")).unwrap(),
            "11\n"
        );
        assert_eq!(
            std::fs::read_to_string(work.join("data_copies.txt")).unwrap(),
            "novo.hex\n"
        );
        // Nada fora da raiz: ../data/outro.hex não vira ../../data.
        assert!(!root.parent().unwrap().join("data/outro.hex").exists());
        assert_eq!(
            missing,
            [
                root.parent().unwrap().join("data/outro.hex"),
                tb_dir.join("falta.hex")
            ]
        );

        // Uma cópia do Lace pode ser atualizada; o arquivo do usuário, não.
        std::fs::write(tb_dir.join("novo.hex"), "22\n").unwrap();
        let (_, notes) = stage_data_files(tb, &tb_dir, &root, &work).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("novo.hex")).unwrap(),
            "22\n"
        );
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn short_inputs_get_a_warning_in_the_testbench_copy() {
        let tb = "integer data_in_0;\n\
                  initial begin\n    data_in_0 = $fopen(\"/p/le/Simulation/input_0.txt\", \"r\");\nend\n\
                  integer scan_result;\n\
                  always @ (negedge clk) begin  \n    \
                  if (data_in_0 != 0 && proc_req_in == 1) scan_result = $fscanf(data_in_0, \"%d\", in_0);\nend\n";
        let mut counts = HashMap::new();
        counts.insert(Utf8PathBuf::from("/p/le/Simulation/input_0.txt"), 2);
        let watched = watch_inputs(tb, &counts).unwrap();
        assert_eq!(
            watched.lines().count(),
            tb.lines().count(),
            "as linhas ficam"
        );
        assert!(watched.contains("integer scan_result; reg lace_eof_0 = 0;"));
        assert!(watched.contains("WARNING: /p/le/Simulation/input_0.txt:3: the program reads more values than this file has (2)"));
        assert!(watch_inputs("module tb; endmodule", &counts).is_none());
        // O aviso que o vvp escreve vira diagnóstico com o arquivo e a linha.
        let d = crate::diagnostics::parse(
            Tool::Vvp,
            "WARNING: /p/le/Simulation/input_0.txt:3: the program reads more values than this file has (2); from here on the port repeats the last one\n",
            "",
            None,
        );
        assert_eq!(d[0].severity, crate::diagnostics::Severity::Warning);
        assert_eq!(d[0].line, Some(3));
    }
}
