//! `solar`: a linha de comando do Solar. Casca fina sobre o `solar-core`:
//! parseia argumentos, chama o Core e formata a saída. Nenhuma regra de
//! negócio mora aqui.

mod commands;
mod output;
mod settings;

use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand, ValueEnum};

use settings::ToolchainArgs;

/// Código de saída quando a operação rodou e não deu certo (erro de
/// compilação, simulação que falhou).
const EXIT_FAILED: u8 = 1;
/// Código de saída quando o Solar nem conseguiu rodar (toolchain, projeto, I/O).
const EXIT_ERROR: u8 = 2;

const AFTER_HELP: &str = "\
Exemplos:
  solar tools
  solar new demo && cd demo
  solar proc add soma
  solar build
  solar sim -p soma --open
  solar synth -p soma --svg

Códigos de saída:
  0  deu certo
  1  a operação rodou e falhou (erro de compilação, simulação que falhou)
  2  o Solar não conseguiu rodar (bundle, projeto, arquivo)

Ferramentas: só as do bundle instalado com o Solar (solar tools). O Verilator
usa o compilador C++, o make e o Perl do sistema (solar config set-compiler).";

#[derive(Parser)]
#[command(
    name = "solar",
    version,
    about = "Orquestrador da toolchain SAPHO: YANC, Icarus, Verilator, Yosys, Graphviz e surfer-aurora",
    after_help = AFTER_HELP
)]
struct Cli {
    /// Projeto: o .spf ou o diretório dele
    #[arg(
        short = 'C',
        long = "project",
        global = true,
        default_value = ".",
        value_name = "CAMINHO"
    )]
    project: Utf8PathBuf,

    /// Saída em JSON no stdout (um objeto por comando)
    #[arg(long, global = true)]
    json: bool,

    /// Mais detalhe: mensagens informativas, saída da simulação e log (-vv: depuração)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(flatten)]
    toolchain: ToolchainArgs,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Cria um projeto vazio em <DIR>/<NOME>/<NOME>.spf")]
    New {
        /// Nome do projeto (vira o nome da pasta e do .spf)
        name: String,
        /// Onde criar a pasta do projeto
        #[arg(long, default_value = ".")]
        dir: Utf8PathBuf,
    },
    /// Resumo do projeto: processadores, arquivos, topo e testbench
    Status,
    /// Processadores do projeto
    #[command(subcommand)]
    Proc(ProcCommand),
    /// Arquivos Verilog e testbenches do projeto
    #[command(subcommand)]
    File(FileCommand),
    #[command(about = "Grava Simulation/input_<PORTA>.txt de um processador, um valor por linha")]
    Input {
        processor: String,
        port: u32,
        /// Valores decimais com sinal
        #[arg(allow_negative_numbers = true, conflicts_with = "from")]
        values: Vec<i64>,
        /// Copia os valores de um arquivo em vez de recebê-los na linha de comando
        #[arg(long, value_name = "ARQUIVO")]
        from: Option<Utf8PathBuf>,
    },
    #[command(about = "Mostra Simulation/output_<PORTA>.txt de um processador")]
    Output { processor: String, port: u32 },
    /// Compila processadores até o Verilog (YANC)
    Build(BuildArgs),
    /// Compila e simula um processador (-p) ou o testbench do projeto
    Sim(SimArgs),
    /// Confere se o projeto elabora a partir do módulo de topo (iverilog -t null)
    Check,
    /// Sintetiza com o Yosys e, com --svg, desenha o esquemático (Yosys + Graphviz)
    Synth(SynthArgs),
    /// Abre uma onda no surfer-aurora
    Wave {
        /// Arquivo de onda (VCD, FST, GHW)
        waveform: Utf8PathBuf,
        /// Arquivo de comandos (.sucl) ou de estado (.surf.ron) do surfer-aurora
        #[arg(long, value_name = "ARQUIVO")]
        view: Option<Utf8PathBuf>,
        /// Espera o Surfer fechar em vez de retornar logo
        #[arg(long)]
        wait: bool,
    },
    /// Mostra o bundle: versões, ferramentas e o compilador do sistema
    Tools {
        /// Confere o SHA-256 de cada executável contra o manifesto
        #[arg(long)]
        verify: bool,
    },
    /// Arquivo de configuração (compilador do sistema para o Verilator)
    #[command(subcommand)]
    Config(ConfigCommand),
    /// Gera o script de autocompletar para um shell
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
enum ProcCommand {
    /// Cria um processador: diretórios, fonte-modelo e entrada no .spf
    Add(ProcAddArgs),
    /// Muda frequência, clocks e exportação de arrays (grava no .spf)
    Set {
        name: String,
        /// Frequência em MHz
        #[arg(long)]
        freq: Option<u32>,
        /// Clocks que o testbench simula
        #[arg(long)]
        clocks: Option<u32>,
        /// Exporta arrays para a simulação
        #[arg(long, value_name = "true|false")]
        show_arrays: Option<bool>,
    },
    /// Lista os processadores
    List,
}

#[derive(clap::Args)]
struct ProcAddArgs {
    /// Nome: letras, dígitos e _, começando por letra ou _
    name: String,
    #[arg(long, value_enum, default_value_t = Lang::Cmm)]
    lang: Lang,
    /// Portas de entrada (#NUIOIN)
    #[arg(long, default_value_t = 1)]
    inputs: u32,
    /// Portas de saída (#NUIOOU)
    #[arg(long, default_value_t = 1)]
    outputs: u32,
    /// Largura da palavra (#NUBITS, só C±)
    #[arg(long)]
    nubits: Option<u32>,
    /// Bits de mantissa (#NBMANT, só C±)
    #[arg(long)]
    nbmant: Option<u32>,
    /// Bits de expoente (#NBEXPO, só C±)
    #[arg(long)]
    nbexpo: Option<u32>,
    /// Ganho de norm(x), potência de dois (#NUGAIN, só C±)
    #[arg(long)]
    nugain: Option<u32>,
    /// Profundidade da pilha de dados (#NDSTAC, só C±)
    #[arg(long)]
    ndstac: Option<u32>,
    /// Profundidade da pilha de instruções (#SDEPTH, só C±)
    #[arg(long)]
    sdepth: Option<u32>,
}

#[derive(Subcommand)]
enum FileCommand {
    /// Registra um arquivo (com --create, cria vazio)
    Add {
        path: Utf8PathBuf,
        /// É testbench (padrão: sintetizável)
        #[arg(long)]
        testbench: bool,
        /// Cria o arquivo vazio; recusa se ele já existir
        #[arg(long)]
        create: bool,
    },
    /// Tira um arquivo do projeto (não apaga do disco)
    Remove {
        path: Utf8PathBuf,
        #[arg(long)]
        testbench: bool,
    },
    /// Marca o módulo de topo
    Top { path: Utf8PathBuf },
    /// Escolhe o testbench da simulação do projeto
    Testbench { path: Utf8PathBuf },
    /// Lista os arquivos
    List,
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Mostra o arquivo de configuração
    Show,
    /// Mostra onde fica o arquivo de configuração
    Path,
    /// Declara onde está o compilador do sistema para o Verilator: diretório
    /// com perl, make e g++/clang++ (Linux, macOS) ou a raiz do MSYS2 (Windows)
    SetCompiler { dir: Utf8PathBuf },
    /// Volta a procurar o compilador nos locais padrão
    UnsetCompiler,
}

#[derive(clap::Args)]
struct BuildArgs {
    /// Processador a compilar (repetível; sem nenhum, compila todos)
    #[arg(short, long = "processor", value_name = "NOME")]
    processors: Vec<String>,
    #[command(flatten)]
    overrides: BuildOverrides,
    /// Exporta arrays para a simulação (-A do cmmcomp)
    #[arg(long)]
    show_arrays: bool,
}

/// Valores que valem só nesta execução, sem gravar no .spf.
#[derive(clap::Args)]
struct BuildOverrides {
    /// Frequência em MHz, só nesta execução
    #[arg(long)]
    freq: Option<u32>,
    /// Clocks do testbench, só nesta execução
    #[arg(long)]
    clocks: Option<u32>,
}

#[derive(clap::Args)]
struct SimArgs {
    /// Simula só este processador, com o testbench gerado (sem ele: o do projeto)
    #[arg(short, long, value_name = "NOME")]
    processor: Option<String>,
    /// Simulador
    #[arg(long, value_enum, default_value_t = Sim::Icarus)]
    simulator: Sim,
    /// Onda em VCD em vez de FST (Icarus; o Verilator sempre grava VCD)
    #[arg(long)]
    vcd: bool,
    /// Processos paralelos na compilação do Verilator (padrão: todos os núcleos)
    #[arg(long)]
    jobs: Option<u32>,
    /// Não compila os processadores antes
    #[arg(long)]
    no_build: bool,
    /// Abre a onda no surfer-aurora ao terminar
    #[arg(long)]
    open: bool,
    #[command(flatten)]
    overrides: BuildOverrides,
}

#[derive(clap::Args)]
struct SynthArgs {
    /// Sintetiza só este processador (sem ele: o módulo de topo do projeto)
    #[arg(short, long, value_name = "NOME")]
    processor: Option<String>,
    /// Desenha o esquemático em SVG
    #[arg(long)]
    svg: bool,
    /// Módulo a desenhar (padrão: o de topo); veja os nomes com -v
    #[arg(long)]
    module: Option<String>,
    /// Não escreve a largura dos barramentos no esquemático
    #[arg(long)]
    no_widths: bool,
    /// Não compila o processador antes (com -p)
    #[arg(long)]
    no_build: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Lang {
    /// C±
    Cmm,
    /// C
    Cpp,
}

#[derive(Clone, Copy, ValueEnum)]
enum Sim {
    /// Icarus Verilog: compila rápido, simula devagar
    Icarus,
    /// Verilator: compila devagar (C++), simula rápido
    Verilator,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    let out = output::Output::new(cli.json, cli.verbose > 0);

    match commands::run(&cli, &out) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(EXIT_FAILED),
        Err(error) => {
            out.error(&error);
            ExitCode::from(EXIT_ERROR)
        }
    }
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
}
