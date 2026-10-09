//! Erros de infraestrutura do Core.
//!
//! # Duas categorias de falha
//!
//! O Lace separa "não consegui rodar" de "rodei e deu errado":
//!
//! - **`Err(LaceError)`**: a operação nem começou ou foi interrompida por algo
//!   fora do código do usuário. Bundle ausente, projeto mal formado, falha de
//!   I/O, processo que não subiu.
//! - **`Ok(resultado)` com `status` diferente de
//!   [`Status::Succeeded`](crate::Status::Succeeded)**: as ferramentas rodaram
//!   e recusaram a entrada. Erro de sintaxe em C±, Verilog que não elabora,
//!   simulação que termina com código diferente de 0. Os detalhes estão nos
//!   `diagnostics` do resultado.
//!
//! Uma interface trata a primeira categoria como falha do ambiente ("configure
//! a toolchain") e a segunda como feedback para o usuário ("linha 16: variável
//! não declarada"). As duas nunca se misturam.
//!
//! # Tratando erros
//!
//! ```
//! use lace_core::{LaceError, Toolchain};
//!
//! match Toolchain::open("/nao/existe") {
//!     Ok(_) => unreachable!(),
//!     Err(LaceError::InvalidBundle { path, .. }) => {
//!         assert_eq!(path, "/nao/existe");
//!     }
//!     Err(other) => panic!("inesperado: {other}"),
//! }
//! ```
//!
//! Para registrar ou serializar sem depender do enum, use
//! [`LaceError::code`], que devolve um identificador estável por variante.

use camino::Utf8PathBuf;

/// Atalho para `Result<T, LaceError>`, usado em toda a API.
pub type Result<T, E = LaceError> = std::result::Result<T, E>;

/// Tudo que impede uma operação do Lace de rodar.
///
/// O enum é `#[non_exhaustive]`: novas variantes podem surgir sem quebrar a
/// API, então todo `match` precisa de um braço `_`. A mensagem de
/// [`Display`](std::fmt::Display) é em inglês e pronta para mostrar ao
/// usuário.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LaceError {
    /// Não há bundle instalado junto com o Lace.
    ///
    /// Vem de [`Toolchain::locate`](crate::Toolchain::locate). O Lace só
    /// executa ferramentas do bundle; sem ele, nada que dependa de ferramenta
    /// roda.
    #[error("Tool bundle not found (searched: {})", searched.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(", "))]
    BundleNotFound {
        /// Os diretórios em que o Lace procurou um `bundle.json`.
        searched: Vec<Utf8PathBuf>,
    },

    /// O diretório não é um bundle válido para este Lace: sem `bundle.json`,
    /// JSON inválido, formato desconhecido, montado para outra plataforma, ou
    /// com componente ausente ou apontando para fora do bundle.
    ///
    /// Vem de [`Toolchain::open`](crate::Toolchain::open).
    #[error("Invalid bundle at {path}: {reason}")]
    InvalidBundle {
        /// O diretório do bundle.
        path: Utf8PathBuf,
        /// O que está errado.
        reason: String,
    },

    /// O Lace está rodando numa plataforma para a qual não há bundle (só
    /// Linux x64, macOS arm64 e Windows x64).
    #[error(
        "Unsupported platform: {os} {arch} (bundles exist for linux-x64, darwin-arm64 and windows-x64)"
    )]
    UnsupportedPlatform {
        /// `std::env::consts::OS`.
        os: String,
        /// `std::env::consts::ARCH`.
        arch: String,
    },

    /// O componente de que a ferramenta faz parte não foi instalado: o usuário
    /// não o escolheu no instalador, ou é um bundle de desenvolvimento só com
    /// o YANC.
    ///
    /// Vem de [`Toolchain::tool`](crate::Toolchain::tool). O texto não diz
    /// como instalar: cada interface tem o seu caminho (`lace install`, o
    /// botão Instalar do Studio).
    #[error("Component {0} is not installed")]
    ComponentMissing(String),

    /// Um arquivo que o bundle deveria ter não está lá: o executável de uma
    /// ferramenta, ou um diretório do YANC (`HDL`, `Macros`, `Header`).
    ///
    /// Vem de [`Toolchain::tool`](crate::Toolchain::tool) e dos métodos de
    /// diretório do YANC. Indica bundle corrompido ou incompleto: reinstale.
    #[error("Incomplete bundle: {what} not found at {path}")]
    ToolchainIncomplete {
        /// O que faltou, em texto (por exemplo "yosys executable").
        what: String,
        /// Onde se esperava encontrar.
        path: Utf8PathBuf,
    },

    /// No Linux e no macOS, o Verilator precisa de compilador C++, `make` e
    /// Perl do sistema, e eles não foram encontrados nos locais padrão (ver
    /// [`SystemCompiler`](crate::SystemCompiler)). No Windows os três vêm com
    /// o componente verilator, e a falta deles é
    /// [`LaceError::ToolchainIncomplete`].
    #[error(
        "Verilator needs a system C++ compiler, make and Perl, and they were not found \
         (Linux: build-essential and perl in /usr/bin; macOS: Xcode Command Line Tools)"
    )]
    SystemCompilerMissing,

    /// A compilação para uma placa Intel precisa do Quartus Prime, que não
    /// vem no bundle e não foi achado ([`crate::fpga::Quartus`]). Ele roda
    /// no Windows e no Linux.
    #[error(
        "Intel FPGA boards need Quartus Prime, which does not come in the bundle, and it was not found"
    )]
    QuartusMissing,

    /// Gravar na placa sem a compilação para ela: falta o `.sof`
    /// ([`crate::fpga::program()`]).
    #[error("No bitstream for the board: {0} does not exist")]
    NoBitstream(Utf8PathBuf),

    /// Nenhum cabo de gravação respondeu ao `quartus_pgm -l`: a placa
    /// desligada ou desconectada, a porta USB errada ou o driver do
    /// USB-Blaster ausente.
    #[error("No programming cable found: the Quartus Programmer sees no USB-Blaster")]
    NoCable,

    /// O `.sof` não descreve o projeto de agora: um fonte, uma memória, o
    /// `fpga.json` ou a placa mudou depois da compilação, o `.sof` foi
    /// trocado, ou a compilação não terminou. A placa receberia outro design.
    #[error("The bitstream does not match the project: {}", reasons.join("; "))]
    StaleBitstream {
        /// Cada diferença, em texto.
        reasons: Vec<String>,
    },

    /// O projeto no disco não está como deveria: diretório ausente, arquivo
    /// onde deveria haver diretório, arquivo registrado que sumiu, arquivo que
    /// o Lace se recusa a sobrescrever.
    #[error("Invalid project at {path}: {reason}")]
    InvalidProject {
        /// O caminho com problema.
        path: Utf8PathBuf,
        /// O que está errado, em texto.
        reason: String,
    },

    /// Já existe um `.spf` onde [`Project::create`](crate::Project::create)
    /// ia criar o projeto. Nada foi alterado.
    #[error("Project already exists: {0}")]
    ProjectExists(Utf8PathBuf),

    /// Um caminho que precisa estar dentro da pasta do projeto está fora
    /// dela. Vem de [`Project::move_path`](crate::Project::move_path).
    #[error("{0} is outside the project folder")]
    OutsideProject(Utf8PathBuf),

    /// [`Project::move_path`](crate::Project::move_path) recusou o movimento:
    /// o `.spf`, a pasta `.lace`, a pasta de um processador, as pastas
    /// `Software/`, `Hardware/` e `Simulation/` dele e o fonte ficam onde o
    /// Core os procura; uma pasta não vai para dentro dela mesma; nada vai
    /// para dentro de `.lace`. Nada foi alterado.
    #[error("Cannot move {path}: {reason}")]
    CannotMove {
        /// O que seria movido.
        path: Utf8PathBuf,
        /// Por que não.
        reason: String,
    },

    /// O destino já existe. Nada foi alterado: o Lace não sobrescreve.
    #[error("{0} already exists")]
    PathExists(Utf8PathBuf),

    /// [`Project::add_processor`](crate::Project::add_processor) recebeu um
    /// nome que já está no projeto. Nada foi alterado.
    #[error("Processor '{0}' already exists in the project")]
    ProcessorExists(String),

    /// O projeto não tem o processador pedido.
    ///
    /// Vem de [`Project::require_processor`](crate::Project::require_processor)
    /// e das operações que recebem um nome de processador.
    #[error("No processor '{name}' in the project (available: {})", available.join(", "))]
    ProcessorNotFound {
        /// O nome pedido.
        name: String,
        /// Os processadores que o projeto tem, na ordem do `.spf`.
        available: Vec<String>,
    },

    /// Nome de projeto, de processador ou de módulo de topo que não pode ser
    /// usado.
    ///
    /// Processador e módulo de topo precisam ser identificadores
    /// (`[A-Za-z_][A-Za-z0-9_]*`), porque viram `#PRNAME`, nome de módulo
    /// Verilog e nome de arquivo. Projeto precisa ser um nome de pasta válido
    /// no Windows e no Linux.
    #[error("Invalid name '{name}': {reason}")]
    InvalidName {
        /// O nome recusado.
        name: String,
        /// Por quê.
        reason: String,
    },

    /// O `.spf` não pôde ser lido: JSON quebrado mesmo depois da leitura
    /// tolerante, falta a seção `structure`, ou um campo tem tipo ou valor
    /// inválido (por exemplo `clk: 12.5`).
    #[error("Invalid project file {path}: {reason}")]
    InvalidProjectFile {
        /// O `.spf`.
        path: Utf8PathBuf,
        /// O que está errado.
        reason: String,
    },

    /// O programa-fonte de um processador não pode ser compilado como está:
    /// não existe, não se chama `<processador>.<ext>`, não tem `#PRNAME`, ou
    /// declara um nome diferente do processador.
    ///
    /// Vem de [`build`](crate::build), antes de rodar qualquer compilador. Ver
    /// as armadilhas de nome do YANC na documentação de `build`.
    #[error("Invalid source {path}: {reason}")]
    InvalidSource {
        /// O arquivo-fonte.
        path: Utf8PathBuf,
        /// O que está errado.
        reason: String,
    },

    /// A operação precisa de artefatos que [`build`](crate::build) ainda não
    /// gerou para este processador (testbench, Verilog).
    #[error("Processor '{processor}' has not been built: {missing} is missing")]
    NotBuilt {
        /// O processador.
        processor: String,
        /// O primeiro artefato que faltou.
        missing: Utf8PathBuf,
    },

    /// [`simulate_project`](crate::simulate_project) foi chamado num projeto
    /// sem testbench registrado. Crie ou registre um com
    /// [`Project::add_verilog`](crate::Project::add_verilog).
    #[error("Project {0} has no testbench")]
    NoTestbench(Utf8PathBuf),

    /// O testbench cocotb (`.py`) não diz qual módulo testa (a linha
    /// `# aurora-toplevel: <módulo>`) e o projeto não tem módulo de topo
    /// para o lugar dele.
    #[error(
        "The cocotb testbench {0} does not say which module it tests: add a `# aurora-toplevel: <module>` line to it, or choose the project top module"
    )]
    NoCocotbToplevel(Utf8PathBuf),

    /// O Python do componente `cocotb` não carregou o cocotb (a sonda que diz
    /// onde estão a VPI e a biblioteca do Python falhou), ou o cocotb dele
    /// não traz a biblioteca do simulador pedido.
    #[error("cocotb could not be loaded by {python}: {reason}")]
    CocotbUnavailable {
        /// O comando da sonda.
        python: String,
        /// O fim do que a sonda escreveu, ou o motivo.
        reason: String,
    },

    /// [`synthesize`](crate::synthesize) com
    /// [`DesignTarget::TopLevel`](crate::DesignTarget::TopLevel) num projeto
    /// sem módulo de topo. Escolha um com
    /// [`Project::set_top`](crate::Project::set_top).
    #[error("Project {0} has no top module")]
    NoTopLevel(Utf8PathBuf),

    /// [`check`](crate::check) num projeto sem nenhum arquivo Verilog
    /// registrado e sem processadores.
    #[error("Project {0} has no Verilog files added")]
    EmptyProject(Utf8PathBuf),

    /// Um módulo pedido pelo nome não está em nenhum arquivo registrado do
    /// projeto ([`Project::set_top`](crate::Project::set_top)), ou o arquivo de
    /// topo não declara um módulo que dê para usar.
    #[error("Module {name} not found{}", if available.is_empty() { String::new() } else { format!(" (project modules: {})", available.join(", ")) })]
    ModuleNotFound {
        /// O nome pedido (ou o arquivo, quando ele não declara módulo).
        name: String,
        /// Os módulos dos arquivos sintetizáveis registrados.
        available: Vec<String>,
    },

    /// O módulo pedido pelo nome está declarado em mais de um arquivo
    /// registrado ([`Project::set_top`](crate::Project::set_top)). Escolha
    /// pelo arquivo. Nada foi alterado.
    #[error("Module {name} is declared in more than one file: {}", files.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(", "))]
    AmbiguousModule {
        /// O nome pedido.
        name: String,
        /// Os arquivos que o declaram, na ordem do `.spf`.
        files: Vec<Utf8PathBuf>,
    },

    /// Um parâmetro de processador fora do que o YANC compila: `#NUBITS`
    /// diferente de `#NBMANT + #NBEXPO + 1`, `#NUGAIN` que não é potência de
    /// dois, pilha vazia, clocks ou frequência fora da faixa. Vem de
    /// [`Project::add_processor`](crate::Project::add_processor) e de
    /// [`Project::configure_processor`](crate::Project::configure_processor);
    /// nada foi alterado.
    #[error("Invalid {name} ({value}): {reason}")]
    InvalidParameter {
        /// O parâmetro, como aparece no fonte ou no `.spf` (`#NUBITS`,
        /// `numClocks`).
        name: String,
        /// O valor recusado.
        value: String,
        /// A regra.
        reason: String,
    },

    /// Outra operação do Lace (build ou simulação) está usando o mesmo
    /// processador, ou o projeto: duas ao mesmo tempo regravariam os mesmos
    /// arquivos e quebrariam as duas. Vem de [`build`](crate::build),
    /// [`simulate`](crate::simulate) e
    /// [`simulate_project`](crate::simulate_project); nada foi alterado.
    #[error(
        "Another Lace operation is using {name} (a build or a simulation); try again when it finishes"
    )]
    OperationInProgress {
        /// O processador ou o projeto.
        name: String,
    },

    /// O netlist JSON do Yosys não pôde ser lido ou não tem o módulo pedido.
    ///
    /// Vem de [`render_schematic`](crate::render_schematic).
    #[error("Invalid netlist {path}: {reason}")]
    InvalidNetlist {
        /// O arquivo de netlist.
        path: Utf8PathBuf,
        /// O que está errado.
        reason: String,
    },

    /// Um arquivo de dados da simulação (`input_<n>.txt`, `output_<n>.txt`)
    /// tem uma linha que não é um inteiro decimal.
    ///
    /// Vem de [`read_data_file`](crate::read_data_file) e de
    /// [`Processor::read_output_values`](crate::Processor::read_output_values).
    #[error("{path}:{line}: {reason}")]
    InvalidDataFile {
        /// O arquivo.
        path: Utf8PathBuf,
        /// A linha, a partir de 1.
        line: u32,
        /// O que está errado.
        reason: String,
    },

    /// Nenhuma pasta, da pedida até a raiz do sistema, tem um `.spf`
    /// ([`Project::discover`](crate::Project::discover)).
    #[error("No Lace project here: no .spf file in {0} or in any folder above it")]
    ProjectNotFound(Utf8PathBuf),

    /// O projeto ainda não tem relatório guardado: nenhuma operação foi
    /// registrada no histórico ([`history`](crate::history)).
    #[error("Project {0} has no stored reports yet")]
    NoReports(Utf8PathBuf),

    /// Não há relatório com esse identificador no histórico do projeto.
    #[error("Report {0} not found")]
    ReportNotFound(String),

    /// Um relatório guardado não pôde ser lido: o `record.json` sumiu, não é
    /// JSON, ou é de um formato que este Lace não entende.
    #[error("Invalid report {path}: {reason}")]
    InvalidReport {
        /// O arquivo.
        path: Utf8PathBuf,
        /// O que está errado.
        reason: String,
    },

    /// Os relatórios pedidos não se comparam: de projetos diferentes, sem
    /// estatísticas de síntese nem tempos de simulação, ou sem um anterior
    /// compatível.
    #[error("{0}")]
    NotComparable(String),

    /// Nenhuma placa com esse identificador ([`crate::fpga::board()`]).
    #[error("No board {name} (known boards: {})", available.join(", "))]
    BoardNotFound {
        /// O que foi pedido.
        name: String,
        /// As placas conhecidas.
        available: Vec<String>,
    },

    /// O JSON embutido de uma placa é inválido. Os testes impedem isso; se
    /// acontecer, é bug.
    #[error("Invalid board {id}: {reason}")]
    InvalidBoard {
        /// A placa.
        id: String,
        /// O que está errado.
        reason: String,
    },

    /// O projeto não tem `fpga.json` ([`crate::fpga::config`]): falta dizer a
    /// placa e as ligações.
    #[error("The project has no fpga.json ({0})")]
    NoFpgaConfig(Utf8PathBuf),

    /// O `fpga.json` não é válido: JSON malformado, ou ligações que não
    /// batem com a placa e o topo (uma por linha).
    #[error("Invalid {path}: {reason}")]
    InvalidFpgaConfig {
        /// O arquivo.
        path: Utf8PathBuf,
        /// O que está errado.
        reason: String,
    },

    /// Um caminho não é UTF-8. A API serializa caminhos para JSON e usa
    /// [`camino`](https://docs.rs/camino) em toda a superfície, então recusa
    /// esses caminhos na entrada. O texto é a representação com perdas.
    #[error("Path is not UTF-8: {0}")]
    NonUtf8Path(String),

    /// Caminho com caractere fora do ASCII (acento, `ç`) onde o simulador
    /// precisa abrir arquivo: o `$readmemb` e o `$fopen` do Icarus recusam o
    /// nome, e o processador simularia sem programa nem entradas. Mova o
    /// projeto para uma pasta sem acento.
    #[error(
        "Path with characters outside ASCII ({path}): the simulator cannot open the processor files there"
    )]
    NonAsciiPath {
        /// O caminho recusado.
        path: Utf8PathBuf,
    },

    /// Caminho longo demais para os buffers fixos dos compiladores YANC (ver
    /// [`YANC_PATH_LIMIT`](crate::YANC_PATH_LIMIT)). Mova o projeto para um
    /// diretório mais curto.
    #[error("Path too long for YANC ({len} bytes, limit {limit}): {path}")]
    PathTooLong {
        /// O caminho recusado.
        path: Utf8PathBuf,
        /// O tamanho dele, em bytes.
        len: usize,
        /// O limite.
        limit: usize,
    },

    /// Um subprocesso não chegou a iniciar: executável ausente, sem permissão
    /// de execução, CWD inexistente.
    ///
    /// Um processo que inicia e falha não é este erro; ele aparece como
    /// [`Termination`](crate::Termination) no relatório do passo.
    #[error("Could not run {program}: {source}")]
    Spawn {
        /// O programa que se tentou executar.
        program: Utf8PathBuf,
        /// O erro do sistema operacional.
        #[source]
        source: std::io::Error,
    },

    /// Um processo de longa duração (o Surfer) terminou logo depois de
    /// iniciar. Vem de
    /// [`RunningProcess::ensure_started`](crate::RunningProcess::ensure_started).
    #[error("{program} exited right after starting ({termination:?}); end of log {log}:\n{tail}")]
    ProcessExitedEarly {
        /// O programa.
        program: Utf8PathBuf,
        /// Como terminou.
        termination: crate::Termination,
        /// O arquivo de log completo.
        log: Utf8PathBuf,
        /// As últimas linhas do log.
        tail: String,
    },

    /// Falha de I/O do próprio Lace: criar diretório, ler ou gravar arquivo.
    #[error("{context}: {path}: {source}")]
    Io {
        /// O que o Lace estava fazendo, em texto ("Creating directory").
        context: &'static str,
        /// O caminho envolvido.
        path: Utf8PathBuf,
        /// O erro do sistema operacional.
        #[source]
        source: std::io::Error,
    },
}

impl LaceError {
    /// Identificador estável da variante, em `snake_case`, para JSON, logs e
    /// uma futura fronteira FFI. Nunca muda para uma variante existente.
    ///
    /// ```
    /// # use lace_core::LaceError;
    /// let err = LaceError::ProcessorExists("soma".into());
    /// assert_eq!(err.code(), "processor_exists");
    /// ```
    pub fn code(&self) -> &'static str {
        match self {
            LaceError::BundleNotFound { .. } => "bundle_not_found",
            LaceError::InvalidBundle { .. } => "invalid_bundle",
            LaceError::UnsupportedPlatform { .. } => "unsupported_platform",
            LaceError::ComponentMissing(_) => "component_missing",
            LaceError::ToolchainIncomplete { .. } => "toolchain_incomplete",
            LaceError::SystemCompilerMissing => "system_compiler_missing",
            LaceError::QuartusMissing => "quartus_missing",
            LaceError::NoBitstream(_) => "no_bitstream",
            LaceError::NoCable => "no_cable",
            LaceError::StaleBitstream { .. } => "stale_bitstream",
            LaceError::InvalidProject { .. } => "invalid_project",
            LaceError::ProjectExists(_) => "project_exists",
            LaceError::OutsideProject(_) => "outside_project",
            LaceError::CannotMove { .. } => "cannot_move",
            LaceError::PathExists(_) => "path_exists",
            LaceError::ProcessorExists(_) => "processor_exists",
            LaceError::ProcessorNotFound { .. } => "processor_not_found",
            LaceError::InvalidName { .. } => "invalid_name",
            LaceError::InvalidProjectFile { .. } => "invalid_project_file",
            LaceError::InvalidSource { .. } => "invalid_source",
            LaceError::NotBuilt { .. } => "not_built",
            LaceError::NoTestbench(_) => "no_testbench",
            LaceError::NoCocotbToplevel(_) => "no_cocotb_toplevel",
            LaceError::CocotbUnavailable { .. } => "cocotb_unavailable",
            LaceError::NoTopLevel(_) => "no_top_level",
            LaceError::EmptyProject(_) => "empty_project",
            LaceError::ModuleNotFound { .. } => "module_not_found",
            LaceError::AmbiguousModule { .. } => "ambiguous_module",
            LaceError::InvalidParameter { .. } => "invalid_parameter",
            LaceError::OperationInProgress { .. } => "operation_in_progress",
            LaceError::InvalidNetlist { .. } => "invalid_netlist",
            LaceError::InvalidDataFile { .. } => "invalid_data_file",
            LaceError::NonUtf8Path(_) => "non_utf8_path",
            LaceError::PathTooLong { .. } => "path_too_long",
            LaceError::NonAsciiPath { .. } => "non_ascii_path",
            LaceError::Spawn { .. } => "spawn",
            LaceError::ProcessExitedEarly { .. } => "process_exited_early",
            LaceError::Io { .. } => "io",
            LaceError::ProjectNotFound(_) => "project_not_found",
            LaceError::NoReports(_) => "no_reports",
            LaceError::ReportNotFound(_) => "report_not_found",
            LaceError::InvalidReport { .. } => "invalid_report",
            LaceError::NotComparable(_) => "not_comparable",
            LaceError::BoardNotFound { .. } => "board_not_found",
            LaceError::InvalidBoard { .. } => "invalid_board",
            LaceError::NoFpgaConfig(_) => "no_fpga_config",
            LaceError::InvalidFpgaConfig { .. } => "invalid_fpga_config",
        }
    }

    /// Fecho que transforma um `std::io::Error` em [`LaceError::Io`], para
    /// usar com `map_err`.
    pub(crate) fn io(
        context: &'static str,
        path: impl Into<Utf8PathBuf>,
    ) -> impl FnOnce(std::io::Error) -> LaceError {
        let path = path.into();
        move |source| LaceError::Io {
            context,
            path,
            source,
        }
    }
}
