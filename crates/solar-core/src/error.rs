//! Erros de infraestrutura do Core.
//!
//! # Duas categorias de falha
//!
//! O Solar separa "não consegui rodar" de "rodei e deu errado":
//!
//! - **`Err(SolarError)`**: a operação nem começou ou foi interrompida por algo
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
//! use solar_core::{SolarError, Toolchain};
//!
//! match Toolchain::open("/nao/existe") {
//!     Ok(_) => unreachable!(),
//!     Err(SolarError::InvalidBundle { path, .. }) => {
//!         assert_eq!(path, "/nao/existe");
//!     }
//!     Err(other) => panic!("inesperado: {other}"),
//! }
//! ```
//!
//! Para registrar ou serializar sem depender do enum, use
//! [`SolarError::code`], que devolve um identificador estável por variante.

use camino::Utf8PathBuf;

/// Atalho para `Result<T, SolarError>`, usado em toda a API.
pub type Result<T, E = SolarError> = std::result::Result<T, E>;

/// Tudo que impede uma operação do Solar de rodar.
///
/// O enum é `#[non_exhaustive]`: novas variantes podem surgir sem quebrar a
/// API, então todo `match` precisa de um braço `_`. A mensagem de
/// [`Display`](std::fmt::Display) é em português e pronta para mostrar ao
/// usuário.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SolarError {
    /// Não há bundle instalado junto com o Solar.
    ///
    /// Vem de [`Toolchain::locate`](crate::Toolchain::locate). O Solar só
    /// executa ferramentas do bundle; sem ele, nada que dependa de ferramenta
    /// roda.
    #[error("bundle de ferramentas não encontrado (procurado em: {})", searched.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(", "))]
    BundleNotFound {
        /// Os diretórios em que o Solar procurou um `bundle.json`.
        searched: Vec<Utf8PathBuf>,
    },

    /// O diretório não é um bundle válido para este Solar: sem `bundle.json`,
    /// JSON inválido, formato desconhecido, montado para outra plataforma, ou
    /// com componente ausente ou apontando para fora do bundle.
    ///
    /// Vem de [`Toolchain::open`](crate::Toolchain::open).
    #[error("bundle inválido em {path}: {reason}")]
    InvalidBundle {
        /// O diretório do bundle.
        path: Utf8PathBuf,
        /// O que está errado.
        reason: String,
    },

    /// O Solar está rodando numa plataforma para a qual não há bundle (só
    /// Linux x64, macOS arm64 e Windows x64).
    #[error(
        "plataforma não suportada: {os} {arch} (há bundle para linux-x64, darwin-arm64 e windows-x64)"
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
    /// Vem de [`Toolchain::tool`](crate::Toolchain::tool).
    #[error("o componente {0} não está instalado (rode o instalador do Solar de novo e escolha-o)")]
    ComponentMissing(String),

    /// Um arquivo que o bundle deveria ter não está lá: o executável de uma
    /// ferramenta, ou um diretório do YANC (`HDL`, `Macros`, `Header`).
    ///
    /// Vem de [`Toolchain::tool`](crate::Toolchain::tool) e dos métodos de
    /// diretório do YANC. Indica bundle corrompido ou incompleto: reinstale.
    #[error("bundle incompleto: {what} não encontrado em {path}")]
    ToolchainIncomplete {
        /// O que faltou, em texto (por exemplo "executável do yosys").
        what: String,
        /// Onde se esperava encontrar.
        path: Utf8PathBuf,
    },

    /// O Verilator precisa de compilador C++, `make` e Perl do sistema, e eles
    /// não foram encontrados nos locais padrão (ver
    /// [`SystemCompiler`](crate::SystemCompiler)).
    #[error(
        "o Verilator precisa de compilador C++, make e Perl do sistema, e eles não foram encontrados \
         (Linux: build-essential e perl em /usr/bin; macOS: Command Line Tools do Xcode; \
         Windows: MSYS2 em C:\\msys64 com gcc, make e perl)"
    )]
    SystemCompilerMissing,

    /// O projeto no disco não está como deveria: diretório ausente, arquivo
    /// onde deveria haver diretório, arquivo registrado que sumiu, arquivo que
    /// o Solar se recusa a sobrescrever.
    #[error("projeto inválido em {path}: {reason}")]
    InvalidProject {
        /// O caminho com problema.
        path: Utf8PathBuf,
        /// O que está errado, em texto.
        reason: String,
    },

    /// Já existe um `.spf` onde [`Project::create`](crate::Project::create)
    /// ia criar o projeto. Nada foi alterado.
    #[error("o projeto já existe: {0}")]
    ProjectExists(Utf8PathBuf),

    /// [`Project::add_processor`](crate::Project::add_processor) recebeu um
    /// nome que já está no projeto. Nada foi alterado.
    #[error("o processador '{0}' já existe no projeto")]
    ProcessorExists(String),

    /// O projeto não tem o processador pedido.
    ///
    /// Vem de [`Project::require_processor`](crate::Project::require_processor)
    /// e das operações que recebem um nome de processador.
    #[error("o projeto não tem o processador '{name}' (disponíveis: {})", available.join(", "))]
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
    #[error("nome inválido '{name}': {reason}")]
    InvalidName {
        /// O nome recusado.
        name: String,
        /// Por quê.
        reason: String,
    },

    /// O `.spf` não pôde ser lido: JSON quebrado mesmo depois da leitura
    /// tolerante, falta a seção `structure`, ou um campo tem tipo ou valor
    /// inválido (por exemplo `clk: 12.5`).
    #[error("arquivo de projeto inválido {path}: {reason}")]
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
    #[error("fonte inválido {path}: {reason}")]
    InvalidSource {
        /// O arquivo-fonte.
        path: Utf8PathBuf,
        /// O que está errado.
        reason: String,
    },

    /// A operação precisa de artefatos que [`build`](crate::build) ainda não
    /// gerou para este processador (testbench, Verilog).
    #[error("o processador '{processor}' não foi compilado: falta {missing}")]
    NotBuilt {
        /// O processador.
        processor: String,
        /// O primeiro artefato que faltou.
        missing: Utf8PathBuf,
    },

    /// [`simulate_project`](crate::simulate_project) foi chamado num projeto
    /// sem testbench. Registre um com
    /// [`Project::set_testbench`](crate::Project::set_testbench).
    #[error("o projeto {0} não tem testbench: registre um com set_testbench")]
    NoTestbench(Utf8PathBuf),

    /// [`check_syntax`](crate::check_syntax) ou
    /// [`synthesize`](crate::synthesize) com
    /// [`DesignTarget::TopLevel`](crate::DesignTarget::TopLevel) num projeto
    /// sem módulo de topo. Registre um com
    /// [`Project::set_top_level`](crate::Project::set_top_level).
    #[error("o projeto {0} não tem módulo de topo: registre um com set_top_level")]
    NoTopLevel(Utf8PathBuf),

    /// O netlist JSON do Yosys não pôde ser lido ou não tem o módulo pedido.
    ///
    /// Vem de [`render_schematic`](crate::render_schematic).
    #[error("netlist inválido {path}: {reason}")]
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

    /// Um caminho não é UTF-8. A API serializa caminhos para JSON e usa
    /// [`camino`](https://docs.rs/camino) em toda a superfície, então recusa
    /// esses caminhos na entrada. O texto é a representação com perdas.
    #[error("caminho não é UTF-8: {0}")]
    NonUtf8Path(String),

    /// Caminho longo demais para os buffers fixos dos compiladores YANC (ver
    /// [`YANC_PATH_LIMIT`](crate::YANC_PATH_LIMIT)). Mova o projeto para um
    /// diretório mais curto.
    #[error("caminho longo demais para o YANC ({len} bytes, limite {limit}): {path}")]
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
    #[error("não foi possível executar {program}: {source}")]
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
    #[error("{program} fechou logo ao abrir ({termination:?}); fim do log {log}:\n{tail}")]
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

    /// Falha de I/O do próprio Solar: criar diretório, ler ou gravar arquivo.
    #[error("{context}: {path}: {source}")]
    Io {
        /// O que o Solar estava fazendo, em texto ("criando diretório").
        context: &'static str,
        /// O caminho envolvido.
        path: Utf8PathBuf,
        /// O erro do sistema operacional.
        #[source]
        source: std::io::Error,
    },
}

impl SolarError {
    /// Identificador estável da variante, em `snake_case`, para JSON, logs e
    /// uma futura fronteira FFI. Nunca muda para uma variante existente.
    ///
    /// ```
    /// # use solar_core::SolarError;
    /// let err = SolarError::ProcessorExists("soma".into());
    /// assert_eq!(err.code(), "processor_exists");
    /// ```
    pub fn code(&self) -> &'static str {
        match self {
            SolarError::BundleNotFound { .. } => "bundle_not_found",
            SolarError::InvalidBundle { .. } => "invalid_bundle",
            SolarError::UnsupportedPlatform { .. } => "unsupported_platform",
            SolarError::ComponentMissing(_) => "component_missing",
            SolarError::ToolchainIncomplete { .. } => "toolchain_incomplete",
            SolarError::SystemCompilerMissing => "system_compiler_missing",
            SolarError::InvalidProject { .. } => "invalid_project",
            SolarError::ProjectExists(_) => "project_exists",
            SolarError::ProcessorExists(_) => "processor_exists",
            SolarError::ProcessorNotFound { .. } => "processor_not_found",
            SolarError::InvalidName { .. } => "invalid_name",
            SolarError::InvalidProjectFile { .. } => "invalid_project_file",
            SolarError::InvalidSource { .. } => "invalid_source",
            SolarError::NotBuilt { .. } => "not_built",
            SolarError::NoTestbench(_) => "no_testbench",
            SolarError::NoTopLevel(_) => "no_top_level",
            SolarError::InvalidNetlist { .. } => "invalid_netlist",
            SolarError::InvalidDataFile { .. } => "invalid_data_file",
            SolarError::NonUtf8Path(_) => "non_utf8_path",
            SolarError::PathTooLong { .. } => "path_too_long",
            SolarError::Spawn { .. } => "spawn",
            SolarError::ProcessExitedEarly { .. } => "process_exited_early",
            SolarError::Io { .. } => "io",
        }
    }

    /// Fecho que transforma um `std::io::Error` em [`SolarError::Io`], para
    /// usar com `map_err`.
    pub(crate) fn io(
        context: &'static str,
        path: impl Into<Utf8PathBuf>,
    ) -> impl FnOnce(std::io::Error) -> SolarError {
        let path = path.into();
        move |source| SolarError::Io {
            context,
            path,
            source,
        }
    }
}
