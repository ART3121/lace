//! Um comando por função. Cada uma abre o que precisa, chama o Core e passa o
//! resultado para `Output`. Devolvem `Ok(true)` se deu certo, `Ok(false)` se
//! a operação rodou e falhou, e `Err` se nem conseguiu rodar.

use std::time::Duration;

use anyhow::bail;
use clap::CommandFactory;
use serde::Serialize;
use solar_core::{
    BuildOptions, BuildResult, DesignTarget, FileRole, Language, NewProcessor, OnFailure,
    ProcessorConfig, Project, SchematicOptions, SimulationOptions, Simulator, ViewerOptions,
};

use crate::output::Output;
use crate::settings::{self, ConfigFile};
use crate::{BuildOverrides, Cli, Command, ConfigCommand, FileCommand, Lang, ProcCommand, Sim};

pub fn run(cli: &Cli, out: &Output) -> anyhow::Result<bool> {
    match &cli.command {
        Command::New { name, dir } => {
            let project = Project::create(dir, name)?;
            out.message("projeto criado", project.spf_path())?;
        }
        Command::Status => out.status(&open(cli)?)?,
        Command::Proc(command) => proc_command(cli, command, out)?,
        Command::File(command) => file_command(cli, command, out)?,
        Command::Input {
            processor,
            port,
            values,
            from,
        } => {
            let project = open(cli)?;
            // `--from` passa pelo mesmo leitor do Core: um arquivo com lixo é
            // recusado aqui, e não lido como zero pelo testbench.
            let values = match from {
                Some(file) => solar_core::read_data_file(file)?,
                None => values.clone(),
            };
            let path = project
                .require_processor(processor)?
                .write_input_values(*port, &values)?;
            out.message("entrada gravada", &path)?;
        }
        Command::Output { processor, port } => {
            let project = open(cli)?;
            let processor = project.require_processor(processor)?;
            if cli.json {
                let values = processor.read_output_values(*port)?;
                out.json(
                    &serde_json::json!({ "path": processor.output_path(*port), "values": values }),
                )?;
            } else {
                print!("{}", processor.read_output(*port)?);
            }
        }
        Command::Build(args) => {
            let toolchain = cli.toolchain.resolve()?;
            let project = open(cli)?;
            let processors = if args.processors.is_empty() {
                if project.processors().is_empty() {
                    bail!("o projeto {} não tem processadores", project.spf_path());
                }
                project.processors().iter().collect()
            } else {
                args.processors
                    .iter()
                    .map(|name| project.require_processor(name))
                    .collect::<Result<Vec<_>, _>>()?
            };
            let mut options = overrides(&args.overrides);
            options.show_arrays = args.show_arrays.then_some(true);
            let results = solar_core::build_processors(
                &toolchain,
                processors,
                &options,
                OnFailure::Continue,
                |r| out.build(r, project.root()),
            )?;
            #[derive(Serialize)]
            struct Report<'a> {
                project: &'a camino::Utf8Path,
                results: &'a [BuildResult],
            }
            out.json(&Report {
                project: project.spf_path(),
                results: &results,
            })?;
            return Ok(results.iter().all(BuildResult::succeeded));
        }
        Command::Sim(args) => return sim(cli, args, out),
        Command::Check => {
            let toolchain = cli.toolchain.resolve()?;
            let project = open(cli)?;
            let result = solar_core::check_syntax(&toolchain, &project)?;
            out.check(&result, project.root());
            out.json(&result)?;
            return Ok(result.succeeded());
        }
        Command::Synth(args) => return synth(cli, args, out),
        Command::Wave {
            waveform,
            view,
            wait,
        } => {
            let toolchain = cli.toolchain.resolve()?;
            let mut options = ViewerOptions::default();
            options.layout = view.clone();
            let mut surfer = solar_core::open_waveform(&toolchain, waveform, &options)?;
            surfer.ensure_started(SURFER_GRACE)?;
            out.opened(&surfer);
            out.json(&serde_json::json!({ "pid": surfer.id(), "log": surfer.log_file() }))?;
            if *wait {
                surfer.wait()?;
            }
        }
        Command::Tools { verify } => {
            let toolchain = cli.toolchain.resolve()?;
            let mismatches = if *verify {
                Some(toolchain.verify()?)
            } else {
                None
            };
            out.tools(&toolchain, mismatches.as_deref())?;
            if mismatches.is_some_and(|m| !m.is_empty()) {
                return Ok(false);
            }
        }
        Command::Config(command) => config_command(cli, command, out)?,
        Command::Completions { shell } => {
            clap_complete::generate(*shell, &mut Cli::command(), "solar", &mut std::io::stdout());
        }
    }
    Ok(true)
}

/// Quanto esperar para saber se o Surfer abriu: sem display ou com onda
/// inválida, ele fecha em menos de um segundo.
const SURFER_GRACE: Duration = Duration::from_millis(1500);

fn open(cli: &Cli) -> anyhow::Result<Project> {
    Ok(Project::open(&cli.project)?)
}

fn overrides(args: &BuildOverrides) -> BuildOptions {
    let mut options = BuildOptions::default();
    options.frequency_mhz = args.freq;
    options.clocks = args.clocks;
    options
}

fn proc_command(cli: &Cli, command: &ProcCommand, out: &Output) -> anyhow::Result<()> {
    let mut project = open(cli)?;
    match command {
        ProcCommand::Add(args) => {
            let language = match args.lang {
                Lang::Cmm => Language::Cmm,
                Lang::Cpp => Language::Cpp,
            };
            let mut spec = NewProcessor::new(&args.name, language);
            spec.input_ports = args.inputs;
            spec.output_ports = args.outputs;
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
            out.message("processador criado", &processor.source)?;
        }
        ProcCommand::Set {
            name,
            freq,
            clocks,
            show_arrays,
        } => {
            if freq.is_none() && clocks.is_none() && show_arrays.is_none() {
                bail!("nada para mudar: use --freq, --clocks ou --show-arrays");
            }
            let mut config = ProcessorConfig::default();
            config.frequency_mhz = *freq;
            config.clocks = *clocks;
            config.show_arrays = *show_arrays;
            let processor = project.configure_processor(name, &config)?.clone();
            if cli.json {
                out.json(&processor)?;
            } else {
                println!(
                    "{}: {} MHz, {} clocks{} (vale a partir do próximo build)",
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
        ProcCommand::List => out.processors(&project)?,
    }
    Ok(())
}

fn file_command(cli: &Cli, command: &FileCommand, out: &Output) -> anyhow::Result<()> {
    let role = |testbench: bool| {
        if testbench {
            FileRole::Testbench
        } else {
            FileRole::Synthesizable
        }
    };
    let mut project = open(cli)?;
    match command {
        FileCommand::Add {
            path,
            testbench,
            create,
        } => {
            let path = project.add_file(role(*testbench), path, create.then_some(""))?;
            out.message("arquivo registrado", &path)?;
        }
        FileCommand::Remove { path, testbench } => {
            if !project.remove_file(role(*testbench), path)? {
                bail!(
                    "{path} não está registrado como {}",
                    if *testbench {
                        "testbench"
                    } else {
                        "sintetizável"
                    }
                );
            }
            out.message("arquivo removido do projeto", path)?;
        }
        FileCommand::Top { path } => {
            project.set_top_level(path)?;
            out.message("módulo de topo", path)?;
        }
        FileCommand::Testbench { path } => {
            project.set_testbench(path)?;
            out.message("testbench do projeto", path)?;
        }
        FileCommand::List => out.files(&project)?,
    }
    Ok(())
}

fn sim(cli: &Cli, args: &crate::SimArgs, out: &Output) -> anyhow::Result<bool> {
    let toolchain = cli.toolchain.resolve()?;
    let project = open(cli)?;

    let targets = match &args.processor {
        Some(name) => vec![project.require_processor(name)?],
        None => project.buildable_processors(),
    };
    let builds = if args.no_build {
        Vec::new()
    } else {
        solar_core::build_processors(
            &toolchain,
            targets,
            &overrides(&args.overrides),
            OnFailure::Stop,
            |r| out.build(r, project.root()),
        )?
    };
    if !builds.iter().all(BuildResult::succeeded) {
        out.json(&serde_json::json!({ "builds": builds, "simulation": null }))?;
        return Ok(false);
    }

    let mut options = SimulationOptions::new(match args.simulator {
        Sim::Icarus => Simulator::Icarus,
        Sim::Verilator => Simulator::Verilator,
    });
    options.fst = !args.vcd;
    options.build_jobs = args.jobs;
    let result = match &args.processor {
        Some(name) => solar_core::simulate(&toolchain, project.require_processor(name)?, &options)?,
        None => solar_core::simulate_project(&toolchain, &project, &options)?,
    };
    out.simulation(&result, project.root());

    let mut surfer_pid = None;
    if args.open
        && let Some(wave) = &result.waveform
    {
        let mut surfer =
            solar_core::open_waveform(&toolchain, &wave.path, &ViewerOptions::default())?;
        surfer.ensure_started(SURFER_GRACE)?;
        out.opened(&surfer);
        surfer_pid = Some(surfer.id());
    }
    out.json(
        &serde_json::json!({ "builds": builds, "simulation": result, "surfer_pid": surfer_pid }),
    )?;
    Ok(result.succeeded())
}

fn synth(cli: &Cli, args: &crate::SynthArgs, out: &Output) -> anyhow::Result<bool> {
    let toolchain = cli.toolchain.resolve()?;
    let project = open(cli)?;
    let mut builds = Vec::new();
    let target = match &args.processor {
        Some(name) => {
            if !args.no_build {
                let processor = project.require_processor(name)?;
                builds = solar_core::build_processors(
                    &toolchain,
                    [processor],
                    &BuildOptions::default(),
                    OnFailure::Stop,
                    |r| out.build(r, project.root()),
                )?;
                if !builds.iter().all(BuildResult::succeeded) {
                    out.json(&serde_json::json!({ "builds": builds, "synthesis": null }))?;
                    return Ok(false);
                }
            }
            DesignTarget::Processor(name.clone())
        }
        None => DesignTarget::TopLevel,
    };
    let result = solar_core::synthesize(&toolchain, &project, &target)?;
    out.synthesis(&result, project.root());

    let mut schematic = None;
    if args.svg
        && let Some(netlist) = &result.netlist
    {
        let module = args.module.clone().unwrap_or_else(|| result.top.clone());
        let mut options = SchematicOptions::default();
        options.bus_widths = !args.no_widths;
        let svg = solar_core::render_schematic(&toolchain, netlist, &module, &options)?;
        out.schematic(&svg, project.root());
        schematic = Some(svg);
    }
    let ok = result.succeeded() && schematic.as_ref().is_none_or(|s| s.succeeded());
    out.json(
        &serde_json::json!({ "builds": builds, "synthesis": result, "schematic": schematic }),
    )?;
    Ok(ok)
}

fn config_command(cli: &Cli, command: &ConfigCommand, out: &Output) -> anyhow::Result<()> {
    let path = cli.toolchain.config_path()?;
    let existing = ConfigFile::load(&path)?;
    match command {
        ConfigCommand::Path => out.message("configuração", &path)?,
        ConfigCommand::Show => out.config(&path, existing.as_ref())?,
        ConfigCommand::SetCompiler { dir } => {
            let dir = settings::absolute(dir)?;
            let Some(found) = settings::compiler_in(&dir) else {
                bail!(
                    "{dir} não tem perl, make e um compilador C++{}",
                    if cfg!(windows) {
                        " (esperado: raiz do MSYS2 com usr/bin e ucrt64/bin ou mingw64/bin)"
                    } else {
                        ""
                    }
                );
            };
            let mut file = existing.unwrap_or_default();
            file.compiler_dir = Some(dir);
            file.save(&path)?;
            out.message(&format!("compilador do sistema: {}", found.cxx), &path)?;
        }
        ConfigCommand::UnsetCompiler => {
            let mut file = existing.unwrap_or_default();
            file.compiler_dir = None;
            file.save(&path)?;
            out.message("compilador do sistema: locais padrão", &path)?;
        }
    }
    Ok(())
}
