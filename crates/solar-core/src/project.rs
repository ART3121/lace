//! O projeto SAPHO no disco, no mesmo formato da AURORA.
//!
//! ```text
//! <raiz>/
//!   <projeto>.spf              # JSON, ver o módulo spf
//!   <processador>/
//!     Software/<processador>.cmm | .cpp
//!     Hardware/                # .v e .mif gerados pelo asmcomp
//!     Simulation/              # input_N.txt / output_N.txt
//!   .solar/Temp/<processador>/ # intermediários (-t dos compiladores)
//! ```
//!
//! A raiz do projeto é sempre o diretório do `.spf`; o `basePath` gravado no
//! arquivo é ignorado na leitura, como a AURORA faz ao abrir um projeto
//! copiado de outra máquina.
//!
//! # Compatibilidade com a AURORA
//!
//! Um projeto criado pelo Solar abre na AURORA e vice-versa. O Solar lê o
//! `.spf` com a mesma tolerância (comentários, vírgula sobrando, BOM), grava
//! de forma atômica com indentação de 2 espaços, e preserva os campos que não
//! entende (`commandOverrides`, `metadata.lastOpened`, ...) e a ordem das
//! chaves. Ao contrário da AURORA, abrir um projeto não o regrava.
//!
//! # Exemplo
//!
//! ```
//! use solar_core::{Language, NewProcessor, Project};
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
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::{Result, SolarError};
use crate::{paths, spf};

/// Frequência padrão quando o `.spf` não traz `clk` (a da AURORA).
pub const DEFAULT_FREQUENCY_MHZ: u32 = 100;
/// Número de clocks padrão quando o `.spf` não traz `numClocks` (o da AURORA).
pub const DEFAULT_CLOCKS: u32 = 2000;

/// Linguagem do programa de um processador. Em JSON: `"cmm"` ou `"cpp"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Processor {
    /// O nome: diretório, `#PRNAME`, nome do módulo Verilog e dos artefatos.
    pub name: String,
    /// A linguagem do fonte.
    pub language: Language,
    /// `<raiz>/<nome>`, o `-p` dos compiladores.
    pub dir: Utf8PathBuf,
    /// O programa-fonte, em `Software/`.
    pub source: Utf8PathBuf,
    /// Diretório de intermediários, o `-t` dos compiladores.
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

    /// Já foi compilado: o Verilog e o testbench gerados existem no disco.
    /// Não diz se estão atualizados em relação ao fonte.
    pub fn is_built(&self) -> bool {
        self.hardware_dir()
            .join(format!("{}.v", self.name))
            .is_file()
            && self.temp_dir.join(format!("{}_tb.v", self.name)).is_file()
    }

    /// Cria `Software/`, `Hardware/`, `Simulation/` e o diretório temporário,
    /// se faltarem. Os compiladores YANC não criam diretório de forma
    /// confiável (o `cmmcomp` não cria nenhum), então o Solar faz isso antes
    /// de qualquer execução.
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [
            self.software_dir(),
            self.hardware_dir(),
            self.simulation_dir(),
            self.temp_dir.clone(),
        ] {
            if dir.exists() && !dir.is_dir() {
                return Err(SolarError::InvalidProject {
                    path: dir,
                    reason: "existe, mas não é um diretório".into(),
                });
            }
            std::fs::create_dir_all(&dir).map_err(SolarError::io("criando diretório", &dir))?;
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
    /// use solar_core::{Language, NewProcessor};
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
/// `set_top_level`, ...) gravam o `.spf` na hora; não há "salvar". Se outro
/// programa alterar o `.spf` enquanto o projeto estiver aberto, abra de novo:
/// o `Project` não recarrega sozinho, e uma gravação sobrescreve o arquivo
/// com a versão em memória.
#[derive(Debug, Clone, Serialize)]
pub struct Project {
    name: String,
    pub(crate) root: Utf8PathBuf,
    spf_path: Utf8PathBuf,
    processors: Vec<Processor>,
    /// O `.spf` inteiro, para regravar sem perder o que o Solar não entende.
    #[serde(skip)]
    pub(crate) document: Value,
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
    /// - [`SolarError::InvalidProject`] se o caminho não existir, ou se o
    ///   diretório não tiver `.spf` ou tiver mais de um;
    /// - [`SolarError::InvalidProjectFile`] se o `.spf` não for JSON mesmo com
    ///   a leitura tolerante, se não tiver a seção `structure`, ou se um
    ///   processador tiver `language` desconhecida ou `clk`/`numClocks` que
    ///   não seja inteiro positivo;
    /// - [`SolarError::Io`] se o arquivo não puder ser lido.
    pub fn open(path: impl AsRef<Utf8Path>) -> Result<Self> {
        let spf_path = locate_spf(path.as_ref())?;
        let spf_path = paths::canonicalize(&spf_path)?;
        let text = std::fs::read_to_string(&spf_path)
            .map_err(SolarError::io("lendo projeto", &spf_path))?;
        let document = spf::parse(&spf_path, &text)?;
        Self::from_document(spf_path, document)
    }

    fn from_document(spf_path: Utf8PathBuf, document: Value) -> Result<Self> {
        let root = spf_path
            .parent()
            .expect("um arquivo canonicalizado tem diretório pai")
            .to_owned();
        let name = spf_path.file_stem().unwrap_or_default().to_owned();

        let processors = spf::processors(&spf_path, &document)?
            .into_iter()
            .map(|entry| processor_from_entry(&spf_path, &root, entry))
            .collect::<Result<Vec<_>>>()?;

        Ok(Project {
            name,
            root,
            spf_path,
            processors,
            document,
        })
    }

    /// Cria `<pai>/<nome>/<nome>.spf`, sem processadores, no formato da
    /// AURORA (`metadata` e `structure` completos). Cria `<pai>/<nome>` se
    /// preciso; `<pai>` precisa existir.
    ///
    /// # Erros
    ///
    /// - [`SolarError::InvalidName`] se o nome estiver vazio, tiver
    ///   separador de caminho ou caractere proibido no Windows, ou começar
    ///   com `.`;
    /// - [`SolarError::ProjectExists`] se o `.spf` já existir (nada é
    ///   alterado);
    /// - [`SolarError::Io`] se `<pai>` não existir ou não puder ser escrito.
    pub fn create(parent: impl AsRef<Utf8Path>, name: &str) -> Result<Self> {
        validate_project_name(name)?;
        let parent = paths::canonicalize(parent.as_ref())?;
        let root = parent.join(name);
        let spf_path = root.join(format!("{name}.spf"));
        if spf_path.exists() {
            return Err(SolarError::ProjectExists(spf_path));
        }
        std::fs::create_dir_all(&root).map_err(SolarError::io("criando projeto", &root))?;

        let now = iso8601_now();
        let document = json!({
            "metadata": {
                "projectName": name,
                "createdAt": now,
                "lastModified": now,
                "computerName": computer_name(),
                "appVersion": concat!("solar ", env!("CARGO_PKG_VERSION")),
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
        spf::write(&spf_path, &document)?;
        tracing::info!(%spf_path, "projeto criado");
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
    /// - [`SolarError::InvalidName`] se o nome não for um identificador (a
    ///   AURORA aceita `-`, mas o lexer do `cmmcomp` não);
    /// - [`SolarError::ProcessorExists`] se o nome já estiver no projeto,
    ///   inclusive com outra caixa (`Soma` e `soma` colidem no Windows e no
    ///   macOS);
    /// - [`SolarError::InvalidProject`] se o fonte já existir no disco: o
    ///   Solar nunca sobrescreve código do usuário.
    pub fn add_processor(&mut self, spec: &NewProcessor) -> Result<&Processor> {
        validate_processor_name(&spec.name)?;
        // Windows e macOS não distinguem maiúsculas em nome de pasta: `Soma`
        // e `soma` iriam para o mesmo diretório.
        if let Some(existing) = self
            .processors
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&spec.name))
        {
            return Err(SolarError::ProcessorExists(existing.name.clone()));
        }

        let processor = self.processor_paths(&spec.name, spec.language, None);
        if processor.source.exists() {
            return Err(SolarError::InvalidProject {
                path: processor.source,
                reason: "o fonte já existe; o Solar não sobrescreve código".into(),
            });
        }
        processor.ensure_dirs()?;
        std::fs::write(&processor.source, spec.source_text())
            .map_err(SolarError::io("criando fonte", &processor.source))?;

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

        tracing::info!(name = %spec.name, "processador criado");
        self.processors.push(processor);
        Ok(self.processors.last().expect("acabou de entrar"))
    }

    /// Regrava o `.spf`, alinhando `basePath` e `projectPath` à raiz atual e
    /// atualizando `lastModified`, como o escritor da AURORA.
    pub(crate) fn save(&mut self) -> Result<()> {
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

    /// Intermediários do projeto inteiro (`<raiz>/.solar/Temp`): o `.vvp` e o
    /// `obj_dir` da simulação do projeto, a síntese, os esquemáticos.
    pub fn temp_dir(&self) -> Utf8PathBuf {
        self.root.join(".solar").join("Temp")
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
            .ok_or_else(|| SolarError::ProcessorNotFound {
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
    /// use solar_core::{Language, NewProcessor, Project, ProcessorConfig};
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
    /// - [`SolarError::ProcessorNotFound`] se o processador não existir;
    /// - [`SolarError::InvalidProjectFile`] se `frequency_mhz` ou `clocks`
    ///   for 0 (o `asmcomp` precisa de valores positivos).
    pub fn configure_processor(
        &mut self,
        name: &str,
        config: &ProcessorConfig,
    ) -> Result<&Processor> {
        let index = self
            .processors
            .iter()
            .position(|p| p.name == name)
            .ok_or_else(|| SolarError::ProcessorNotFound {
                name: name.to_owned(),
                available: self.processors.iter().map(|p| p.name.clone()).collect(),
            })?;
        for (field, value) in [("clk", config.frequency_mhz), ("numClocks", config.clocks)] {
            if value == Some(0) {
                return Err(spf::invalid(
                    &self.spf_path,
                    format!("processador '{name}': {field} precisa ser um inteiro positivo"),
                ));
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
        temp_dir: root.join(".solar").join("Temp").join(name),
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
                format!("processador '{name}': linguagem desconhecida '{lang}'"),
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
    if let Some(clk) = spf::positive_int(spf_path, &name, "clk", entry.clk.as_ref())? {
        processor.frequency_mhz = clk;
    }
    if let Some(n) = spf::positive_int(spf_path, &name, "numClocks", entry.num_clocks.as_ref())? {
        processor.clocks = n;
    }
    if let Some(b) = spf::boolean(entry.show_arrays.as_ref()) {
        processor.show_arrays = b;
    }
    Ok(processor)
}

fn locate_spf(path: &Utf8Path) -> Result<Utf8PathBuf> {
    if path.is_file() {
        return Ok(path.to_owned());
    }
    if !path.is_dir() {
        return Err(SolarError::InvalidProject {
            path: path.to_owned(),
            reason: "não existe".into(),
        });
    }
    // Canonicaliza antes para que "." tenha nome de diretório.
    let path = &paths::canonicalize(path)?;
    if let Some(stem) = path.file_name() {
        let candidate = path.join(format!("{stem}.spf"));
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    let entries = path
        .read_dir_utf8()
        .map_err(SolarError::io("listando projeto", path))?;
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(SolarError::io("listando projeto", path))?;
        if entry.path().extension() == Some("spf") && entry.path().is_file() {
            found.push(entry.path().to_owned());
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(SolarError::InvalidProject {
            path: path.to_owned(),
            reason: "nenhum arquivo .spf no diretório".into(),
        }),
        n => Err(SolarError::InvalidProject {
            path: path.to_owned(),
            reason: format!("{n} arquivos .spf no diretório; indique qual abrir"),
        }),
    }
}

/// O nome do processador vira `#PRNAME`, nome de módulo Verilog e nome de
/// arquivo, então precisa ser um identificador válido para os três. A AURORA
/// aceita `-`, mas o lexer do cmmcomp não (`{LETRA}({LETRA}|[0-9])*`).
fn validate_processor_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    let invalid = |reason: &str| SolarError::InvalidName {
        name: name.to_owned(),
        reason: reason.into(),
    };
    if !valid {
        return Err(invalid(
            "use letras, dígitos e '_', começando por letra ou '_'",
        ));
    }
    if is_windows_reserved(name) {
        return Err(invalid(WINDOWS_RESERVED_REASON));
    }
    Ok(())
}

const WINDOWS_RESERVED_REASON: &str = "é um nome reservado do Windows (CON, PRN, AUX, NUL, COM0-9, LPT0-9): a pasta não poderia ser criada lá";

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

fn validate_project_name(name: &str) -> Result<()> {
    let reason = if name.trim().is_empty() {
        Some("vazio")
    } else if name.contains(['/', '\\', '<', '>', ':', '"', '|', '?', '*']) {
        Some("tem caractere proibido em nome de pasta")
    } else if name.ends_with(['.', ' ']) || name.starts_with('.') {
        Some("não pode começar com '.' nem terminar em '.' ou espaço")
    } else if name.chars().any(char::is_control) {
        Some("tem caractere de controle")
    } else if is_windows_reserved(name) {
        Some(WINDOWS_RESERVED_REASON)
    } else {
        None
    };
    match reason {
        Some(reason) => Err(SolarError::InvalidName {
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
        assert_eq!(soma.temp_dir, root.join(".solar/Temp/soma"));
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
            Err(SolarError::ProcessorExists(_))
        ));
        assert!(matches!(
            project.add_processor(&NewProcessor::new("proc-1", Language::Cmm)),
            Err(SolarError::InvalidName { .. })
        ));
        assert!(matches!(
            Project::create(&dir, "p"),
            Err(SolarError::ProjectExists(_))
        ));
        assert!(matches!(
            Project::create(&dir, "a/b"),
            Err(SolarError::InvalidName { .. })
        ));
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
            Err(SolarError::InvalidName { .. })
        ));
        project
            .add_processor(&NewProcessor::new("Soma", Language::Cmm))
            .unwrap();
        assert!(matches!(
            project.add_processor(&NewProcessor::new("soma", Language::Cmm)),
            Err(SolarError::ProcessorExists(name)) if name == "Soma"
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
            Err(SolarError::ProcessorNotFound { .. })
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
}
