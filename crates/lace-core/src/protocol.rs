//! O contrato do orquestrador: o que um cliente pede, o que recebe de volta
//! e o que ouve enquanto espera, em tipos com schema.
//!
//! É o mesmo contrato para quem linka o core (a CLI, o Studio) e para quem
//! fala com o Lace por um processo (`lace serve`, JSON-RPC 2.0 por linhas
//! sobre stdio; `lace mcp`, para um agente). Aqui moram só os tipos: quem
//! despacha um [`Request`] é o servidor, que linka também o instalador e o
//! lace-learn, de quem vêm os resultados de `update.*`, `install.*` e
//! `learn.*` ([`Response::Other`]).
//!
//! Os nomes dos métodos têm um grupo e um ponto (`project.status`,
//! `flow.run`); os campos são os das operações do core. O schema sai de
//! [`request_schema`] e [`response_schema`], gerados dos tipos: o primeiro
//! na vista de quem desserializa (os campos com padrão são opcionais), o
//! segundo na de quem serializa.

use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::experimental::FeatureState;
use crate::flows::{
    FlowOptions, FlowOutcome, FlowRequest, WaveChoice, WaveChoiceOutcome, WaveOpened, WaveRequest,
};
use crate::fpga::{BitstreamState, Board, FpgaConfig, Prepared};
use crate::history::{Cleanup, RunComparison, RunRecord, RunSummary};
use crate::locate::ToolchainInfo;
use crate::{
    AddedFile, ErrorInfo, FileMismatch, LayoutConversion, ListPosition, MovedPath, NewProcessor,
    Processor, ProcessorConfig, ProjectFile, ProjectStatus, RemovedProcessor, WaveSignals,
};

pub use crate::delivery::Notification;

/// A versão do protocolo. Sobe quando um método muda de forma de um jeito
/// que um cliente antigo não entenderia; um método novo não a sobe.
pub const PROTOCOL_VERSION: u32 = 1;

fn yes() -> bool {
    true
}

/// Declara os métodos num lugar só: a variante de [`Request`], o nome em
/// JSON e o tipo do resultado (o `result` do JSON-RPC). Daqui saem
/// [`METHODS`] e [`result_schema`]. Uma variante de [`Request`] que falte
/// aqui não compila: o `match` de `declared` não tem `_`.
macro_rules! methods {
    ($($variant:ident = $name:literal => $result:ty,)*) => {
        /// Os métodos do protocolo, como vão em JSON: o que um servidor anuncia
        /// em [`ServerInfo::methods`] e usa para distinguir um método
        /// desconhecido de um com parâmetros inválidos. Mais `$/cancel`, a
        /// notificação que não é pedido.
        pub const METHODS: &[&str] = &[$($name),*];

        /// O resultado de cada método, uma propriedade por método.
        #[derive(JsonSchema)]
        #[allow(dead_code, non_snake_case)]
        struct Results {
            $(
                #[serde(rename = $name)]
                $variant: $result,
            )*
        }

        #[allow(dead_code)]
        fn declared(request: &Request) {
            match request {
                $(Request::$variant { .. } => {})*
            }
        }
    };
}

methods! {
    Initialize = "initialize" => ServerInfo,
    Shutdown = "shutdown" => (),
    ProjectCreate = "project.create" => ProjectStatus,
    ProjectStatus = "project.status" => ProjectStatus,
    ProjectAdd = "project.add" => Vec<AddedFile>,
    ProjectRemove = "project.remove" => RemoveOutcome,
    ProjectTop = "project.top" => TopOutcome,
    ProjectTestbench = "project.testbench" => ProjectStatus,
    ProjectOrder = "project.order" => Vec<ProjectFile>,
    ProjectMove = "project.move" => MovedPath,
    ProjectConvert = "project.convert" => LayoutConversion,
    ProjectCheckName = "project.check_name" => NameCheck,
    ProcessorCheckName = "processor.check_name" => NameCheck,
    ProcessorAdd = "processor.add" => Processor,
    ProcessorSet = "processor.set" => Processor,
    ProcessorRemove = "processor.remove" => ProcessorRemoval,
    ProcessorOutputs = "processor.outputs" => Vec<i64>,
    FlowRun = "flow.run" => FlowOutcome,
    WaveOpen = "wave.open" => WaveOpened,
    WaveSignals = "wave.signals" => WaveSignals,
    WaveChoose = "wave.choose" => WaveChoiceOutcome,
    WaveResetLayout = "wave.reset_layout" => bool,
    HistoryList = "history.list" => Vec<RunSummary>,
    HistoryShow = "history.show" => ReportView,
    HistoryCompare = "history.compare" => RunComparison,
    HistoryPlanCleanup = "history.plan_cleanup" => Vec<String>,
    HistoryClean = "history.clean" => CleanOutcome,
    ToolchainInfo = "toolchain.info" => ToolchainInfo,
    ToolchainVerify = "toolchain.verify" => Vec<FileMismatch>,
    ExperimentalList = "experimental.list" => ExperimentalInfo,
    ExperimentalSet = "experimental.set" => ExperimentalInfo,
    FpgaBoards = "fpga.boards" => Vec<Board>,
    FpgaConfigGet = "fpga.config" => Option<FpgaConfig>,
    FpgaConfigSet = "fpga.config_set" => (),
    FpgaCheck = "fpga.check" => Prepared,
    FpgaModules = "fpga.modules" => Vec<String>,
    FpgaStatus = "fpga.status" => BitstreamState,
    FpgaCables = "fpga.cables" => Vec<String>,
    // O resultado vem do lace-installer e do lace-learn, que o core não
    // conhece ([`Response::Other`]).
    UpdateCheck = "update.check" => serde_json::Value,
    UpdateApply = "update.apply" => serde_json::Value,
    InstallAdd = "install.add" => serde_json::Value,
    LearnCheck = "learn.check" => serde_json::Value,
}

/// O que um cliente pede. Em JSON, `{"method": "...", "params": {...}}`;
/// um método sem parâmetros vai sem `params`. Os caminhos são absolutos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Request {
    /// O primeiro pedido de uma sessão: quem é o cliente. Resposta:
    /// [`ServerInfo`].
    Initialize {
        /// O cliente e a versão dele (`lace-vscode 0.1.0`).
        #[serde(default)]
        client: Option<String>,
    },
    /// Encerrar a sessão: os pedidos em andamento são cancelados.
    Shutdown,

    /// Cria `<parent>/<name>/<name>.spf`. Resposta: o [`ProjectStatus`] do
    /// projeto novo.
    #[serde(rename = "project.create")]
    ProjectCreate {
        /// A pasta de cima, que precisa existir.
        #[schemars(with = "String")]
        parent: Utf8PathBuf,
        /// O nome do projeto.
        name: String,
    },
    /// O retrato do projeto. Resposta: [`ProjectStatus`].
    #[serde(rename = "project.status")]
    ProjectStatus {
        /// O `.spf` ou a pasta do projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
    },
    /// Registra arquivos Verilog (criando os que não existem). Resposta: um
    /// [`AddedFile`] por arquivo.
    #[serde(rename = "project.add")]
    ProjectAdd {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// Os arquivos.
        #[schemars(with = "Vec<String>")]
        files: Vec<Utf8PathBuf>,
        /// Registrar como testbench em vez de decidir pelo conteúdo.
        #[serde(default)]
        testbench: bool,
    },
    /// Tira arquivos do projeto, sem apagar. Resposta: [`RemoveOutcome`].
    #[serde(rename = "project.remove")]
    ProjectRemove {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// Os arquivos.
        #[schemars(with = "Vec<String>")]
        files: Vec<Utf8PathBuf>,
    },
    /// Escolhe o topo por arquivo ou por nome de módulo; sem alvo, só
    /// consulta. Resposta: [`TopOutcome`].
    #[serde(rename = "project.top")]
    ProjectTop {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// O arquivo ou o módulo.
        #[serde(default)]
        target: Option<String>,
    },
    /// Escolhe o testbench da simulação do projeto. Resposta:
    /// [`ProjectStatus`].
    #[serde(rename = "project.testbench")]
    ProjectTestbench {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// O testbench.
        #[schemars(with = "String")]
        path: Utf8PathBuf,
    },
    /// Muda a posição de um arquivo na lista dele. Resposta: a lista, na
    /// ordem nova ([`ProjectFile`]).
    #[serde(rename = "project.order")]
    ProjectOrder {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// O arquivo.
        #[schemars(with = "String")]
        file: Utf8PathBuf,
        /// Para onde.
        position: ListPosition,
    },
    /// Move ou renomeia um arquivo ou pasta do projeto, mantendo o `.spf` em
    /// dia; com `after`, só registra um movimento já feito. Resposta:
    /// [`MovedPath`].
    #[serde(rename = "project.move")]
    ProjectMove {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// De onde.
        #[schemars(with = "String")]
        from: Utf8PathBuf,
        /// Para onde (o caminho final).
        #[schemars(with = "String")]
        to: Utf8PathBuf,
        /// O movimento já aconteceu fora do Lace.
        #[serde(default)]
        after: bool,
    },
    /// Converte um projeto da estrutura do YANC 6.x. Resposta:
    /// [`LayoutConversion`].
    #[serde(rename = "project.convert")]
    ProjectConvert {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
    },
    /// Um nome serve para um projeto novo? Resposta: [`NameCheck`].
    #[serde(rename = "project.check_name")]
    ProjectCheckName {
        /// O nome.
        name: String,
    },
    /// Um nome serve para um processador novo? Resposta: [`NameCheck`].
    #[serde(rename = "processor.check_name")]
    ProcessorCheckName {
        /// O nome.
        name: String,
    },
    /// Cria um processador. Resposta: o [`Processor`].
    #[serde(rename = "processor.add")]
    ProcessorAdd {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// O processador; só `name` e `language` são obrigatórios.
        spec: NewProcessor,
    },
    /// Muda os parâmetros de simulação de um processador. Resposta: o
    /// [`Processor`].
    #[serde(rename = "processor.set")]
    ProcessorSet {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// O processador.
        name: String,
        /// O que muda (`None` mantém).
        config: ProcessorConfig,
    },
    /// Tira um processador do projeto. Resposta: [`ProcessorRemoval`].
    #[serde(rename = "processor.remove")]
    ProcessorRemove {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// O processador.
        name: String,
        /// Mandar a pasta dele para a lixeira do sistema.
        #[serde(default)]
        to_trash: bool,
    },
    /// Os valores de uma porta de saída da última simulação do processador.
    /// Resposta: os valores.
    #[serde(rename = "processor.outputs")]
    ProcessorOutputs {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// O processador.
        processor: String,
        /// A porta.
        port: u32,
    },

    /// Roda um fluxo ([`FlowRequest`]). Enquanto roda, as notificações
    /// (`phase`, `events`, `output_omitted`, `build`); no fim, o
    /// [`FlowOutcome`].
    #[serde(rename = "flow.run")]
    FlowRun {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// O fluxo.
        request: FlowRequest,
        /// As opções da interface; sem elas, o comando é o da CLI
        /// equivalente, a onda abre em janela e o RTL viewer no navegador.
        #[serde(default)]
        options: Option<FlowOptions>,
        /// Receber a saída das ferramentas ao vivo (`events`). Sem isso, o
        /// `vvp` roda com buffer, que é mais rápido.
        #[serde(default = "yes")]
        output: bool,
    },

    /// Abre uma onda no surfer-aurora. Resposta: [`WaveOpened`].
    #[serde(rename = "wave.open")]
    WaveOpen {
        /// O projeto, para uma onda do projeto; `None` para um arquivo.
        #[serde(default)]
        #[schemars(with = "Option<String>")]
        project: Option<Utf8PathBuf>,
        /// Qual onda e com que layout.
        request: WaveRequest,
    },
    /// A árvore de sinais do testbench do projeto, com a escolha gravada.
    /// Resposta: [`WaveSignals`].
    #[serde(rename = "wave.signals")]
    WaveSignals {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
    },
    /// Muda a escolha de sinais. Resposta: [`WaveChoiceOutcome`].
    #[serde(rename = "wave.choose")]
    WaveChoose {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// A mudança.
        choice: WaveChoice,
    },
    /// Apaga o layout salvo de uma onda. Resposta: se havia.
    #[serde(rename = "wave.reset_layout")]
    WaveResetLayout {
        /// A onda.
        #[schemars(with = "String")]
        waveform: Utf8PathBuf,
    },

    /// Os relatórios do projeto, do mais novo para o mais antigo. Resposta:
    /// [`RunSummary`].
    #[serde(rename = "history.list")]
    HistoryList {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// Quantos, no máximo.
        #[serde(default)]
        limit: Option<usize>,
    },
    /// Um relatório (o mais novo sem `id`). Resposta: [`ReportView`].
    #[serde(rename = "history.show")]
    HistoryShow {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// `run-000042` ou `42`.
        #[serde(default)]
        id: Option<String>,
    },
    /// Compara dois relatórios. Resposta: [`RunComparison`].
    #[serde(rename = "history.compare")]
    HistoryCompare {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// O atual (o mais novo sem ele).
        #[serde(default)]
        id: Option<String>,
        /// A referência (o anterior comparável sem ela).
        #[serde(default)]
        against: Option<String>,
    },
    /// Quais relatórios uma limpeza apagaria, sem apagar. Resposta: os
    /// identificadores.
    #[serde(rename = "history.plan_cleanup")]
    HistoryPlanCleanup {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// A regra.
        cleanup: Cleanup,
    },
    /// Apaga exatamente estes relatórios. Resposta: [`CleanOutcome`].
    #[serde(rename = "history.clean")]
    HistoryClean {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// Os identificadores (do plano).
        ids: Vec<String>,
    },

    /// O bundle e as ferramentas. Resposta: [`ToolchainInfo`].
    #[serde(rename = "toolchain.info")]
    ToolchainInfo,
    /// Confere o SHA-256 dos executáveis. Resposta: os que não conferem
    /// ([`FileMismatch`]; vazio, todos conferem).
    #[serde(rename = "toolchain.verify")]
    ToolchainVerify,

    /// Os recursos experimentais e se estão ligados. Resposta:
    /// [`ExperimentalInfo`].
    #[serde(rename = "experimental.list")]
    ExperimentalList,
    /// Liga ou desliga um recurso experimental. Resposta:
    /// [`ExperimentalInfo`].
    #[serde(rename = "experimental.set")]
    ExperimentalSet {
        /// O recurso (`fpga`).
        feature: String,
        /// Ligado.
        enabled: bool,
    },

    /// As placas conhecidas, ou uma. Resposta: [`Board`].
    #[serde(rename = "fpga.boards")]
    FpgaBoards {
        /// Uma placa; `None`, todas.
        #[serde(default)]
        board: Option<String>,
    },
    /// O `fpga.json` do projeto. Resposta: o [`FpgaConfig`], ou `null`.
    #[serde(rename = "fpga.config")]
    FpgaConfigGet {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
    },
    /// Grava o `fpga.json`. Resposta: nada.
    #[serde(rename = "fpga.config_set")]
    FpgaConfigSet {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
        /// A configuração.
        config: FpgaConfig,
    },
    /// Confere o `fpga.json` contra a placa e o topo. Resposta:
    /// [`Prepared`].
    #[serde(rename = "fpga.check")]
    FpgaCheck {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
    },
    /// Os módulos do projeto que podem ir para a placa. Resposta: os
    /// nomes, na ordem dos arquivos.
    #[serde(rename = "fpga.modules")]
    FpgaModules {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
    },
    /// O arquivo de gravação da última compilação e se serve. Resposta:
    /// [`BitstreamState`].
    #[serde(rename = "fpga.status")]
    FpgaStatus {
        /// O projeto.
        #[schemars(with = "String")]
        project: Utf8PathBuf,
    },
    /// Os cabos de gravação ligados. Resposta: os nomes.
    #[serde(rename = "fpga.cables")]
    FpgaCables,

    /// As versões: este Lace, a última release, o bundle, cada componente.
    /// Resposta: o relatório do instalador.
    #[serde(rename = "update.check")]
    UpdateCheck,
    /// Atualiza a instalação. Enquanto roda, `install`; no fim, o
    /// relatório do instalador. Com `updated`, o servidor encerra em
    /// seguida: o binário dele foi trocado.
    #[serde(rename = "update.apply")]
    UpdateApply {
        /// O instalador inteiro, sem tentar por componentes.
        #[serde(default)]
        full: bool,
        /// Um instalador no disco, sem rede.
        #[serde(default)]
        #[schemars(with = "Option<String>")]
        from: Option<Utf8PathBuf>,
    },
    /// Instala componentes do bundle. Enquanto roda, `install`; no fim, o
    /// relatório do instalador.
    #[serde(rename = "install.add")]
    InstallAdd {
        /// Os componentes.
        components: Vec<String>,
    },

    /// Corrige um exercício do `lace learn` (ou todos). Resposta: o
    /// relatório da correção do lace-learn.
    #[serde(rename = "learn.check")]
    LearnCheck {
        /// A pasta de exercícios, ou uma pasta dentro dela.
        #[schemars(with = "String")]
        root: Utf8PathBuf,
        /// O exercício; `None`, o atual.
        #[serde(default)]
        exercise: Option<String>,
        /// Todos os exercícios.
        #[serde(default)]
        all: bool,
    },
}

impl Request {
    /// O nome do método (`project.status`), como vai em JSON.
    pub fn method(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.get("method")?.as_str().map(str::to_owned))
            .unwrap_or_default()
    }

    /// O projeto a que o pedido se refere, quando há um: é por ele que um
    /// servidor enfileira os pedidos (um de cada vez por projeto).
    pub fn project(&self) -> Option<&Utf8PathBuf> {
        match self {
            Request::ProjectStatus { project }
            | Request::ProjectAdd { project, .. }
            | Request::ProjectRemove { project, .. }
            | Request::ProjectTop { project, .. }
            | Request::ProjectTestbench { project, .. }
            | Request::ProjectOrder { project, .. }
            | Request::ProjectMove { project, .. }
            | Request::ProjectConvert { project }
            | Request::ProcessorAdd { project, .. }
            | Request::ProcessorSet { project, .. }
            | Request::ProcessorRemove { project, .. }
            | Request::ProcessorOutputs { project, .. }
            | Request::FlowRun { project, .. }
            | Request::WaveSignals { project }
            | Request::WaveChoose { project, .. }
            | Request::HistoryList { project, .. }
            | Request::HistoryShow { project, .. }
            | Request::HistoryCompare { project, .. }
            | Request::HistoryPlanCleanup { project, .. }
            | Request::HistoryClean { project, .. }
            | Request::FpgaConfigGet { project }
            | Request::FpgaConfigSet { project, .. }
            | Request::FpgaCheck { project }
            | Request::FpgaModules { project }
            | Request::FpgaStatus { project } => Some(project),
            Request::WaveOpen { project, .. } => project.as_ref(),
            _ => None,
        }
    }

    /// O pedido só lê (o disco, o bundle) e não roda ferramenta: um
    /// servidor pode atendê-lo em paralelo com um fluxo longo do mesmo
    /// projeto, sem fila.
    pub fn is_read_only(&self) -> bool {
        matches!(
            self,
            Request::Initialize { .. }
                | Request::ProjectStatus { .. }
                | Request::ProjectCheckName { .. }
                | Request::ProcessorCheckName { .. }
                | Request::ProcessorOutputs { .. }
                | Request::HistoryList { .. }
                | Request::HistoryShow { .. }
                | Request::HistoryCompare { .. }
                | Request::HistoryPlanCleanup { .. }
                | Request::ToolchainInfo
                | Request::ToolchainVerify
                | Request::ExperimentalList
                | Request::FpgaBoards { .. }
                | Request::FpgaConfigGet { .. }
                | Request::FpgaModules { .. }
                | Request::FpgaStatus { .. }
        )
    }

    /// O pedido mexe na instalação do Lace (instalar, atualizar): um
    /// servidor o enfileira sozinho, com a trava da instalação.
    pub fn is_installation(&self) -> bool {
        matches!(
            self,
            Request::UpdateCheck | Request::UpdateApply { .. } | Request::InstallAdd { .. }
        )
    }
}

/// O que volta de um pedido que deu certo. Serializa como o valor em si (o
/// `result` do JSON-RPC), sem etiqueta: cada método diz qual é o dele.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Response {
    /// `initialize`.
    Server(ServerInfo),
    /// `project.create`, `project.status`, `project.testbench`.
    Status(Box<ProjectStatus>),
    /// `project.add`.
    Added(Vec<AddedFile>),
    /// `project.remove`.
    Removed(RemoveOutcome),
    /// `project.top`.
    Top(TopOutcome),
    /// `project.order`.
    Files(Vec<ProjectFile>),
    /// `project.move`.
    Moved(MovedPath),
    /// `project.convert`.
    Converted(LayoutConversion),
    /// `project.check_name`, `processor.check_name`.
    Name(NameCheck),
    /// `processor.add`, `processor.set`.
    Processor(Box<Processor>),
    /// `processor.remove`.
    ProcessorRemoved(ProcessorRemoval),
    /// `processor.outputs`.
    Values(Vec<i64>),
    /// `flow.run`.
    Flow(Box<FlowOutcome>),
    /// `wave.open`.
    Wave(WaveOpened),
    /// `wave.signals`.
    Signals(Box<WaveSignals>),
    /// `wave.choose`.
    Choice(WaveChoiceOutcome),
    /// `wave.reset_layout`.
    Flag(bool),
    /// `history.list`.
    Reports(Vec<RunSummary>),
    /// `history.show`.
    Report(Box<ReportView>),
    /// `history.compare`.
    Comparison(Box<RunComparison>),
    /// `history.plan_cleanup`.
    Ids(Vec<String>),
    /// `history.clean`.
    Cleaned(CleanOutcome),
    /// `toolchain.info`.
    Toolchain(Box<ToolchainInfo>),
    /// `toolchain.verify`.
    Mismatches(Vec<FileMismatch>),
    /// `experimental.list`, `experimental.set`.
    Experimental(ExperimentalInfo),
    /// `fpga.boards`.
    Boards(Vec<Board>),
    /// `fpga.config`.
    FpgaConfig(Option<FpgaConfig>),
    /// `fpga.check`.
    Prepared(Box<Prepared>),
    /// `fpga.modules`.
    Modules(Vec<String>),
    /// `fpga.status`.
    Bitstream(BitstreamState),
    /// `fpga.cables`.
    Cables(Vec<String>),
    /// `shutdown`, `fpga.config_set`: nada.
    Nothing(()),
    /// Um resultado de outro crate (`update.*`, `install.*`, `learn.*`),
    /// já em JSON; o schema dele é do crate de origem.
    Other(serde_json::Value),
}

/// O que `initialize` devolve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServerInfo {
    /// A versão do Lace.
    pub lace: String,
    /// A versão do protocolo ([`PROTOCOL_VERSION`]).
    pub protocol: u32,
    /// A pasta da instalação do Lace, quando o servidor é o `lace`
    /// instalado.
    #[schemars(with = "Option<String>")]
    pub prefix: Option<Utf8PathBuf>,
    /// Cada recurso experimental e se está ligado.
    pub experimental: BTreeMap<String, bool>,
    /// Os métodos que este servidor atende.
    pub methods: Vec<String>,
}

/// `project.remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RemoveOutcome {
    /// Tirados do projeto (continuam no disco).
    #[schemars(with = "Vec<String>")]
    pub removed: Vec<Utf8PathBuf>,
    /// Pedidos, mas que não estavam no projeto.
    #[schemars(with = "Vec<String>")]
    pub not_registered: Vec<Utf8PathBuf>,
}

/// `project.top`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TopOutcome {
    /// O arquivo de topo, se escolhido.
    #[schemars(with = "Option<String>")]
    pub top_level: Option<Utf8PathBuf>,
    /// O módulo de topo.
    pub module: Option<String>,
    /// Por que não dá para saber o módulo de um arquivo de topo escolhido.
    pub module_error: Option<ErrorInfo>,
}

/// `project.check_name` e `processor.check_name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct NameCheck {
    /// O nome serve.
    pub ok: bool,
    /// Por que não serve (`invalid_name`).
    pub error: Option<ErrorInfo>,
}

/// `processor.remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ProcessorRemoval {
    /// O que saiu: o nome, a pasta e os arquivos que saíram das listas.
    pub removed: RemovedProcessor,
    /// A pasta foi para a lixeira do sistema.
    pub trashed: bool,
}

/// `history.show`.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct ReportView {
    /// O identificador (`run-000042`).
    pub id: String,
    /// O `report.txt`.
    #[schemars(with = "String")]
    pub path: Utf8PathBuf,
    /// O que a comparação usa.
    pub record: RunRecord,
    /// O `report.txt`, sem mudança.
    pub text: String,
}

/// `history.clean`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CleanOutcome {
    /// Os apagados, do mais antigo para o mais novo.
    pub removed: Vec<String>,
    /// Quantos ficaram.
    pub kept: usize,
}

/// `experimental.list` e `experimental.set`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ExperimentalInfo {
    /// O `experimental.json`; `None` sem pasta de configuração.
    #[schemars(with = "Option<String>")]
    pub path: Option<Utf8PathBuf>,
    /// Cada recurso e se está ligado.
    pub components: Vec<FeatureState>,
}

/// Como um pedido terminou, do ponto de vista de quem o atende.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownReason {
    /// O cliente pediu (`shutdown`) ou fechou a entrada.
    Requested,
    /// O processo do servidor recebeu um sinal.
    Signal,
    /// A instalação foi atualizada e o binário do servidor é outro: o
    /// cliente o abre de novo.
    Updated,
}

/// O schema dos pedidos, na vista de quem desserializa: um campo com padrão
/// é opcional. Em `$defs`, cada tipo que um pedido carrega.
pub fn request_schema() -> schemars::Schema {
    schemars::generate::SchemaSettings::draft2020_12()
        .for_deserialize()
        .into_generator()
        .into_root_schema_for::<Request>()
}

/// O schema das respostas e das notificações, na vista de quem serializa:
/// um campo que sai sempre é obrigatório. Em `$defs`, cada tipo de
/// resultado, mais [`Notification`], [`ErrorInfo`], [`ServerInfo`] e
/// [`ShutdownReason`].
pub fn response_schema() -> schemars::Schema {
    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct Responses {
        response: Response,
        notification: Notification,
        error: ErrorInfo,
        shutdown: ShutdownReason,
    }
    schemars::generate::SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<Responses>()
}

/// O schema do resultado de cada método, na vista de quem serializa: um
/// objeto com uma propriedade por método (`project.status`), na ordem de
/// [`METHODS`], que é o `result` que o servidor devolve a ele. Em `$defs`,
/// os tipos dos resultados.
pub fn result_schema() -> schemars::Schema {
    schemars::generate::SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<Results>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_have_dotted_names_and_optional_params() {
        let request: Request =
            serde_json::from_str(r#"{"method":"project.status","params":{"project":"/p"}}"#)
                .unwrap();
        assert_eq!(request.method(), "project.status");
        assert_eq!(request.project().unwrap(), "/p");
        assert!(request.is_read_only());

        let request: Request = serde_json::from_str(r#"{"method":"toolchain.info"}"#).unwrap();
        assert_eq!(request, Request::ToolchainInfo);
        assert!(request.project().is_none());

        let request: Request = serde_json::from_str(
            r#"{"method":"flow.run","params":{"project":"/p","request":{"flow":"build"}}}"#,
        )
        .unwrap();
        assert!(matches!(
            request,
            Request::FlowRun {
                output: true,
                options: None,
                ..
            }
        ));
        assert!(!request.is_read_only());
        assert_eq!(request.method(), "flow.run");

        let request: Request = serde_json::from_str(
            r#"{"method":"processor.add","params":{"project":"/p","spec":{"name":"soma","language":"cmm"}}}"#,
        )
        .unwrap();
        if let Request::ProcessorAdd { spec, .. } = request {
            assert_eq!(spec.nubits, 23, "os padrões valem no protocolo");
        } else {
            panic!("outro pedido");
        }
        assert!(
            Request::UpdateApply {
                full: false,
                from: None
            }
            .is_installation()
        );
    }

    #[test]
    fn the_method_list_matches_the_request_enum() {
        // Cada nome da lista é um método que o enum aceita (com parâmetros
        // vazios ou inválidos, o erro é de parâmetros, não de método), e
        // cada variante do schema está na lista.
        let schema = serde_json::to_value(request_schema()).unwrap();
        let mut from_schema: Vec<String> = schema["oneOf"]
            .as_array()
            .expect("oneOf")
            .iter()
            .filter_map(|v| {
                v["properties"]["method"]["const"]
                    .as_str()
                    .map(str::to_owned)
            })
            .collect();
        from_schema.sort();
        let mut listed: Vec<String> = METHODS.iter().map(|m| (*m).to_owned()).collect();
        listed.sort();
        assert_eq!(listed, from_schema);
    }

    #[test]
    fn every_method_has_a_result_in_the_schema() {
        let schema = serde_json::to_value(result_schema()).unwrap();
        let results = schema["properties"].as_object().expect("properties");
        let methods: Vec<&str> = results.keys().map(String::as_str).collect();
        assert_eq!(methods, METHODS, "um resultado por método, na ordem");
        assert_eq!(results["flow.run"]["$ref"], "#/$defs/FlowOutcome");
        assert_eq!(results["shutdown"]["type"], "null");
        assert_eq!(results["processor.outputs"]["type"], "array");
    }

    #[test]
    fn responses_serialize_as_the_value_itself() {
        let value = serde_json::to_value(Response::Values(vec![1, 2])).unwrap();
        assert_eq!(value, serde_json::json!([1, 2]));
        let value = serde_json::to_value(Response::Nothing(())).unwrap();
        assert!(value.is_null());
        let value = serde_json::to_value(Response::Name(NameCheck {
            ok: false,
            error: Some(ErrorInfo::new("invalid_name", "x")),
        }))
        .unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "invalid_name");
    }

    #[test]
    fn the_schemas_cover_the_request_and_the_results() {
        let request = serde_json::to_value(request_schema()).unwrap();
        let defs = request["$defs"].as_object().expect("defs");
        for name in [
            "FlowRequest",
            "NewProcessor",
            "ListPosition",
            "Cleanup",
            "WaveRequest",
        ] {
            assert!(
                defs.contains_key(name),
                "{name} falta no schema dos pedidos"
            );
        }
        // Na vista de quem desserializa, os campos com padrão são opcionais.
        let spec = &defs["NewProcessor"];
        let required: Vec<&str> = spec["required"]
            .as_array()
            .map(|r| r.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        assert_eq!(required, ["name", "language"], "{spec}");

        let response = serde_json::to_value(response_schema()).unwrap();
        let defs = response["$defs"].as_object().expect("defs");
        for name in [
            "FlowOutcome",
            "ProjectStatus",
            "ToolchainInfo",
            "Notification",
            "ErrorInfo",
            "ServerInfo",
            "WaveOpened",
        ] {
            assert!(
                defs.contains_key(name),
                "{name} falta no schema das respostas"
            );
        }
    }
}
