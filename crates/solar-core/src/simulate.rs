//! Simulação com Icarus Verilog ou Verilator, de um processador ou do projeto.
//!
//! Dois modos:
//!
//! - [`simulate`]: um processador já compilado, com o testbench que o
//!   `asmcomp` gerou (`<temp>/<nome>_tb.v`). Roda no diretório temporário do
//!   processador, onde já estão o `pc_<nome>_mem.txt` que o `.v` lê por nome
//!   relativo e onde o testbench grava a onda.
//! - [`simulate_project`]: o testbench escolhido no `.spf` com os arquivos
//!   sintetizáveis do projeto, como o botão Wave da AURORA
//!   (`js/compilation/icarus_da_onda.ts`, `verilator_da_onda.ts`). Roda na
//!   raiz do projeto, para onde são copiados os `pc_*_mem.txt` de todos os
//!   processadores e os arquivos que o testbench lê por nome relativo.
//!
//! Comandos, com `H` = biblioteca SAPHO (`HDL/`):
//!
//! ```text
//! Icarus:    iverilog -y H -s <topo> -o <trabalho>/<topo>.vvp <arquivos...> <testbench>
//!            vvp <trabalho>/<topo>.vvp [-fst]
//! Verilator: verilator --binary --main --trace -j 0 ... --top-module <topo>
//!                      -Mdir <trabalho>/obj_dir_<topo> -y H <arquivos...> <testbench>
//!            <trabalho>/obj_dir_<topo>/V<topo>
//! ```
//!
//! A biblioteca entra por `-y`: o simulador só carrega o módulo que faltar,
//! então `myFIFO.v` só entra quando alguém o instancia. As memórias (`.mif`)
//! e os arquivos de `Simulation/` são abertos por caminho absoluto, embutido
//! pelo `asmcomp`, e não precisam ser copiados.
//!
//! A onda sai com o nome do `$dumpfile` do testbench. Com `-fst` (Icarus), o
//! conteúdo é FST mesmo que o nome termine em `.vcd`; o Surfer identifica pelo
//! conteúdo. O Verilator grava VCD: o FST dele exige lz4 e zlib do sistema,
//! fora da exceção do compilador.

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::error::{Result, SolarError};
use crate::pipeline::{
    Artifact, ArtifactKind, ArtifactTracker, PlannedStep, Runner, Status, Step, StepReport,
    final_status,
};
use crate::project::{Processor, Project};
use crate::source::strip_comments;
use crate::toolchain::{Tool, Toolchain};

/// Qual simulador usar. Em JSON: `"icarus"` ou `"verilator"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// O formato do conteúdo de uma onda. Pode não bater com a extensão: o
/// testbench gerado sempre chama a onda de `.vcd`, e com FST o conteúdo é
/// FST mesmo assim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Só Icarus: onda em FST (menor e mais rápida de abrir) em vez de VCD. É
    /// o padrão, como na AURORA. O Verilator do bundle sempre grava VCD: o
    /// FST dele exige lz4 e zlib do sistema.
    pub fst: bool,
    /// Só Verilator: processos paralelos na compilação C++ do modelo. `None`
    /// usa todos os núcleos (`-j 0`), como a AURORA.
    pub build_jobs: Option<u32>,
}

impl SimulationOptions {
    /// `simulator` com onda FST e `-j 0` no Verilator.
    ///
    /// ```
    /// use solar_core::{SimulationOptions, Simulator};
    ///
    /// let mut options = SimulationOptions::new(Simulator::Verilator);
    /// options.build_jobs = Some(4);
    /// options.fst = false; // VCD
    /// ```
    pub fn new(simulator: Simulator) -> Self {
        SimulationOptions {
            simulator,
            fst: true,
            build_jobs: None,
        }
    }
}

/// O resultado de [`simulate`] e de [`simulate_project`].
///
/// Uma simulação que roda até o `$finish` e termina com código 0 é
/// `Succeeded`, mesmo que o circuito esteja errado: conferir o comportamento
/// é olhar `outputs` e a onda. O stdout do testbench (`$display`) está no
/// `stdout` do passo `simulate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    pub outputs: Vec<Utf8PathBuf>,
    /// Arquivos que o testbench tenta ler e que não existem. Não impedem a
    /// simulação (o testbench gerado pelo `asmcomp` confere o `$fopen` antes
    /// de ler), mas a porta correspondente não recebe dado nenhum.
    pub missing_inputs: Vec<Utf8PathBuf>,
}

impl SimulationResult {
    /// `status == Succeeded`.
    pub fn succeeded(&self) -> bool {
        self.status == Status::Succeeded
    }
}

/// Uma onda gerada por simulação, pronta para
/// [`open_waveform`](crate::open_waveform).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Waveform {
    /// Caminho absoluto, com o nome que o `$dumpfile` do testbench deu.
    pub path: Utf8PathBuf,
    /// O formato do conteúdo.
    pub format: WaveformFormat,
}

/// Simula `processor` com o testbench gerado pelo `asmcomp`. O processador
/// precisa ter sido compilado antes com [`build`](crate::build); a duração
/// da simulação (clocks) e a frequência já estão no testbench gerado.
///
/// Roda no diretório temporário do processador. A onda sai em
/// `<temp>/<nome>_tb.vcd` e as saídas em `Simulation/output_<n>.txt`.
///
/// ```no_run
/// use solar_core::{BuildOptions, Project, SimulationOptions, Simulator, Toolchain, build, simulate};
/// # let toolchain = Toolchain::open("/opt/solar/toolchain")?;
/// let project = Project::open("/p/soma")?;
/// let soma = project.require_processor("soma")?;
/// soma.write_input(0, "3\n4\n5\n")?;
/// build(&toolchain, soma, &BuildOptions::default())?;
/// let result = simulate(&toolchain, soma, &SimulationOptions::new(Simulator::Icarus))?;
/// if result.succeeded() {
///     println!("saída 0: {}", soma.read_output(0)?);
/// }
/// # Ok::<(), solar_core::SolarError>(())
/// ```
///
/// # Erros
///
/// Falha de elaboração ou de simulação volta como `Ok` com `status = Failed`.
/// `Err` quando:
///
/// - [`SolarError::NotBuilt`]: falta o testbench ou o Verilog (rode
///   [`build`](crate::build) antes);
/// - [`SolarError::ComponentMissing`] / [`SolarError::ToolchainIncomplete`]:
///   falta o simulador;
/// - [`SolarError::Spawn`] / [`SolarError::Io`]: o simulador não pôde ser
///   executado, ou um arquivo não pôde ser lido.
pub fn simulate(
    toolchain: &Toolchain,
    processor: &Processor,
    options: &SimulationOptions,
) -> Result<SimulationResult> {
    let _span = tracing::info_span!("simulate", processor = %processor.name).entered();
    let name = &processor.name;
    let testbench = processor.temp_dir.join(format!("{name}_tb.v"));
    let verilog = processor.hardware_dir().join(format!("{name}.v"));
    for path in [&testbench, &verilog] {
        if !path.is_file() {
            return Err(SolarError::NotBuilt {
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

    let plan = Plan {
        missing_inputs,
        top: format!("{name}_tb"),
        wave: dump_path(&text, &processor.temp_dir)
            .unwrap_or_else(|| processor.temp_dir.join(format!("{name}_tb.vcd"))),
        files: vec![verilog, testbench],
        cwd: processor.temp_dir.clone(),
        work: processor.temp_dir.clone(),
        output_dirs: vec![processor.simulation_dir()],
        sapho: true,
    };
    execute(toolchain, &plan, options)
}

/// Simula o projeto: o testbench do `.spf` ([`Project::testbench`]) com os
/// arquivos sintetizáveis e o Verilog de cada processador compilado.
///
/// O módulo de topo é o nome do arquivo do testbench sem extensão. A
/// simulação roda na raiz do projeto: a onda sai lá, com o nome do
/// `$dumpfile`, e o `.vvp` ou o `obj_dir` ficam em
/// [`Project::temp_dir`]. Antes de rodar, o Solar copia para a raiz:
///
/// - o `pc_<nome>_mem.txt` de cada processador compilado, que o `.v` dele lê
///   por nome relativo;
/// - os arquivos que o testbench lê por nome relativo (`$readmemb/h("x")`,
///   `$fopen("x", "r")`), a partir da pasta do testbench.
///
/// Se o testbench não grava onda (`$dumpfile`), o Solar simula uma cópia com
/// `$dumpfile("<topo>.vcd"); $dumpvars(1, <topo>);` antes do último
/// `endmodule`, como a AURORA faz quando não há seleção de sinais. O arquivo
/// do usuário não é alterado.
///
/// Os processadores não são compilados aqui: chame [`build`](crate::build)
/// para cada um antes, como o botão Wave da AURORA.
///
/// # Erros
///
/// - [`SolarError::NoTestbench`]: o projeto não tem testbench;
/// - [`SolarError::InvalidProject`]: o testbench ou um arquivo sintetizável
///   registrado não existe;
/// - os mesmos de [`simulate`] para ferramenta e I/O.
pub fn simulate_project(
    toolchain: &Toolchain,
    project: &Project,
    options: &SimulationOptions,
) -> Result<SimulationResult> {
    let testbench = project
        .testbench()
        .ok_or_else(|| SolarError::NoTestbench(project.spf_path().to_owned()))?;
    if !testbench.is_file() {
        return Err(SolarError::InvalidProject {
            path: testbench,
            reason: "o testbench do projeto não existe".into(),
        });
    }
    let top = testbench.file_stem().unwrap_or_default().to_owned();
    let _span = tracing::info_span!("simulate_project", %top).entered();
    let root = project.root().to_owned();
    let work = project.temp_dir();
    std::fs::create_dir_all(&work).map_err(SolarError::io("criando diretório", &work))?;

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
            return Err(SolarError::InvalidProject {
                path: file.path,
                reason: "arquivo sintetizável registrado no projeto não existe".into(),
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
            copy(&map, &root.join(map.file_name().expect("tem nome")))?;
        }
    }

    // Dados que o testbench lê por nome relativo vêm da pasta dele.
    let text = read(&testbench)?;
    let tb_dir = testbench.parent().expect("arquivo tem pasta").to_owned();
    let mut missing_inputs = Vec::new();
    for name in data_files_read(&text) {
        let from = tb_dir.join(&name);
        let to = root.join(&name);
        if from.is_file() {
            if from != to {
                copy(&from, &to)?;
            }
        } else if !to.is_file() {
            missing_inputs.push(from);
        }
    }
    missing_inputs.extend(absolute_inputs(&text).into_iter().filter(|p| !p.is_file()));

    let (testbench, wave) = match dump_path(&text, &root) {
        Some(wave) => (testbench, wave),
        None => {
            let instrumented = work.join(format!(
                "instr_{}",
                testbench.file_name().expect("tem nome")
            ));
            let text = with_default_dump(&text, &top);
            std::fs::write(&instrumented, text)
                .map_err(SolarError::io("gravando testbench", &instrumented))?;
            (instrumented, root.join(format!("{top}.vcd")))
        }
    };
    files.push(testbench);

    let plan = Plan {
        missing_inputs,
        top,
        wave,
        files,
        cwd: root,
        work,
        output_dirs: project
            .processors()
            .iter()
            .map(Processor::simulation_dir)
            .collect(),
        sapho: !project.processors().is_empty(),
    };
    execute(toolchain, &plan, options)
}

struct Plan {
    missing_inputs: Vec<Utf8PathBuf>,
    top: String,
    /// Onde a onda vai aparecer.
    wave: Utf8PathBuf,
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
}

fn execute(
    toolchain: &Toolchain,
    plan: &Plan,
    options: &SimulationOptions,
) -> Result<SimulationResult> {
    let top = &plan.top;
    let hdl = toolchain.sapho_library(plan.sapho)?;
    let mut tracker = ArtifactTracker::new();
    let mut runner = Runner::new();
    let outputs_before = output_files(&plan.output_dirs);

    match options.simulator {
        Simulator::Icarus => {
            let image = plan.work.join(format!("{top}.vvp"));
            tracker.expect(ArtifactKind::IcarusImage, &image, true);
            tracker.expect(ArtifactKind::Waveform, &plan.wave, true);

            let mut elaborate = toolchain.invocation(Tool::Iverilog, &plan.cwd)?;
            if let Some(hdl) = &hdl {
                elaborate = elaborate.arg("-y").path_arg(hdl);
            }
            let mut elaborate = elaborate.arg("-s").arg(top).arg("-o").path_arg(&image);
            for file in &plan.files {
                elaborate = elaborate.path_arg(file);
            }
            if runner.run(PlannedStep::new(Step::Elaborate, Tool::Iverilog, elaborate))? {
                let mut run = toolchain.invocation(Tool::Vvp, &plan.cwd)?.path_arg(&image);
                if options.fst {
                    run = run.arg("-fst");
                }
                runner.run(PlannedStep::new(Step::Simulate, Tool::Vvp, run))?;
            }
        }
        Simulator::Verilator => {
            let obj_dir = plan.work.join(format!("obj_dir_{top}"));
            let model = obj_dir.join(format!("V{top}{}", std::env::consts::EXE_SUFFIX));
            tracker.expect(ArtifactKind::VerilatedModel, &model, true);
            tracker.expect(ArtifactKind::Waveform, &plan.wave, true);

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
                // VCD sempre: o FST do Verilator 5.053 do bundle compila contra
                // lz4 e zlib do sistema, que ficam fora da exceção do
                // compilador (decisão do autor: só compilador, make e Perl).
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
            // O PATH (make e compilador do sistema, python3 do bundle), o
            // VERILATOR_ROOT e o LC_ALL=C vêm de Toolchain::invocation.
            if runner.run(PlannedStep::new(Step::Verilate, Tool::Verilator, verilate))? {
                let run = crate::process::Invocation::new(model.clone(), &plan.cwd)
                    .search_path(&toolchain.verilated_model_path());
                runner.run(PlannedStep::new(Step::Simulate, Tool::Verilator, run))?;
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
    let format = match options.simulator {
        Simulator::Icarus if options.fst => WaveformFormat::Fst,
        _ => WaveformFormat::Vcd,
    };
    let waveform = (status == Status::Succeeded).then(|| Waveform {
        path: plan.wave.clone(),
        format,
    });
    Ok(SimulationResult {
        top: top.clone(),
        simulator: options.simulator,
        status,
        failed_step: runner.failed_step,
        steps: runner.steps,
        diagnostics: runner.diagnostics,
        artifacts,
        waveform,
        outputs,
        missing_inputs: plan.missing_inputs.clone(),
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
    std::fs::read_to_string(path).map_err(SolarError::io("lendo testbench", path))
}

fn copy(from: &Utf8Path, to: &Utf8Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(SolarError::io("criando diretório", parent))?;
    }
    std::fs::copy(from, to)
        .map(drop)
        .map_err(SolarError::io("copiando para a simulação", to))
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

/// Onde a onda vai parar: o `$dumpfile` do testbench, relativo ao CWD.
fn dump_path(testbench: &str, cwd: &Utf8Path) -> Option<Utf8PathBuf> {
    let code = strip_comments(testbench);
    let (name, _) = string_calls(&code, "$dumpfile").into_iter().last()?;
    let path = Utf8Path::new(name);
    Some(if crate::paths::is_rooted(name) {
        path.to_owned()
    } else {
        cwd.join(path)
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

/// Testbench com um bloco de dump antes do último `endmodule`.
fn with_default_dump(testbench: &str, top: &str) -> String {
    let block = format!(
        "\n  // Solar: dump padrão, o testbench não grava onda\n  initial begin\n    $dumpfile(\"{top}.vcd\");\n    $dumpvars(1, {top});\n  end\n"
    );
    match testbench.rfind("endmodule") {
        Some(at) => format!("{}{block}{}", &testbench[..at], &testbench[at..]),
        None => format!("{testbench}{block}"),
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
/// [`SolarError::NotBuilt`] se o processador ainda não tem testbench.
pub fn missing_inputs(processor: &Processor) -> Result<Vec<Utf8PathBuf>> {
    let testbench = processor.temp_dir.join(format!("{}_tb.v", processor.name));
    if !testbench.is_file() {
        return Err(SolarError::NotBuilt {
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
        let path = make_path(Utf8Path::new("D:\\a\\_temp\\oss\\lib/python3.exe"));
        if cfg!(windows) {
            assert_eq!(path, "D:/a/_temp/oss/lib/python3.exe");
        } else {
            assert_eq!(path, "D:\\a\\_temp\\oss\\lib/python3.exe");
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
            dump_path(TB, Utf8Path::new("/t")),
            Some(Utf8PathBuf::from("/t/soma_tb.vcd"))
        );
        assert_eq!(data_files_read(TB), ["tabela.hex"]);
        assert_eq!(
            absolute_inputs(TB),
            [Utf8PathBuf::from("/p/soma/Simulation/input_0.txt")]
        );
    }

    #[test]
    fn injects_dump_before_last_endmodule() {
        let tb = "module a; endmodule\nmodule top_tb;\n  initial #10 $finish;\nendmodule\n";
        let out = with_default_dump(tb, "top_tb");
        assert!(dump_path(tb, Utf8Path::new("/r")).is_none());
        assert_eq!(
            dump_path(&out, Utf8Path::new("/r")),
            Some(Utf8PathBuf::from("/r/top_tb.vcd"))
        );
        assert!(out.trim_end().ends_with("endmodule"));
        assert!(out.find("$dumpvars(1, top_tb)").unwrap() > out.find("module top_tb").unwrap());
    }
}
