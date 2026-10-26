//! Um comando por função. Cada uma resolve o que a linha de comando diz (os
//! caminhos relativos ao shell, o processador da pasta atual), pede ao
//! despacho do `lace serve` no mesmo processo ([`Local`]) e passa a resposta
//! para `Output`, que escreve o texto ou o relatório do `--json`. Devolvem
//! `Ok(true)` se deu certo, `Ok(false)` se a operação rodou e falhou, e
//! `Err` se nem conseguiu rodar.
//!
//! Caminhos de arquivo da linha de comando são relativos ao diretório atual
//! do shell e vão absolutos no pedido. Relativos, o Core os resolveria a
//! partir da raiz do projeto, e com `-C` seria outro arquivo.

use std::io::{BufRead, IsTerminal, Write};
use std::time::{Duration, SystemTime};

use anyhow::bail;
use camino::{Utf8Path, Utf8PathBuf};
use clap::CommandFactory;
use lace_core::flows::{
    self, FlowOptions, FlowOutcome, FlowRequest, LayoutPolicy, Phase, Progress, WaveRequest,
    WaveSource,
};
use lace_core::history::{self, Cleanup};
use lace_core::protocol::Request;
use lace_core::{
    Control, Language, ListPosition, NewProcessorFields, Processor, ProcessorConfig, Project,
    Simulator,
};

use crate::local::{Local, answer};
use crate::output::Output;
use crate::report::{
    AddReport, BuildReport, CheckReport, ConvertReport, MoveReport, OrderReport, PortValues,
    ProcRemoveReport, ReportCleanReport, ReportListReport, ReportShowReport, SavedLayoutReport,
    SimReport, SynthReport, WaveReport,
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
            let request = Request::ProjectCreate {
                parent: settings::absolute(dir)?,
                name: name.clone(),
            };
            let status = answer!(Local::new(cli).call(request)?, Status);
            out.created(&status)?;
        }
        Command::Status => status(cli, out)?,
        Command::Add { files, tb } => add(cli, files, *tb, out)?,
        Command::Remove { files } => remove(cli, files, out)?,
        Command::Top { target } => top(cli, target.as_deref(), out)?,
        Command::Move {
            after,
            sources,
            dest,
        } => move_paths(cli, sources, dest, *after, out)?,
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
            let project = open(cli)?;
            let path = settings::absolute(file)?;
            let request = Request::ProjectOrder {
                project: project.spf_path().to_owned(),
                file: path.clone(),
                position,
            };
            let files = answer!(Local::new(cli).call(request)?, Files);
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
        Command::Convert => {
            let project = open(cli)?;
            let request = Request::ProjectConvert {
                project: project.spf_path().to_owned(),
            };
            let conversion = answer!(Local::new(cli).call(request)?, Converted);
            out.converted(&conversion, project.root());
            out.json(&ConvertReport {
                project: project.spf_path().to_owned(),
                conversion,
            })?;
        }
        Command::Build(args) => return build(cli, args, out, control),
        Command::Check(args) => return check(cli, args, out, control),
        Command::Hierarchy(args) => return hierarchy(cli, args, out, control),
        Command::Sim(args) => return sim(cli, args, out, control),
        Command::Wave(args) => return wave(cli, args, out, control),
        Command::Synth(args) => return synth(cli, args, out, control),
        Command::Report(args) => report(cli, args, out)?,
        Command::Learn(args) => return crate::learn::run(cli, args, out, control),
        Command::Fpga(command) => return crate::fpga::run(cli, command, out, control),
        Command::Experimental(args) => crate::experimental::run(cli, args, out)?,
        Command::Tools { verify } => return tools(cli, *verify, out),
        Command::Serve(args) => {
            return crate::serve::run(&cli.project, &cli.toolchain, args, control.cancel_token());
        }
        // O assistente de IA (o lace-agent) é experimental: desligado, nem
        // o chat nem o servidor MCP abrem.
        Command::Chat(args) => {
            lace_core::experimental::require(lace_core::experimental::Feature::Assistant)?;
            return crate::chat::run(&cli.project, &cli.toolchain, args, control.cancel_token());
        }
        Command::Mcp(args) => {
            lace_core::experimental::require(lace_core::experimental::Feature::Assistant)?;
            return crate::mcp::run(&cli.project, &cli.toolchain, args, control.cancel_token());
        }
        Command::Install(args) => {
            let from = args.from.as_deref().map(settings::absolute).transpose()?;
            crate::install::run(
                out,
                &args.components,
                from.as_deref(),
                control.cancel_token(),
            )?;
        }
        Command::Update {
            check,
            yes,
            full,
            from,
            reopen_studio,
        } => {
            let from = from.as_deref().map(settings::absolute).transpose()?;
            crate::update::run(
                out,
                cli.toolchain.resolve().ok().as_ref(),
                crate::update::Options {
                    check: *check,
                    yes: *yes,
                    full: *full,
                    from: from.as_deref(),
                    reopen_studio: *reopen_studio,
                },
                control.cancel_token(),
            )?
        }
        Command::Setup {
            payload,
            components,
            prefix,
        } => crate::install::setup(
            &settings::absolute(payload)?,
            components,
            &settings::absolute(prefix)?,
        )?,
        Command::Uninstall { yes } => crate::uninstall::run(out, *yes)?,
        Command::Completions { shell } => {
            clap_complete::generate(*shell, &mut Cli::command(), "lace", &mut std::io::stdout());
        }
    }
    Ok(true)
}

/// O projeto da pasta atual (ou do `-C`): o `.spf` dela ou da primeira
/// pasta acima que tiver um. Assim todo comando funciona de dentro do
/// projeto, não só da raiz. A CLI só o lê, para montar o pedido (o `.spf`, o
/// processador da pasta); quem muda o projeto é o despacho.
pub(crate) fn open(cli: &Cli) -> anyhow::Result<Project> {
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

fn status(cli: &Cli, out: &Output) -> anyhow::Result<()> {
    let project = open(cli)?;
    let here = processor_here(cli, &project)?.map(|p| p.name.clone());
    let request = Request::ProjectStatus {
        project: project.spf_path().to_owned(),
    };
    let status = answer!(Local::new(cli).call(request)?, Status);
    out.status(&status, here.as_deref())
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
    let files = files
        .iter()
        .map(|file| settings::absolute(file))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let added = project.add_verilog_files(toolchain.as_ref(), &files, testbench)?;
    for file in &added {
        out.added(file, project.root());
    }
    out.json(&AddReport {
        project: project.spf_path().to_owned(),
        files: added,
    })
}

fn remove(cli: &Cli, files: &[Utf8PathBuf], out: &Output) -> anyhow::Result<()> {
    let project = open(cli)?;
    let typed = files
        .iter()
        .map(|file| Ok((file.as_path(), settings::absolute(file)?)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let request = Request::ProjectRemove {
        project: project.spf_path().to_owned(),
        files: typed.iter().map(|(_, path)| path.clone()).collect(),
    };
    let outcome = answer!(Local::new(cli).call(request)?, Removed);
    // O despacho responde na ordem do pedido: cada arquivo, como foi
    // digitado, vai para a lista em que caiu (o mesmo arquivo duas vezes sai
    // na primeira e não está mais na segunda).
    let mut taken = outcome.removed.iter().peekable();
    let mut removed = Vec::new();
    let mut unknown = Vec::new();
    for (typed, path) in typed {
        if taken.peek() == Some(&&path) {
            taken.next();
            removed.push((typed, path));
        } else {
            unknown.push((typed, path));
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
    let project = open(cli)?;
    // Um arquivo que existe a partir do diretório atual vai absoluto; o
    // resto vai como está, e o Core tenta como caminho a partir da raiz e
    // depois como nome de módulo.
    let target = match target {
        Some(target) => {
            let path = settings::absolute(Utf8Path::new(target))?;
            Some(if path.is_file() {
                path.into_string()
            } else {
                target.to_owned()
            })
        }
        None => None,
    };
    let request = Request::ProjectTop {
        project: project.spf_path().to_owned(),
        target,
    };
    let top = answer!(Local::new(cli).call(request)?, Top);
    out.top(&top, project.root())
}

/// `lace move`, com os argumentos do `mv`: para uma pasta que existe (ou um
/// destino terminado em `/`, criado se faltar), cada origem vai para dentro
/// dela; senão, a única origem passa a ter o caminho do destino. O resto (o
/// que fica no lugar, o `.spf`) é do Core.
///
/// Com `--after` o movimento já aconteceu, e uma pasta no destino pode ser a
/// própria origem renomeada (`old` para `new`): ela só conta como pasta de
/// destino se terminar em `/`, se houver mais de uma origem ou se a origem
/// já estiver dentro dela.
fn move_paths(
    cli: &Cli,
    sources: &[Utf8PathBuf],
    dest: &Utf8Path,
    after: bool,
    out: &Output,
) -> anyhow::Result<()> {
    let project = open(cli)?;
    let folder = dest.as_str().ends_with(['/', '\\']);
    let dest = settings::absolute(dest)?;
    let into = folder
        || if after {
            dest.is_dir()
                && (sources.len() > 1
                    || sources
                        .iter()
                        .filter_map(|s| s.file_name())
                        .all(|name| dest.join(name).exists()))
        } else {
            dest.is_dir()
        };
    if !into && sources.len() > 1 {
        bail!(
            "{} is not a folder: with more than one source, the destination must be an existing folder",
            from_shell(&dest)
        );
    }
    let local = Local::new(cli);
    let mut moved = Vec::new();
    for source in sources {
        let from = settings::absolute(source)?;
        let to = match from.file_name() {
            Some(name) if into => dest.join(name),
            _ => dest.clone(),
        };
        let request = Request::ProjectMove {
            project: project.spf_path().to_owned(),
            from,
            to,
            after,
            overwrite: false,
        };
        let result = answer!(local.call(request)?, Moved);
        out.moved(&result, project.root(), after);
        moved.push(result);
    }
    out.json(&MoveReport {
        project: project.spf_path().to_owned(),
        moved,
    })
}

fn proc_command(cli: &Cli, command: &ProcCommand, out: &Output) -> anyhow::Result<()> {
    let project = open(cli)?;
    let spf = project.spf_path().to_owned();
    let local = Local::new(cli);
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
            // O que não veio fica com o padrão do Core: só a mantissa ou o
            // expoente mudou, a palavra acompanha (#NUBITS = #NBMANT +
            // #NBEXPO + 1, a única que o asmcomp aceita); sem #NDSTAC e
            // #SDEPTH, o YANC calcula as pilhas pelo programa.
            let spec = NewProcessorFields {
                nubits: args.nubits,
                nbmant: args.nbmant,
                nbexpo: args.nbexpo,
                nugain: args.nugain,
                ndstac: args.ndstac,
                sdepth: args.sdepth,
                input_ports: Some(args.inputs),
                output_ports: Some(args.outputs),
                ..NewProcessorFields::new(&args.name, language)
            };
            let request = Request::ProcessorAdd {
                project: spf,
                spec: spec.into(),
            };
            let processor = answer!(local.call(request)?, Processor);
            out.message("Processor created", &processor.source);
            out.json(&*processor)?;
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
            let request = Request::ProcessorSet {
                project: spf,
                name,
                config,
            };
            let processor = answer!(local.call(request)?, Processor);
            if !out.is_text() {
                out.json(&*processor)?;
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
        ProcCommand::Remove { name, delete, yes } => {
            // Com --delete, a pergunta vem antes de mexer no projeto.
            let dir = project.require_processor(name)?.dir.clone();
            if *delete && !*yes && dir.exists() {
                confirm_trash(name, &dir)?;
            }
            let request = Request::ProcessorRemove {
                project: spf,
                name: name.clone(),
                to_trash: *delete,
            };
            let removal = answer!(local.call(request)?, ProcessorRemoved);
            out.processor_removed(&removal.removed, removal.trashed, project.root());
            out.json(&ProcRemoveReport {
                removed: removal.removed,
                trashed: removal.trashed,
            })?;
        }
    }
    Ok(())
}

/// `lace proc remove --delete` sem `--yes`: a pasta do processador só vai
/// para a lixeira com um sim no terminal.
fn confirm_trash(name: &str, dir: &Utf8Path) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("No terminal to confirm: use lace proc remove {name} --delete --yes");
    }
    eprint!(
        "Remove processor {name} from the project and move its folder {dir} to the trash? [y/N] "
    );
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        bail!("Nothing was removed")
    }
}

/// A saída da CLI para o que um fluxo diz enquanto roda
/// ([`lace_core::flows::Progress`]), na ordem em que acontece: é o que cada
/// comando imprimia entre uma chamada e outra do Core.
pub(crate) struct Printer<'a> {
    out: &'a Output,
    root: &'a Utf8Path,
    /// `--lint`, para o resumo da verificação.
    lint: bool,
    /// `--fast`: a fase que não rodou se chama "Fast simulation".
    fast: bool,
    /// O prazo da simulação, para o resumo.
    timeout: Option<Duration>,
    /// A dica para abrir a onda, impressa depois do resumo da simulação
    /// quando ela tem onda e não se pediu `--open`.
    wave_hint: Option<String>,
}

impl<'a> Printer<'a> {
    pub(crate) fn new(out: &'a Output, root: &'a Utf8Path) -> Self {
        Printer {
            out,
            root,
            lint: false,
            fast: false,
            timeout: None,
            wave_hint: None,
        }
    }

    fn on(&self, progress: Progress<'_>) {
        let out = self.out;
        match progress {
            Progress::Phase(_) => {}
            Progress::Built(result) => out.build(result, self.root),
            Progress::Registered(files) => {
                for file in files {
                    out.added(file, self.root);
                }
            }
            Progress::RegisterFailed(error) => out.warning(&format!(
                "The generated files could not be added to the project: {error}"
            )),
            Progress::NotRun { what, builds } => {
                out.not_run(not_run_name(what, self.fast), builds);
            }
            Progress::Checked(result) => out.check(result, self.lint, self.root),
            Progress::Elaborated { result, not_built } => {
                let names: Vec<&str> = not_built.iter().map(String::as_str).collect();
                out.hierarchy(result, &names, self.root);
            }
            Progress::Simulated { result, outputs } => {
                out.simulation(result, &port_values(outputs), self.root, self.timeout);
                if result.waveform.is_some()
                    && let Some(hint) = &self.wave_hint
                {
                    out.next(hint);
                }
            }
            Progress::Synthesized(result) => out.synthesis(result, self.root),
            Progress::Drawn(result) => out.schematic(result, self.root),
            Progress::FpgaBuilt(result) => out.fpga_build(result, self.root),
            Progress::Programmed(result) => out.fpga_program(result, self.root),
            Progress::Recorded(id) => out.recorded(id),
            Progress::ReportNotSaved(error) => out.report_not_saved(error),
            Progress::WaveOpened(opened) => out.opened(opened),
            Progress::RtlViewer { path, opener } => out.rtl_viewer(path, opener),
        }
    }
}

/// O nome da fase que não rodou, como a CLI sempre o escreveu.
fn not_run_name(phase: Phase, fast: bool) -> &'static str {
    match phase {
        Phase::Simulate if fast => "Fast simulation",
        Phase::Simulate => "Simulation",
        Phase::Synthesize => "Synthesis",
        Phase::Fpga => "FPGA build",
        _ => "Operation",
    }
}

/// Os valores das portas como o `sim.json` sempre os escreveu: o motivo de
/// uma porta ilegível em texto.
pub(crate) fn port_values(outputs: &[flows::PortValues]) -> Vec<PortValues> {
    outputs
        .iter()
        .map(|o| PortValues {
            port: o.port,
            path: o.path.clone(),
            values: o.values.clone(),
            error: o.error.as_ref().map(|e| e.message.clone()),
        })
        .collect()
}

/// Roda um fluxo pelo despacho, com a saída da CLI: o `started` tomado
/// antes de abrir o projeto, a linha de comando digitada no histórico, a
/// onda em janela e o RTL viewer no navegador (ou só gravado, com
/// `LACE_OPENER`). Um processador que não existe é erro antes do bundle.
pub(crate) fn run_flow(
    cli: &Cli,
    project: &Project,
    request: FlowRequest,
    started: SystemTime,
    printer: &Printer<'_>,
    control: &Control,
) -> anyhow::Result<FlowOutcome> {
    let mut options = FlowOptions::new(command_line());
    options.started = Some(started);
    options.viewer_opener = std::env::var(OPENER_ENV).ok().filter(|v| !v.is_empty());
    let request = Request::FlowRun {
        project: project.spf_path().to_owned(),
        request,
        options: Some(options),
        output: true,
        step_output_limit: None,
    };
    let response = Local::new(cli).run(request, control, &|progress| printer.on(progress))?;
    Ok(*answer!(response, Flow))
}

fn build(cli: &Cli, args: &BuildArgs, out: &Output, control: &Control) -> anyhow::Result<bool> {
    let started = SystemTime::now();
    let project = open(cli)?;
    // Um projeto só de Verilog não tem o que compilar, e isso não é erro;
    // nem o bundle é preciso.
    let nothing = args.processors.is_empty() && project.processors().is_empty();
    // Sem nomes, dentro da pasta de um processador compila ele; no resto do
    // projeto, todos.
    let processors = if !args.processors.is_empty() {
        args.processors.clone()
    } else if let Some(here) = processor_here(cli, &project)? {
        vec![here.name.clone()]
    } else {
        Vec::new()
    };
    let printer = Printer::new(out, project.root());
    let request = FlowRequest::Build { processors };
    let outcome = run_flow(cli, &project, request, started, &printer, control)?;
    if nothing {
        out.no_processors();
    }
    out.json(&BuildReport {
        project: project.spf_path().to_owned(),
        results: outcome.builds,
        registered: outcome.registered,
        report: outcome.report,
    })?;
    Ok(outcome.succeeded)
}

fn check(cli: &Cli, args: &CheckArgs, out: &Output, control: &Control) -> anyhow::Result<bool> {
    let started = SystemTime::now();
    let project = open(cli)?;
    // Sem arquivo nem `-p`, dentro da pasta de um processador verifica ele;
    // no resto do projeto, o projeto.
    let processor = match (&args.processor, &args.file) {
        (Some(name), _) => Some(name.clone()),
        (None, None) => processor_here(cli, &project)?.map(|p| p.name.clone()),
        (None, Some(_)) => None,
    };
    let mut printer = Printer::new(out, project.root());
    printer.lint = args.lint;
    let request = FlowRequest::Check {
        file: args.file.as_deref().map(settings::absolute).transpose()?,
        processor,
        lint: args.lint,
    };
    let outcome = run_flow(cli, &project, request, started, &printer, control)?;
    if let Some(component) = &outcome.lint_missing {
        out.next(&format!(
            "Install the linter with: lace install {component}"
        ));
    }
    out.json(&CheckReport {
        check: outcome.check.expect("a verificação rodou"),
        report: outcome.report,
    })?;
    Ok(outcome.succeeded)
}

/// `lace hierarchy`. Não compila os processadores nem grava relatório: é a
/// hierarquia do que está no disco.
fn hierarchy(
    cli: &Cli,
    args: &HierarchyArgs,
    out: &Output,
    control: &Control,
) -> anyhow::Result<bool> {
    let started = SystemTime::now();
    let project = open(cli)?;
    let processor = match &args.processor {
        Some(name) => Some(name.clone()),
        None => processor_here(cli, &project)?.map(|p| p.name.clone()),
    };
    let printer = Printer::new(out, project.root());
    let request = FlowRequest::Hierarchy { processor };
    let outcome = run_flow(cli, &project, request, started, &printer, control)?;
    out.json(&outcome.hierarchy.expect("a hierarquia rodou"))?;
    Ok(outcome.succeeded)
}

fn sim(cli: &Cli, args: &SimArgs, out: &Output, control: &Control) -> anyhow::Result<bool> {
    let started = SystemTime::now();
    let project = open(cli)?;
    // Sem `-p` nem testbench, dentro da pasta de um processador simula
    // ele; no resto do projeto, a simulação do projeto.
    let processor = match (&args.processor, &args.testbench) {
        (Some(name), _) => Some(name.clone()),
        (None, None) => processor_here(cli, &project)?.map(|p| p.name.clone()),
        (None, Some(_)) => None,
    };
    let mut printer = Printer::new(out, project.root());
    printer.fast = args.fast;
    printer.timeout = args.timeout.map(Duration::from_secs);
    if !args.open {
        printer.wave_hint = Some(match &processor {
            Some(p) => format!("Open the waveform with: lace wave -p {p}"),
            None => "Open the waveform with: lace wave".to_owned(),
        });
    }
    let request = FlowRequest::Simulate {
        processor,
        testbench: args
            .testbench
            .as_deref()
            .map(settings::absolute)
            .transpose()?,
        simulator: if args.verilator {
            Simulator::Verilator
        } else {
            Simulator::Icarus
        },
        fast: args.fast,
        timeout_s: args.timeout,
        open_wave: args.open,
    };
    let outcome = run_flow(cli, &project, request, started, &printer, control)?;
    // O Surfer que não abriu é erro do comando, depois do relatório gravado,
    // como sempre foi.
    if let Some(error) = outcome.wave_error {
        return Err(error.into());
    }
    out.json(&SimReport {
        builds: outcome.builds,
        registered: outcome.registered,
        simulation: outcome.simulation,
        outputs: port_values(&outcome.outputs),
        surfer_pid: outcome.wave.as_ref().map(|w| w.pid),
        report: outcome.report,
    })?;
    Ok(outcome.succeeded)
}

fn wave(cli: &Cli, args: &WaveArgs, out: &Output, control: &Control) -> anyhow::Result<bool> {
    if let Some(command) = &args.command {
        return crate::wave_choice::run(cli, command, out, control);
    }
    // A onda que ainda não existe (ou o processador não compilado) é erro
    // antes de abrir o bundle; a de um exercício do lace learn abre com o
    // layout da correção. As duas regras são do despacho.
    let (project, source) = match &args.waveform {
        Some(path) => (
            None,
            WaveSource::File {
                path: settings::absolute(path)?,
            },
        ),
        None => {
            let project = open(cli)?;
            let processor = match &args.processor {
                Some(name) => Some(name.clone()),
                None => processor_here(cli, &project)?.map(|p| p.name.clone()),
            };
            (
                Some(project.spf_path().to_owned()),
                WaveSource::Project { processor },
            )
        }
    };
    let layout = if args.no_layout {
        LayoutPolicy::None
    } else if args.reset_layout {
        LayoutPolicy::Reset
    } else {
        LayoutPolicy::Generated
    };
    let request = Request::WaveOpen {
        project,
        request: WaveRequest { source, layout },
    };
    let opened = answer!(Local::new(cli).call(request)?, Wave);
    if let Some(testbench) = &opened.reset_deleted
        && out.is_text()
    {
        println!("The saved layout of {testbench} was deleted; opening the generated one");
    }
    out.opened(&opened);
    out.json(&WaveReport {
        pid: opened.pid,
        log: opened.log,
        layout: opened.layout,
        saved_layout: opened.saved_layout.map(|s| SavedLayoutReport {
            path: s.path,
            customized: s.customized,
        }),
        processors: opened.processors,
        waveform: opened.waveform,
    })?;
    Ok(true)
}

fn synth(cli: &Cli, args: &SynthArgs, out: &Output, control: &Control) -> anyhow::Result<bool> {
    let started = SystemTime::now();
    let project = open(cli)?;
    // O topo do projeto sintetiza com o Verilog de todos os processadores;
    // com -p, ou dentro da pasta de um processador, só o daquele.
    let processor = match &args.processor {
        Some(name) => Some(name.clone()),
        None => processor_here(cli, &project)?.map(|p| p.name.clone()),
    };
    if args.no_schematic_limit {
        tracing::debug!("--no-schematic-limit is ignored: the schematic has no connection limit");
    }
    let printer = Printer::new(out, project.root());
    let request = FlowRequest::Synthesize {
        processor,
        schematic: args.svg,
        module: args.module.clone(),
        open_viewer: args.open,
    };
    let outcome = run_flow(cli, &project, request, started, &printer, control)?;
    // Um módulo fora do netlist e um RTL viewer que não abriu são erros do
    // comando, depois do relatório gravado, como sempre foram.
    if let Some(error) = outcome.schematic_error {
        return Err(error.into());
    }
    if let Some(error) = outcome.rtl_viewer_error {
        return Err(error.into());
    }
    out.json(&SynthReport {
        builds: outcome.builds,
        registered: outcome.registered,
        synthesis: outcome.synthesis,
        schematic: outcome.schematic,
        rtl_viewer: outcome.rtl_viewer,
        report: outcome.report,
    })?;
    Ok(outcome.succeeded)
}

/// Quem abre o RTL viewer no lugar do navegador. A extensão do VS Code a
/// define (`vscode`) no terminal integrado: lá, o `lace synth --open` só
/// grava o HTML, e ela abre o netlist numa aba.
const OPENER_ENV: &str = "LACE_OPENER";

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
    let local = Local::new(cli);
    match &args.command {
        None => show_report(&local, &project, None, out),
        Some(ReportCommand::Show { id }) => show_report(&local, &project, Some(id.clone()), out),
        Some(ReportCommand::List { limit }) => {
            let request = Request::HistoryList {
                project: project.spf_path().to_owned(),
                limit: limit.map(|n| usize::try_from(n).unwrap_or(usize::MAX)),
            };
            let reports = answer!(local.call(request)?, Reports);
            out.report_list(&reports);
            out.json(&ReportListReport { reports })
        }
        Some(ReportCommand::Compare {
            id,
            against,
            summary,
        }) => {
            let request = Request::HistoryCompare {
                project: project.spf_path().to_owned(),
                id: id.clone(),
                against: against.clone(),
            };
            let comparison = answer!(local.call(request)?, Comparison);
            out.report_comparison(&comparison, *summary);
            out.json(&*comparison)
        }
        Some(ReportCommand::Clean { ids, keep, yes }) => {
            clean_reports(&local, &project, ids, *keep, *yes, out)
        }
    }
}

/// `lace report clean`. Apagar todos, ou todos menos os mais novos, pede
/// confirmação; os relatórios pedidos pelo nome saem sem perguntar.
fn clean_reports(
    local: &Local,
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
    let request = Request::HistoryPlanCleanup {
        project: project.spf_path().to_owned(),
        cleanup,
    };
    let targets = answer!(local.call(request)?, Ids);
    if !targets.is_empty() && ids.is_empty() && !yes {
        confirm_cleanup(project, targets.len(), keep)?;
    }
    let request = Request::HistoryClean {
        project: project.spf_path().to_owned(),
        ids: targets,
    };
    let cleaned = answer!(local.call(request)?, Cleaned);
    out.reports_removed(&cleaned.removed, cleaned.kept);
    out.json(&ReportCleanReport {
        removed: cleaned.removed,
        kept: cleaned.kept,
    })
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

fn show_report(
    local: &Local,
    project: &Project,
    id: Option<String>,
    out: &Output,
) -> anyhow::Result<()> {
    let request = Request::HistoryShow {
        project: project.spf_path().to_owned(),
        id,
    };
    let view = answer!(local.call(request)?, Report);
    out.report_text(&view.text);
    out.json(&ReportShowReport {
        id: view.id,
        path: view.path,
        record: view.record,
        text: view.text,
    })
}

fn tools(cli: &Cli, verify: bool, out: &Output) -> anyhow::Result<bool> {
    let local = Local::new(cli);
    let info = answer!(local.call(Request::ToolchainInfo)?, Toolchain);
    let mismatches = if verify {
        Some(answer!(local.call(Request::ToolchainVerify)?, Mismatches))
    } else {
        None
    };
    out.tools(&info, mismatches.as_deref())?;
    Ok(mismatches.is_none_or(|m| m.is_empty()))
}
