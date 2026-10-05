//! O projeto SAPHO no disco, no mesmo formato da AURORA.
//!
//! ```text
//! <raiz>/
//!   <projeto>.spf              # JSON, ver o módulo spf
//!   <processador>/
//!     Software/<processador>.cmm | .cpp
//!     Hardware/                # .v e .mif gerados pelo asmcomp
//!     Simulation/              # input_N.txt / output_N.txt
//!   .lace/Temp/<processador>/ # intermediários (-t dos compiladores)
//! ```
//!
//! A raiz do projeto é sempre o diretório do `.spf`; o `basePath` gravado no
//! arquivo é ignorado na leitura, como a AURORA faz ao abrir um projeto
//! copiado de outra máquina.
//!
//! # Compatibilidade com a AURORA
//!
//! Um projeto criado pelo Lace abre na AURORA e vice-versa. O Lace lê o
//! `.spf` com a mesma tolerância (comentários, vírgula sobrando, BOM), grava
//! de forma atômica com indentação de 2 espaços, e preserva os campos que não
//! entende (`commandOverrides`, `metadata.lastOpened`, ...) e a ordem das
//! chaves. Ao contrário da AURORA, abrir um projeto não o regrava.
//!
//! # Exemplo
//!
//! ```
//! use lace_core::{Language, NewProcessor, Project};
//! # let tmp = tempfile::tempdir()?;
//! # let dir = camino::Utf8Path::from_path(tmp.path()).unwrap();
//!
//! let mut project = Project::create(dir, "demo")?;
//! let soma = project.add_processor(&NewProcessor::new("soma", Language::Cmm))?;
//! assert!(soma.source.ends_with("soma/Software/soma.cmm"));
//!
//! let reopened = Project::open(dir.join("demo"))?;
//! assert_eq!(reopened.processors().len(), 1);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::time::{SystemTime, UNIX_EPOCH};

use camino::{Utf8Path, Utf8PathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::{LaceError, Result};
use crate::{paths, spf};

/// Frequência padrão quando o `.spf` não traz `clk` (a da AURORA).
pub const DEFAULT_FREQUENCY_MHZ: u32 = 100;
/// Número de clocks padrão quando o `.spf` não traz `numClocks` (o da AURORA).
pub const DEFAULT_CLOCKS: u32 = 2000;

/// Maior `numClocks`: o testbench gerado conta os clocks num inteiro de 32
/// bits com sinal, e o `asmcomp` recusa acima disso.
pub const MAX_CLOCKS: u32 = i32::MAX as u32;

/// Maior frequência, em MHz. O testbench gerado usa `timescale 1ns/1ps`:
/// acima de 500000 MHz o meio período fica abaixo de 1 ps, vira 0, e o
/// Icarus recusa o `always` sem atraso.
pub const MAX_FREQUENCY_MHZ: u32 = 500_000;

/// Mais portas de entrada ou de saída que um processador novo pode ter. É
/// um teto do Lace, folgado para os projetos do laboratório: o `asmcomp`
/// estoura um buffer e gera testbench corrompido muito antes do limite do
/// `int` (com 100000 portas, 904 erros de sintaxe).
pub const MAX_PORTS: u32 = 256;

/// Maior nome de projeto. O nome aparece duas vezes em cada caminho
/// (`<nome>/<nome>.spf`), e o `.spf` é gravado por um temporário com sufixo:
/// com 248 caracteres, o nome do temporário passava do limite de 255 do
/// sistema de arquivos.
pub const MAX_PROJECT_NAME: usize = 64;

/// Maior nome de processador. O `cmmcomp` guarda o nome em buffers de 100 a
/// 128 bytes e morre com sinal 11 acima de 97 caracteres; o Lace deixa
/// folga.
pub const MAX_PROCESSOR_NAME: usize = 64;

/// Linguagem do programa de um processador. Em JSON: `"cmm"` ou `"cpp"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, JsonSchema, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Language {
    /// C±, compilado pelo `cmmcomp`.
    Cmm,
    /// C, pré-processado pelo `cpppp` e compilado pelo `cppcomp`. A AURORA e o
    /// YANC chamam de "cpp" e usam a extensão `.cpp`.
    Cpp,
}

impl Language {
    /// A extensão do fonte, sem ponto: `"cmm"` ou `"cpp"`.
    pub fn extension(self) -> &'static str {
        match self {
            Language::Cmm => "cmm",
            Language::Cpp => "cpp",
        }
    }

    fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "cmm" => Some(Language::Cmm),
            "cpp" => Some(Language::Cpp),
            _ => None,
        }
    }
}

/// Um processador do projeto, com todos os caminhos já absolutos.
///
/// Os valores de simulação (`frequency_mhz`, `clocks`, `show_arrays`) vêm do
/// `.spf` (`clk`, `numClocks`, `showArrays`) ou dos padrões da AURORA
/// ([`DEFAULT_FREQUENCY_MHZ`], [`DEFAULT_CLOCKS`], `false`). Para mudá-los
/// só num build, use [`BuildOptions`](crate::BuildOptions).
///
/// A linguagem é resolvida como na AURORA: `language` declarada no `.spf`;
/// senão a extensão de `sourceFile`/`cmmFile`; senão o arquivo que existir em
/// `Software/` (`<nome>.cmm` antes de `<nome>.cpp`); senão C±.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Processor {
    /// O nome: diretório, `#PRNAME`, nome do módulo Verilog e dos artefatos.
    pub name: String,
    /// A linguagem do fonte.
    pub language: Language,
    /// `<raiz>/<nome>`, o `-p` dos compiladores.
    #[schemars(with = "String")]
    pub dir: Utf8PathBuf,
    /// O programa-fonte, em `Software/`.
    #[schemars(with = "String")]
    pub source: Utf8PathBuf,
    /// Diretório de intermediários, o `-t` dos compiladores.
    #[schemars(with = "String")]
    pub temp_dir: Utf8PathBuf,
    /// Frequência de operação em MHz (`-f` do `asmcomp`).
    pub frequency_mhz: u32,
    /// Clocks a simular (`-c` do `asmcomp`, vai para o testbench).
    pub clocks: u32,
    /// Exporta arrays para a simulação (`-A` do `cmmcomp`).
    pub show_arrays: bool,
}

impl Processor {
    /// `<dir>/Software`: o fonte e o `.asm` gerado.
    pub fn software_dir(&self) -> Utf8PathBuf {
        self.dir.join("Software")
    }

    /// `<dir>/Hardware`: o Verilog e as memórias gerados pelo `asmcomp`.
    pub fn hardware_dir(&self) -> Utf8PathBuf {
        self.dir.join("Hardware")
    }

    /// `<dir>/Simulation`: `input_<n>.txt` (do usuário) e `output_<n>.txt`
    /// (escritos pela simulação).
    pub fn simulation_dir(&self) -> Utf8PathBuf {
        self.dir.join("Simulation")
    }

    /// O testbench do processador, `Simulation/<nome>_tb.v`: o que o
    /// `asmcomp` gera na pasta temporária, copiado para cá pelo
    /// [`build`](crate::build), como a AURORA faz. É o que a simulação roda.
    pub fn testbench_path(&self) -> Utf8PathBuf {
        self.simulation_dir().join(format!("{}_tb.v", self.name))
    }

    /// Onde o `asmcomp` grava o testbench: `<temp>/<nome>_tb.v`.
    pub(crate) fn generated_testbench(&self) -> Utf8PathBuf {
        self.temp_dir.join(format!("{}_tb.v", self.name))
    }

    /// O testbench que a simulação usa: o de `Simulation/` e, num projeto
    /// compilado por um Lace que ainda não o copiava, o da pasta temporária.
    pub(crate) fn simulated_testbench(&self) -> Utf8PathBuf {
        let copied = self.testbench_path();
        if copied.is_file() {
            copied
        } else {
            self.generated_testbench()
        }
    }

    /// Já foi compilado: o Verilog e o testbench gerados existem no disco.
    /// Não diz se estão atualizados em relação ao fonte.
    pub fn is_built(&self) -> bool {
        self.hardware_dir()
            .join(format!("{}.v", self.name))
            .is_file()
            && self.simulated_testbench().is_file()
    }

    /// Cria `Software/`, `Hardware/`, `Simulation/` e o diretório temporário,
    /// se faltarem. Os compiladores YANC não criam diretório de forma
    /// confiável (o `cmmcomp` não cria nenhum), então o Lace faz isso antes
    /// de qualquer execução.
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [
            self.software_dir(),
            self.hardware_dir(),
            self.simulation_dir(),
            self.temp_dir.clone(),
        ] {
            if dir.exists() && !dir.is_dir() {
                return Err(LaceError::InvalidProject {
                    path: dir,
                    reason: "Exists but is not a directory".into(),
                });
            }
            std::fs::create_dir_all(&dir).map_err(LaceError::io("Creating directory", &dir))?;
        }
        Ok(())
    }
}

/// Parâmetros do cabeçalho de um processador novo.
///
/// Os padrões são os da AURORA (`js/project/processor_defaults.ts`): o float
/// estreito do SAPHO, 23 = 16 + 6 + 1 bits. No fluxo C só as portas vão para
/// o fonte; o resto fica com o que o `cppcomp` foi compilado para assumir.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NewProcessor {
    /// Nome do processador: letras, dígitos e `_`, começando por letra ou `_`.
    pub name: String,
    /// Linguagem do fonte-modelo.
    pub language: Language,
    /// `#NUBITS`: largura da palavra de dados, em bits. Só C±.
    pub nubits: u32,
    /// `#NBMANT`: bits de mantissa do ponto flutuante do SAPHO (não é IEEE
    /// 754). Só C±.
    pub nbmant: u32,
    /// `#NBEXPO`: bits de expoente do ponto flutuante. Só C±.
    pub nbexpo: u32,
    /// `#NUGAIN`: divisor de `norm(x)`. Precisa ser potência de dois, senão o
    /// `cmmcomp` recusa (um divisor genérico viraria o caminho crítico da
    /// ULA). Só C±.
    pub nugain: u32,
    /// `#NDSTAC`: profundidade da pilha de dados. Só C±.
    pub ndstac: u32,
    /// `#SDEPTH`: profundidade da pilha de instruções (chamadas). Só C±.
    pub sdepth: u32,
    /// `#NUIOIN` / `#pragma yanc nuioin`: número de portas de entrada.
    pub input_ports: u32,
    /// `#NUIOOU` / `#pragma yanc nuioou`: número de portas de saída.
    pub output_ports: u32,
}

impl NewProcessor {
    /// Um processador com os padrões da AURORA: 23 bits de palavra, mantissa
    /// 16, expoente 6, ganho 128, pilhas de 5, uma porta de entrada e uma de
    /// saída.
    ///
    /// ```
    /// use lace_core::{Language, NewProcessor};
    ///
    /// let mut spec = NewProcessor::new("fft", Language::Cmm);
    /// spec.nubits = 32;
    /// spec.input_ports = 2;
    /// ```
    pub fn new(name: impl Into<String>, language: Language) -> Self {
        NewProcessor {
            name: name.into(),
            language,
            nubits: 23,
            nbmant: 16,
            nbexpo: 6,
            nugain: 128,
            ndstac: 5,
            sdepth: 5,
            input_ports: 1,
            output_ports: 1,
        }
    }

    /// Confere os parâmetros contra o que o YANC compila, a regra de
    /// [`Project::add_processor`]:
    ///
    /// - portas de entrada e de saída: de 0 a [`MAX_PORTS`];
    /// - só em C±: `#NBEXPO` de 2 a 8 (acima de 8 o `cmmcomp` converte
    ///   constantes com o `float` da máquina e perde a faixa), `#NBMANT` de
    ///   pelo menos 2, `#NUBITS` até 32 e igual a `#NBMANT + #NBEXPO + 1`
    ///   (o `asmcomp` exige), `#NUGAIN` potência de dois, `#NDSTAC` e
    ///   `#SDEPTH` de pelo menos 1 (pilha vazia passa no build e quebra a
    ///   simulação).
    ///
    /// ```
    /// use lace_core::{Language, NewProcessor};
    ///
    /// let mut spec = NewProcessor::new("fft", Language::Cmm);
    /// assert!(spec.validate().is_ok());
    /// spec.nubits = 32;
    /// assert!(spec.validate().is_err(), "32 != 16 + 6 + 1");
    /// ```
    ///
    /// # Erros
    ///
    /// [`LaceError::InvalidParameter`] no primeiro parâmetro recusado.
    pub fn validate(&self) -> Result<()> {
        let bad = |name: &str, value: u32, reason: String| {
            Err(LaceError::InvalidParameter {
                name: name.to_owned(),
                value: value.to_string(),
                reason,
            })
        };
        for (name, value) in [
            ("#NUIOIN", self.input_ports),
            ("#NUIOOU", self.output_ports),
        ] {
            if value > MAX_PORTS {
                return bad(name, value, format!("at most {MAX_PORTS} ports"));
            }
        }
        if self.language == Language::Cpp {
            return Ok(());
        }
        if !(2..=8).contains(&self.nbexpo) {
            return bad(
                "#NBEXPO",
                self.nbexpo,
                "must be between 2 and 8 bits".into(),
            );
        }
        if self.nbmant < 2 {
            return bad("#NBMANT", self.nbmant, "must be at least 2 bits".into());
        }
        let width = self.nbmant.saturating_add(self.nbexpo).saturating_add(1);
        if width > 32 {
            return bad(
                "#NBMANT",
                self.nbmant,
                format!(
                    "#NBMANT + #NBEXPO + 1 is {width}, and the word (#NUBITS) has at most 32 bits"
                ),
            );
        }
        if self.nubits != width {
            return bad(
                "#NUBITS",
                self.nubits,
                format!(
                    "must be #NBMANT + #NBEXPO + 1 = {} + {} + 1 = {width}",
                    self.nbmant, self.nbexpo
                ),
            );
        }
        if !self.nugain.is_power_of_two() {
            return bad(
                "#NUGAIN",
                self.nugain,
                "must be a power of two (1, 2, 4, 8, ...)".into(),
            );
        }
        for (name, value) in [("#NDSTAC", self.ndstac), ("#SDEPTH", self.sdepth)] {
            if value == 0 {
                return bad(name, value, "a stack needs at least 1 position".into());
            }
        }
        Ok(())
    }

    fn source_text(&self) -> String {
        match self.language {
            // O `cmmcomp` recusa função de corpo vazio (só comentário também
            // conta como vazio): o modelo traz uma instrução. `out(0, 0)` só
            // compila se houver porta de saída.
            Language::Cmm => format!(
                "#PRNAME {}\n#NUBITS {}\n#NDSTAC {}\n#SDEPTH {}\n#NUIOIN {}\n#NUIOOU {}\n\
                 #NBMANT {}\n#NBEXPO {}\n#NUGAIN {}\n\nvoid main()\n{{\n{}}}\n",
                self.name,
                self.nubits,
                self.ndstac,
                self.sdepth,
                self.input_ports,
                self.output_ports,
                self.nbmant,
                self.nbexpo,
                self.nugain,
                if self.output_ports > 0 {
                    "    // out(porta, valor) escreve numa porta de saída\n    out(0, 0);\n"
                } else {
                    "    int x = 0;\n"
                },
            ),
            Language::Cpp => format!(
                "#pragma yanc prname {}\n#pragma yanc nuioin {}\n#pragma yanc nuioou {}\n\n\
                 void main(void)\n{{\n}}\n",
                self.name, self.input_ports, self.output_ports,
            ),
        }
    }
}

/// Um projeto aberto: o `.spf` em memória e seus processadores.
///
/// Os métodos que alteram o projeto (`add_processor`, `add_file`,
/// `set_top_level`, ...) gravam o `.spf` na hora; não há "salvar". Cada um
/// trava `.lace/spf.lock`, relê o `.spf` do disco, aplica a mudança e grava:
/// duas gravações ao mesmo tempo (o Lace Studio e a CLI no terminal, dois
/// `lace add`) entram uma depois da outra, e nenhuma perde a da outra. Para
/// ler o que outro programa mudou sem gravar, abra de novo: o `Project` só
/// relê ao gravar.
#[derive(Debug, Clone, Serialize)]
pub struct Project {
    name: String,
    pub(crate) root: Utf8PathBuf,
    spf_path: Utf8PathBuf,
    processors: Vec<Processor>,
    /// O `.spf` inteiro, para regravar sem perder o que o Lace não entende.
    #[serde(skip)]
    pub(crate) document: Value,
    /// A trava de gravação, de [`Project::begin_write`] até o `save`.
    #[serde(skip)]
    write_lock: Option<std::sync::Arc<std::fs::File>>,
}

impl Project {
    /// Abre um projeto a partir do `.spf` ou do diretório que o contém
    /// (`<dir>/<nome-do-dir>.spf`, ou o único `.spf` do diretório).
    ///
    /// Só lê: ao contrário da AURORA, abrir não regrava o arquivo. Os
    /// diretórios dos processadores não precisam existir para abrir; eles são
    /// criados no primeiro [`build`](crate::build).
    ///
    /// # Erros
    ///
    /// - [`LaceError::InvalidProject`] se o caminho não existir, ou se o
    ///   diretório não tiver `.spf` ou tiver mais de um;
    /// - [`LaceError::InvalidProjectFile`] se o `.spf` não for JSON mesmo com
    ///   a leitura tolerante, se não tiver a seção `structure`, ou se um
    ///   processador tiver `language` desconhecida ou `clk`/`numClocks` que
    ///   não seja inteiro positivo;
    /// - [`LaceError::Io`] se o arquivo não puder ser lido.
    pub fn open(path: impl AsRef<Utf8Path>) -> Result<Self> {
        let spf_path = locate_spf(path.as_ref())?;
        let spf_path = paths::canonicalize(&spf_path)?;
        let text = std::fs::read_to_string(&spf_path)
            .map_err(LaceError::io("Reading project", &spf_path))?;
        let document = spf::parse(&spf_path, &text)?;
        Self::from_document(spf_path, document)
    }

    /// Acha o projeto que contém `start`: o `.spf` de `start` ou do primeiro
    /// diretório acima dele que tenha um, como o git acha o repositório.
    /// Assim um comando funciona de qualquer pasta do projeto: da pasta de um
    /// processador, de `Software/`, de uma pasta de fontes.
    ///
    /// `start` pode ser o próprio `.spf`, que é aberto direto, ou um arquivo
    /// do projeto, e aí a busca começa na pasta dele. Em cada pasta vale a
    /// regra de [`Project::open`]: `<pasta>/<nome-da-pasta>.spf`, ou o único
    /// `.spf` dela.
    ///
    /// # Erros
    ///
    /// - [`LaceError::ProjectNotFound`] se nenhuma pasta, de `start` até a
    ///   raiz do sistema, tem `.spf`;
    /// - [`LaceError::InvalidProject`] se `start` não existe, ou se a
    ///   primeira pasta com `.spf` tem mais de um;
    /// - os de [`Project::open`] para o `.spf` achado.
    pub fn discover(start: impl AsRef<Utf8Path>) -> Result<Self> {
        let start = start.as_ref();
        if start.is_file() && start.extension() == Some("spf") {
            return Self::open(start);
        }
        if !start.exists() {
            return Err(LaceError::InvalidProject {
                path: start.to_owned(),
                reason: "Does not exist".into(),
            });
        }
        let start = paths::canonicalize(start)?;
        let first = if start.is_file() {
            start.parent().unwrap_or(&start).to_owned()
        } else {
            start
        };
        for dir in first.ancestors() {
            if let Some(spf) = spf_in(dir)? {
                return Self::open(spf);
            }
        }
        Err(LaceError::ProjectNotFound(first))
    }

    /// O processador cuja pasta contém `path` (a pasta dele, com
    /// `Software/`, `Hardware/` e `Simulation/`, ou a temporária dele em
    /// `.lace/Temp/`). `None` fora de todas.
    pub fn processor_at(&self, path: impl AsRef<Utf8Path>) -> Option<&Processor> {
        let path = path.as_ref();
        let path = paths::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        self.processors
            .iter()
            .find(|p| path.starts_with(&p.dir) || path.starts_with(&p.temp_dir))
    }

    /// O que está estranho no `.spf` mas não impede de abrir: um caminho de
    /// outra máquina achado pela cauda, um `topLevelFile` ou `testbenchFile`
    /// fora da lista (ignorado), um topo com nome de testbench (ignorado), um
    /// processador com nome que o YANC não compila. As interfaces mostram
    /// como aviso ao abrir (`lace status`, painel de problemas do Studio).
    ///
    /// Só lê; o que dá para consertar sozinho (o caminho achado) é consertado
    /// na próxima gravação.
    pub fn issues(&self) -> Vec<ProjectIssue> {
        let mut issues = self.file_issues();
        for processor in &self.processors {
            if let Err(LaceError::InvalidName { reason, .. }) =
                validate_processor_name(&processor.name)
            {
                issues.push(ProjectIssue::new(
                    IssueKind::InvalidProcessorName,
                    Some(processor.dir.clone()),
                    Some(processor.name.clone()),
                    format!("Processor '{}' cannot be built: {reason}", processor.name),
                ));
            }
        }
        // Outro projeto numa pasta acima: o `status` dele lista os arquivos
        // deste como não registrados, e um arquivo pode acabar nos dois.
        if let Some(outer) = self
            .root
            .ancestors()
            .skip(1)
            .find_map(|dir| spf_in(dir).ok().flatten())
        {
            issues.push(ProjectIssue::new(
                IssueKind::NestedProject,
                Some(outer.clone()),
                None,
                format!("This project is inside the folder of another one ({outer}); files here also show up there"),
            ));
        }
        issues
    }

    fn from_document(spf_path: Utf8PathBuf, document: Value) -> Result<Self> {
        let root = spf_path
            .parent()
            .expect("a canonicalized file has a parent directory")
            .to_owned();
        let name = spf_path.file_stem().unwrap_or_default().to_owned();

        let entries = spf::processors(&spf_path, &document)?;
        let mut seen: Vec<&str> = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            let name = entry.name.as_str();
            let reason = if name.trim().is_empty() {
                Some("the name is empty".to_owned())
            } else if !is_single_folder_name(name) {
                Some(format!("'{name}' is not a folder name inside the project"))
            } else if seen.iter().any(|s| s.eq_ignore_ascii_case(name)) {
                Some(format!("'{name}' appears more than once"))
            } else {
                None
            };
            if let Some(reason) = reason {
                return Err(spf::invalid(
                    &spf_path,
                    format!("structure.processors[{i}]: {reason}"),
                ));
            }
            seen.push(name);
        }
        let processors = entries
            .into_iter()
            .map(|entry| processor_from_entry(&spf_path, &root, entry))
            .collect::<Result<Vec<_>>>()?;

        Ok(Project {
            name,
            root,
            spf_path,
            processors,
            document,
            write_lock: None,
        })
    }

    /// Cria `<pai>/<nome>/<nome>.spf`, sem processadores, no formato da
    /// AURORA (`metadata` e `structure` completos). Cria `<pai>/<nome>` se
    /// preciso; `<pai>` precisa existir.
    ///
    /// # Erros
    ///
    /// - [`LaceError::InvalidName`] se o nome não passar em
    ///   [`validate_project_name`] (espaço, acento, caractere especial,
    ///   começo que não é letra, nome reservado do Windows);
    /// - [`LaceError::ProjectExists`] se o `.spf` já existir (nada é
    ///   alterado);
    /// - [`LaceError::Io`] se `<pai>` não existir ou não puder ser escrito.
    pub fn create(parent: impl AsRef<Utf8Path>, name: &str) -> Result<Self> {
        validate_project_name(name)?;
        let parent = paths::canonicalize(parent.as_ref())?;
        let root = parent.join(name);
        let spf_path = root.join(format!("{name}.spf"));
        if spf_path.exists() {
            return Err(LaceError::ProjectExists(spf_path));
        }
        // `Ok1` ao lado de `ok1`: duas pastas no Linux, a mesma no Windows e
        // no macOS.
        if let Ok(entries) = parent.read_dir_utf8()
            && let Some(other) = entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_owned())
                .find(|other| other != name && other.eq_ignore_ascii_case(name))
        {
            return Err(LaceError::InvalidName {
                name: name.to_owned(),
                reason: format!(
                    "'{other}' already exists here, and Windows and macOS do not tell the two names apart"
                ),
            });
        }
        let created = !root.exists();
        std::fs::create_dir_all(&root).map_err(LaceError::io("Creating project", &root))?;

        let now = iso8601_now();
        let document = json!({
            "metadata": {
                "projectName": name,
                "createdAt": now,
                "lastModified": now,
                "computerName": computer_name(),
                "appVersion": concat!("lace ", env!("CARGO_PKG_VERSION")),
                "projectPath": root,
            },
            "structure": {
                "basePath": root,
                "processors": [],
                "folders": [],
                "topLevelFile": "",
                "testbenchFile": "",
                "synthesizableFiles": [],
                "testbenchFiles": [],
            },
        });
        if let Err(error) = spf::write(&spf_path, &document) {
            // Nada fica para trás: a pasta só sai se foi criada agora e
            // continua vazia.
            if created {
                let _ = std::fs::remove_dir(&root);
            }
            return Err(error);
        }
        tracing::info!(%spf_path, "Project created");
        Self::from_document(spf_path, document)
    }

    /// Cria um processador: `Software/`, `Hardware/`, `Simulation/`, o
    /// diretório temporário, o fonte a partir do modelo e a entrada no `.spf`
    /// (`{"name"}` em C±, `{"name", "language": "cpp"}` em C). Grava o `.spf`.
    ///
    /// O fonte-modelo em C± tem as nove diretivas (`#PRNAME`, `#NUBITS`, ...)
    /// e um `main` vazio; em C, os pragmas de nome e de portas.
    ///
    /// # Erros
    ///
    /// - [`LaceError::InvalidName`] se o nome não passar em
    ///   [`validate_processor_name`]: não é um identificador (a AURORA aceita
    ///   `-`, mas o lexer do `cmmcomp` não), passa de
    ///   [`MAX_PROCESSOR_NAME`] caracteres, ou é palavra do C± ou do
    ///   Verilog, ou módulo da biblioteca SAPHO;
    /// - [`LaceError::InvalidParameter`] se um parâmetro não passar em
    ///   [`NewProcessor::validate`];
    /// - [`LaceError::ProcessorExists`] se o nome já estiver no projeto,
    ///   inclusive com outra caixa (`Soma` e `soma` colidem no Windows e no
    ///   macOS);
    /// - [`LaceError::InvalidProject`] se o fonte já existir no disco: o
    ///   Lace nunca sobrescreve código do usuário.
    pub fn add_processor(&mut self, spec: &NewProcessor) -> Result<&Processor> {
        self.begin_write()?;
        validate_processor_name(&spec.name)?;
        spec.validate()?;
        // Windows e macOS não distinguem maiúsculas em nome de pasta: `Soma`
        // e `soma` iriam para o mesmo diretório.
        if let Some(existing) = self
            .processors
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&spec.name))
        {
            return Err(LaceError::ProcessorExists(existing.name.clone()));
        }

        let processor = self.processor_paths(&spec.name, spec.language, None);
        if processor.source.exists() {
            return Err(LaceError::InvalidProject {
                path: processor.source,
                reason: "The source already exists; Lace does not overwrite code".into(),
            });
        }
        processor.ensure_dirs()?;
        std::fs::write(&processor.source, spec.source_text())
            .map_err(LaceError::io("Creating source", &processor.source))?;

        let mut entry = json!({ "name": spec.name });
        if spec.language == Language::Cpp {
            entry["language"] = "cpp".into();
        }
        let structure = &mut self.document["structure"];
        if !structure.get("processors").is_some_and(Value::is_array) {
            structure["processors"] = json!([]);
        }
        structure["processors"]
            .as_array_mut()
            .expect("acabou de virar array")
            .push(entry);
        self.save()?;

        tracing::info!(name = %spec.name, "Processor created");
        self.processors.push(processor);
        Ok(self.processors.last().expect("acabou de entrar"))
    }

    /// Regrava o `.spf`, alinhando `basePath` e `projectPath` à raiz atual e
    /// atualizando `lastModified`, como o escritor da AURORA.
    /// Antes de mudar o projeto: trava `.lace/spf.lock` (espera quem estiver
    /// gravando, de outro processo ou de outra thread) e relê o `.spf` do
    /// disco, para a mudança cair sobre a versão mais nova. A trava fica até
    /// o próximo `save`. Chamado no começo de cada método que grava, antes de
    /// ler o estado.
    pub(crate) fn begin_write(&mut self) -> Result<()> {
        if self.write_lock.is_some() {
            return Ok(());
        }
        let dir = self.root.join(".lace");
        std::fs::create_dir_all(&dir).map_err(LaceError::io("Creating directory", &dir))?;
        let path = dir.join("spf.lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(LaceError::io("Locking project", &path))?;
        file.lock()
            .map_err(LaceError::io("Locking project", &path))?;
        let fresh = Self::open(&self.spf_path)?;
        self.document = fresh.document;
        self.processors = fresh.processors;
        self.write_lock = Some(std::sync::Arc::new(file));
        Ok(())
    }

    pub(crate) fn save(&mut self) -> Result<()> {
        let result = self.write_document();
        // Solta a trava de `begin_write`, gravando ou não.
        self.write_lock = None;
        result
    }

    fn write_document(&mut self) -> Result<()> {
        self.repair_rescued();
        self.repair_duplicates();
        let root = Value::from(self.root.as_str());
        if let Some(meta) = self
            .document
            .get_mut("metadata")
            .and_then(Value::as_object_mut)
        {
            meta.insert("projectPath".into(), root.clone());
            meta.insert("lastModified".into(), iso8601_now().into());
        }
        self.document["structure"]["basePath"] = root;
        spf::write(&self.spf_path, &self.document)
    }

    /// O nome do projeto: o nome do `.spf` sem a extensão.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// A raiz: o diretório do `.spf`, canonicalizado.
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// O `.spf`, canonicalizado.
    pub fn spf_path(&self) -> &Utf8Path {
        &self.spf_path
    }

    /// Intermediários do projeto inteiro (`<raiz>/.lace/Temp`): o `.vvp` e o
    /// `obj_dir` da simulação do projeto, a síntese, os esquemáticos.
    pub fn temp_dir(&self) -> Utf8PathBuf {
        self.root.join(".lace").join("Temp")
    }

    /// Os processadores, na ordem do `.spf`.
    pub fn processors(&self) -> &[Processor] {
        &self.processors
    }

    /// Os processadores que têm o fonte no disco, na ordem do `.spf`: os que o
    /// botão Wave da AURORA compila antes de simular o projeto. Um processador
    /// registrado sem fonte fica de fora em vez de fazer o build falhar.
    pub fn buildable_processors(&self) -> Vec<&Processor> {
        self.processors
            .iter()
            .filter(|p| p.source.is_file())
            .collect()
    }

    /// O processador com esse nome, se houver.
    pub fn processor(&self, name: &str) -> Option<&Processor> {
        self.processors.iter().find(|p| p.name == name)
    }

    /// Como [`Project::processor`], mas com um erro que lista os disponíveis.
    pub fn require_processor(&self, name: &str) -> Result<&Processor> {
        self.processor(name)
            .ok_or_else(|| LaceError::ProcessorNotFound {
                name: name.to_owned(),
                available: self.processors.iter().map(|p| p.name.clone()).collect(),
            })
    }

    /// Muda os parâmetros de simulação de um processador e grava no `.spf`
    /// (`clk`, `numClocks`, `showArrays`, os mesmos campos que o painel de
    /// configuração da AURORA escreve). Campos `None` ficam como estão.
    ///
    /// Os novos valores valem a partir do próximo [`build`](crate::build): o
    /// testbench gerado embute frequência e clocks.
    ///
    /// ```
    /// use lace_core::{Language, NewProcessor, Project, ProcessorConfig};
    /// # let tmp = tempfile::tempdir()?;
    /// # let dir = camino::Utf8Path::from_path(tmp.path()).unwrap();
    /// let mut project = Project::create(dir, "p")?;
    /// project.add_processor(&NewProcessor::new("soma", Language::Cmm))?;
    ///
    /// let mut config = ProcessorConfig::default();
    /// config.frequency_mhz = Some(50);
    /// let soma = project.configure_processor("soma", &config)?;
    /// assert_eq!((soma.frequency_mhz, soma.clocks), (50, 2000));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Erros
    ///
    /// - [`LaceError::ProcessorNotFound`] se o processador não existir;
    /// - [`LaceError::InvalidParameter`] se `frequency_mhz` não estiver entre
    ///   1 e [`MAX_FREQUENCY_MHZ`], ou `clocks` entre 1 e [`MAX_CLOCKS`].
    ///   Nada é gravado.
    pub fn configure_processor(
        &mut self,
        name: &str,
        config: &ProcessorConfig,
    ) -> Result<&Processor> {
        self.begin_write()?;
        let index = self
            .processors
            .iter()
            .position(|p| p.name == name)
            .ok_or_else(|| LaceError::ProcessorNotFound {
                name: name.to_owned(),
                available: self.processors.iter().map(|p| p.name.clone()).collect(),
            })?;
        for (field, value) in [
            ("frequency", config.frequency_mhz),
            ("clocks", config.clocks),
        ] {
            if let Some(value) = value
                && let Some(reason) = simulation_range(field, value)
            {
                return Err(LaceError::InvalidParameter {
                    name: field.to_owned(),
                    value: value.to_string(),
                    reason: reason.to_owned(),
                });
            }
        }

        let list = self.document["structure"]["processors"]
            .as_array_mut()
            .expect("o processador veio desta lista");
        let entry = list
            .iter_mut()
            .find(|e| e.as_str() == Some(name) || e["name"].as_str() == Some(name))
            .expect("o processador veio desta lista");
        // Entrada no formato antigo (só o nome) vira objeto.
        if entry.is_string() {
            *entry = json!({ "name": name });
        }
        if let Some(clk) = config.frequency_mhz {
            entry["clk"] = clk.into();
        }
        if let Some(clocks) = config.clocks {
            entry["numClocks"] = clocks.into();
        }
        if let Some(show) = config.show_arrays {
            entry["showArrays"] = show.into();
        }
        self.save()?;

        let processor = &mut self.processors[index];
        processor.frequency_mhz = config.frequency_mhz.unwrap_or(processor.frequency_mhz);
        processor.clocks = config.clocks.unwrap_or(processor.clocks);
        processor.show_arrays = config.show_arrays.unwrap_or(processor.show_arrays);
        Ok(&self.processors[index])
    }

    fn processor_paths(&self, name: &str, language: Language, file: Option<&str>) -> Processor {
        processor_paths(&self.root, name, language, file)
    }
}

/// Parâmetros de simulação de um processador, para
/// [`Project::configure_processor`]. `None` em um campo mantém o valor
/// atual.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ProcessorConfig {
    /// Frequência de operação em MHz (`clk` no `.spf`).
    pub frequency_mhz: Option<u32>,
    /// Clocks que o testbench gerado simula (`numClocks` no `.spf`).
    pub clocks: Option<u32>,
    /// Exportar arrays para a simulação (`showArrays` no `.spf`).
    pub show_arrays: Option<bool>,
}

fn processor_paths(
    root: &Utf8Path,
    name: &str,
    language: Language,
    file: Option<&str>,
) -> Processor {
    let dir = root.join(name);
    let file = file.map_or_else(|| format!("{name}.{}", language.extension()), str::to_owned);
    Processor {
        name: name.to_owned(),
        language,
        source: dir.join("Software").join(file),
        temp_dir: root.join(".lace").join("Temp").join(name),
        dir,
        frequency_mhz: DEFAULT_FREQUENCY_MHZ,
        clocks: DEFAULT_CLOCKS,
        show_arrays: false,
    }
}

/// Resolve a linguagem como a AURORA (`processor_source.ts` e
/// `processor_dispatch.ts`): `language` declarada; senão a extensão de
/// `sourceFile`/`cmmFile`; senão procura `<nome>.cmm` e depois `<nome>.cpp` em
/// `Software/`; senão C±.
fn processor_from_entry(
    spf_path: &Utf8Path,
    root: &Utf8Path,
    entry: spf::RawEntry,
) -> Result<Processor> {
    let name = entry.name;
    // Só o nome do arquivo importa: o cmmcomp sempre procura o fonte em
    // `<proc>/Software/`, então um diretório no sourceFile não teria efeito.
    let file = entry
        .source_file
        .or(entry.cmm_file)
        .filter(|f| !f.trim().is_empty())
        .map(|f| {
            Utf8Path::new(&f.replace('\\', "/"))
                .file_name()
                .unwrap_or_default()
                .to_owned()
        });

    let declared = match entry.language.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(lang) => Some(Language::from_extension(lang).ok_or_else(|| {
            spf::invalid(
                spf_path,
                format!("Processor '{name}': unknown language '{lang}'"),
            )
        })?),
    };
    let from_file = file
        .as_deref()
        .and_then(|f| Utf8Path::new(f).extension())
        .and_then(Language::from_extension);
    let software = root.join(&name).join("Software");
    let probed = || {
        [Language::Cmm, Language::Cpp]
            .into_iter()
            .find(|l| software.join(format!("{name}.{}", l.extension())).is_file())
    };
    let language = declared
        .or(from_file)
        .or_else(probed)
        .unwrap_or(Language::Cmm);

    let mut processor = processor_paths(root, &name, language, file.as_deref());
    let in_range = |field: &str, kind: &str, value: u32| match simulation_range(kind, value) {
        Some(reason) => Err(spf::invalid(
            spf_path,
            format!("Processor '{name}': {field} {value}: {reason}"),
        )),
        None => Ok(value),
    };
    if let Some(clk) = spf::positive_int(spf_path, &name, "clk", entry.clk.as_ref())? {
        processor.frequency_mhz = in_range("clk", "frequency", clk)?;
    }
    if let Some(n) = spf::positive_int(spf_path, &name, "numClocks", entry.num_clocks.as_ref())? {
        processor.clocks = in_range("numClocks", "clocks", n)?;
    }
    if let Some(b) = spf::boolean(entry.show_arrays.as_ref()) {
        processor.show_arrays = b;
    }
    Ok(processor)
}

/// Por que um valor de simulação está fora da faixa, ou `None` se serve.
/// `kind` é `"frequency"` (MHz) ou `"clocks"`.
fn simulation_range(kind: &str, value: u32) -> Option<&'static str> {
    match kind {
        _ if value == 0 => Some("must be at least 1"),
        "frequency" if value > MAX_FREQUENCY_MHZ => Some(
            "the generated testbench works in picoseconds (timescale 1ns/1ps): above 500000 MHz the half period rounds to 0",
        ),
        "clocks" if value > MAX_CLOCKS => {
            Some("the generated testbench counts clocks in a 32-bit integer: at most 2147483647")
        }
        _ => None,
    }
}

/// Um nome que vira uma pasta direto dentro da raiz: sem barra, sem `:`, e
/// diferente de `.` e `..`. Um nome vazio tomaria a raiz como pasta do
/// processador; `../x` sairia do projeto.
fn is_single_folder_name(name: &str) -> bool {
    !name.contains(['/', '\\', ':']) && name != "." && name != ".."
}

fn locate_spf(path: &Utf8Path) -> Result<Utf8PathBuf> {
    if path.is_file() {
        return Ok(path.to_owned());
    }
    if !path.is_dir() {
        return Err(LaceError::InvalidProject {
            path: path.to_owned(),
            reason: "Does not exist".into(),
        });
    }
    // Canonicaliza antes para que "." tenha nome de diretório.
    let path = &paths::canonicalize(path)?;
    spf_in(path)?.ok_or_else(|| LaceError::InvalidProject {
        path: path.to_owned(),
        reason: "No .spf file in the directory".into(),
    })
}

/// O `.spf` de uma pasta: `<pasta>/<nome-da-pasta>.spf`, ou o único `.spf`
/// dela. `None` sem nenhum, ou se a pasta não pode ser listada.
pub(crate) fn spf_in(dir: &Utf8Path) -> Result<Option<Utf8PathBuf>> {
    if let Some(stem) = dir.file_name() {
        let candidate = dir.join(format!("{stem}.spf"));
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }
    let Ok(entries) = dir.read_dir_utf8() else {
        return Ok(None);
    };
    let mut found: Vec<Utf8PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
        .filter(|p| p.extension() == Some("spf") && p.is_file())
        .collect();
    match found.len() {
        0 => Ok(None),
        1 => Ok(Some(found.remove(0))),
        n => Err(LaceError::InvalidProject {
            path: dir.to_owned(),
            reason: format!("{n} .spf files in the directory; name the one to open"),
        }),
    }
}

/// O nome do processador vira `#PRNAME`, nome de módulo Verilog e nome de
/// arquivo, então precisa ser um identificador válido para os três. A AURORA
/// aceita `-`, mas o lexer do cmmcomp não (`{LETRA}({LETRA}|[0-9])*`).
/// Um identificador Verilog simples (letras, dígitos e `_`, começando por
/// letra ou `_`). `context` diz por que o nome precisa ser assim.
pub(crate) fn validate_identifier(name: &str, context: &str) -> Result<()> {
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valid {
        Ok(())
    } else {
        Err(LaceError::InvalidName {
            name: name.to_owned(),
            reason: format!(
                "{context}, so use only letters, digits and '_', starting with a letter or '_'"
            ),
        })
    }
}

/// Confere um nome de processador, a regra de
/// [`Project::add_processor`]. O nome vira `#PRNAME`, a pasta, os arquivos
/// e o módulo Verilog `<nome>` (e `<nome>_tb`), então precisa servir para
/// todos:
///
/// - identificador: letras sem acento, dígitos e `_`, começando por letra ou
///   `_` (a AURORA aceita `-`, o lexer do `cmmcomp` não);
/// - até [`MAX_PROCESSOR_NAME`] caracteres;
/// - fora das palavras do C± (`void`, `int`, `in`, `out`, ...), das do
///   Verilog e do SystemVerilog (`module`, `wire`, `logic`, ...), dos
///   módulos da biblioteca SAPHO (`core`, `processor`, `ula`, ...) e dos
///   nomes reservados do Windows.
///
/// Uma interface pode chamar enquanto o usuário digita.
///
/// ```
/// use lace_core::validate_processor_name;
///
/// assert!(validate_processor_name("filtro_fir").is_ok());
/// for bad in ["proc-1", "void", "module", "core", "con"] {
///     assert!(validate_processor_name(bad).is_err(), "{bad}");
/// }
/// ```
///
/// # Erros
///
/// [`LaceError::InvalidName`], com o motivo em `reason`.
pub fn validate_processor_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    let invalid = |reason: String| LaceError::InvalidName {
        name: name.to_owned(),
        reason,
    };
    if !valid {
        return Err(invalid(
            "Use only letters, digits and '_', starting with a letter or '_'".into(),
        ));
    }
    if name.len() > MAX_PROCESSOR_NAME {
        return Err(invalid(format!(
            "At most {MAX_PROCESSOR_NAME} characters (it has {}): the YANC compilers keep the name in fixed buffers",
            name.len()
        )));
    }
    if is_windows_reserved(name) {
        return Err(invalid(WINDOWS_RESERVED_REASON.into()));
    }
    if CMM_WORDS.contains(&name) {
        return Err(invalid(
            "It is a C± keyword or built-in function, and #PRNAME would not compile".into(),
        ));
    }
    if VERILOG_WORDS.contains(&name) {
        return Err(invalid(
            "It is a Verilog keyword, and the processor module would not compile".into(),
        ));
    }
    if SAPHO_MODULES.contains(&name) {
        return Err(invalid(
            "It is a module of the SAPHO library, and the processor would instantiate itself"
                .into(),
        ));
    }
    Ok(())
}

/// Palavras do lexer do `cmmcomp` (`CMMComp/Sources/CMMComp.l`).
const CMM_WORDS: &[&str] = &[
    "abs", "atan", "break", "case", "ceil", "comp", "complex", "conj", "continue", "copy", "cos",
    "cosh", "default", "do", "else", "exp", "fase", "fin", "float", "floor", "for", "fout", "if",
    "imag", "in", "int", "log", "norm", "out", "pow", "pset", "real", "return", "round", "sign",
    "sin", "sinh", "sqrt", "switch", "tan", "tanh", "void", "while",
];

/// Palavras reservadas do Verilog 2005 e as mais comuns do SystemVerilog
/// (o Icarus lê os processadores com `-g2012`).
const VERILOG_WORDS: &[&str] = &[
    "always",
    "and",
    "assign",
    "automatic",
    "begin",
    "buf",
    "bufif0",
    "bufif1",
    "case",
    "casex",
    "casez",
    "cell",
    "cmos",
    "config",
    "deassign",
    "default",
    "defparam",
    "design",
    "disable",
    "edge",
    "else",
    "end",
    "endcase",
    "endconfig",
    "endfunction",
    "endgenerate",
    "endmodule",
    "endprimitive",
    "endspecify",
    "endtable",
    "endtask",
    "event",
    "for",
    "force",
    "forever",
    "fork",
    "function",
    "generate",
    "genvar",
    "highz0",
    "highz1",
    "if",
    "ifnone",
    "incdir",
    "include",
    "initial",
    "inout",
    "input",
    "instance",
    "integer",
    "join",
    "large",
    "liblist",
    "library",
    "localparam",
    "macromodule",
    "medium",
    "module",
    "nand",
    "negedge",
    "nmos",
    "nor",
    "noshowcancelled",
    "not",
    "notif0",
    "notif1",
    "or",
    "output",
    "parameter",
    "pmos",
    "posedge",
    "primitive",
    "pull0",
    "pull1",
    "pulldown",
    "pullup",
    "pulsestyle_ondetect",
    "pulsestyle_onevent",
    "rcmos",
    "real",
    "realtime",
    "reg",
    "release",
    "repeat",
    "rnmos",
    "rpmos",
    "rtran",
    "rtranif0",
    "rtranif1",
    "scalared",
    "showcancelled",
    "signed",
    "small",
    "specify",
    "specparam",
    "strong0",
    "strong1",
    "supply0",
    "supply1",
    "table",
    "task",
    "time",
    "tran",
    "tranif0",
    "tranif1",
    "tri",
    "tri0",
    "tri1",
    "triand",
    "trior",
    "trireg",
    "unsigned",
    "use",
    "uwire",
    "vectored",
    "wait",
    "wand",
    "weak0",
    "weak1",
    "while",
    "wire",
    "wor",
    "xnor",
    "xor",
    "always_comb",
    "always_ff",
    "always_latch",
    "assert",
    "assume",
    "bit",
    "break",
    "byte",
    "chandle",
    "class",
    "const",
    "continue",
    "cover",
    "do",
    "endclass",
    "endinterface",
    "endpackage",
    "endprogram",
    "enum",
    "export",
    "extern",
    "final",
    "foreach",
    "import",
    "inside",
    "int",
    "interface",
    "logic",
    "longint",
    "modport",
    "new",
    "null",
    "package",
    "priority",
    "program",
    "return",
    "sequence",
    "shortint",
    "static",
    "string",
    "struct",
    "super",
    "this",
    "type",
    "typedef",
    "union",
    "unique",
    "virtual",
    "void",
];

/// Os módulos da biblioteca SAPHO do YANC (`SAPHO/*.v`), compilados junto
/// com cada processador.
const SAPHO_MODULES: &[&str] = &[
    "addr_dec",
    "core",
    "instr_dec",
    "instr_fetch",
    "io_ctrl",
    "mem_ctrl",
    "mem_data",
    "mem_instr",
    "myFIFO",
    "norm_mux",
    "pc",
    "prefetch",
    "processor",
    "rel_addr",
    "stack",
    "ula",
    "ula_abs",
    "ula_add",
    "ula_and",
    "ula_denorm",
    "ula_div",
    "ula_equ",
    "ula_f2i",
    "ula_fabs",
    "ula_fadd",
    "ula_fcmp",
    "ula_fdiv",
    "ula_fmlt",
    "ula_fneg",
    "ula_fpst",
    "ula_frot",
    "ula_fsgn",
    "ula_gre",
    "ula_i2f",
    "ula_in1_ctrl",
    "ula_in2_ctrl",
    "ula_inv",
    "ula_lan",
    "ula_les",
    "ula_lin",
    "ula_lor",
    "ula_mlt",
    "ula_mod",
    "ula_mux",
    "ula_neg",
    "ula_nmux",
    "ula_norm",
    "ula_nrm",
    "ula_or",
    "ula_pst",
    "ula_scl",
    "ula_sgn",
    "ula_shift",
    "ula_xor",
    "ula_xpo",
];

/// Um aviso de [`Project::issues`]: algo estranho no `.spf` que não impede
/// de abrir.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct ProjectIssue {
    /// O tipo, para a interface traduzir.
    pub kind: IssueKind,
    /// O arquivo ou a pasta envolvida, absoluta.
    #[schemars(with = "Option<String>")]
    pub path: Option<Utf8PathBuf>,
    /// O complemento do tipo: o caminho como estava gravado
    /// (`rescued_path`), o campo do `.spf` (`selection_not_registered`,
    /// `testbench_as_top`) ou o nome do processador
    /// (`invalid_processor_name`).
    pub detail: Option<String>,
    /// O aviso inteiro, em inglês, pronto para mostrar.
    pub message: String,
}

impl ProjectIssue {
    pub(crate) fn new(
        kind: IssueKind,
        path: Option<Utf8PathBuf>,
        detail: Option<String>,
        message: String,
    ) -> Self {
        ProjectIssue {
            kind,
            path,
            detail,
            message,
        }
    }
}

/// O tipo de um [`ProjectIssue`]. Em JSON, `snake_case`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum IssueKind {
    /// Um absoluto de outra máquina que não existe aqui foi achado pela
    /// cauda dentro da raiz; o `.spf` sai consertado na próxima gravação.
    RescuedPath,
    /// `topLevelFile` ou `testbenchFile` aponta para um arquivo que não está
    /// na lista do papel, e é ignorado.
    SelectionNotRegistered,
    /// `topLevelFile` tem nome de testbench (`tb_x.v`, `x_tb.v`), que não
    /// pode ser topo, e é ignorado.
    TestbenchAsTop,
    /// Um processador do `.spf` com nome que o YANC não compila (`x-y`,
    /// `void`); o build dele recusa com `invalid_name`.
    InvalidProcessorName,
    /// Um arquivo repetido numa lista, ou nas duas: conta uma vez, e a
    /// próxima gravação tira as entradas que sobram.
    DuplicateFile,
    /// O projeto está dentro da pasta de outro (uma pasta acima tem `.spf`):
    /// os arquivos dele aparecem também no outro.
    NestedProject,
}

const WINDOWS_RESERVED_REASON: &str = "Reserved name on Windows (CON, PRN, AUX, NUL, COM0-9, LPT0-9): the folder could not be created there";

/// Nomes que o Windows não aceita como arquivo ou pasta, com qualquer
/// extensão e em qualquer caixa. Recusados em todo sistema, para que um
/// projeto criado no Linux abra no Windows.
fn is_windows_reserved(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end()
        .to_ascii_uppercase();
    match stem.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" => true,
        _ => {
            let (prefix, digit) = stem.split_at(stem.len().min(3));
            matches!(prefix, "COM" | "LPT")
                && digit.len() == 1
                && digit.as_bytes()[0].is_ascii_digit()
        }
    }
}

/// Confere o nome de um projeto novo, a regra de [`Project::create`]:
/// letras sem acento (`A-Z`, `a-z`), dígitos, `_` e `-`, começando por letra,
/// até [`MAX_PROJECT_NAME`] caracteres, e fora dos nomes reservados do
/// Windows (`CON`, `COM1`...).
///
/// O nome vira a pasta, o `.spf` e parte do caminho de tudo que as
/// ferramentas recebem; espaço e acento num caminho exigem que cada script e
/// cada ferramenta citem o argumento direito, e o Lace prefere recusar o nome
/// (decisão de 2026-10-04). Projetos que já existem com outros nomes, como
/// os da AURORA, continuam abrindo: a regra só vale para criar.
///
/// Uma interface pode chamar esta função enquanto o usuário digita, para
/// avisar antes de tentar criar.
///
/// ```
/// use lace_core::validate_project_name;
///
/// assert!(validate_project_name("filtro_fir-2").is_ok());
/// assert!(validate_project_name("Projeto Teste").is_err());
/// assert!(validate_project_name("ação").is_err());
/// assert!(validate_project_name("2fir").is_err());
/// ```
///
/// # Erros
///
/// [`LaceError::InvalidName`], com o motivo em `reason`.
pub fn validate_project_name(name: &str) -> Result<()> {
    let reason = if name.is_empty() {
        Some("Empty")
    } else if name.chars().any(char::is_whitespace) {
        Some("Contains a space; use '_' or '-' instead")
    } else if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        Some("Must start with a letter (A-Z, a-z)")
    } else if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Some("Use only letters without accents, digits, '_' and '-'")
    } else if is_windows_reserved(name) {
        Some(WINDOWS_RESERVED_REASON)
    } else if name.len() > MAX_PROJECT_NAME {
        Some(
            "Too long: at most 64 characters (the name repeats in the folder, the .spf and every path the tools receive)",
        )
    } else {
        None
    };
    match reason {
        Some(reason) => Err(LaceError::InvalidName {
            name: name.to_owned(),
            reason: reason.into(),
        }),
        None => Ok(()),
    }
}

fn computer_name() -> String {
    ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok())
        .unwrap_or_default()
}

/// Data e hora UTC no formato do `Date.toISOString()` do JavaScript.
fn iso8601_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        now.subsec_millis()
    )
}

/// `time` em UTC, até os segundos (`2026-10-03T21:04:05Z`), para o
/// relatório (`history.rs`).
pub(crate) fn utc_seconds(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Dias desde 1970-01-01 para (ano, mês, dia), algoritmo de Howard Hinnant.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = paths::canonicalize(Utf8Path::from_path(dir.path()).unwrap()).unwrap();
        (dir, path)
    }

    #[test]
    fn discover_climbs_to_the_project_from_any_inner_folder() {
        let (_guard, dir) = tmp();
        let mut project = Project::create(&dir, "demo").unwrap();
        let soma = project
            .add_processor(&NewProcessor::new("soma", Language::Cmm))
            .unwrap()
            .clone();
        soma.ensure_dirs().unwrap();
        let root = project.root().to_owned();
        for inner in [
            root.clone(),
            soma.dir.clone(),
            soma.software_dir(),
            soma.simulation_dir(),
            soma.temp_dir.clone(),
            soma.software_dir().join("soma.cmm"),
            project.spf_path().to_owned(),
        ] {
            let found = Project::discover(&inner).unwrap();
            assert_eq!(found.spf_path(), project.spf_path(), "de {inner}");
        }

        // A pasta diz o processador; fora das dele, nenhum.
        let found = Project::discover(soma.software_dir()).unwrap();
        for inner in [&soma.dir, &soma.software_dir(), &soma.temp_dir] {
            assert_eq!(found.processor_at(inner).unwrap().name, "soma", "{inner}");
        }
        assert!(found.processor_at(&root).is_none());
        assert!(found.processor_at(root.join(".lace")).is_none());

        // Fora de qualquer projeto.
        let (_outside_guard, outside) = tmp();
        assert!(matches!(
            Project::discover(&outside),
            Err(LaceError::ProjectNotFound(p)) if p == outside
        ));
        assert!(matches!(
            Project::discover(outside.join("nao-existe")),
            Err(LaceError::InvalidProject { .. })
        ));
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_724), (2026, 9, 28));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn create_add_and_reopen() {
        let (_guard, dir) = tmp();
        let mut project = Project::create(&dir, "meu_projeto").unwrap();
        project
            .add_processor(&NewProcessor::new("soma", Language::Cmm))
            .unwrap();
        project
            .add_processor(&NewProcessor::new("filtro", Language::Cpp))
            .unwrap();

        let root = dir.join("meu_projeto");
        for sub in ["Software", "Hardware", "Simulation"] {
            assert!(root.join("soma").join(sub).is_dir(), "{sub}");
        }
        let src = std::fs::read_to_string(root.join("soma/Software/soma.cmm")).unwrap();
        assert!(src.starts_with("#PRNAME soma\n"));
        assert!(root.join("filtro/Software/filtro.cpp").is_file());

        let reopened = Project::open(&root).unwrap();
        assert_eq!(reopened.name(), "meu_projeto");
        let names: Vec<_> = reopened
            .processors()
            .iter()
            .map(|p| (p.name.as_str(), p.language))
            .collect();
        assert_eq!(names, [("soma", Language::Cmm), ("filtro", Language::Cpp)]);
        let soma = reopened.processor("soma").unwrap();
        assert_eq!(soma.frequency_mhz, DEFAULT_FREQUENCY_MHZ);
        assert_eq!(soma.temp_dir, root.join(".lace/Temp/soma"));
    }

    #[test]
    fn refuses_duplicates_and_bad_names() {
        let (_guard, dir) = tmp();
        let mut project = Project::create(&dir, "p").unwrap();
        project
            .add_processor(&NewProcessor::new("a", Language::Cmm))
            .unwrap();
        assert!(matches!(
            project.add_processor(&NewProcessor::new("a", Language::Cmm)),
            Err(LaceError::ProcessorExists(_))
        ));
        assert!(matches!(
            project.add_processor(&NewProcessor::new("proc-1", Language::Cmm)),
            Err(LaceError::InvalidName { .. })
        ));
        assert!(matches!(
            Project::create(&dir, "p"),
            Err(LaceError::ProjectExists(_))
        ));
        assert!(matches!(
            Project::create(&dir, "a/b"),
            Err(LaceError::InvalidName { .. })
        ));
    }

    #[test]
    fn project_names_without_spaces_accents_or_symbols() {
        for good in ["p", "contador", "Filtro_FIR", "soma-2", "a1_b-c"] {
            assert!(
                validate_project_name(good).is_ok(),
                "deveria aceitar {good}"
            );
        }
        for bad in [
            "",
            "Projeto Teste",
            "ação",
            "café",
            "2fir",
            "_x",
            "-x",
            "a.b",
            "a@b",
            "a/b",
            "tab\tname",
            "con",
        ] {
            assert!(
                matches!(
                    validate_project_name(bad),
                    Err(LaceError::InvalidName { .. })
                ),
                "deveria recusar {bad:?}"
            );
        }
        // A regra é a de criar; nada foi gravado.
        let (_guard, dir) = tmp();
        assert!(Project::create(&dir, "Projeto Teste").is_err());
        assert!(!dir.join("Projeto Teste").exists());
    }

    #[test]
    fn names_that_break_on_windows_or_macos_are_refused() {
        let (_guard, dir) = tmp();
        for name in ["con", "Aux", "NUL", "com1", "LPT9"] {
            assert!(Project::create(&dir, name).is_err(), "projeto {name}");
        }
        assert!(Project::create(&dir, "con.v2").is_err());
        assert!(Project::create(&dir, "console").is_ok());
        assert!(Project::create(&dir, "com10").is_ok());

        let mut project = Project::create(&dir, "p").unwrap();
        assert!(matches!(
            project.add_processor(&NewProcessor::new("aux", Language::Cmm)),
            Err(LaceError::InvalidName { .. })
        ));
        project
            .add_processor(&NewProcessor::new("Soma", Language::Cmm))
            .unwrap();
        assert!(matches!(
            project.add_processor(&NewProcessor::new("soma", Language::Cmm)),
            Err(LaceError::ProcessorExists(name)) if name == "Soma"
        ));
    }

    #[test]
    fn preserves_unknown_fields_on_save() {
        let (_guard, dir) = tmp();
        let spf = dir.join("x.spf");
        std::fs::write(
            &spf,
            r#"{"metadata": {"projectName": "x"}, "structure": {"basePath": "C:\\velho", "processors": [], "commandOverrides": {"asm": {"x": 1}}}}"#,
        )
        .unwrap();
        let mut project = Project::open(&spf).unwrap();
        project
            .add_processor(&NewProcessor::new("p", Language::Cmm))
            .unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&spf).unwrap()).unwrap();
        assert_eq!(doc["structure"]["commandOverrides"]["asm"]["x"], 1);
        assert_eq!(doc["structure"]["basePath"], dir.as_str());
    }

    #[test]
    fn configure_processor_persists_and_converts_legacy_entry() {
        let (_guard, dir) = tmp();
        std::fs::write(
            dir.join("x.spf"),
            r#"{"structure": {"processors": ["velho", {"name": "novo", "language": "cpp", "extra": 1}]}}"#,
        )
        .unwrap();
        let mut project = Project::open(dir.join("x.spf")).unwrap();
        let mut config = ProcessorConfig {
            frequency_mhz: Some(25),
            show_arrays: Some(true),
            ..ProcessorConfig::default()
        };
        project.configure_processor("velho", &config).unwrap();
        config.clocks = Some(99);
        project.configure_processor("novo", &config).unwrap();

        let reopened = Project::open(dir.join("x.spf")).unwrap();
        let velho = reopened.processor("velho").unwrap();
        assert_eq!(
            (velho.frequency_mhz, velho.clocks, velho.show_arrays),
            (25, DEFAULT_CLOCKS, true)
        );
        assert_eq!(reopened.processor("novo").unwrap().clocks, 99);
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("x.spf")).unwrap()).unwrap();
        assert_eq!(
            doc["structure"]["processors"][0],
            json!({"name": "velho", "clk": 25, "showArrays": true})
        );
        assert_eq!(doc["structure"]["processors"][1]["extra"], 1);

        config.clocks = Some(0);
        assert!(project.configure_processor("novo", &config).is_err());
        assert!(matches!(
            project.configure_processor("nada", &ProcessorConfig::default()),
            Err(LaceError::ProcessorNotFound { .. })
        ));
    }

    #[test]
    fn language_resolution_order() {
        let (_guard, dir) = tmp();
        std::fs::create_dir_all(dir.join("sonda/Software")).unwrap();
        std::fs::write(dir.join("sonda/Software/sonda.cpp"), "").unwrap();
        std::fs::write(
            dir.join("x.spf"),
            r#"{"structure": {"processors": [
                "sonda",
                {"name": "ext", "sourceFile": "C:\\a\\ext.cpp"},
                {"name": "decl", "language": "cpp", "clk": 50, "numClocks": 10, "showArrays": true},
                {"name": "padrao"}
            ]}}"#,
        )
        .unwrap();
        let project = Project::open(dir.join("x.spf")).unwrap();
        let lang = |n| project.processor(n).unwrap().language;
        assert_eq!(lang("sonda"), Language::Cpp);
        assert_eq!(lang("ext"), Language::Cpp);
        assert_eq!(
            project.processor("ext").unwrap().source,
            dir.join("ext/Software/ext.cpp")
        );
        assert_eq!(lang("padrao"), Language::Cmm);
        let decl = project.processor("decl").unwrap();
        assert_eq!(
            (
                decl.language,
                decl.frequency_mhz,
                decl.clocks,
                decl.show_arrays
            ),
            (Language::Cpp, 50, 10, true)
        );
    }

    #[test]
    fn broken_processor_entries_are_refused_on_open() {
        let (_guard, dir) = tmp();
        let spf = dir.join("x.spf");
        for (list, expected) in [
            (r#"[{"name": ""}]"#, "empty"),
            (r#"["../evil"]"#, "not a folder name"),
            (r#"["soma", "Soma"]"#, "more than once"),
        ] {
            std::fs::write(
                &spf,
                format!(r#"{{"structure": {{"processors": {list}}}}}"#),
            )
            .unwrap();
            match Project::open(&spf) {
                Err(LaceError::InvalidProjectFile { reason, .. }) => {
                    assert!(reason.contains(expected), "{list}: {reason}")
                }
                other => panic!("{list}: {other:?}"),
            }
        }
        // Nome que não compila abre, com aviso; o build recusa.
        std::fs::write(
            &spf,
            r#"{"structure": {"processors": ["x-y", "void", "ok"]}}"#,
        )
        .unwrap();
        let project = Project::open(&spf).unwrap();
        let names: Vec<_> = project
            .issues()
            .into_iter()
            .map(|i| i.detail.unwrap())
            .collect();
        assert_eq!(names, ["x-y", "void"]);
    }

    #[test]
    fn processor_names_that_never_compile_are_refused() {
        for bad in [
            "void",
            "int",
            "out",
            "module",
            "wire",
            "logic",
            "core",
            "processor",
            "ula",
        ] {
            assert!(validate_processor_name(bad).is_err(), "{bad}");
        }
        assert!(validate_processor_name(&"a".repeat(64)).is_ok());
        assert!(validate_processor_name(&"a".repeat(65)).is_err());
        assert!(validate_processor_name("soma_2").is_ok());
    }

    #[test]
    fn processor_parameters_are_checked_before_creating() {
        let (_guard, dir) = tmp();
        let mut project = Project::create(&dir, "p").unwrap();
        let base = NewProcessor::new("a", Language::Cmm);
        type Change = fn(&mut NewProcessor);
        let cases: [(&str, Change); 8] = [
            ("#NUBITS", |s| s.nubits = 0),
            ("#NUBITS", |s| s.nubits = 200),
            ("#NUGAIN", |s| s.nugain = 3),
            ("#NBMANT", |s| {
                s.nbmant = 30;
                s.nubits = 37;
            }),
            ("#NBEXPO", |s| s.nbexpo = 9),
            ("#NDSTAC", |s| s.ndstac = 0),
            ("#SDEPTH", |s| s.sdepth = 0),
            ("#NUIOIN", |s| s.input_ports = 100_000),
        ];
        for (field, change) in cases {
            let mut spec = base.clone();
            change(&mut spec);
            match project.add_processor(&spec) {
                Err(LaceError::InvalidParameter { name, .. }) => assert_eq!(name, field),
                other => panic!("{field}: {other:?}"),
            }
        }
        assert!(!dir.join("p/a").exists(), "nada foi criado");
        let mut c = NewProcessor::new("c", Language::Cpp);
        c.nubits = 0;
        assert!(
            project.add_processor(&c).is_ok(),
            "C ignora os campos do C±"
        );
    }

    #[test]
    fn clocks_and_frequency_have_ranges() {
        let (_guard, dir) = tmp();
        let mut project = Project::create(&dir, "p").unwrap();
        project
            .add_processor(&NewProcessor::new("calc", Language::Cmm))
            .unwrap();
        for (freq, clocks) in [
            (Some(0), None),
            (None, Some(0)),
            (Some(3_000_000), None),
            (None, Some(3_000_000_000)),
        ] {
            let config = ProcessorConfig {
                frequency_mhz: freq,
                clocks,
                ..ProcessorConfig::default()
            };
            assert!(
                matches!(
                    project.configure_processor("calc", &config),
                    Err(LaceError::InvalidParameter { .. })
                ),
                "{freq:?} {clocks:?}"
            );
        }
        let config = ProcessorConfig {
            frequency_mhz: Some(MAX_FREQUENCY_MHZ),
            clocks: Some(MAX_CLOCKS),
            ..ProcessorConfig::default()
        };
        assert!(project.configure_processor("calc", &config).is_ok());
    }

    #[test]
    fn concurrent_writes_keep_every_change() {
        let (_guard, dir) = tmp();
        let project = Project::create(&dir, "demo").unwrap();
        let root = project.root().to_owned();
        for i in 0..16 {
            std::fs::write(
                root.join(format!("m{i}.v")),
                format!("module m{i}; endmodule\n"),
            )
            .unwrap();
        }
        // Cada thread abre o seu `Project`, como o Studio e a CLI.
        let handles: Vec<_> = (0..16)
            .map(|i| {
                let root = root.clone();
                std::thread::spawn(move || {
                    let mut project = Project::open(&root).unwrap();
                    project
                        .add_file(
                            crate::FileRole::Synthesizable,
                            root.join(format!("m{i}.v")),
                            None,
                        )
                        .unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let project = Project::open(&root).unwrap();
        assert_eq!(project.files(crate::FileRole::Synthesizable).len(), 16);
        // Nenhum temporário sobra ao lado do `.spf`.
        let leftovers: Vec<_> = root
            .read_dir_utf8()
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn long_or_case_colliding_project_names_create_nothing() {
        let (_guard, dir) = tmp();
        let long = format!("a{}", "d".repeat(249));
        assert!(Project::create(&dir, &long).is_err());
        assert!(!dir.join(&long).exists());
        Project::create(&dir, "ok1").unwrap();
        match Project::create(&dir, "Ok1") {
            Err(LaceError::InvalidName { reason, .. }) => assert!(reason.contains("ok1")),
            other => panic!("{other:?}"),
        }
        assert!(!dir.join("Ok1").exists());
    }

    #[test]
    fn a_project_inside_another_is_flagged_and_left_out_of_it() {
        let (_guard, dir) = tmp();
        let outer = Project::create(&dir, "pai").unwrap();
        let inner = Project::create(outer.root(), "filho").unwrap();
        std::fs::write(inner.root().join("m.v"), "module m; endmodule\n").unwrap();
        let issues = Project::open(inner.spf_path()).unwrap().issues();
        assert_eq!(issues[0].kind, IssueKind::NestedProject);
        assert!(
            Project::open(outer.spf_path())
                .unwrap()
                .unregistered_verilog()
                .is_empty()
        );
    }
}
