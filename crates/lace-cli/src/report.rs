//! O contrato do `--json`: o que cada comando escreve, um tipo por comando.
//!
//! A CLI só escreve JSON por `Output::json`, que só aceita tipos [`Report`].
//! O JSON Schema de cada um sai destes tipos (e dos resultados do Core, que
//! eles carregam) e fica versionado em `docs/schema/<comando>.json`. Um teste
//! falha quando o versionado e o gerado diferem. Mudou um tipo daqui ou um
//! resultado do Core? Gere de novo:
//!
//! ```sh
//! LACE_UPDATE_SCHEMA=1 cargo test -p lace-cli schema
//! ```
//!
//! e confira o diff de `docs/schema/` antes de mandar: é a mudança que quem
//! lê o `--json` (uma extensão de editor, um script) vai ver.

use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use lace_core::history::{RunComparison, RunRecord, RunSummary};
use lace_core::{
    AddedFile, BuildResult, BundleComponent, CheckResult, Event, FileMismatch, HierarchyResult,
    MovedPath, Processor, ProjectFile, ProjectIssue, SchematicResult, SimulationResult,
    SynthesisResult, SystemCompiler, WaveProcessor,
};
use schemars::JsonSchema;
use serde::Serialize;

/// Um objeto que a CLI escreve com `--json`. Implemente só pela macro
/// `reports!` abaixo, que também registra o schema do tipo.
pub trait Report: Serialize + JsonSchema {}

macro_rules! reports {
    ($($name:literal => $type:ty),* $(,)?) => {
        $(impl Report for $type {})*

        /// O schema de cada saída, com o nome do arquivo em `docs/schema/`.
        #[cfg(test)]
        pub fn schemas() -> Vec<(&'static str, schemars::Schema)> {
            vec![
                $(($name, schema_of::<$type>()),)*
                ("events", schema_of::<EventLine>()),
            ]
        }
    };
}

/// O schema do que a CLI escreve, e não do que ela aceitaria ler: um campo
/// que sai sempre, mesmo `null`, é obrigatório.
#[cfg(test)]
fn schema_of<T: JsonSchema>() -> schemars::Schema {
    schemars::generate::SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<T>()
}

reports! {
    "new" => NewReport,
    "status" => StatusReport,
    "add" => AddReport,
    "remove" => RemoveReport,
    "top" => TopReport,
    "move" => MoveReport,
    "order" => OrderReport,
    "proc" => Processor,
    "build" => BuildReport,
    "check" => CheckReport,
    "hierarchy" => HierarchyResult,
    "sim" => SimReport,
    "wave" => WaveReport,
    "synth" => SynthReport,
    "report" => ReportShowReport,
    "report-list" => ReportListReport,
    "report-clean" => ReportCleanReport,
    "report-compare" => RunComparison,
    "tools" => ToolsReport,
    "install" => InstallReport,
    "uninstall" => UninstallReport,
    "update" => crate::update::UpdateReport,
    "error" => ErrorReport,
}

/// `lace new`: o projeto criado.
#[derive(Serialize, JsonSchema)]
pub struct NewReport {
    /// Sempre `"Project created"`.
    pub message: String,
    /// O `.spf` criado.
    #[schemars(with = "String")]
    pub path: Utf8PathBuf,
    /// A pasta do projeto.
    #[schemars(with = "String")]
    pub root: Utf8PathBuf,
}

/// `lace status`: o projeto inteiro.
#[derive(Serialize, JsonSchema)]
pub struct StatusReport {
    /// Nome do projeto (o do `.spf`).
    pub name: String,
    /// O `.spf`.
    #[schemars(with = "String")]
    pub spf: Utf8PathBuf,
    /// A pasta do projeto.
    #[schemars(with = "String")]
    pub root: Utf8PathBuf,
    /// Os módulos registrados.
    pub synthesizable: Vec<ProjectFile>,
    /// Os testbenches registrados.
    pub testbench: Vec<ProjectFile>,
    /// O arquivo de topo, se escolhido.
    #[schemars(with = "Option<String>")]
    pub top_level: Option<Utf8PathBuf>,
    /// O módulo de topo: o `topLevelModule` do `.spf`, o único módulo do
    /// arquivo de topo ou o que tem o nome dele. `null` se não dá para saber.
    pub top_module: Option<String>,
    /// O testbench que `lace sim` simula.
    #[schemars(with = "Option<String>")]
    pub selected_testbench: Option<Utf8PathBuf>,
    /// O módulo desse testbench.
    pub testbench_module: Option<String>,
    /// Arquivos `.v` e `.sv` da pasta que não estão no projeto.
    #[schemars(with = "Vec<String>")]
    pub unregistered: Vec<Utf8PathBuf>,
    /// O processador em cuja pasta o comando rodou: o que `build`, `check`,
    /// `sim`, `wave`, `synth` e `proc set` usam quando não recebem nome.
    /// `null` fora da pasta de um processador.
    pub here: Option<String>,
    /// Os processadores SAPHO.
    pub processors: Vec<ProcessorStatus>,
    /// O que está estranho no `.spf` sem impedir de abrir
    /// (`Project::issues`): caminho de outra máquina achado pela cauda,
    /// topo fora da lista, processador com nome que não compila.
    pub issues: Vec<ProjectIssue>,
}

/// Um processador em `lace status`.
#[derive(Serialize, JsonSchema)]
pub struct ProcessorStatus {
    #[serde(flatten)]
    pub processor: Processor,
    /// Já foi compilado: o Verilog e o testbench gerados existem.
    pub built: bool,
}

/// `lace add`.
#[derive(Serialize, JsonSchema)]
pub struct AddReport {
    /// O `.spf`.
    #[schemars(with = "String")]
    pub project: Utf8PathBuf,
    /// Um por arquivo, na ordem da linha de comando.
    pub files: Vec<AddedFile>,
}

/// `lace remove`.
#[derive(Serialize, JsonSchema)]
pub struct RemoveReport {
    /// O `.spf`.
    #[schemars(with = "String")]
    pub project: Utf8PathBuf,
    /// Tirados do projeto (continuam no disco).
    #[schemars(with = "Vec<String>")]
    pub removed: Vec<Utf8PathBuf>,
    /// Pedidos, mas que não estavam no projeto.
    #[schemars(with = "Vec<String>")]
    pub not_registered: Vec<Utf8PathBuf>,
}

/// `lace move`.
#[derive(Serialize, JsonSchema)]
pub struct MoveReport {
    /// O `.spf`.
    #[schemars(with = "String")]
    pub project: Utf8PathBuf,
    /// Um por origem, na ordem da linha de comando.
    pub moved: Vec<MovedPath>,
}

/// `lace order`.
#[derive(Serialize, JsonSchema)]
pub struct OrderReport {
    /// O `.spf`.
    #[schemars(with = "String")]
    pub project: Utf8PathBuf,
    /// A lista em que o arquivo está, na ordem nova.
    pub files: Vec<ProjectFile>,
}

/// `lace top`.
#[derive(Serialize, JsonSchema)]
pub struct TopReport {
    /// O arquivo de topo, se escolhido.
    #[schemars(with = "Option<String>")]
    pub top_level: Option<Utf8PathBuf>,
    /// O módulo de topo.
    pub module: Option<String>,
    /// Por que não dá para saber o módulo de um arquivo de topo escolhido
    /// (vários módulos, nenhum com o nome do arquivo).
    pub module_error: Option<ErrorInfo>,
}

/// `lace build`.
#[derive(Serialize, JsonSchema)]
pub struct BuildReport {
    /// O `.spf`.
    #[schemars(with = "String")]
    pub project: Utf8PathBuf,
    /// Um por processador compilado. Vazio num projeto sem processadores.
    pub results: Vec<BuildResult>,
    /// O relatório gravado no histórico do projeto (`run-000042`), para
    /// `lace report show`; `null` se nada rodou ou se não pôde ser gravado.
    pub report: Option<String>,
}

/// `lace check`.
#[derive(Serialize, JsonSchema)]
pub struct CheckReport {
    /// A verificação. Não compila os processadores: verifica o Verilog que
    /// está no disco.
    pub check: CheckResult,
    /// O relatório gravado no histórico do projeto (`run-000042`), para
    /// `lace report show`; `null` se nada rodou ou se não pôde ser gravado.
    pub report: Option<String>,
}

/// `lace sim`.
#[derive(Serialize, JsonSchema)]
pub struct SimReport {
    /// Os processadores, compilados antes. Se um falha, `simulation` é
    /// `null`.
    pub builds: Vec<BuildResult>,
    /// A simulação.
    pub simulation: Option<SimulationResult>,
    /// Com `-p`, o que a simulação escreveu em cada porta de saída.
    pub outputs: Vec<PortValues>,
    /// Com `--open`, o processo do surfer-aurora.
    pub surfer_pid: Option<u32>,
    /// O relatório gravado no histórico do projeto (`run-000042`), para
    /// `lace report show`; `null` se nada rodou ou se não pôde ser gravado.
    pub report: Option<String>,
}

/// O que a simulação de um processador escreveu numa porta de saída.
#[derive(Serialize, JsonSchema)]
pub struct PortValues {
    /// O número da porta.
    pub port: u32,
    /// `Simulation/output_<porta>.txt`.
    #[schemars(with = "String")]
    pub path: Utf8PathBuf,
    /// Os valores, na ordem em que saíram. Vazio quando a porta não pôde ser
    /// lida.
    pub values: Vec<i64>,
    /// Por que a porta não pôde ser lida: uma linha que não é inteiro, como o
    /// `x` que o simulador escreve numa divisão por zero. `null` quando leu.
    pub error: Option<String>,
}

/// `lace wave`.
#[derive(Serialize, JsonSchema)]
pub struct WaveReport {
    /// A onda aberta.
    #[schemars(with = "String")]
    pub waveform: Utf8PathBuf,
    /// O processo do surfer-aurora, que continua aberto.
    pub pid: u32,
    /// Onde vai o stdout e o stderr do surfer-aurora.
    #[schemars(with = "String")]
    pub log: Utf8PathBuf,
    /// O estado do Surfer gerado para os processadores SAPHO da onda
    /// (`.surf.ron`); `null` sem processador, com `--no-layout` ou numa onda
    /// que não é VCD nem FST.
    #[schemars(with = "Option<String>")]
    pub layout: Option<Utf8PathBuf>,
    /// Os processadores do layout.
    pub processors: Vec<WaveProcessor>,
}

/// `lace synth`.
#[derive(Serialize, JsonSchema)]
pub struct SynthReport {
    /// Os processadores, compilados antes. Se um falha, o resto é `null`.
    pub builds: Vec<BuildResult>,
    /// A síntese.
    pub synthesis: Option<SynthesisResult>,
    /// Com `--svg`, o esquemático.
    pub schematic: Option<SchematicResult>,
    /// O relatório gravado no histórico do projeto (`run-000042`), para
    /// `lace report show`; `null` se nada rodou ou se não pôde ser gravado.
    pub report: Option<String>,
}

/// `lace report` e `lace report show`: um relatório guardado.
#[derive(Serialize, JsonSchema)]
pub struct ReportShowReport {
    /// O identificador (`run-000042`).
    pub id: String,
    /// O `report.txt`.
    #[schemars(with = "String")]
    pub path: Utf8PathBuf,
    /// O que a comparação usa: contexto, estatísticas e tempos.
    pub record: RunRecord,
    /// O `report.txt`, sem mudança.
    pub text: String,
}

/// `lace report list`: os relatórios, do mais novo para o mais antigo.
#[derive(Serialize, JsonSchema)]
pub struct ReportListReport {
    /// Os relatórios, até o `--limit`.
    pub reports: Vec<RunSummary>,
}

/// `lace report clean`: os relatórios apagados.
#[derive(Serialize, JsonSchema)]
pub struct ReportCleanReport {
    /// Os apagados, do mais antigo para o mais novo. Vazio se não havia o
    /// que apagar.
    pub removed: Vec<String>,
    /// Quantos ficaram no histórico.
    pub kept: usize,
}

/// `lace tools`.
#[derive(Serialize, JsonSchema)]
pub struct ToolsReport {
    /// A pasta do bundle.
    #[schemars(with = "String")]
    pub root: Utf8PathBuf,
    /// A versão do bundle.
    pub bundle: String,
    /// A plataforma do bundle (`linux-x64`, `darwin-arm64`, `windows-x64`).
    pub platform: String,
    /// Os componentes instalados.
    pub components: Vec<BundleComponent>,
    /// Os componentes que este Lace conhece e que não estão instalados.
    pub not_installed: Vec<String>,
    /// Cada ferramenta, pelo nome do executável.
    pub tools: BTreeMap<String, ToolEntry>,
    /// O compilador do Verilator, se encontrado: do sistema no Linux e no
    /// macOS, do bundle no Windows (`bundled`).
    pub system_compiler: Option<SystemCompiler>,
    /// Com `--verify`, os executáveis cujo SHA-256 não confere (vazio: todos
    /// conferem).
    pub verify: Option<Vec<FileMismatch>>,
}

/// Uma ferramenta em `lace tools`: onde está, ou por que não está.
#[derive(Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ToolEntry {
    /// Disponível.
    Found {
        /// O executável.
        #[schemars(with = "String")]
        path: Utf8PathBuf,
        /// Vem do sistema, não do bundle (só o Perl do Verilator, no Linux e
        /// no macOS).
        system: bool,
    },
    /// Indisponível.
    Missing {
        /// Por quê.
        error: ErrorInfo,
    },
}

/// `lace install`.
#[derive(Serialize, JsonSchema)]
pub struct InstallReport {
    /// A pasta da instalação.
    #[schemars(with = "String")]
    pub prefix: Utf8PathBuf,
    /// Os aplicativos do bundle instalados agora.
    pub components: Vec<String>,
    /// Os que entraram nesta chamada (com os que eles exigem).
    pub added: Vec<String>,
}

/// `lace uninstall`.
#[derive(Serialize, JsonSchema)]
pub struct UninstallReport {
    /// A pasta da instalação.
    #[schemars(with = "String")]
    pub prefix: Utf8PathBuf,
    /// `true`: a pasta e o atalho já saíram (Linux, macOS). `false`: o
    /// desinstalador do Windows foi aberto e termina depois que o `lace` sai.
    pub removed: bool,
}

/// Qualquer comando que não conseguiu rodar (código de saída 2).
#[derive(Serialize, JsonSchema)]
pub struct ErrorReport {
    /// O erro.
    pub error: ErrorInfo,
}

/// Um erro do Lace.
#[derive(Serialize, JsonSchema)]
pub struct ErrorInfo {
    /// Código estável, em `snake_case` (`no_testbench`, `component_missing`),
    /// ou `cli` para um erro da própria linha de comando.
    pub code: String,
    /// A mensagem, em inglês.
    pub message: String,
    /// O comando que resolve, quando há um.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// Uma linha do `--events`: um evento do Core enquanto as ferramentas rodam,
/// ou, na última linha, o resultado.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
enum EventLine {
    Event(Event),
    Result(ResultLine),
}

/// A última linha do `--events`.
#[derive(JsonSchema)]
#[allow(dead_code)]
struct ResultLine {
    /// Sempre `"result"`.
    event: ResultTag,
    /// O mesmo objeto que o comando escreve com `--json` (veja o schema do
    /// comando).
    result: serde_json::Map<String, serde_json::Value>,
}

#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
enum ResultTag {
    Result,
}

#[cfg(test)]
mod tests {
    use camino::Utf8Path;

    /// `docs/schema/` é o que `schemas()` gera. Com `LACE_UPDATE_SCHEMA`,
    /// regrava os arquivos em vez de comparar.
    #[test]
    fn schema_files_match_the_types() {
        let dir = Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/schema");
        let update = std::env::var_os("LACE_UPDATE_SCHEMA").is_some();
        let schemas = super::schemas();
        let mut stale = Vec::new();
        if update {
            std::fs::create_dir_all(&dir).unwrap();
        }
        for (name, schema) in &schemas {
            let path = dir.join(format!("{name}.json"));
            let text = serde_json::to_string_pretty(schema).unwrap() + "\n";
            if update {
                std::fs::write(&path, &text).unwrap();
            } else if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
                stale.push(path.file_name().unwrap().to_owned());
            }
        }
        // Um arquivo que nenhum comando gera mais também é contrato velho.
        for entry in dir.read_dir_utf8().unwrap() {
            let name = entry.unwrap().file_name().to_owned();
            let known = schemas.iter().any(|(n, _)| format!("{n}.json") == name);
            if !known {
                if update {
                    std::fs::remove_file(dir.join(&name)).unwrap();
                } else {
                    stale.push(name);
                }
            }
        }
        assert!(
            stale.is_empty(),
            "docs/schema não confere com os tipos: {stale:?}\n\
             gere de novo com: LACE_UPDATE_SCHEMA=1 cargo test -p lace-cli schema\n\
             e confira o diff, que é a mudança que os clientes do --json vão ver"
        );
    }
}
