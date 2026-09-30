//! Checagem de sintaxe (Icarus), síntese (Yosys) e esquemático (o `show` do
//! Yosys com o `dot` do Graphviz). A checagem e a síntese espelham a AURORA
//! (`main/ipc/prism.ts`, `main/ipc/prism_yosys_script.js`,
//! `js/compilation/checagem_de_sintaxe.ts`).
//!
//! ```text
//! checagem: iverilog -y <HDL> -t null -s <topo> <sintetizáveis...>         cwd raiz
//! síntese:  yosys -q -l yosys.log -s yosys_script.ys                        cwd trabalho
//!           read_verilog -setattr src "<arquivo>"   (um por arquivo)
//!           hierarchy -top <topo>
//!           proc
//!           setundef -zero
//!           opt_clean -purge
//!           write_json "<trabalho>/hierarchy.json"
//! esquemático: yosys -q -s show.ys                                             cwd trabalho
//!           read_json "<trabalho>/hierarchy.json"
//!           show -format dot -prefix <módulo> [-width] <módulo>
//!           dot -Tsvg <módulo>.dot -o <módulo>.svg                         cwd trabalho
//! ```
//!
//! A AURORA desenha com o netlistsvg (JavaScript, com um fork próprio e cerca
//! de 50 skins). O Solar usa o `show` do Yosys e o `dot` do Graphviz, que vêm
//! no bundle, em vez de exigir Node (decisão do autor, 2026-09-29). O visual é
//! o do Graphviz, diferente do da AURORA.

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::diagnostics::Diagnostic;
use crate::error::{Result, SolarError};
use crate::files::FileRole;
use crate::pipeline::{
    Artifact, ArtifactKind, ArtifactTracker, PlannedStep, Runner, Status, Step, StepReport,
    final_status,
};
use crate::project::Project;
use crate::toolchain::{Tool, Toolchain};

/// O que sintetizar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "name")]
#[non_exhaustive]
pub enum DesignTarget {
    /// O módulo de topo do projeto ([`Project::top_level`]) com todos os
    /// arquivos sintetizáveis e o Verilog de todos os processadores.
    TopLevel,
    /// Um processador compilado sozinho: `Hardware/<nome>.v` + biblioteca.
    Processor(String),
}

/// O resultado de [`check_syntax`]. Não há artefatos: a checagem só diz se
/// elabora.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckResult {
    /// O módulo de topo checado.
    pub top: String,
    /// `Succeeded` se o projeto elabora a partir do topo.
    pub status: Status,
    /// `check_syntax` quando falhou.
    pub failed_step: Option<Step>,
    /// Um passo: o `iverilog -t null`.
    pub steps: Vec<StepReport>,
    /// Os erros e avisos do Icarus, com arquivo e linha.
    pub diagnostics: Vec<Diagnostic>,
}

impl CheckResult {
    /// `status == Succeeded`.
    pub fn succeeded(&self) -> bool {
        self.status == Status::Succeeded
    }
}

/// O resultado de [`synthesize`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SynthesisResult {
    /// O módulo de topo sintetizado.
    pub top: String,
    /// Como a síntese terminou.
    pub status: Status,
    /// `synthesize` quando falhou.
    pub failed_step: Option<Step>,
    /// Um passo: o `yosys`. O log completo fica em `yosys.log`, ao lado do
    /// netlist.
    pub steps: Vec<StepReport>,
    /// Os erros e avisos do Yosys, com arquivo e linha quando ele informa.
    pub diagnostics: Vec<Diagnostic>,
    /// O netlist (`hierarchy.json`).
    pub artifacts: Vec<Artifact>,
    /// `hierarchy.json`, quando a síntese terminou.
    pub netlist: Option<Utf8PathBuf>,
    /// Os módulos do netlist, na ordem do JSON, para escolher o que desenhar.
    pub modules: Vec<String>,
}

impl SynthesisResult {
    /// `status == Succeeded`.
    pub fn succeeded(&self) -> bool {
        self.status == Status::Succeeded
    }
}

/// Como desenhar o esquemático.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SchematicOptions {
    /// Escreve a largura dos barramentos nas arestas (`show -width`). Padrão:
    /// sim.
    pub bus_widths: bool,
}

impl Default for SchematicOptions {
    fn default() -> Self {
        SchematicOptions { bus_widths: true }
    }
}

/// O resultado de [`render_schematic`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SchematicResult {
    /// O módulo desenhado.
    pub module: String,
    /// Como terminou. Uma mensagem de erro do `dot` conta como falha mesmo
    /// que ele saia com código 0.
    pub status: Status,
    /// `graph` (Yosys) ou `render` (`dot`) quando falhou.
    pub failed_step: Option<Step>,
    /// Dois passos: `graph` (`yosys show`) e `render` (`dot`).
    pub steps: Vec<StepReport>,
    /// Mensagens do Yosys e do Graphviz.
    pub diagnostics: Vec<Diagnostic>,
    /// O grafo (`.dot`) e o SVG.
    pub artifacts: Vec<Artifact>,
    /// O SVG, quando foi gerado. Fica ao lado do netlist, com o nome do
    /// módulo (caracteres fora de `[A-Za-z0-9_.-]` viram `_`).
    pub svg: Option<Utf8PathBuf>,
}

impl SchematicResult {
    /// `status == Succeeded`.
    pub fn succeeded(&self) -> bool {
        self.status == Status::Succeeded
    }
}

/// Confere se o projeto elabora a partir do módulo de topo, sem simular nem
/// sintetizar (`iverilog -t null`), como o botão Verilog da AURORA.
///
/// Entram os sintetizáveis do `.spf`, os `.v` de `<raiz>/TopLevel/` (legado
/// da AURORA) e o `Hardware/*.v` de cada processador, sem testbenches. A
/// biblioteca SAPHO entra por `-y` quando o YANC está instalado
/// ([`Toolchain::sapho_library`]). Processadores precisam ter sido
/// compilados para que o `Hardware/<nome>.v` deles exista.
///
/// # Erros
///
/// - [`SolarError::NoTopLevel`]: o projeto não tem módulo de topo;
/// - [`SolarError::InvalidName`]: o nome do arquivo de topo não é um
///   identificador Verilog;
/// - [`SolarError::InvalidProject`]: um sintetizável registrado não existe;
/// - ferramenta ausente ou que não executa, como nas demais operações.
pub fn check_syntax(toolchain: &Toolchain, project: &Project) -> Result<CheckResult> {
    let top_file = project
        .top_level()
        .ok_or_else(|| SolarError::NoTopLevel(project.spf_path().to_owned()))?;
    let top = module_name(&top_file)?;
    let _span = tracing::info_span!("check_syntax", %top).entered();

    let mut invocation = toolchain.invocation(Tool::Iverilog, project.root())?;
    if let Some(hdl) = toolchain.sapho_library(!project.processors().is_empty())? {
        invocation = invocation.arg("-y").path_arg(&hdl);
    }
    let mut invocation = invocation.arg("-tnull").arg("-s").arg(&top);
    for file in project_sources(project)? {
        invocation = invocation.path_arg(&file);
    }
    let mut runner = Runner::new();
    runner.run(PlannedStep::new(
        Step::CheckSyntax,
        Tool::Iverilog,
        invocation,
    ))?;
    Ok(CheckResult {
        top,
        status: runner.status,
        failed_step: runner.failed_step,
        steps: runner.steps,
        diagnostics: runner.diagnostics,
    })
}

/// Sintetiza com o Yosys até um netlist JSON (`hierarchy.json`), que
/// [`render_schematic`] transforma em SVG.
///
/// A síntese é a do PRISM da AURORA: elabora a hierarquia, converte processos
/// em lógica e limpa, sem mapear para uma FPGA. Serve para visualizar, não
/// para estimar área. Os arquivos ficam em
/// `<projeto>/.solar/Temp/synth/<topo>/`: o script (`yosys_script.ys`), o log
/// (`yosys.log`) e o netlist.
///
/// Com [`DesignTarget::TopLevel`], entram os mesmos arquivos de
/// [`check_syntax`]; com [`DesignTarget::Processor`], só o Verilog do
/// processador. Nos dois casos, a biblioteca SAPHO inteira (todo `.v` de
/// `HDL/` que não é testbench) entra primeiro.
///
/// ```no_run
/// use solar_core::{DesignTarget, Project, SchematicOptions, Toolchain, render_schematic, synthesize};
/// # let toolchain = Toolchain::open("/opt/solar/toolchain")?;
/// let project = Project::open("/p/soma")?;
/// let synth = synthesize(&toolchain, &project, &DesignTarget::Processor("soma".into()))?;
/// if let Some(netlist) = &synth.netlist {
///     let svg = render_schematic(&toolchain, netlist, "soma", &SchematicOptions::default())?;
///     println!("{:?}", svg.svg);
/// }
/// # Ok::<(), solar_core::SolarError>(())
/// ```
///
/// # Erros
///
/// - [`SolarError::NoTopLevel`] / [`SolarError::InvalidName`]: como em
///   [`check_syntax`];
/// - [`SolarError::ProcessorNotFound`] / [`SolarError::NotBuilt`]: com
///   `DesignTarget::Processor`, processador inexistente ou não compilado;
/// - [`SolarError::InvalidName`] também se um caminho tiver aspas ou quebra
///   de linha, que não cabem no script do Yosys.
pub fn synthesize(
    toolchain: &Toolchain,
    project: &Project,
    target: &DesignTarget,
) -> Result<SynthesisResult> {
    let (top, mut files) = match target {
        DesignTarget::TopLevel => {
            let top_file = project
                .top_level()
                .ok_or_else(|| SolarError::NoTopLevel(project.spf_path().to_owned()))?;
            (module_name(&top_file)?, project_sources(project)?)
        }
        DesignTarget::Processor(name) => {
            let processor = project.require_processor(name)?;
            let verilog = processor.hardware_dir().join(format!("{name}.v"));
            if !verilog.is_file() {
                return Err(SolarError::NotBuilt {
                    processor: name.clone(),
                    missing: verilog,
                });
            }
            (name.clone(), vec![verilog])
        }
    };
    let _span = tracing::info_span!("synthesize", %top).entered();

    // A biblioteca SAPHO primeiro, como a AURORA: todo `.v` de SAPHO/ que não
    // é testbench. Sem o YANC instalado, um projeto só de Verilog sintetiza
    // sem ela.
    let mut library = match toolchain.sapho_library(!project.processors().is_empty())? {
        Some(hdl) => verilog_in(&hdl)?,
        None => Vec::new(),
    };
    library.retain(|f| !is_test_file(f));
    library.append(&mut files);
    let files = dedup(library);

    let work = project.temp_dir().join("synth").join(&top);
    std::fs::create_dir_all(&work).map_err(SolarError::io("criando diretório", &work))?;
    let netlist = work.join("hierarchy.json");
    let script = work.join("yosys_script.ys");
    let log = work.join("yosys.log");
    std::fs::write(&script, yosys_script(&files, &top, &netlist)?)
        .map_err(SolarError::io("gravando script do Yosys", &script))?;

    let mut tracker = ArtifactTracker::new();
    tracker.expect(ArtifactKind::Netlist, &netlist, true);
    let invocation = toolchain
        .invocation(Tool::Yosys, &work)?
        .arg("-q")
        .arg("-l")
        .path_arg(&log)
        .arg("-s")
        .path_arg(&script);
    let mut runner = Runner::new();
    runner.run(PlannedStep::new(Step::Synthesize, Tool::Yosys, invocation))?;

    let artifacts = tracker.finish();
    let status = final_status(runner.status, &artifacts);
    let (netlist, modules) = if status == Status::Succeeded {
        let modules = netlist_modules(&netlist)?;
        (Some(netlist), modules)
    } else {
        (None, Vec::new())
    };
    Ok(SynthesisResult {
        top,
        status,
        failed_step: runner.failed_step,
        steps: runner.steps,
        diagnostics: runner.diagnostics,
        artifacts,
        netlist,
        modules,
    })
}

/// Desenha um módulo do netlist em SVG: o Yosys gera o grafo do módulo
/// (`show -format dot`) e o `dot` do Graphviz o desenha. Os arquivos ficam ao
/// lado do netlist: `show.ys`, `<módulo>.dot` e `<módulo>.svg` (no Linux,
/// também o `fonts.conf` que aponta o `dot` para as fontes do bundle, e o
/// cache dele).
///
/// Os nomes possíveis estão em [`SynthesisResult::modules`]. Submódulos
/// aparecem como caixas no desenho do módulo pai; para abrir um, chame de
/// novo com o nome dele.
///
/// # Erros
///
/// - [`SolarError::InvalidNetlist`]: o netlist não é JSON ou não tem o
///   módulo;
/// - [`SolarError::ComponentMissing`]: o bundle não tem o Yosys ou o `dot`.
pub fn render_schematic(
    toolchain: &Toolchain,
    netlist: &Utf8Path,
    module: &str,
    options: &SchematicOptions,
) -> Result<SchematicResult> {
    let _span = tracing::info_span!("render_schematic", module).entered();
    if !netlist_modules(netlist)?.iter().any(|m| m == module) {
        return Err(SolarError::InvalidNetlist {
            path: netlist.to_owned(),
            reason: format!("não tem o módulo '{module}'"),
        });
    }

    let dir = netlist.parent().expect("arquivo tem pasta").to_owned();
    let stem = file_safe(module);
    let dot_file = dir.join(format!("{stem}.dot"));
    let svg = dir.join(format!("{stem}.svg"));
    let script = dir.join("show.ys");
    let width = if options.bus_widths { " -width" } else { "" };
    // O `-prefix` do `show` não aceita aspas: vai relativo ao CWD (a pasta
    // da síntese), com o nome já sem espaço nem caractere especial.
    let text = format!(
        "read_json {}\nshow -format dot -prefix {stem}{width} {}\n",
        yosys_quoted(netlist)?,
        yosys_selection(module),
    );
    std::fs::write(&script, text).map_err(SolarError::io("gravando script do Yosys", &script))?;

    let mut tracker = ArtifactTracker::new();
    tracker.expect(ArtifactKind::SchematicGraph, &dot_file, true);
    tracker.expect(ArtifactKind::Schematic, &svg, true);
    let mut runner = Runner::new();
    let graph = toolchain
        .invocation(Tool::Yosys, &dir)?
        .arg("-q")
        .arg("-s")
        .path_arg(&script);
    if runner.run(PlannedStep::new(Step::Graph, Tool::Yosys, graph))? {
        let mut render = toolchain.invocation(Tool::Dot, &dir)?;
        for (key, value) in toolchain.dot_fonts(&dir)? {
            render = render.env(key, value);
        }
        let render = render
            .arg("-Tsvg")
            .path_arg(&dot_file)
            .arg("-o")
            .path_arg(&svg);
        runner.run(PlannedStep::new(Step::Render, Tool::Dot, render))?;
        // O dot pode avisar de erro de sintaxe e sair com 0 em versões antigas.
        runner.fail_on_error_diagnostics(Tool::Dot);
    }

    let artifacts = tracker.finish();
    let status = final_status(runner.status, &artifacts);
    Ok(SchematicResult {
        module: module.to_owned(),
        status,
        failed_step: runner.failed_step,
        steps: runner.steps,
        diagnostics: runner.diagnostics,
        artifacts,
        svg: (status == Status::Succeeded).then_some(svg),
    })
}

/// Um nome de módulo como seleção do Yosys. Nomes gerados
/// (`$paramod\processor\NUBITS=23`) vão entre aspas, como pede a sintaxe de
/// seleção para nomes com `$`, `\\` ou `=`.
fn yosys_selection(module: &str) -> String {
    if module
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        module.to_owned()
    } else {
        format!("\"{}\"", module.replace('"', ""))
    }
}

/// Os fontes do projeto para checagem e síntese, na ordem da AURORA:
/// sintetizáveis do `.spf`, `.v` de `<raiz>/TopLevel/` (legado da AURORA) e o
/// `Hardware/*.v` de cada processador, sem testbenches.
fn project_sources(project: &Project) -> Result<Vec<Utf8PathBuf>> {
    let mut files = Vec::new();
    for file in project.files(FileRole::Synthesizable) {
        if !file.path.is_file() {
            return Err(SolarError::InvalidProject {
                path: file.path,
                reason: "arquivo sintetizável registrado no projeto não existe".into(),
            });
        }
        files.push(file.path);
    }
    let top_level_dir = project.root().join("TopLevel");
    if top_level_dir.is_dir() {
        files.extend(
            verilog_in(&top_level_dir)?
                .into_iter()
                .filter(|f| !is_test_file(f)),
        );
    }
    for processor in project.processors() {
        let hardware = processor.hardware_dir();
        if hardware.is_dir() {
            files.extend(
                verilog_in(&hardware)?
                    .into_iter()
                    .filter(|f| !is_test_file(f)),
            );
        }
    }
    Ok(dedup(files))
}

fn verilog_in(dir: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let entries = dir
        .read_dir_utf8()
        .map_err(SolarError::io("listando diretório", dir))?;
    let mut files: Vec<_> = entries
        .flatten()
        .map(|e| e.into_path())
        .filter(|p| p.extension() == Some("v") && p.is_file())
        .collect();
    // Ordem estável; a AURORA usa a ordem do sistema de arquivos.
    files.sort();
    Ok(files)
}

fn is_test_file(path: &Utf8Path) -> bool {
    let name = path.file_name().unwrap_or_default().to_ascii_lowercase();
    name.contains("_tb") || name.contains("test")
}

fn dedup(files: Vec<Utf8PathBuf>) -> Vec<Utf8PathBuf> {
    let mut out: Vec<Utf8PathBuf> = Vec::with_capacity(files.len());
    for file in files {
        if !out.contains(&file) {
            out.push(file);
        }
    }
    out
}

/// O nome do módulo de topo é o nome do arquivo, e precisa ser um
/// identificador Verilog (a AURORA recusa o resto).
fn module_name(file: &Utf8Path) -> Result<String> {
    let stem = file.file_stem().unwrap_or_default();
    let mut chars = stem.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valid {
        Ok(stem.to_owned())
    } else {
        Err(SolarError::InvalidName {
            name: stem.to_owned(),
            reason: "o nome do arquivo de topo precisa ser um identificador Verilog".into(),
        })
    }
}

/// Um caminho entre aspas para um script do Yosys.
fn yosys_quoted(path: &Utf8Path) -> Result<String> {
    if path.as_str().contains(['"', '\n', '\r']) {
        return Err(SolarError::InvalidName {
            name: path.to_string(),
            reason: "caminho com aspas ou quebra de linha não entra no script do Yosys".into(),
        });
    }
    // O Yosys aceita `/` no Windows; `\` dentro de aspas vira escape.
    Ok(format!("\"{}\"", path.as_str().replace('\\', "/")))
}

fn yosys_script(files: &[Utf8PathBuf], top: &str, netlist: &Utf8Path) -> Result<String> {
    let quoted = yosys_quoted;
    let mut script = String::new();
    for file in files {
        script.push_str(&format!("read_verilog -setattr src {}\n", quoted(file)?));
    }
    script.push_str(&format!(
        "hierarchy -top {top}\nproc\nsetundef -zero\nopt_clean -purge\n"
    ));
    script.push_str(&format!("write_json {}\n", quoted(netlist)?));
    Ok(script)
}

fn netlist_modules(netlist: &Utf8Path) -> Result<Vec<String>> {
    let text =
        std::fs::read_to_string(netlist).map_err(SolarError::io("lendo netlist", netlist))?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| SolarError::InvalidNetlist {
        path: netlist.to_owned(),
        reason: e.to_string(),
    })?;
    Ok(doc["modules"]
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default())
}

/// Nome de módulo do Yosys (`$paramod\processor\NUBITS=...`) como nome de
/// arquivo.
fn file_safe(module: &str) -> String {
    let cleaned: String = module
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    cleaned
        .trim_matches('_')
        .chars()
        .take(120)
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_matches_aurora() {
        let script = yosys_script(
            &["/h/core.v".into(), "C:\\p\\soma.v".into()],
            "soma",
            Utf8Path::new("/t/hierarchy.json"),
        )
        .unwrap();
        assert_eq!(
            script,
            "read_verilog -setattr src \"/h/core.v\"\nread_verilog -setattr src \"C:/p/soma.v\"\n\
             hierarchy -top soma\nproc\nsetundef -zero\nopt_clean -purge\nwrite_json \"/t/hierarchy.json\"\n"
        );
        assert!(yosys_script(&["/a\"b.v".into()], "x", Utf8Path::new("/t/h.json")).is_err());
    }

    #[test]
    fn names() {
        assert_eq!(
            module_name(Utf8Path::new("/p/top_level.v")).unwrap(),
            "top_level"
        );
        assert!(module_name(Utf8Path::new("/p/top-level.v")).is_err());
        assert_eq!(
            file_safe("$paramod\\processor\\NUBITS=23"),
            "paramod_processor_NUBITS_23"
        );
        assert!(is_test_file(Utf8Path::new("x/soma_tb.v")));
        assert!(!is_test_file(Utf8Path::new("x/core.v")));
    }
}
