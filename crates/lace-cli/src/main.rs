//! `lace`: a linha de comando do Lace. Casca fina sobre o `lace-core`:
//! parseia argumentos, chama o Core e formata a saída. Nenhuma regra de
//! negócio mora aqui.

mod commands;
mod install;
mod installation;
mod output;
mod progress;
mod release;
mod report;
mod settings;
mod uninstall;
mod update;

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand, ValueEnum};
use lace_core::CancelToken;

use settings::ToolchainArgs;

/// Código de saída quando a operação rodou e não deu certo (erro de
/// compilação, simulação que falhou ou passou do prazo).
const EXIT_FAILED: u8 = 1;
/// Código de saída quando o Lace nem conseguiu rodar (toolchain, projeto, I/O).
const EXIT_ERROR: u8 = 2;
/// Código de saída quando o usuário cancelou (Ctrl+C ou SIGTERM), o
/// mesmo que o shell usa para um programa interrompido pelo Ctrl+C.
const EXIT_CANCELLED: u8 = 130;

const AFTER_HELP: &str = "\
Examples:
  lace new demo && cd demo
  lace add counter.v counter_tb.v   create a module and its testbench
  lace sim --open                   simulate and open the waveform
  lace proc add adder               create a SAPHO processor
  lace sim -p adder                 build it with YANC and simulate it

Exit status: 0 ok, 1 failed (build error, failed simulation, timeout),
2 could not run, 130 cancelled.";

#[derive(Parser)]
#[command(
    name = "lace",
    version,
    about = "Verilog and SAPHO processor development: Icarus, Verilator, Yosys, YANC and surfer-aurora",
    after_help = AFTER_HELP
)]
struct Cli {
    /// Project: its .spf file or any folder inside it (default: the current folder)
    #[arg(
        short = 'C',
        long = "project",
        global = true,
        default_value = ".",
        value_name = "PATH",
        help_heading = "Global options"
    )]
    project: Utf8PathBuf,

    /// JSON output on stdout
    #[arg(long, global = true, help_heading = "Global options")]
    json: bool,

    /// JSON lines on stdout: one event per line, then the result
    #[arg(long, global = true, help_heading = "Global options")]
    events: bool,

    /// More output (-vv: debug log)
    #[arg(short, long, global = true, action = clap::ArgAction::Count, help_heading = "Global options")]
    verbose: u8,

    #[command(flatten)]
    toolchain: ToolchainArgs,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new project
    New {
        /// Project name, also used for its folder and .spf file
        name: String,
        /// Where to create the project folder
        #[arg(long, default_value = ".")]
        dir: Utf8PathBuf,
    },
    /// Show the project summary
    Status,
    /// Add Verilog files; missing ones are created from a template
    Add {
        /// Verilog files
        #[arg(required = true, value_name = "FILE")]
        files: Vec<Utf8PathBuf>,
        /// Add as testbench (default: detected from content or name)
        #[arg(long)]
        tb: bool,
    },
    /// Remove files from the project (keeps them on disk)
    Remove {
        /// Verilog files
        #[arg(required = true, value_name = "FILE")]
        files: Vec<Utf8PathBuf>,
    },
    /// Show or set the top module
    Top {
        /// File or module name (none: show the current top)
        #[arg(value_name = "FILE|MODULE")]
        target: Option<String>,
    },
    /// Move or rename files and folders; the project file follows
    Move {
        /// Files or folders to move
        #[arg(required = true, value_name = "SOURCE")]
        sources: Vec<Utf8PathBuf>,
        /// An existing folder (the sources go inside it), or the new path of a single source
        #[arg(value_name = "DEST")]
        dest: Utf8PathBuf,
    },
    /// Change the place of a file in its project list (the compilers read the files in that order)
    #[command(group(clap::ArgGroup::new("position").required(true).multiple(false)))]
    Order {
        /// The file to move in the list
        #[arg(value_name = "FILE")]
        file: Utf8PathBuf,
        /// Put it first
        #[arg(long, group = "position")]
        first: bool,
        /// Put it last
        #[arg(long, group = "position")]
        last: bool,
        /// Put it right before this file
        #[arg(long, value_name = "FILE", group = "position")]
        before: Option<Utf8PathBuf>,
        /// Put it right after this file
        #[arg(long, value_name = "FILE", group = "position")]
        after: Option<Utf8PathBuf>,
    },
    /// Manage SAPHO processors
    #[command(subcommand)]
    Proc(ProcCommand),
    /// Compile processors to Verilog (YANC)
    Build(BuildArgs),
    /// Check that the Verilog elaborates (iverilog -tnull)
    Check(CheckArgs),
    /// Show the module hierarchy of the design and of each testbench, as Icarus elaborates it
    Hierarchy(HierarchyArgs),
    /// Simulate the project testbench or a processor
    Sim(SimArgs),
    /// Open a waveform in surfer-aurora
    Wave(WaveArgs),
    /// Synthesize with Yosys
    Synth(SynthArgs),
    /// Show and compare the reports of past operations
    Report(ReportArgs),
    /// Show the bundled tools and their versions
    Tools {
        /// Verify each executable against the manifest SHA-256
        #[arg(long)]
        verify: bool,
    },
    /// Install bundle apps (none: choose from a list)
    Install(InstallArgs),
    /// Update Lace and its bundle to the latest release
    Update {
        /// Only compare installed and published versions
        #[arg(long)]
        check: bool,
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
    /// Uninstall Lace and its bundle
    Uninstall {
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
    /// Generate shell completions
    #[command(hide = true)]
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

/// `lace report`: sem subcomando, o relatório mais novo.
#[derive(clap::Args)]
#[command(args_conflicts_with_subcommands = true)]
struct ReportArgs {
    #[command(subcommand)]
    command: Option<ReportCommand>,
}

#[derive(Subcommand)]
enum ReportCommand {
    /// List the stored reports, newest first
    List {
        /// Show at most N reports
        #[arg(long, value_name = "N", value_parser = clap::value_parser!(u64).range(1..))]
        limit: Option<u64>,
    },
    /// Print a stored report
    Show {
        /// Report ID (run-000042, or just 42)
        id: String,
    },
    /// Compare synthesis statistics and simulation timings of two reports
    Compare {
        /// Current report (default: the latest)
        id: Option<String>,
        /// Baseline report (default: the latest earlier comparable one)
        #[arg(long, value_name = "ID")]
        against: Option<String>,
        /// Show only the summary
        #[arg(long)]
        summary: bool,
    },
    /// Remove stored reports (default: all of them)
    Clean {
        /// Remove only these reports (run-000042, or just 42)
        #[arg(value_name = "ID", conflicts_with = "keep")]
        ids: Vec<String>,
        /// Keep the N newest reports and remove the rest
        #[arg(long, value_name = "N")]
        keep: Option<u64>,
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum ProcCommand {
    /// Create a processor
    Add(ProcAddArgs),
    /// Set frequency, clocks and array export
    Set {
        /// Processor name (default: the processor of the current folder)
        name: Option<String>,
        /// Frequency in MHz
        #[arg(long)]
        freq: Option<u32>,
        /// Clock cycles the testbench simulates
        #[arg(long)]
        clocks: Option<u32>,
        /// Export arrays to the simulation
        #[arg(long, value_name = "true|false")]
        arrays: Option<bool>,
    },
}

#[derive(clap::Args)]
struct ProcAddArgs {
    /// Letters, digits and _, starting with a letter or _
    name: String,
    /// Language: cmm (C±) or cpp (C)
    #[arg(long, value_enum, default_value_t = Lang::Cmm, hide_possible_values = true)]
    lang: Lang,
    /// Input ports (#NUIOIN)
    #[arg(long, default_value_t = 1)]
    inputs: u32,
    /// Output ports (#NUIOOU)
    #[arg(long, default_value_t = 1)]
    outputs: u32,
    /// Word width (#NUBITS); default: #NBMANT + #NBEXPO + 1
    #[arg(long, help_heading = "C± only")]
    nubits: Option<u32>,
    /// Mantissa bits (#NBMANT)
    #[arg(long, help_heading = "C± only")]
    nbmant: Option<u32>,
    /// Exponent bits (#NBEXPO)
    #[arg(long, help_heading = "C± only")]
    nbexpo: Option<u32>,
    /// norm(x) gain, a power of two (#NUGAIN)
    #[arg(long, help_heading = "C± only")]
    nugain: Option<u32>,
    /// Data stack depth (#NDSTAC)
    #[arg(long, help_heading = "C± only")]
    ndstac: Option<u32>,
    /// Instruction stack depth (#SDEPTH)
    #[arg(long, help_heading = "C± only")]
    sdepth: Option<u32>,
}

#[derive(clap::Args)]
struct BuildArgs {
    /// Processor to build (repeatable; default: the processor of the current folder, or all)
    #[arg(short, long = "processor", value_name = "NAME")]
    processors: Vec<String>,
}

#[derive(clap::Args)]
struct InstallArgs {
    /// yanc, icarus, verilator, cocotb, yosys, graphviz, surfer-aurora, studio
    #[arg(value_name = "APP")]
    components: Vec<String>,
    /// Install from a local Lace installer (folder, .tar.gz or payload/) instead of downloading
    #[arg(long, value_name = "PATH")]
    from: Option<Utf8PathBuf>,
}

#[derive(clap::Args)]
struct CheckArgs {
    /// Check only the modules in this file (default: the project and each testbench)
    #[arg(value_name = "FILE", conflicts_with = "processor")]
    file: Option<Utf8PathBuf>,
    /// Check this processor and its YANC testbench (default: the processor of the current folder)
    #[arg(short, long, value_name = "NAME")]
    processor: Option<String>,
    /// Also run the Verilator linter
    #[arg(long)]
    lint: bool,
}

#[derive(clap::Args)]
struct HierarchyArgs {
    /// Show this processor and its YANC testbench (default: the processor of the current folder)
    #[arg(short, long, value_name = "NAME")]
    processor: Option<String>,
}

#[derive(clap::Args)]
struct SimArgs {
    /// Testbench to simulate; becomes the project default
    #[arg(value_name = "TESTBENCH", conflicts_with = "processor")]
    testbench: Option<Utf8PathBuf>,
    /// Simulate this processor with its YANC testbench (default: the processor of the current folder)
    #[arg(short, long, value_name = "NAME")]
    processor: Option<String>,
    /// Use Verilator instead of Icarus
    #[arg(long)]
    verilator: bool,
    /// Stop the simulation after this many seconds
    #[arg(long, value_name = "SECONDS", value_parser = clap::value_parser!(u64).range(1..))]
    timeout: Option<u64>,
    /// Open the waveform when done
    #[arg(long)]
    open: bool,
}

#[derive(clap::Args)]
struct WaveArgs {
    /// VCD, FST or GHW file (default: the project simulation)
    #[arg(value_name = "WAVEFORM", conflicts_with = "processor")]
    waveform: Option<Utf8PathBuf>,
    /// Open this processor's waveform (default: the processor of the current folder)
    #[arg(short, long, value_name = "NAME")]
    processor: Option<String>,
    /// Open without the SAPHO processor layout (variables, assembly and C± lines)
    #[arg(long)]
    no_layout: bool,
}

#[derive(clap::Args)]
struct SynthArgs {
    /// Synthesize this processor (default: the processor of the current folder, or the top module)
    #[arg(short, long, value_name = "NAME")]
    processor: Option<String>,
    /// Draw the schematic as SVG (needs Graphviz)
    #[arg(long)]
    svg: bool,
    /// Module to draw (default: top; -v lists them)
    #[arg(long, requires = "svg")]
    module: Option<String>,
    /// Draw even a module with more connections than the schematic limit (Graphviz may take minutes)
    #[arg(long, requires = "svg")]
    no_schematic_limit: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Lang {
    Cmm,
    Cpp,
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return usage_error(&error),
    };
    init_tracing(cli.verbose);
    let mode = if cli.events {
        output::Mode::Events
    } else if cli.json {
        output::Mode::Json
    } else {
        output::Mode::Text
    };
    let out = output::Output::new(mode, cli.verbose > 0);
    let cancel = cancel_on_signals();
    let control = out.control(cancel.clone());

    let code = match commands::run(&cli, &out, &control) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(EXIT_FAILED),
        Err(error) => {
            out.error(&error);
            ExitCode::from(EXIT_ERROR)
        }
    };
    if cancel.is_cancelled() {
        ExitCode::from(EXIT_CANCELLED)
    } else {
        code
    }
}

/// Um erro que o clap achou nos argumentos. A ajuda e a versão saem como o
/// clap as escreve. Os erros saem com `Error:` e `Tip:` em maiúscula, como os
/// do Lace, e com cor nas mesmas condições que o clap usaria (o `anstream`
/// tira os códigos fora de um terminal ou com `NO_COLOR`).
fn usage_error(error: &clap::Error) -> ExitCode {
    use clap::error::ErrorKind;
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) {
        error.exit();
    }
    let text = capitalize_labels(&error.render().ansi().to_string());
    let _ = std::io::Write::write_all(&mut anstream::stderr(), text.as_bytes());
    ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(EXIT_ERROR))
}

/// As linhas que começam com `error:` ou `tip:` (depois de espaços e
/// códigos ANSI) ganham maiúscula no rótulo e no começo da mensagem:
/// `error: unexpected argument` vira `Error: Unexpected argument`.
fn capitalize_labels(text: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            let start = visible_start(line);
            for label in ["error:", "tip:"] {
                if let Some(rest) = line[start..].strip_prefix(label) {
                    let at = visible_start(rest);
                    return format!(
                        "{}{}{}{}",
                        &line[..start],
                        capitalize(label),
                        &rest[..at],
                        capitalize(&rest[at..])
                    );
                }
            }
            line.to_owned()
        })
        .collect()
}

/// Onde começa o texto visível: depois dos espaços e das sequências ANSI
/// (`ESC [ ... letra`).
fn visible_start(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut i = 0;
    loop {
        match bytes.get(i) {
            Some(b' ') => i += 1,
            Some(0x1b) => {
                i += 1;
                if bytes.get(i) == Some(&b'[') {
                    i += 1;
                    while let Some(b) = bytes.get(i) {
                        i += 1;
                        if b.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
            }
            _ => return i,
        }
    }
}

/// O texto com a primeira letra em maiúscula.
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// O pedido de cancelamento que o Ctrl+C (SIGINT) e o SIGTERM fazem; no
/// Unix, também o SIGHUP de um terminal fechado. O primeiro sinal pede o
/// cancelamento: o Core encerra a ferramenta que roda, com tudo o que ela
/// iniciou, e o comando termina mostrando o que chegou a rodar. O segundo
/// sai na hora, com 130.
///
/// Sem isto, o sinal mataria só o Lace: as ferramentas rodam num grupo de
/// processos próprio e continuariam rodando sozinhas.
fn cancel_on_signals() -> CancelToken {
    use signal_hook::consts::signal::{SIGINT, SIGTERM};
    let flag = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    let signals = [SIGINT, SIGTERM, signal_hook::consts::signal::SIGHUP];
    #[cfg(not(unix))]
    let signals = [SIGINT, SIGTERM];
    for signal in signals {
        // A ordem importa: a saída imediata confere a flag antes de a
        // segunda chamada marcá-la, então só dispara a partir do segundo sinal.
        let registered = signal_hook::flag::register_conditional_shutdown(
            signal,
            i32::from(EXIT_CANCELLED),
            Arc::clone(&flag),
        )
        .and_then(|_| signal_hook::flag::register(signal, Arc::clone(&flag)));
        if let Err(error) = registered {
            tracing::warn!(signal, %error, "No signal handler, Ctrl+C will not cancel");
        }
    }
    CancelToken::from(flag)
}

/// O log do Core (`tracing`) vai para o stderr: avisos por padrão, `info`
/// com -v, `debug` com -vv. `RUST_LOG` tem precedência.
fn init_tracing(verbose: u8) {
    let default = match verbose {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default));
    // Cor no log só num terminal e sem NO_COLOR, como no resto da saída.
    let ansi = std::io::IsTerminal::is_terminal(&std::io::stderr())
        && std::env::var_os("NO_COLOR").is_none();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_ansi(ansi)
        .init();
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    /// Confere nomes duplicados, conflitos e flags de todos os subcomandos,
    /// coisa que o clap só verificaria ao rodar cada um.
    #[test]
    fn cli_definition_is_valid() {
        super::Cli::command().debug_assert();
    }

    #[test]
    fn clap_labels_are_capitalized() {
        let plain = "error: unexpected argument '--x' found\n\n  \
                     tip: a similar argument exists: '--json'\n\nUsage: lace\n";
        assert_eq!(
            super::capitalize_labels(plain),
            "Error: Unexpected argument '--x' found\n\n  \
             Tip: A similar argument exists: '--json'\n\nUsage: lace\n"
        );
        let colored = "\x1b[1m\x1b[31merror:\x1b[0m 'lace' requires a subcommand\n";
        assert_eq!(
            super::capitalize_labels(colored),
            "\x1b[1m\x1b[31mError:\x1b[0m 'lace' requires a subcommand\n"
        );
        let usage = "  <NAME>\n";
        assert_eq!(super::capitalize_labels(usage), usage);
    }
}
