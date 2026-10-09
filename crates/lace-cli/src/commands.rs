//! Um comando por função. Cada uma abre o que precisa, chama o Core e passa o
//! resultado para `Output`. Devolvem `Ok(true)` se deu certo, `Ok(false)` se
//! a operação rodou e falhou, e `Err` se nem conseguiu rodar.
//!
//! Caminhos de arquivo da linha de comando são relativos ao diretório atual
//! do shell e vão absolutos para o Core. Relativos, o Core os resolveria a
//! partir da raiz do projeto, e com `-C` seria outro arquivo.

use std::io::{BufRead, IsTerminal, Write};
use std::time::{Duration, SystemTime};

use anyhow::bail;
use camino::{Utf8Path, Utf8PathBuf};
use clap::CommandFactory;
use lace_core::history::{self, Cleanup, Operation};
use lace_core::{
    BuildOptions, BuildResult, CheckOptions, Control, DesignTarget, HierarchyOptions, LaceError,
    Language, ListPosition, NewProcessor, OnFailure, PreparedLayout, Processor, ProcessorConfig,
    Project, RunningProcess, SchematicOptions, SimulationOptions, SimulationResult, Simulator,
    Tool, Toolchain, ViewerOptions,
};

use crate::output::Output;
use crate::report::{
    AddReport, BuildReport, CheckReport, MoveReport, OrderReport, PortValues, ReportCleanReport,
    ReportListReport, ReportShowReport, SimReport, SynthReport, WaveReport,
};
use crate::settings;
use crate::{
    BuildArgs, CheckArgs, Cli, Command, HierarchyArgs, Lang, ProcCommand, ReportArgs,
    ReportCommand, SimArgs, SynthArgs, WaveArgs,
};

/// `control` cancela as operações (Ctrl+C) e mostra a saída das ferramentas
/// enquanto elas rodam (`Output::control`).
pub fn run(cli: &Cli, out: &Output, control: &Control) -> anyhow::Result<bool> {
    match &cli.command {
        Command::New { name, dir } => {
            let project = Project::create(settings::absolute(dir)?, name)?;
            out.created(&project)?;
        }
        Command::Status => {
            let project = open(cli)?;
            out.status(&project, processor_here(cli, &project)?)?
        }
        Command::Add { files, tb } => add(cli, files, *tb, out)?,
        Command::Remove { files } => remove(cli, files, out)?,
        Command::Top { target } => top(cli, target.as_deref(), out)?,
        Command::Move { sources, dest } => move_paths(cli, sources, dest, out)?,
        Command::Order {
            file,
            first,
            last,
            before,
            after,
        } => {
            let position = match (before, after) {
                (Some(other), _) => ListPosition::Before(settings::absolute(other)?),
                (_, Some(other)) => ListPosition::After(settings::absolute(other)?),
                _ if *first => ListPosition::First,
                _ => {
                    debug_assert!(*last);
                    ListPosition::Last
                }
            };
            let mut project = open(cli)?;
            let path = settings::absolute(file)?;
            let files = project.reorder_file(&path, &position)?;
            let moved = files
                .iter()
                .find(|f| f.path.file_name() == path.file_name())
                .map_or(path.clone(), |f| f.path.clone());
            out.ordered(&files, &moved, project.root());
            out.json(&OrderReport {
                project: project.spf_path().to_owned(),
                files,
            })?;
        }
        Command::Proc(command) => proc_command(cli, command, out)?,
        Command::Build(args) => return build(cli, args, out, control),
        Command::Check(args) => return check(cli, args, out, control),
        Command::Hierarchy(args) => return hierarchy(cli, args, out, control),
        Command::Sim(args) => return sim(cli, args, out, control),
        Command::Wave(args) => wave(cli, args, out)?,
        Command::Synth(args) => return synth(cli, args, out, control),
        Command::Report(args) => report(cli, args, out)?,
        Command::Learn(args) => return crate::learn::run(cli, args, out, control),
        Command::Fpga(command) => return crate::fpga::run(cli, command, out, control),
        Command::Tools { verify } => return tools(cli, *verify, out),
        Command::Install(args) => {
            let from = args.from.as_deref().map(settings::absolute).transpose()?;
            crate::install::run(out, &args.components, from.as_deref())?;
        }
        Command::Update { check, yes } => {
            crate::update::run(out, cli.toolchain.resolve().ok().as_ref(), *check, *yes)?
        }
        Command::Uninstall { yes } => crate::uninstall::run(out, *yes)?,
        Command::Completions { shell } => {
            clap_complete::generate(*shell, &mut Cli::command(), "lace", &mut std::io::stdout());
        }
    }
    Ok(true)
}

/// Quanto esperar para saber se o Surfer abriu: sem display ou com onda
/// inválida, ele fecha em menos de um segundo.
const SURFER_GRACE: Duration = Duration::from_millis(1500);

/// O projeto da pasta atual (ou do `-C`): o `.spf` dela ou da primeira
/// pasta acima que tiver um. Assim todo comando funciona de dentro do
/// projeto, não só da raiz.
fn open(cli: &Cli) -> anyhow::Result<Project> {
    Ok(Project::discover(&cli.project)?)
}

/// O processador em cuja pasta o comando foi rodado (ou o `-C`): o que
/// `build`, `check`, `sim`, `wave`, `synth` e `proc set` usam quando não
/// recebem um nome. Fora da pasta de um processador, nenhum.
fn processor_here<'a>(cli: &Cli, project: &'a Project) -> anyhow::Result<Option<&'a Processor>> {
    Ok(project.processor_at(settings::absolute(&cli.project)?))
}

/// `path` como se digitaria a partir do diretório atual: relativo quando
/// está dentro dele.
pub(crate) fn from_shell(path: &Utf8Path) -> Utf8PathBuf {
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|dir| Utf8PathBuf::from_path_buf(dir).ok());
    match cwd.as_deref().and_then(|cwd| path.strip_prefix(cwd).ok()) {
        Some(rel) if rel.as_str().is_empty() => ".".into(),
        Some(rel) => rel.to_owned(),
        None => path.to_owned(),
    }
}

fn add(cli: &Cli, files: &[Utf8PathBuf], testbench: bool, out: &Output) -> anyhow::Result<()> {
    let mut project = open(cli)?;
    // Sem bundle o arquivo é criado do mesmo jeito: o Core lê as portas do
    // módulo testado com o leitor embutido em vez do Yosys.
    let toolchain = cli
        .toolchain
        .resolve()
        .inspect_err(|e| tracing::info!("No bundle, reading ports with the built-in parser: {e:#}"))
        .ok();
    // Confere todos antes de registrar o primeiro: um nome ruim no meio da
    // lista não deixa metade registrada.
    let files = files
        .iter()
        .map(|file| settings::absolute(file))
        .collect::<anyhow::Result<Vec<_>>>()?;
    for file in &files {
        project.check_add_verilog(file)?;
    }
    let mut added = Vec::new();
    for file in files {
        let file = project.add_verilog(toolchain.as_ref(), file, testbench)?;
        out.added(&file, project.root());
        added.push(file);
    }
    out.json(&AddReport {
        project: project.spf_path().to_owned(),
        files: added,
    })
}

fn remove(cli: &Cli, files: &[Utf8PathBuf], out: &Output) -> anyhow::Result<()> {
    let mut project = open(cli)?;
    let mut removed = Vec::new();
    let mut unknown = Vec::new();
    for file in files {
        let path = settings::absolute(file)?;
        if project.remove_verilog(&path)? {
            removed.push((file.as_path(), path));
        } else {
            unknown.push((file.as_path(), path));
        }
    }
    if removed.is_empty() {
        let list = unknown
            .iter()
            .map(|(typed, _)| typed.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if unknown.len() == 1 {
            bail!("{list} is not in the project");
        }
        bail!("None of these files are in the project: {list}");
    }
    out.removed(project.spf_path(), &removed, &unknown)
}

fn top(cli: &Cli, target: Option<&str>, out: &Output) -> anyhow::Result<()> {
    let mut project = open(cli)?;
    if let Some(target) = target {
        // Um arquivo que existe a partir do diretório atual vai absoluto; o
        // resto vai como está, e o Core tenta como caminho a partir da raiz
        // e depois como nome de módulo.
        let path = settings::absolute(Utf8Path::new(target))?;
        let target = if path.is_file() {
            path.as_str()
        } else {
            target
        };
        project.set_top(target)?;
    }
    out.top(&project)
}

/// `lace move`, com os argumentos do `mv`: para uma pasta que existe (ou um
/// destino terminado em `/`, criado se faltar), cada origem vai para dentro
/// dela; senão, a única origem passa a ter o caminho do destino. O resto (o
/// que fica no lugar, o `.spf`) é do Core.
fn move_paths(
    cli: &Cli,
    sources: &[Utf8PathBuf],
    dest: &Utf8Path,
    out: &Output,
) -> anyhow::Result<()> {
    let mut project = open(cli)?;
    let folder = dest.as_str().ends_with(['/', '\\']);
    let dest = settings::absolute(dest)?;
    let into = folder || dest.is_dir();
    if !into && sources.len() > 1 {
        bail!(
            "{} is not a folder: with more than one source, the destination must be an existing folder",
            from_shell(&dest)
        );
    }
    let mut moved = Vec::new();
    for source in sources {
        let from = settings::absolute(source)?;
        let to = match from.file_name() {
            Some(name) if into => dest.join(name),
            _ => dest.clone(),
        };
        let result = project.move_path(&from, &to)?;
        out.moved(&result, project.root());
        moved.push(result);
    }
    out.json(&MoveReport {
        project: project.spf_path().to_owned(),
        moved,
    })
}

fn proc_command(cli: &Cli, command: &ProcCommand, out: &Output) -> anyhow::Result<()> {
    let mut project = open(cli)?;
    match command {
        ProcCommand::Add(args) => {
            let language = match args.lang {
                Lang::Cmm => Language::Cmm,
                Lang::Cpp => Language::Cpp,
            };
            let cmm_only = [
                ("--nubits", args.nubits),
                ("--nbmant", args.nbmant),
                ("--nbexpo", args.nbexpo),
                ("--nugain", args.nugain),
                ("--ndstac", args.ndstac),
                ("--sdepth", args.sdepth),
            ];
            if language == Language::Cpp
                && let Some((flag, _)) = cmm_only.iter().find(|(_, v)| v.is_some())
            {
                bail!(
                    "{flag} only applies to C± (--lang cmm): a C processor uses the cppcomp defaults"
                );
            }
            let mut spec = NewProcessor::new(&args.name, language);
            spec.input_ports = args.inputs;
            spec.output_ports = args.outputs;
            // Só a mantissa ou o expoente mudou: a palavra acompanha, que é
            // a única que o asmcomp aceita (#NUBITS = #NBMANT + #NBEXPO + 1).
            if args.nubits.is_none() && (args.nbmant.is_some() || args.nbexpo.is_some()) {
                let nbmant = args.nbmant.unwrap_or(spec.nbmant);
                let nbexpo = args.nbexpo.unwrap_or(spec.nbexpo);
                spec.nubits = nbmant.saturating_add(nbexpo).saturating_add(1);
            }
            for (value, field) in [
                (args.nubits, &mut spec.nubits),
                (args.nbmant, &mut spec.nbmant),
                (args.nbexpo, &mut spec.nbexpo),
                (args.nugain, &mut spec.nugain),
                (args.ndstac, &mut spec.ndstac),
                (args.sdepth, &mut spec.sdepth),
            ] {
                if let Some(value) = value {
                    *field = value;
                }
            }
            let processor = project.add_processor(&spec)?;
            out.message("Processor created", &processor.source);
            out.json(processor)?;
        }
        ProcCommand::Set {
            name,
            freq,
            clocks,
            arrays,
        } => {
            if freq.is_none() && clocks.is_none() && arrays.is_none() {
                bail!("Nothing to change: use --freq, --clocks or --arrays");
            }
            let name = match name {
                Some(name) => name.clone(),
                None => match processor_here(cli, &project)? {
                    Some(here) => here.name.clone(),
                    None => bail!(
                        "Which processor? Name it (lace proc set <NAME> ...) or run inside its folder"
                    ),
                },
            };
            let mut config = ProcessorConfig::default();
            config.frequency_mhz = *freq;
            config.clocks = *clocks;
            config.show_arrays = *arrays;
            let processor = project.configure_processor(&name, &config)?.clone();
            if !out.is_text() {
                out.json(&processor)?;
            } else {
                println!(
                    "{}: {} MHz, {} clocks{} (applies from the next build)",
                    processor.name,
                    processor.frequency_mhz,
                    processor.clocks,
                    if processor.show_arrays {
                        ", arrays"
                    } else {
                        ""
                    }
                );
            }
        }
    }
    Ok(())
}

fn build(cli: &Cli, args: &BuildArgs, out: &Output, control: &Control) -> anyhow::Result<bool> {
    let started = SystemTime::now();
    let project = open(cli)?;
    // Um projeto só de Verilog não tem o que compilar, e isso não é erro.
    if args.processors.is_empty() && project.processors().is_empty() {
        out.no_processors();
        out.json(&BuildReport {
            project: project.spf_path().to_owned(),
            results: Vec::new(),
            report: None,
        })?;
        return Ok(true);
    }
    // Sem nomes, dentro da pasta de um processador compila ele; no resto do
    // projeto, todos.
    let processors = if !args.processors.is_empty() {
        args.processors
            .iter()
            .map(|name| project.require_processor(name))
            .collect::<Result<Vec<_>, _>>()?
    } else if let Some(here) = processor_here(cli, &project)? {
        vec![here]
    } else {
        project.processors().iter().collect()
    };
    let toolchain = cli.toolchain.resolve()?;
    let results = lace_core::build_processors(
        &toolchain,
        processors,
        &BuildOptions::default(),
        OnFailure::Continue,
        control,
        |r| out.build(r, project.root()),
    )?;
    let ok = results.iter().all(BuildResult::succeeded);
    let operation = Operation::new(command_line(), started).with_builds(&results);
    let report = record(&project, &toolchain, &operation, out);
    out.json(&BuildReport {
        project: project.spf_path().to_owned(),
        results,
        report,
    })?;
    Ok(ok)
}

/// Compila os processadores antes de verificar, simular ou sintetizar, e
/// para no primeiro que falhar: com um processador quebrado, o resto não
/// serve. Sem processadores, não faz nada.
pub(crate) fn build_first<'a>(
    toolchain: &Toolchain,
    project: &Project,
    processors: impl IntoIterator<Item = &'a Processor>,
    out: &Output,
    control: &Control,
) -> anyhow::Result<Vec<BuildResult>> {
    Ok(lace_core::build_processors(
        toolchain,
        processors,
        &BuildOptions::default(),
        OnFailure::Stop,
        control,
        |r| out.build(r, project.root()),
    )?)
}

fn check(cli: &Cli, args: &CheckArgs, out: &Output, control: &Control) -> anyhow::Result<bool> {
    let started = SystemTime::now();
    let project = open(cli)?;
    let toolchain = cli.toolchain.resolve()?;
    // Sem arquivo nem `-p`, dentro da pasta de um processador verifica ele;
    // no resto do projeto, o projeto.
    let processor = match (&args.processor, &args.file) {
        (Some(name), _) => Some(project.require_processor(name)?),
        (None, None) => processor_here(cli, &project)?,
        (None, Some(_)) => None,
    };
    // Só o Icarus (e o Verilator, com --lint) sobre o que está no disco: o
    // check não recompila os processadores.
    let mut options = CheckOptions::default();
    options.file = args.file.as_deref().map(settings::absolute).transpose()?;
    options.lint = args.lint;
    options.processor = processor.map(|p| p.name.clone());
    let result = lace_core::check(&toolchain, &project, &options, control)?;
    out.check(&result, args.lint, project.root());
    if args.lint
        && !result.steps.iter().any(|s| s.tool == Tool::Verilator)
        && let Err(LaceError::ComponentMissing(component)) = toolchain.tool(Tool::Verilator)
    {
        out.next(&format!(
            "Install the linter with: lace install {component}"
        ));
    }
    let ok = result.succeeded();
    let operation = Operation::new(command_line(), started).with_check(&result);
    let report = record(&project, &toolchain, &operation, out);
    out.json(&CheckReport {
        check: result,
        report,
    })?;
    Ok(ok)
}

/// `lace hierarchy`. Não compila os processadores nem grava relatório: é a
/// hierarquia do que está no disco (`lace_core::hierarchy`).
fn hierarchy(
    cli: &Cli,
    args: &HierarchyArgs,
    out: &Output,
    control: &Control,
) -> anyhow::Result<bool> {
    let project = open(cli)?;
    let toolchain = cli.toolchain.resolve()?;
    let processor = match &args.processor {
        Some(name) => Some(project.require_processor(name)?),
        None => processor_here(cli, &project)?,
    };
    let mut options = HierarchyOptions::default();
    options.processor = processor.map(|p| p.name.clone());
    let result = lace_core::hierarchy(&toolchain, &project, &options, control)?;
    // Os que ficaram de fora por não terem sido compilados.
    let not_built: Vec<&str> = match processor {
        Some(_) => Vec::new(),
        None => project
            .buildable_processors()
            .into_iter()
            .filter(|p| !p.is_built())
            .map(|p| p.name.as_str())
            .collect(),
    };
    out.hierarchy(&result, &not_built, project.root());
    out.json(&result)?;
    Ok(result.succeeded())
}

fn sim(cli: &Cli, args: &SimArgs, out: &Output, control: &Control) -> anyhow::Result<bool> {
    let started = SystemTime::now();
    let mut project = open(cli)?;
    let toolchain = cli.toolchain.resolve()?;
    if let Some(testbench) = &args.testbench {
        project.set_testbench(settings::absolute(testbench)?)?;
    }

    // Sem `-p` nem testbench, dentro da pasta de um processador simula
    // ele; no resto do projeto, a simulação do projeto.
    let processor = match (&args.processor, &args.testbench) {
        (Some(name), _) => Some(project.require_processor(name)?),
        (None, None) => processor_here(cli, &project)?,
        (None, Some(_)) => None,
    };
    let targets = match processor {
        Some(processor) => vec![processor],
        None => project.buildable_processors(),
    };
    let builds = build_first(&toolchain, &project, targets, out, control)?;
    if !builds.iter().all(BuildResult::succeeded) {
        out.not_run(
            if args.fast {
                "Fast simulation"
            } else {
                "Simulation"
            },
            &builds,
        );
        let operation = Operation::new(command_line(), started).with_builds(&builds);
        let report = record(&project, &toolchain, &operation, out);
        out.json(&SimReport {
            builds,
            simulation: None,
            outputs: Vec::new(),
            surfer_pid: None,
            report,
        })?;
        return Ok(false);
    }

    let mut options = SimulationOptions::new(if args.verilator {
        Simulator::Verilator
    } else {
        Simulator::Icarus
    });
    options.timeout = args.timeout.map(Duration::from_secs);
    options.fast = args.fast;
    let result = match processor {
        Some(processor) => lace_core::simulate(&toolchain, processor, &options, control)?,
        None => lace_core::simulate_project(&toolchain, &project, &options, control)?,
    };
    let outputs = match processor {
        Some(processor) if result.succeeded() => port_values(processor, &result),
        _ => Vec::new(),
    };
    out.simulation(
        &result,
        &outputs,
        project.root(),
        args.timeout.map(Duration::from_secs),
    );
    if result.waveform.is_some() && !args.open {
        out.next(&match processor {
            Some(p) => format!("Open the waveform with: lace wave -p {}", p.name),
            None => "Open the waveform with: lace wave".to_owned(),
        });
    }
    let operation = Operation::new(command_line(), started)
        .with_builds(&builds)
        .with_simulation(&result);
    let report = record(&project, &toolchain, &operation, out);

    let mut surfer_pid = None;
    if args.open
        && let Some(wave) = &result.waveform
    {
        let (mut surfer, layout) = open_with_layout(&toolchain, &wave.path, true)?;
        surfer.ensure_started(SURFER_GRACE)?;
        out.opened(&surfer, layout.as_ref());
        surfer_pid = Some(surfer.id());
    }
    let ok = result.succeeded();
    out.json(&SimReport {
        builds,
        simulation: Some(result),
        outputs,
        surfer_pid,
        report,
    })?;
    Ok(ok)
}

/// Os valores de cada porta de saída que a simulação do processador
/// escreveu, na ordem das portas. A porta sai do nome do arquivo
/// (`output_<porta>.txt`), porque o `Processor` não guarda o `#NUIOOU`.
/// Uma porta que não dá para ler (um `x` do simulador numa divisão por
/// zero) vem com o motivo em `error`, sem derrubar o resto: o resumo, as
/// outras portas e o relatório continuam.
fn port_values(processor: &Processor, result: &SimulationResult) -> Vec<PortValues> {
    let dir = processor.simulation_dir();
    let mut ports: Vec<u32> = result
        .outputs
        .iter()
        .filter(|path| path.parent() == Some(dir.as_path()))
        .filter_map(|path| {
            path.file_name()?
                .strip_prefix("output_")?
                .strip_suffix(".txt")?
                .parse()
                .ok()
        })
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
        .into_iter()
        .map(|port| {
            let (values, error) = match processor.read_output_values(port) {
                Ok(values) => (values, None),
                Err(error) => (Vec::new(), Some(error.to_string())),
            };
            PortValues {
                port,
                path: processor.output_path(port),
                values,
                error,
            }
        })
        .collect()
}

fn wave(cli: &Cli, args: &WaveArgs, out: &Output) -> anyhow::Result<()> {
    let waveform = match &args.waveform {
        Some(path) => settings::absolute(path)?,
        None => {
            let project = open(cli)?;
            let processor = match &args.processor {
                Some(name) => Some(project.require_processor(name)?),
                None => processor_here(cli, &project)?,
            };
            let path = lace_core::waveform_path(&project, processor)?;
            if !path.is_file() {
                let sim = match processor {
                    Some(p) => format!("lace sim -p {}", p.name),
                    None => "lace sim".to_owned(),
                };
                bail!(
                    "Waveform {} does not exist yet; simulate first with: {sim}",
                    from_shell(&path)
                );
            }
            path
        }
    };
    let toolchain = cli.toolchain.resolve()?;
    let (mut surfer, layout) = open_with_layout(&toolchain, &waveform, !args.no_layout)?;
    surfer.ensure_started(SURFER_GRACE)?;
    out.opened(&surfer, layout.as_ref());
    out.json(&WaveReport {
        pid: surfer.id(),
        log: surfer.log_file().to_owned(),
        layout: layout.as_ref().map(|l| l.state.clone()),
        processors: layout.map(|l| l.layout.processors).unwrap_or_default(),
        waveform,
    })
}

/// Abre a onda no Surfer, com o layout dos processadores SAPHO quando
/// `layout` e a onda tem processador.
fn open_with_layout(
    toolchain: &Toolchain,
    waveform: &Utf8Path,
    layout: bool,
) -> anyhow::Result<(RunningProcess, Option<PreparedLayout>)> {
    let prepared = if layout {
        lace_core::prepare_wave_layout(waveform)?
    } else {
        None
    };
    let options = prepared
        .as_ref()
        .map(ViewerOptions::with_layout)
        .unwrap_or_default();
    Ok((
        lace_core::open_waveform(toolchain, waveform, &options)?,
        prepared,
    ))
}

fn synth(cli: &Cli, args: &SynthArgs, out: &Output, control: &Control) -> anyhow::Result<bool> {
    let started = SystemTime::now();
    let project = open(cli)?;
    let toolchain = cli.toolchain.resolve()?;
    // O topo do projeto sintetiza com o Verilog de todos os processadores;
    // com -p, ou dentro da pasta de um processador, só o daquele.
    let processor = match &args.processor {
        Some(name) => Some(project.require_processor(name)?),
        None => processor_here(cli, &project)?,
    };
    let (target, processors) = match processor {
        Some(processor) => (
            DesignTarget::Processor(processor.name.clone()),
            vec![processor],
        ),
        None => (DesignTarget::TopLevel, project.buildable_processors()),
    };
    let builds = build_first(&toolchain, &project, processors, out, control)?;
    if !builds.iter().all(BuildResult::succeeded) {
        out.not_run("Synthesis", &builds);
        let operation = Operation::new(command_line(), started).with_builds(&builds);
        let report = record(&project, &toolchain, &operation, out);
        out.json(&SynthReport {
            builds,
            synthesis: None,
            schematic: None,
            report,
        })?;
        return Ok(false);
    }

    let result = lace_core::synthesize(&toolchain, &project, &target, control)?;
    out.synthesis(&result, project.root());

    let mut schematic = None;
    // Um módulo fora do netlist não desfaz a síntese: o relatório dela é
    // gravado, e o erro vem depois.
    let mut refused = None;
    if args.no_schematic_limit {
        tracing::debug!("--no-schematic-limit is ignored: the schematic has no connection limit");
    }
    if args.svg
        && let Some(netlist) = &result.netlist
    {
        let module = args.module.as_deref().unwrap_or(&result.top);
        let options = SchematicOptions::default();
        match lace_core::render_schematic(&toolchain, netlist, module, &options, control) {
            Ok(svg) => {
                out.schematic(&svg, project.root());
                schematic = Some(svg);
            }
            Err(error @ LaceError::ModuleNotFound { .. }) => refused = Some(error),
            Err(error) => return Err(error.into()),
        }
    }
    let ok = result.succeeded() && schematic.as_ref().is_none_or(|s| s.succeeded());
    let mut operation = Operation::new(command_line(), started)
        .with_builds(&builds)
        .with_synthesis(&result);
    if let Some(svg) = &schematic {
        operation = operation.with_schematic(svg);
    }
    let report = record(&project, &toolchain, &operation, out);
    if let Some(error) = refused {
        return Err(error.into());
    }
    out.json(&SynthReport {
        builds,
        synthesis: Some(result),
        schematic,
        report,
    })?;
    Ok(ok)
}

/// Grava a operação no histórico do projeto, para `lace report`. Uma falha
/// ao gravar não muda o resultado do comando: vira um aviso.
fn record(
    project: &Project,
    toolchain: &Toolchain,
    operation: &Operation,
    out: &Output,
) -> Option<String> {
    match history::record(project, toolchain, operation) {
        Ok(record) => {
            out.recorded(&record.id);
            Some(record.id)
        }
        Err(error) => {
            out.report_not_saved(&error);
            None
        }
    }
}

/// A linha de comando como o usuário a escreveu, para o relatório.
fn command_line() -> String {
    std::iter::once("lace".to_owned())
        .chain(std::env::args().skip(1).map(|arg| {
            if arg.is_empty() || arg.contains(char::is_whitespace) {
                format!("\"{arg}\"")
            } else {
                arg
            }
        }))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `lace report`: mostra, lista e compara os relatórios guardados. Só lê o
/// histórico; nenhuma ferramenta roda.
fn report(cli: &Cli, args: &ReportArgs, out: &Output) -> anyhow::Result<()> {
    let project = open(cli)?;
    match &args.command {
        None => show_report(&project, None, out),
        Some(ReportCommand::Show { id }) => show_report(&project, Some(id), out),
        Some(ReportCommand::List { limit }) => {
            let mut reports = history::list(&project)?;
            if let Some(limit) = limit {
                reports.truncate(usize::try_from(*limit).unwrap_or(usize::MAX));
            }
            out.report_list(&reports);
            out.json(&ReportListReport { reports })
        }
        Some(ReportCommand::Compare {
            id,
            against,
            summary,
        }) => {
            let comparison = history::compare_reports(&project, id.as_deref(), against.as_deref())?;
            out.report_comparison(&comparison, *summary);
            out.json(&comparison)
        }
        Some(ReportCommand::Clean { ids, keep, yes }) => {
            clean_reports(&project, ids, *keep, *yes, out)
        }
    }
}

/// `lace report clean`. Apagar todos, ou todos menos os mais novos, pede
/// confirmação; os relatórios pedidos pelo nome saem sem perguntar.
fn clean_reports(
    project: &Project,
    ids: &[String],
    keep: Option<u64>,
    yes: bool,
    out: &Output,
) -> anyhow::Result<()> {
    let cleanup = match keep {
        _ if !ids.is_empty() => Cleanup::Reports(ids.to_vec()),
        Some(n) => Cleanup::KeepLatest(usize::try_from(n).unwrap_or(usize::MAX)),
        None => Cleanup::All,
    };
    let targets = history::plan_cleanup(project, &cleanup)?;
    if !targets.is_empty() && ids.is_empty() && !yes {
        confirm_cleanup(project, targets.len(), keep)?;
    }
    let removed = history::remove(project, &targets)?;
    let kept = history::list(project)?.len();
    out.reports_removed(&removed, kept);
    out.json(&ReportCleanReport { removed, kept })
}

fn confirm_cleanup(project: &Project, count: usize, keep: Option<u64>) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("No terminal to confirm: use lace report clean --yes");
    }
    let reports = if count == 1 { "report" } else { "reports" };
    let what = match keep {
        Some(1) => format!("{count} {reports}, keeping the newest,"),
        Some(n) if n > 1 => format!("{count} {reports}, keeping the {n} newest,"),
        _ if count == 1 => "the only report".to_owned(),
        _ => format!("all {count} reports"),
    };
    let dir = project.root().join(history::REPORTS_DIR);
    eprint!("Remove {what} from {dir}? [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        bail!("Nothing was removed")
    }
}

fn show_report(project: &Project, id: Option<&str>, out: &Output) -> anyhow::Result<()> {
    let record = match id {
        Some(id) => history::load(project, id)?,
        None => history::latest(project)?,
    };
    let text = history::report_text(project, &record.id)?;
    out.report_text(&text);
    out.json(&ReportShowReport {
        id: record.id.clone(),
        path: history::report_path(project, &record.id)?,
        record,
        text,
    })
}

fn tools(cli: &Cli, verify: bool, out: &Output) -> anyhow::Result<bool> {
    let toolchain = cli.toolchain.resolve()?;
    let mismatches = if verify {
        Some(toolchain.verify()?)
    } else {
        None
    };
    out.tools(&toolchain, mismatches.as_deref())?;
    Ok(mismatches.is_none_or(|m| m.is_empty()))
}
