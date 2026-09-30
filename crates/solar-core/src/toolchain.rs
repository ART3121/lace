//! O bundle de ferramentas instalado com o Solar, e só ele.
//!
//! O Solar não executa programa do `PATH` nem de um caminho que o usuário
//! configure. Toda ferramenta sai de um **bundle**: um diretório instalado com
//! o Solar, com um manifesto que fixa a versão exata de cada componente, e um
//! executável num caminho fixo por plataforma.
//!
//! ```text
//! <instalação>/
//!   bin/solar[.exe]
//!   toolchain/                 o bundle
//!     bundle.json              cabeçalho: formato, versão do bundle, plataforma
//!     components/<nome>.json   um por componente instalado: versão, origem, hashes
//!     yanc/                    bin/ SAPHO/ Macros/ Header/
//!     oss-cad-suite/           a parte do OSS CAD Suite de cada ferramenta instalada
//!     surfer-aurora/           o fork do Surfer da AURORA
//!     graphviz/                só no Windows, onde o OSS CAD Suite não traz o dot
//! ```
//!
//! O instalador deixa o usuário escolher os componentes. Um componente que
//! não foi instalado não tem arquivo em `components/`, e as ferramentas dele
//! dão [`SolarError::ComponentMissing`].
//!
//! A única exceção é a do Verilator: o compilador C++, o `make` e o Perl vêm
//! do sistema ([`SystemCompiler`]), procurados em locais padrão fixos, nunca
//! no `PATH`.
//!
//! # Uso
//!
//! ```no_run
//! use solar_core::{Tool, Toolchain};
//!
//! // O bundle ao lado do executável: <exe>/../toolchain ou <exe>/toolchain.
//! let exe = camino::Utf8PathBuf::try_from(std::env::current_exe()?)?;
//! let toolchain = Toolchain::locate(&exe)?;
//! println!("bundle {}", toolchain.manifest().bundle);
//! let yosys = toolchain.tool(Tool::Yosys)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Result, SolarError};
use crate::paths;
use crate::process::{GUI_ENV, Invocation};

/// Versão do formato do manifesto que este Solar entende.
pub const BUNDLE_SCHEMA: u32 = 2;

/// Nome do cabeçalho do manifesto dentro do diretório do bundle.
pub const MANIFEST_FILE: &str = "bundle.json";

/// Diretório, dentro do bundle, com um `<nome>.json` por componente
/// instalado.
pub const COMPONENTS_DIR: &str = "components";

/// Os componentes que um bundle pode ter, pelo nome usado no manifesto.
///
/// Icarus, Verilator, Yosys e o `dot` do Linux e do macOS vêm da mesma
/// release do OSS CAD Suite e dividem o diretório `oss-cad-suite/`: cada um
/// traz a sua parte do pacote, e as bibliotecas em comum vêm com qualquer um
/// deles.
pub mod component {
    /// Os compiladores YANC e a biblioteca SAPHO.
    pub const YANC: &str = "yanc";
    /// Icarus Verilog (`iverilog`, `vvp`).
    pub const ICARUS: &str = "icarus";
    /// Verilator, com o Python do pacote que o `make` dele chama.
    pub const VERILATOR: &str = "verilator";
    /// Yosys.
    pub const YOSYS: &str = "yosys";
    /// O `dot` do Graphviz, para o esquemático: do OSS CAD Suite no Linux e no
    /// macOS, do pacote oficial do Graphviz no Windows.
    pub const GRAPHVIZ: &str = "graphviz";
    /// O fork do Surfer da AURORA.
    pub const SURFER_AURORA: &str = "surfer-aurora";

    /// Todos, na ordem em que o Solar os lista.
    pub const ALL: &[&str] = &[YANC, ICARUS, VERILATOR, YOSYS, GRAPHVIZ, SURFER_AURORA];
}

/// Uma plataforma suportada, com o nome que o OSS CAD Suite usa nos pacotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Platform {
    /// Linux em x86-64.
    #[serde(rename = "linux-x64")]
    LinuxX64,
    /// macOS em Apple Silicon.
    #[serde(rename = "darwin-arm64")]
    MacosArm64,
    /// Windows 10 e 11 em x86-64.
    #[serde(rename = "windows-x64")]
    WindowsX64,
}

impl Platform {
    /// A plataforma em que o Solar está rodando, se for suportada.
    pub fn current() -> Option<Platform> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => Some(Platform::LinuxX64),
            ("macos", "aarch64") => Some(Platform::MacosArm64),
            ("windows", "x86_64") => Some(Platform::WindowsX64),
            _ => None,
        }
    }

    /// O nome no manifesto e nos pacotes (`linux-x64`, `darwin-arm64`,
    /// `windows-x64`).
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::LinuxX64 => "linux-x64",
            Platform::MacosArm64 => "darwin-arm64",
            Platform::WindowsX64 => "windows-x64",
        }
    }

    /// O contrário de [`Platform::as_str`].
    pub fn from_name(name: &str) -> Option<Platform> {
        [
            Platform::LinuxX64,
            Platform::MacosArm64,
            Platform::WindowsX64,
        ]
        .into_iter()
        .find(|p| p.as_str() == name)
    }

    fn exe(self) -> &'static str {
        if self == Platform::WindowsX64 {
            ".exe"
        } else {
            ""
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// As ferramentas que o Solar sabe orquestrar.
///
/// Em JSON e em [`Display`](fmt::Display), cada uma aparece pelo nome do
/// programa ([`Tool::binary_name`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Tool {
    /// Compilador C± do YANC: `.cmm` para `.asm`.
    Cmmcomp,
    /// Pré-assembler do YANC: conta instruções e variáveis do `.asm` e
    /// registra o `#PRNAME` em `app_log.txt`.
    Appcomp,
    /// Assembler do YANC: `.asm` para o Verilog do processador, as memórias
    /// `.mif` e o testbench.
    Asmcomp,
    /// Pré-processador do fluxo C do YANC (`#include`, `#define`).
    Cpppp,
    /// Compilador C do YANC: `.cpp` pré-processado para `.asm`.
    Cppcomp,
    /// Compilador do Icarus Verilog.
    Iverilog,
    /// Simulador do Icarus: executa o `.vvp` que o `iverilog` gera.
    Vvp,
    /// Verilator: compila o Verilog para um executável C++ e o roda como
    /// simulação. O compilador C++ vem do sistema ([`SystemCompiler`]).
    Verilator,
    /// Yosys: sintetiza Verilog para netlist e gera o grafo do esquemático.
    Yosys,
    /// `dot` do Graphviz: desenha o grafo do esquemático em SVG.
    Dot,
    /// O visualizador de ondas surfer-aurora. Aplicação gráfica: abre e fica
    /// aberta.
    Surfer,
    /// O Perl do sistema, que executa o script `verilator`. Parte da exceção
    /// do [`SystemCompiler`].
    Perl,
}

impl Tool {
    /// Nome do programa, sem extensão.
    pub fn binary_name(self) -> &'static str {
        match self {
            Tool::Cmmcomp => "cmmcomp",
            Tool::Appcomp => "appcomp",
            Tool::Asmcomp => "asmcomp",
            Tool::Cpppp => "cpppp",
            Tool::Cppcomp => "cppcomp",
            Tool::Iverilog => "iverilog",
            Tool::Vvp => "vvp",
            Tool::Verilator => "verilator",
            Tool::Yosys => "yosys",
            Tool::Dot => "dot",
            Tool::Surfer => "surfer-aurora",
            Tool::Perl => "perl",
        }
    }

    /// Faz parte do YANC?
    pub fn is_yanc(self) -> bool {
        matches!(
            self,
            Tool::Cmmcomp | Tool::Appcomp | Tool::Asmcomp | Tool::Cpppp | Tool::Cppcomp
        )
    }

    /// Vem do sistema, e não do bundle (a exceção do Verilator)?
    pub fn is_system(self) -> bool {
        self == Tool::Perl
    }

    /// Todas as ferramentas, na ordem da declaração.
    pub fn all() -> &'static [Tool] {
        &[
            Tool::Cmmcomp,
            Tool::Appcomp,
            Tool::Asmcomp,
            Tool::Cpppp,
            Tool::Cppcomp,
            Tool::Iverilog,
            Tool::Vvp,
            Tool::Verilator,
            Tool::Yosys,
            Tool::Dot,
            Tool::Surfer,
            Tool::Perl,
        ]
    }

    /// O contrário de [`Tool::binary_name`].
    pub fn from_name(name: &str) -> Option<Tool> {
        Tool::all()
            .iter()
            .copied()
            .find(|t| t.binary_name() == name)
    }

    /// O componente do bundle de que a ferramenta faz parte (nomes em
    /// [`component`]). `None` para as ferramentas do sistema.
    pub fn component(self) -> Option<&'static str> {
        Some(match self {
            Tool::Cmmcomp | Tool::Appcomp | Tool::Asmcomp | Tool::Cpppp | Tool::Cppcomp => {
                component::YANC
            }
            Tool::Iverilog | Tool::Vvp => component::ICARUS,
            Tool::Verilator => component::VERILATOR,
            Tool::Yosys => component::YOSYS,
            Tool::Dot => component::GRAPHVIZ,
            Tool::Surfer => component::SURFER_AURORA,
            Tool::Perl => return None,
        })
    }

    /// Componente do bundle e caminho do executável dentro do diretório dele.
    /// `None` para as ferramentas do sistema.
    fn location(self, platform: Platform) -> Option<(&'static str, String)> {
        let exe = platform.exe();
        let rel = match self {
            // O `verilator` do OSS CAD Suite é um script Perl em todas as
            // plataformas (no Windows, sem extensão).
            Tool::Verilator => "bin/verilator".to_owned(),
            Tool::Surfer => format!("surfer-aurora{exe}"),
            // No Windows o `dot` vem do Graphviz (`graphviz/bin/dot.exe`); nos
            // outros, do OSS CAD Suite (`oss-cad-suite/bin/dot`). O diretório
            // vem do manifesto do componente.
            _ => format!("bin/{}{exe}", self.binary_name()),
        };
        Some((self.component()?, rel))
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.binary_name())
    }
}

/// O manifesto do bundle, escrito pelo empacotamento (`scripts/bundle.py`)
/// e pelos instaladores, e só lido pelo Solar: o cabeçalho em `bundle.json` e
/// um arquivo por componente instalado em `components/`.
///
/// ```json
/// // bundle.json
/// { "schema": 2, "bundle": "2026.09.29", "platform": "linux-x64" }
///
/// // components/icarus.json
/// { "name": "icarus", "version": "2026-09-29", "dir": "oss-cad-suite",
///   "source": "https://github.com/YosysHQ/oss-cad-suite-build/releases/download/...",
///   "sha256": "eba89154...",
///   "files": { "oss-cad-suite/bin/iverilog": "<sha256>", "...": "..." } }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BundleManifest {
    /// Versão do formato ([`BUNDLE_SCHEMA`]).
    pub schema: u32,
    /// Identificador da versão do bundle.
    pub bundle: String,
    /// Plataforma para a qual o bundle foi montado ([`Platform::as_str`]).
    pub platform: String,
    /// Os componentes instalados, na ordem de [`component::ALL`] (um nome
    /// que este Solar não conhece vai para o fim). Lidos de `components/`,
    /// não do `bundle.json`.
    #[serde(default, skip_deserializing)]
    pub components: Vec<BundleComponent>,
}

/// Um componente instalado (`components/<nome>.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BundleComponent {
    /// Nome (ver o módulo [`component`]); é também o nome do arquivo.
    pub name: String,
    /// Versão exata: data da release do OSS CAD Suite, tag do YANC, do
    /// surfer-aurora, do Graphviz.
    pub version: String,
    /// Diretório do componente, relativo ao bundle. Componentes do OSS CAD
    /// Suite dividem o mesmo.
    pub dir: Utf8PathBuf,
    /// De onde veio: URL do pacote, ou `git+<repositório>@<commit>` quando o
    /// empacotamento compilou do fonte.
    pub source: String,
    /// SHA-256 do pacote de origem, quando foi baixado pronto.
    #[serde(default)]
    pub sha256: Option<String>,
    /// SHA-256 dos executáveis do componente que o Solar roda, por caminho
    /// relativo ao bundle, conferido por [`Toolchain::verify`].
    #[serde(default)]
    pub files: BTreeMap<Utf8PathBuf, String>,
}

/// A exceção do Verilator: compilador C++, `make` e Perl do sistema.
///
/// Procurados em locais fixos por plataforma ([`SystemCompiler::detect`]),
/// nunca no `PATH`: `/usr/bin` no Linux; `/usr/bin` com as Command Line Tools
/// do Xcode no macOS; o MSYS2 em `C:\msys64` no Windows (`ucrt64` ou
/// `mingw64`). Ou num diretório declarado ([`SystemCompiler::in_msys2`] e
/// [`SystemCompiler::in_dir`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SystemCompiler {
    /// O Perl que executa o script `verilator`.
    pub perl: Utf8PathBuf,
    /// O `make` que o Verilator chama.
    pub make: Utf8PathBuf,
    /// O compilador C++ que o `make` chama.
    pub cxx: Utf8PathBuf,
    /// Os diretórios postos no `PATH` do Verilator para ele achar `make` e o
    /// compilador.
    pub path: Vec<Utf8PathBuf>,
}

impl SystemCompiler {
    /// Procura nos locais padrão da plataforma atual. `None` se algum dos três
    /// programas faltar.
    pub fn detect() -> Option<SystemCompiler> {
        match Platform::current()? {
            Platform::LinuxX64 => Self::in_dir("/usr/bin".into()),
            Platform::MacosArm64 => {
                // Os `/usr/bin/clang++` e `/usr/bin/make` existem sempre no
                // macOS, mas são atalhos que só funcionam com as Command Line
                // Tools ou o Xcode instalados.
                let installed = [
                    "/Library/Developer/CommandLineTools/usr/bin/clang++",
                    "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/clang++",
                ]
                .iter()
                .any(|p| Utf8Path::new(p).is_file());
                installed.then(|| Self::in_dir("/usr/bin".into())).flatten()
            }
            Platform::WindowsX64 => ["C:/msys64", "C:/tools/msys64"]
                .iter()
                .find_map(|root| Self::in_msys2(Utf8Path::new(root))),
        }
    }

    /// Compilador, `make` e Perl num único diretório (`/usr/bin`).
    pub fn in_dir(dir: Utf8PathBuf) -> Option<SystemCompiler> {
        let exe = Platform::current().map_or("", Platform::exe);
        let find = |names: &[&str]| {
            names
                .iter()
                .map(|n| dir.join(format!("{n}{exe}")))
                .find(|p| p.is_file())
        };
        Some(SystemCompiler {
            perl: find(&["perl"])?,
            make: find(&["make"])?,
            cxx: find(&["g++", "c++", "clang++"])?,
            path: vec![dir.clone()],
        })
    }

    /// Uma instalação do MSYS2 (Windows): Perl e `make` em `usr/bin`,
    /// compilador em `ucrt64/bin` ou `mingw64/bin`.
    pub fn in_msys2(root: &Utf8Path) -> Option<SystemCompiler> {
        let usr = root.join("usr/bin");
        let perl = usr.join("perl.exe");
        let make = usr.join("make.exe");
        let cxx_dir = ["ucrt64/bin", "mingw64/bin"]
            .iter()
            .map(|d| root.join(d))
            .find(|d| d.join("g++.exe").is_file())?;
        (perl.is_file() && make.is_file()).then(|| SystemCompiler {
            perl,
            make,
            cxx: cxx_dir.join("g++.exe"),
            path: vec![cxx_dir, usr],
        })
    }
}

/// Um executável do bundle cujo hash não bate com o manifesto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct FileMismatch {
    /// O componente que lista o arquivo.
    pub component: String,
    /// Caminho relativo ao bundle.
    pub path: Utf8PathBuf,
    /// O hash do manifesto.
    pub expected: String,
    /// O hash do arquivo, ou `None` se ele não existe.
    pub actual: Option<String>,
}

/// O bundle resolvido: manifesto lido, plataforma conferida, diretórios dos
/// componentes presentes. Construa com [`Toolchain::open`] ou
/// [`Toolchain::locate`]; é barata de clonar e pode ser compartilhada entre
/// threads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Toolchain {
    root: Utf8PathBuf,
    platform: Platform,
    manifest: BundleManifest,
    system: Option<SystemCompiler>,
}

impl Toolchain {
    /// Abre o bundle em `dir`: lê o `bundle.json` e os manifestos em
    /// `components/`, confere o formato e a plataforma, e que o diretório de
    /// cada componente existe. Procura o [`SystemCompiler`] nos locais padrão.
    ///
    /// Um bundle pode ter só parte dos componentes (os que o usuário escolheu
    /// no instalador, ou um bundle de desenvolvimento só com o YANC): a
    /// ferramenta de um componente ausente dá erro só quando for usada.
    ///
    /// # Erros
    ///
    /// - [`SolarError::UnsupportedPlatform`] fora de Linux x64, macOS arm64 e
    ///   Windows x64;
    /// - [`SolarError::InvalidBundle`] sem `bundle.json`, com JSON inválido,
    ///   formato desconhecido, plataforma diferente da atual, manifesto de
    ///   componente ilegível ou com outro nome, ou diretório de componente
    ///   ausente ou fora do bundle (por `..` ou por symlink).
    pub fn open(dir: impl AsRef<Utf8Path>) -> Result<Self> {
        let dir = dir.as_ref();
        let platform = Platform::current().ok_or_else(|| SolarError::UnsupportedPlatform {
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
        })?;
        let invalid = |reason: String| SolarError::InvalidBundle {
            path: dir.to_owned(),
            reason,
        };
        if !dir.is_dir() {
            return Err(invalid("não existe".into()));
        }
        let root = paths::canonicalize(dir)?;
        let manifest_path = root.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| invalid(format!("sem {MANIFEST_FILE} legível: {e}")))?;
        let mut manifest: BundleManifest =
            serde_json::from_str(&text).map_err(|e| invalid(format!("{MANIFEST_FILE}: {e}")))?;
        if manifest.schema != BUNDLE_SCHEMA {
            return Err(invalid(format!(
                "formato {} do manifesto; este Solar entende o {BUNDLE_SCHEMA}",
                manifest.schema
            )));
        }
        if manifest.platform != platform.as_str() {
            return Err(invalid(format!(
                "bundle montado para {}, mas o Solar está rodando em {platform}",
                manifest.platform
            )));
        }
        manifest.components = read_components(&root.join(COMPONENTS_DIR)).map_err(invalid)?;
        for c in &manifest.components {
            if !paths::is_contained(&c.dir) || !c.files.keys().all(|f| paths::is_contained(f)) {
                return Err(invalid(format!(
                    "o componente {} aponta para fora do bundle",
                    c.name
                )));
            }
            let dir = root.join(&c.dir);
            if !dir.is_dir() {
                return Err(invalid(format!(
                    "falta o diretório do componente {} ({})",
                    c.name, c.dir
                )));
            }
            // Um symlink não pode levar o componente para fora do bundle.
            if !paths::canonicalize(&dir)?.starts_with(&root) {
                return Err(invalid(format!(
                    "o componente {} aponta para fora do bundle",
                    c.name
                )));
            }
        }
        let toolchain = Toolchain {
            root,
            platform,
            manifest,
            system: SystemCompiler::detect(),
        };
        tracing::debug!(root = %toolchain.root, bundle = %toolchain.manifest.bundle, "bundle aberto");
        Ok(toolchain)
    }

    /// Acha o bundle instalado com o executável do Solar: `<dir do
    /// executável>/../toolchain` (instalação com `bin/`) ou `<dir do
    /// executável>/toolchain`, com os symlinks do caminho resolvidos.
    ///
    /// # Erros
    ///
    /// [`SolarError::BundleNotFound`] se nenhum dos dois tiver um
    /// `bundle.json`; os de [`Toolchain::open`] se o achado for inválido.
    pub fn locate(executable: &Utf8Path) -> Result<Self> {
        // O executável pode ser um symlink (`~/.local/bin/solar` apontando
        // para a instalação): o bundle fica ao lado do arquivo de verdade.
        let executable = paths::canonicalize(executable).unwrap_or_else(|_| executable.to_owned());
        let dir = executable.parent().unwrap_or(Utf8Path::new("."));
        let candidates: Vec<Utf8PathBuf> = [
            dir.parent().map(|p| p.join("toolchain")),
            Some(dir.join("toolchain")),
        ]
        .into_iter()
        .flatten()
        .collect();
        match candidates.iter().find(|c| c.join(MANIFEST_FILE).is_file()) {
            Some(found) => Self::open(found),
            None => Err(SolarError::BundleNotFound {
                searched: candidates,
            }),
        }
    }

    /// Troca o [`SystemCompiler`] detectado por um declarado.
    pub fn with_system_compiler(mut self, compiler: Option<SystemCompiler>) -> Self {
        self.system = compiler;
        self
    }

    /// O diretório do bundle, canonicalizado.
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// A plataforma do bundle (a atual).
    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// O manifesto.
    pub fn manifest(&self) -> &BundleManifest {
        &self.manifest
    }

    /// Um componente do manifesto, pelo nome.
    pub fn component(&self, name: &str) -> Option<&BundleComponent> {
        self.manifest.components.iter().find(|c| c.name == name)
    }

    /// O compilador do sistema para o Verilator, se foi encontrado.
    pub fn system_compiler(&self) -> Option<&SystemCompiler> {
        self.system.as_ref()
    }

    fn component_dir(&self, name: &str) -> Result<Utf8PathBuf> {
        self.component(name)
            .map(|c| self.root.join(&c.dir))
            .ok_or_else(|| SolarError::ComponentMissing(name.to_owned()))
    }

    fn yanc_dir(&self, sub: &str) -> Result<Utf8PathBuf> {
        let dir = self.component_dir(component::YANC)?.join(sub);
        if dir.is_dir() {
            Ok(dir)
        } else {
            Err(SolarError::ToolchainIncomplete {
                what: format!("diretório {sub} do YANC"),
                path: dir,
            })
        }
    }

    /// Biblioteca SAPHO (`yanc/SAPHO`): `-d` do `asmcomp`, `-y` do Icarus e
    /// do Verilator. O YANC 5.6 ainda copia a mesma pasta como `HDL/` para a
    /// AURORA, e avisa que essa cópia vai sair; o Solar usa o nome novo.
    pub fn hdl_dir(&self) -> Result<Utf8PathBuf> {
        self.yanc_dir("SAPHO")
    }

    /// A biblioteca SAPHO para o `-y` de simulação, verificação e síntese, se
    /// o YANC estiver instalado. Um projeto só de Verilog não precisa dela;
    /// com `required` (o projeto tem processadores SAPHO) e sem o YANC, é
    /// [`SolarError::ComponentMissing`].
    pub fn sapho_library(&self, required: bool) -> Result<Option<Utf8PathBuf>> {
        if self.component(component::YANC).is_none() {
            return if required {
                Err(SolarError::ComponentMissing(component::YANC.into()))
            } else {
                Ok(None)
            };
        }
        self.hdl_dir().map(Some)
    }

    /// Macros do assembler (`yanc/Macros`).
    pub fn macros_dir(&self) -> Result<Utf8PathBuf> {
        self.yanc_dir("Macros")
    }

    /// Headers do fluxo C (`yanc/Header`).
    pub fn headers_dir(&self) -> Result<Utf8PathBuf> {
        self.yanc_dir("Header")
    }

    /// Caminho absoluto do executável de `tool`, conferido no disco agora.
    ///
    /// # Erros
    ///
    /// - [`SolarError::ComponentMissing`] se o componente da ferramenta não
    ///   está neste bundle;
    /// - [`SolarError::ToolchainIncomplete`] se está, mas o executável não
    ///   (para o Verilator, também sem o `bin/verilator_bin` que o script
    ///   Perl chama: sem ele, o script iria buscá-lo no `PATH`);
    /// - [`SolarError::InvalidBundle`] se o executável é um symlink que sai
    ///   do bundle;
    /// - [`SolarError::SystemCompilerMissing`] para o [`Tool::Perl`] sem
    ///   compilador do sistema.
    pub fn tool(&self, tool: Tool) -> Result<Utf8PathBuf> {
        let Some((component, rel)) = tool.location(self.platform) else {
            return self
                .system
                .as_ref()
                .map(|s| s.perl.clone())
                .ok_or(SolarError::SystemCompilerMissing);
        };
        let dir = self.component_dir(component)?;
        let path = self.bundled_file(&dir, &rel, &format!("executável do {tool}"))?;
        if tool == Tool::Verilator {
            // O script Perl roda o `verilator_bin` ao lado dele e, se não
            // achar, procura no PATH. Sem o do bundle, o Solar recusa antes.
            let companion = format!("bin/verilator_bin{}", self.platform.exe());
            self.bundled_file(&dir, &companion, "verilator_bin do Verilator")?;
        }
        Ok(path)
    }

    /// `dir/rel` se for um arquivo que, resolvido, continua dentro do bundle.
    fn bundled_file(&self, dir: &Utf8Path, rel: &str, what: &str) -> Result<Utf8PathBuf> {
        let path = dir.join(rel);
        let incomplete = || SolarError::ToolchainIncomplete {
            what: what.to_owned(),
            path: path.clone(),
        };
        if !path.is_file() {
            return Err(incomplete());
        }
        match paths::canonicalize(&path) {
            Ok(real) if real.starts_with(&self.root) => Ok(path),
            _ => Err(SolarError::InvalidBundle {
                path: path.clone(),
                reason: format!("o {what} aponta para fora do bundle"),
            }),
        }
    }

    /// Quais ferramentas este bundle (e o sistema, para o Perl) tem agora.
    pub fn available(&self) -> Vec<Tool> {
        Tool::all()
            .iter()
            .copied()
            .filter(|&t| self.tool(t).is_ok())
            .collect()
    }

    /// Confere o SHA-256 de cada executável listado no manifesto. Lê os
    /// arquivos inteiros; é para instalação e diagnóstico, não para cada
    /// operação.
    pub fn verify(&self) -> Result<Vec<FileMismatch>> {
        let mut mismatches = Vec::new();
        for component in &self.manifest.components {
            for (rel, expected) in &component.files {
                let path = self.root.join(rel);
                let actual = match std::fs::File::open(&path) {
                    Ok(file) => {
                        Some(sha256(file).map_err(SolarError::io("conferindo bundle", &path))?)
                    }
                    Err(_) => None,
                };
                if actual.as_deref() != Some(expected.as_str()) {
                    mismatches.push(FileMismatch {
                        component: component.name.clone(),
                        path: rel.clone(),
                        expected: expected.clone(),
                        actual,
                    });
                }
            }
        }
        Ok(mismatches)
    }

    /// Quantos executáveis os manifestos listam (o que [`Toolchain::verify`]
    /// confere).
    pub fn verified_files(&self) -> usize {
        self.manifest.components.iter().map(|c| c.files.len()).sum()
    }

    /// Invocação de `tool` com CWD `cwd`, com o interpretador na frente e o
    /// ambiente que a ferramenta precisa, e nada além.
    ///
    /// - Linux e macOS: os programas do OSS CAD Suite em `bin/` são lançadores
    ///   em bash que carregam o binário de `libexec/` com as bibliotecas do
    ///   próprio pacote (no Linux, pelo `ld-linux` do pacote). O Solar roda o
    ///   lançador do bundle pelo `/bin/bash` do sistema, com `PATH` fixo em
    ///   `/usr/bin:/bin` para o `dirname` e o `readlink` que ele usa.
    /// - Windows: não há lançadores. O Solar roda o `.exe` com `PATH` em
    ///   `bin;lib` do pacote, como o `environment.bat` faz, e
    ///   `%SystemRoot%\System32` no fim, para o `cmd.exe` do `system()`.
    /// - Verilator: script Perl, rodado pelo Perl do sistema, com o `PATH` do
    ///   `make` e do compilador do sistema depois do `bin/` do pacote.
    pub(crate) fn invocation(&self, tool: Tool, cwd: impl Into<Utf8PathBuf>) -> Result<Invocation> {
        let program = self.tool(tool)?;
        let windows = self.platform == Platform::WindowsX64;
        let invocation = match tool {
            Tool::Verilator => {
                let system = self
                    .system
                    .as_ref()
                    .ok_or(SolarError::SystemCompilerMissing)?;
                let oss = self.component_dir(component::VERILATOR)?;
                // O bin/ do pacote primeiro (o `verilator_bin` que o script
                // chama), depois o make e o compilador do sistema. O
                // VERILATOR_ROOT fica sem definir: o script o deduz do próprio
                // caminho e reclama se ele vier diferente.
                let mut path = oss_path_dirs(&oss, self.platform);
                path.extend(system.path.iter().cloned());
                path.extend(base_path(self.platform));
                Invocation::new(system.perl.clone(), cwd)
                    .path_arg(&program)
                    .env("LC_ALL", "C")
                    .search_path(&path)
            }
            Tool::Iverilog | Tool::Vvp | Tool::Yosys | Tool::Dot if windows => {
                let dir = self.component_dir(tool.component().expect("ferramenta do bundle"))?;
                let mut path = oss_path_dirs(&dir, self.platform);
                path.extend(base_path(self.platform));
                Invocation::new(program, cwd).search_path(&path)
            }
            Tool::Iverilog | Tool::Vvp | Tool::Yosys | Tool::Dot => {
                Invocation::new(UNIX_SHELL, cwd)
                    .path_arg(&program)
                    .search_path(&unix_base_path())
            }
            Tool::Surfer => Invocation::new(program, cwd).inherit(GUI_ENV),
            _ => Invocation::new(program, cwd),
        };
        Ok(invocation)
    }

    /// O Python do pacote, para o `make` do Verilator (`PYTHON3=`): sem isso
    /// ele chamaria o `python3` do sistema.
    pub(crate) fn bundled_python(&self) -> Result<Utf8PathBuf> {
        let oss = self.component_dir(component::VERILATOR)?;
        let path = match self.platform {
            Platform::WindowsX64 => oss.join("lib/python3.exe"),
            _ => oss.join("bin/tabbypy3"),
        };
        if path.is_file() {
            Ok(path)
        } else {
            Err(SolarError::ToolchainIncomplete {
                what: "Python do OSS CAD Suite".into(),
                path,
            })
        }
    }

    /// Ambiente de fontes do `dot`. Sem configuração, o fontconfig do pacote
    /// lê a do sistema (`/etc/fonts`) e o desenho muda de máquina para
    /// máquina. O pacote de Linux traz fontes e um modelo de `fonts.conf`,
    /// que os lançadores gráficos dele (`xdot`, `gtkwave`) preenchem com o
    /// caminho do pacote; o Solar faz o mesmo, gravando `fonts.conf` e o
    /// cache do fontconfig em `work`. Os pacotes de macOS e o Graphviz de
    /// Windows não trazem fontes: lá o `dot` usa as do sistema, e esta função
    /// não devolve nada.
    pub(crate) fn dot_fonts(&self, work: &Utf8Path) -> Result<Vec<(&'static str, String)>> {
        if self.platform == Platform::WindowsX64 {
            return Ok(Vec::new());
        }
        let oss = self.component_dir(component::GRAPHVIZ)?;
        let etc = oss.join("etc/fonts");
        let template = etc.join("fonts.conf.template");
        let Ok(text) = std::fs::read_to_string(&template) else {
            return Ok(Vec::new());
        };
        let cache = work.join("fontconfig-cache");
        let text = with_cachedir(
            &text.replace("TARGET_DIR", &xml_escaped(oss.as_str())),
            &xml_escaped(cache.as_str()),
        );
        let config = work.join("fonts.conf");
        std::fs::write(&config, text)
            .map_err(SolarError::io("gravando configuração de fontes", &config))?;
        Ok(vec![
            ("FONTCONFIG_FILE", config.into_string()),
            ("FONTCONFIG_PATH", etc.into_string()),
        ])
    }

    /// O `PATH` para rodar um programa que o Verilator gerou (o modelo
    /// compilado precisa das bibliotecas do compilador no Windows).
    pub(crate) fn verilated_model_path(&self) -> Vec<Utf8PathBuf> {
        self.system
            .as_ref()
            .map(|s| s.path.clone())
            .unwrap_or_default()
    }
}

/// O shell que roda os lançadores do OSS CAD Suite no Linux e no macOS.
/// Parte do sistema base nos dois.
const UNIX_SHELL: &str = "/bin/bash";

/// `PATH` dos lançadores: só os utilitários do sistema base (`dirname`,
/// `readlink`).
/// Os manifestos em `components/`, na ordem de [`component::ALL`]. Sem o
/// diretório, nenhum componente.
fn read_components(dir: &Utf8Path) -> std::result::Result<Vec<BundleComponent>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{COMPONENTS_DIR}/ ilegível: {e}")),
    };
    let mut components = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| format!("{COMPONENTS_DIR}/ ilegível: {e}"))?
            .path();
        let Some(path) = Utf8Path::from_path(&path) else {
            continue;
        };
        if path.extension() != Some("json") {
            continue;
        }
        let file = format!("{COMPONENTS_DIR}/{}", path.file_name().unwrap_or_default());
        let text = std::fs::read_to_string(path).map_err(|e| format!("{file}: {e}"))?;
        let c: BundleComponent = serde_json::from_str(&text).map_err(|e| format!("{file}: {e}"))?;
        if path.file_stem() != Some(c.name.as_str()) {
            return Err(format!("{file} descreve o componente {}", c.name));
        }
        components.push(c);
    }
    let rank = |name: &str| {
        component::ALL
            .iter()
            .position(|c| *c == name)
            .unwrap_or(usize::MAX)
    };
    components.sort_by(|a, b| (rank(&a.name), &a.name).cmp(&(rank(&b.name), &b.name)));
    Ok(components)
}

/// Troca o `<cachedir>` do modelo do OSS CAD Suite (um caminho fixo em
/// `/tmp`) por `dir`, ou acrescenta um se o modelo não tiver.
fn with_cachedir(config: &str, dir: &str) -> String {
    let element = format!("<cachedir>{dir}</cachedir>");
    match (config.find("<cachedir>"), config.find("</cachedir>")) {
        (Some(start), Some(end)) if start < end => {
            format!(
                "{}{element}{}",
                &config[..start],
                &config[end + "</cachedir>".len()..]
            )
        }
        _ => config.replacen("</fontconfig>", &format!("\t{element}\n</fontconfig>"), 1),
    }
}

fn xml_escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unix_base_path() -> Vec<Utf8PathBuf> {
    vec!["/usr/bin".into(), "/bin".into()]
}

/// O sistema base no fim do `PATH` das ferramentas do OSS CAD Suite: no
/// Linux e no macOS, `/usr/bin:/bin` (o `dirname` e o `readlink` dos
/// lançadores); no Windows, `%SystemRoot%\System32` (o `cmd.exe` que o
/// `system()` da biblioteca C usa).
fn base_path(platform: Platform) -> Vec<Utf8PathBuf> {
    match platform {
        Platform::WindowsX64 => {
            let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
            vec![Utf8PathBuf::from(root).join("System32")]
        }
        _ => unix_base_path(),
    }
}

/// Os diretórios do `PATH` que o `environment` do OSS CAD Suite monta.
fn oss_path_dirs(root: &Utf8Path, platform: Platform) -> Vec<Utf8PathBuf> {
    match platform {
        Platform::WindowsX64 => vec![root.join("bin"), root.join("lib")],
        _ => vec![root.join("bin")],
    }
}

fn sha256(mut reader: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(components: &[(&str, &str)], platform: &str) -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dunce::canonicalize(dir.path()).unwrap()).unwrap();
        let header =
            serde_json::json!({ "schema": BUNDLE_SCHEMA, "bundle": "teste", "platform": platform });
        std::fs::write(root.join(MANIFEST_FILE), header.to_string()).unwrap();
        for (name, d) in components {
            if !d.contains("..") {
                std::fs::create_dir_all(root.join(d)).unwrap();
            }
            write_component(
                &root,
                serde_json::json!({ "name": name, "version": "1", "dir": d, "source": "teste" }),
            );
        }
        (dir, root)
    }

    fn write_component(root: &Utf8Path, manifest: serde_json::Value) {
        let dir = root.join(COMPONENTS_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        let name = manifest["name"].as_str().unwrap().to_owned();
        std::fs::write(dir.join(format!("{name}.json")), manifest.to_string()).unwrap();
    }

    fn current() -> &'static str {
        Platform::current()
            .expect("testes rodam numa plataforma suportada")
            .as_str()
    }

    #[test]
    fn tools_come_only_from_the_bundle() {
        let (_guard, root) = bundle(&[("yanc", "yanc")], current());
        let tc = Toolchain::open(&root).unwrap();
        // Componente presente, executável ausente.
        assert!(matches!(
            tc.tool(Tool::Cmmcomp),
            Err(SolarError::ToolchainIncomplete { .. })
        ));
        // Componente não instalado.
        assert!(
            matches!(tc.tool(Tool::Yosys), Err(SolarError::ComponentMissing(c)) if c == "yosys")
        );

        let exe = Platform::current().unwrap().exe();
        std::fs::create_dir_all(root.join("yanc/bin")).unwrap();
        std::fs::write(root.join(format!("yanc/bin/cmmcomp{exe}")), "").unwrap();
        assert_eq!(
            tc.tool(Tool::Cmmcomp).unwrap(),
            root.join(format!("yanc/bin/cmmcomp{exe}"))
        );
    }

    #[test]
    fn rejects_wrong_platform_schema_and_escapes() {
        let other = if current() == "linux-x64" {
            "windows-x64"
        } else {
            "linux-x64"
        };
        let (_g, root) = bundle(&[], other);
        assert!(
            matches!(Toolchain::open(&root), Err(SolarError::InvalidBundle { reason, .. }) if reason.contains("montado para"))
        );

        let (_g, root) = bundle(&[("yanc", "../fora")], current());
        assert!(
            matches!(Toolchain::open(&root), Err(SolarError::InvalidBundle { reason, .. }) if reason.contains("fora do bundle"))
        );

        let (_g, root) = bundle(&[], current());
        std::fs::write(
            root.join(MANIFEST_FILE),
            r#"{"schema": 99, "bundle": "x", "platform": "x"}"#,
        )
        .unwrap();
        assert!(
            matches!(Toolchain::open(&root), Err(SolarError::InvalidBundle { reason, .. }) if reason.contains("formato 99"))
        );

        // O arquivo do componente precisa ter o nome dele.
        let (_g, root) = bundle(&[("yanc", "yanc")], current());
        std::fs::rename(
            root.join("components/yanc.json"),
            root.join("components/yosys.json"),
        )
        .unwrap();
        assert!(
            matches!(Toolchain::open(&root), Err(SolarError::InvalidBundle { reason, .. }) if reason.contains("descreve o componente yanc"))
        );
    }

    #[test]
    fn components_share_a_directory_and_come_in_a_fixed_order() {
        let (_g, root) = bundle(
            &[
                ("yosys", "oss-cad-suite"),
                ("icarus", "oss-cad-suite"),
                ("yanc", "yanc"),
            ],
            current(),
        );
        let tc = Toolchain::open(&root).unwrap();
        let names: Vec<_> = tc
            .manifest()
            .components
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, ["yanc", "icarus", "yosys"]);
        // O Verilator usa o mesmo diretório, mas não foi instalado.
        std::fs::create_dir_all(root.join("oss-cad-suite/bin")).unwrap();
        std::fs::write(root.join("oss-cad-suite/bin/verilator"), "").unwrap();
        assert!(
            matches!(tc.tool(Tool::Verilator), Err(SolarError::ComponentMissing(c)) if c == "verilator")
        );
    }

    #[test]
    fn sapho_library_is_optional_without_processors() {
        let (_g, root) = bundle(&[("yosys", "oss-cad-suite")], current());
        let tc = Toolchain::open(&root).unwrap();
        assert_eq!(tc.sapho_library(false).unwrap(), None);
        assert!(
            matches!(tc.sapho_library(true), Err(SolarError::ComponentMissing(c)) if c == "yanc")
        );
        let (_g, root) = bundle(&[("yanc", "yanc")], current());
        std::fs::create_dir_all(root.join("yanc/SAPHO")).unwrap();
        let tc = Toolchain::open(&root).unwrap();
        assert_eq!(
            tc.sapho_library(false).unwrap(),
            Some(root.join("yanc/SAPHO"))
        );
    }

    #[test]
    fn verilator_requires_its_bundled_verilator_bin() {
        let (_guard, root) = bundle(&[("verilator", "oss-cad-suite")], current());
        let tc = Toolchain::open(&root).unwrap();
        std::fs::create_dir_all(root.join("oss-cad-suite/bin")).unwrap();
        std::fs::write(root.join("oss-cad-suite/bin/verilator"), "").unwrap();
        assert!(
            matches!(tc.tool(Tool::Verilator), Err(SolarError::ToolchainIncomplete { what, .. }) if what.contains("verilator_bin"))
        );
        let exe = Platform::current().unwrap().exe();
        std::fs::write(
            root.join(format!("oss-cad-suite/bin/verilator_bin{exe}")),
            "",
        )
        .unwrap();
        assert_eq!(
            tc.tool(Tool::Verilator).unwrap(),
            root.join("oss-cad-suite/bin/verilator")
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_leave_the_bundle() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(outside.path().join("bin")).unwrap();
        std::fs::write(outside.path().join("bin/cmmcomp"), "").unwrap();

        // O diretório do componente.
        let (_g, root) = bundle(&[], current());
        std::os::unix::fs::symlink(outside.path(), root.join("yanc")).unwrap();
        write_component(
            &root,
            serde_json::json!({ "name": "yanc", "version": "1", "dir": "yanc", "source": "teste" }),
        );
        assert!(
            matches!(Toolchain::open(&root), Err(SolarError::InvalidBundle { reason, .. }) if reason.contains("fora do bundle"))
        );

        // O executável.
        let (_g, root) = bundle(&[("yanc", "yanc")], current());
        std::fs::create_dir_all(root.join("yanc/bin")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("bin/cmmcomp"),
            root.join("yanc/bin/cmmcomp"),
        )
        .unwrap();
        let tc = Toolchain::open(&root).unwrap();
        assert!(
            matches!(tc.tool(Tool::Cmmcomp), Err(SolarError::InvalidBundle { reason, .. }) if reason.contains("fora do bundle"))
        );
    }

    #[test]
    fn locate_finds_bundle_next_to_executable() {
        let dir = tempfile::tempdir().unwrap();
        let install = Utf8PathBuf::from_path_buf(dunce::canonicalize(dir.path()).unwrap()).unwrap();
        let toolchain_dir = install.join("toolchain");
        std::fs::create_dir_all(&toolchain_dir).unwrap();
        std::fs::create_dir_all(install.join("bin")).unwrap();
        let manifest = serde_json::json!({
            "schema": BUNDLE_SCHEMA, "bundle": "teste", "platform": current(),
        });
        std::fs::write(toolchain_dir.join(MANIFEST_FILE), manifest.to_string()).unwrap();

        let tc = Toolchain::locate(&install.join("bin").join("solar")).unwrap();
        assert_eq!(tc.root(), toolchain_dir);

        // Por um symlink em outro diretório (`~/.local/bin/solar`).
        #[cfg(unix)]
        {
            let exe = install.join("bin/solar");
            std::fs::write(&exe, "").unwrap();
            let links = install.join("links");
            std::fs::create_dir_all(&links).unwrap();
            std::os::unix::fs::symlink(&exe, links.join("solar")).unwrap();
            let tc = Toolchain::locate(&links.join("solar")).unwrap();
            assert_eq!(tc.root(), toolchain_dir);
        }
        assert!(matches!(
            Toolchain::locate(Utf8Path::new("/nao/existe/bin/solar")),
            Err(SolarError::BundleNotFound { .. })
        ));
    }

    #[test]
    fn verify_reports_changed_and_missing_files() {
        let (_guard, root) = bundle(&[], current());
        std::fs::create_dir_all(root.join("yanc")).unwrap();
        std::fs::write(root.join("yanc/a"), "abc").unwrap();
        write_component(
            &root,
            serde_json::json!({ "name": "yanc", "version": "1", "dir": "yanc", "source": "teste",
                "files": { "yanc/a": sha256("abc".as_bytes()).unwrap(), "yanc/b": "00" } }),
        );
        let tc = Toolchain::open(&root).unwrap();
        assert_eq!(tc.verified_files(), 2);
        let bad = tc.verify().unwrap();
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0].component, "yanc");
        assert_eq!(bad[0].path, "yanc/b");
        assert_eq!(bad[0].actual, None);
        assert_eq!(
            sha256("abc".as_bytes()).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn fontconfig_cache_goes_to_the_work_dir() {
        let template = "<fontconfig>\n\t<dir>TARGET_DIR/share/fonts</dir>\n\t<cachedir>/tmp/yosyshq/cache/fontconfig</cachedir>\n</fontconfig>\n";
        let out = with_cachedir(template, "/p/x");
        assert!(out.contains("<cachedir>/p/x</cachedir>"));
        assert!(!out.contains("/tmp/yosyshq"));
        let out = with_cachedir("<fontconfig>\n</fontconfig>\n", "/p/x");
        assert_eq!(
            out,
            "<fontconfig>\n\t<cachedir>/p/x</cachedir>\n</fontconfig>\n"
        );
        assert_eq!(xml_escaped("a&b<c>"), "a&amp;b&lt;c&gt;");
    }

    #[test]
    fn tool_names_round_trip() {
        for &tool in Tool::all() {
            assert_eq!(Tool::from_name(tool.binary_name()), Some(tool));
        }
        for p in [
            Platform::LinuxX64,
            Platform::MacosArm64,
            Platform::WindowsX64,
        ] {
            assert_eq!(Platform::from_name(p.as_str()), Some(p));
        }
    }
}
