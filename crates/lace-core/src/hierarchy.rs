//! A hierarquia do design depois da elaboração: que módulo instancia qual,
//! com os nomes das instâncias, os parâmetros resolvidos e os blocos
//! `generate` expandidos, do design e de cada testbench.
//!
//! ```text
//! design:     iverilog [-g2012] [-y <SAPHO>] -o <trabalho>/design.vvp <design>     cwd raiz
//! testbench:  iverilog [-g2012] [-y <SAPHO>] -s <módulo do testbench>
//!             -o <trabalho>/tb-<n>.vvp <design> <testbench>                         cwd raiz
//! processador: iverilog [-g2012] -y <SAPHO> -s <nome>_tb
//!             -o <trabalho>/proc-<nome>.vvp Hardware/<nome>.v Simulation/<nome>_tb.v
//! ```
//!
//! `<trabalho>` é `<raiz>/.lace/Temp/hierarchy/`, refeita a cada chamada. Os
//! arquivos do design e dos testbenches são os de [`check`](crate::check).
//!
//! O `.vvp` que o `iverilog` grava lista cada escopo elaborado numa linha:
//!
//! ```text
//! S_0x55.. .scope module, "soma_tb" "soma_tb" 2 3;
//! S_0x56.. .scope module, "proc" "soma" 2 26, 3 1 0, S_0x55..;
//! ```
//!
//! O tipo, o nome da instância, o módulo, o arquivo e a linha da instância,
//! o arquivo e a linha da definição e o escopo pai (numa raiz, só o arquivo e
//! a linha da definição); os arquivos são índices da tabela `:file_names` do
//! fim do arquivo. É a hierarquia que o simulador vai usar, inclusive a da
//! biblioteca SAPHO que o `-y` trouxe, e não uma leitura do fonte.
//!
//! A AURORA monta a hierarquia com o Yosys. O Lace usa o Icarus porque é o
//! que o [`check`](crate::check) já roda, é rápido e não sintetiza nada.

use std::collections::HashMap;
use std::time::Instant;

use camino::{Utf8Path, Utf8PathBuf};
use schemars::JsonSchema;
use serde::Serialize;

use crate::control::Control;
use crate::diagnostics::Diagnostic;
use crate::error::{LaceError, Result};
use crate::files::FileRole;
use crate::pipeline::{
    Artifact, ArtifactKind, ArtifactTracker, PlannedStep, Runner, Status, Step, StepReport,
    elapsed_ms, final_status,
};
use crate::project::Project;
use crate::synth::{processor_verilog, project_sources};
use crate::toolchain::{Tool, Toolchain};

/// O que [`hierarchy`] elabora.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct HierarchyOptions {
    /// Só este processador, compilado antes: o `Hardware/<nome>.v` dele como
    /// design e o testbench que o build gerou. `None`: o projeto inteiro.
    pub processor: Option<String>,
}

/// O resultado de [`hierarchy`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct HierarchyResult {
    /// O design, sem testbench: os módulos que nenhum outro instancia (em
    /// geral o topo), com tudo o que eles instanciam. `None` num projeto sem
    /// Verilog de design.
    pub design: Option<Elaboration>,
    /// Cada testbench elaborado com o design: os registrados, na ordem do
    /// `.spf`, e depois o testbench gerado de cada processador compilado.
    pub testbenches: Vec<Elaboration>,
    /// `Succeeded` se todas as elaborações deram certo; senão, como terminou
    /// a primeira que não deu.
    pub status: Status,
    /// `elaborate` quando uma elaboração falhou ou foi cancelada.
    pub failed_step: Option<Step>,
    /// Um `iverilog` para o design e um por testbench, na ordem.
    pub steps: Vec<StepReport>,
    /// Os erros e avisos de todas as elaborações, com arquivo e linha.
    pub diagnostics: Vec<Diagnostic>,
    /// Os `.vvp` de cada elaboração.
    pub artifacts: Vec<Artifact>,
    /// Quanto as elaborações levaram juntas, em milissegundos.
    pub duration_ms: u64,
}

impl HierarchyResult {
    /// `status == Succeeded`.
    pub fn succeeded(&self) -> bool {
        self.status == Status::Succeeded
    }
}

/// Uma chamada do `iverilog` e a árvore que saiu dela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Elaboration {
    /// O testbench elaborado; `None` no design.
    #[schemars(with = "Option<String>")]
    pub testbench: Option<Utf8PathBuf>,
    /// O processador, quando o testbench é o que o build dele gerou.
    pub processor: Option<String>,
    /// Como esta elaboração terminou. Uma que falha não impede as outras.
    pub status: Status,
    /// Os módulos que nenhum outro instancia, com o que eles instanciam.
    /// Vazio quando a elaboração falhou.
    pub roots: Vec<ModuleInstance>,
    /// Os erros e avisos desta elaboração (também em
    /// [`HierarchyResult::diagnostics`]).
    pub diagnostics: Vec<Diagnostic>,
}

/// Uma instância de módulo na hierarquia elaborada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct ModuleInstance {
    /// O nome da instância (`dut`). Numa raiz, o nome do módulo. Dentro de
    /// blocos `generate`, `begin` ou `fork`, com o nome deles na frente,
    /// separado por `.` (`op_add.my_add`): os blocos não viram nós.
    pub name: String,
    /// O módulo instanciado.
    pub module: String,
    /// O arquivo que define o módulo, absoluto.
    #[schemars(with = "Option<String>")]
    pub file: Option<Utf8PathBuf>,
    /// A linha da definição (`module ...`).
    pub line: Option<u32>,
    /// O arquivo da instância, no módulo de cima. `None` numa raiz.
    #[schemars(with = "Option<String>")]
    pub instance_file: Option<Utf8PathBuf>,
    /// A linha da instância.
    pub instance_line: Option<u32>,
    /// O módulo vem da biblioteca SAPHO
    /// ([`Toolchain::sapho_library`](crate::Toolchain::sapho_library)).
    pub library: bool,
    /// As instâncias de dentro, na ordem do fonte.
    pub children: Vec<ModuleInstance>,
}

/// Elabora o design e os testbenches do projeto com o Icarus e devolve a
/// árvore de instâncias de cada um.
///
/// Sem `options.processor`, uma elaboração do design (os arquivos de
/// [`check`](crate::check), sem `-s`: o Icarus elabora como raiz cada
/// módulo que ninguém instancia), uma por testbench registrado (com o
/// design e `-s <módulo do testbench>`) e uma pelo testbench que o build de
/// cada processador compilado gerou (com o Verilog dele). Com `processor`,
/// o Verilog do processador e o testbench dele.
///
/// Uma elaboração que falha não impede as outras: a árvore do design
/// aparece mesmo com um testbench quebrado. O cancelamento do `Control`
/// para tudo. Os processadores não são compilados aqui: a hierarquia é a do
/// que está no disco; um processador nunca compilado fica de fora. Nada vai
/// para o histórico de relatórios: é uma consulta, como
/// [`Project::files`](crate::Project::files).
///
/// ```no_run
/// use lace_core::*;
/// # let toolchain = Toolchain::open("/opt/lace/toolchain")?;
/// let project = Project::open("/p/contador")?;
/// let result = hierarchy(&toolchain, &project, &HierarchyOptions::default(), &Control::default())?;
/// fn print(node: &ModuleInstance, depth: usize) {
///     println!("{}{} ({})", "  ".repeat(depth), node.name, node.module);
///     for child in &node.children {
///         print(child, depth + 1);
///     }
/// }
/// for root in result.design.iter().flat_map(|d| &d.roots) {
///     print(root, 0);
/// }
/// # Ok::<(), lace_core::LaceError>(())
/// ```
///
/// # Erros
///
/// - [`LaceError::EmptyProject`]: nada para elaborar (sem Verilog, sem
///   testbench e sem processador compilado);
/// - [`LaceError::InvalidProject`]: um arquivo registrado não existe;
/// - [`LaceError::ModuleNotFound`]: um testbench sem módulo que dê para
///   usar (a regra de [`Project::testbench_module`](crate::Project::testbench_module));
/// - [`LaceError::ProcessorNotFound`] / [`LaceError::NotBuilt`]: com
///   `processor`, processador inexistente ou não compilado;
/// - [`LaceError::ComponentMissing`] sem o Icarus, ou sem o YANC num
///   projeto com processadores (a biblioteca SAPHO vem dele).
pub fn hierarchy(
    toolchain: &Toolchain,
    project: &Project,
    options: &HierarchyOptions,
    control: &Control,
) -> Result<HierarchyResult> {
    let _span = tracing::info_span!("hierarchy").entered();
    let started = Instant::now();

    // O que elaborar: o design e cada testbench, com o que entra junto.
    let mut targets: Vec<Target> = Vec::new();
    let design = match &options.processor {
        Some(name) => {
            let verilog = processor_verilog(project, name)?;
            let processor = project.require_processor(name)?;
            let testbench = processor.simulated_testbench();
            if testbench.is_file() {
                targets.push(Target::processor(name, &verilog, testbench)?);
            }
            vec![verilog]
        }
        None => {
            let design = project_sources(project)?;
            for tb in project.files(FileRole::Testbench) {
                // Um testbench cocotb é Python: não tem hierarquia de Verilog.
                if crate::cocotb::is_testbench(&tb.path) {
                    continue;
                }
                if !tb.path.is_file() {
                    return Err(LaceError::InvalidProject {
                        path: tb.path,
                        reason: "Testbench added to the project does not exist".into(),
                    });
                }
                let top = crate::files::testbench_module_of(&tb.path)?;
                let mut files = design.clone();
                files.push(tb.path.clone());
                targets.push(Target {
                    name: format!("tb-{}", targets.len()),
                    files,
                    tops: vec![top],
                    testbench: Some(tb.path),
                    processor: None,
                });
            }
            for processor in project.processors() {
                let testbench = processor.simulated_testbench();
                let Ok(verilog) = processor_verilog(project, &processor.name) else {
                    continue;
                };
                if testbench.is_file() {
                    targets.push(Target::processor(&processor.name, &verilog, testbench)?);
                }
            }
            design
        }
    };
    if !design.is_empty() {
        // Sem `-s` o Icarus acha as raízes sozinho; com recursão
        // parametrizada ele não acha nenhuma, e elas vão com `-s`.
        let top = match &options.processor {
            Some(name) => Some(name.clone()),
            None => project.top_module().ok().flatten(),
        };
        let roots = crate::synth::design_roots(&design, top.as_deref());
        targets.insert(
            0,
            Target {
                name: "design".into(),
                files: design,
                tops: if roots.explicit {
                    roots.names
                } else {
                    Vec::new()
                },
                testbench: None,
                processor: None,
            },
        );
    }
    if targets.is_empty() {
        return Err(LaceError::EmptyProject(project.spf_path().to_owned()));
    }

    let library = toolchain.sapho_library(!project.processors().is_empty())?;
    let work = project.temp_dir().join("hierarchy");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(LaceError::io("Creating directory", &work))?;

    let mut tracker = ArtifactTracker::new();
    let mut status = Status::Succeeded;
    let mut failed_step = None;
    let mut steps = Vec::new();
    let mut diagnostics = Vec::new();
    let mut design_result = None;
    let mut testbenches = Vec::new();
    for target in targets {
        let image = work.join(format!("{}.vvp", target.name));
        tracker.expect(ArtifactKind::IcarusImage, &image, true);
        // Um Runner por elaboração: a falha de uma não pula as outras.
        let (runner, text) = elaborate(
            toolchain,
            project,
            &target.files,
            &target.tops,
            library.as_deref(),
            &image,
            control,
        )?;
        let roots = match &text {
            Some(text) => parse_vvp(text, project.root(), library.as_deref()),
            None => Vec::new(),
        };
        if runner.status != Status::Succeeded && status == Status::Succeeded {
            status = runner.status;
            failed_step = runner.failed_step;
        }
        let cancelled = runner.status == Status::Cancelled;
        if cancelled {
            status = Status::Cancelled;
            failed_step = runner.failed_step;
        }
        steps.extend(runner.steps);
        diagnostics.extend(runner.diagnostics.iter().cloned());
        let elaboration = Elaboration {
            testbench: target.testbench,
            processor: target.processor,
            status: runner.status,
            roots,
            diagnostics: runner.diagnostics,
        };
        if elaboration.testbench.is_none() && elaboration.processor.is_none() {
            design_result = Some(elaboration);
        } else {
            testbenches.push(elaboration);
        }
        if cancelled {
            break;
        }
    }

    let artifacts = tracker.finish();
    Ok(HierarchyResult {
        design: design_result,
        testbenches,
        status: final_status(status, &artifacts),
        failed_step,
        steps,
        diagnostics,
        artifacts,
        duration_ms: elapsed_ms(started),
    })
}

/// Elabora `files` com o Icarus (`-s` para cada um de `tops`; sem nenhum, ele
/// acha as raízes) num `.vvp` em `image`, com CWD na raiz do projeto, e
/// devolve o passo e, se deu certo, o texto do `.vvp`. É a elaboração da
/// [`hierarchy`] e da árvore de sinais da onda
/// ([`wave_signals`](crate::wave_signals)).
pub(crate) fn elaborate<'c>(
    toolchain: &Toolchain,
    project: &Project,
    files: &[Utf8PathBuf],
    tops: &[String],
    library: Option<&Utf8Path>,
    image: &Utf8Path,
    control: &'c Control,
) -> Result<(Runner<'c>, Option<String>)> {
    // Os caminhos que podem ir para a tabela de arquivos do `.vvp` (as
    // fontes, a biblioteca e a pasta dos `include`) vão com `/`
    // (`process::icarus_path`), como no `synth::include_paths`.
    let mut invocation = crate::synth::include_paths(
        toolchain.invocation(Tool::Iverilog, project.root())?,
        None,
        project.root(),
    );
    if files.iter().any(|f| f.extension() == Some("sv")) {
        invocation = invocation.arg("-g2012");
    }
    if let Some(library) = library {
        invocation = invocation.arg("-y").icarus_path_arg(library);
    }
    for top in tops {
        invocation = invocation.arg("-s").arg(top);
    }
    invocation = invocation.arg("-o").path_arg(image);
    for file in files {
        invocation = invocation.icarus_path_arg(file);
    }
    let mut runner = Runner::new(control);
    runner.run(PlannedStep::new(
        Step::Elaborate,
        Tool::Iverilog,
        invocation,
    ))?;
    let text = if runner.status == Status::Succeeded {
        Some(std::fs::read_to_string(image).map_err(LaceError::io("Reading", image))?)
    } else {
        None
    };
    Ok((runner, text))
}

/// Uma elaboração a fazer.
struct Target {
    /// O nome do `.vvp`.
    name: String,
    files: Vec<Utf8PathBuf>,
    /// Os `-s`. No design, vazio para o Icarus achar as raízes, menos com
    /// recursão parametrizada, em que ele não acha nenhuma.
    tops: Vec<String>,
    testbench: Option<Utf8PathBuf>,
    processor: Option<String>,
}

impl Target {
    /// O testbench gerado de um processador, com o Verilog dele.
    fn processor(name: &str, verilog: &Utf8Path, testbench: Utf8PathBuf) -> Result<Self> {
        Ok(Target {
            name: format!("proc-{name}"),
            files: vec![verilog.to_owned(), testbench.clone()],
            tops: vec![crate::files::testbench_module_of(&testbench)?],
            testbench: Some(testbench),
            processor: Some(name.to_owned()),
        })
    }
}

// ------------------------------------------------------------------ o .vvp

/// Uma linha `.scope` do `.vvp`.
pub(crate) struct Scope {
    pub(crate) label: String,
    /// `module`, `generate`, `begin`, `fork`, `function`, `task`...
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) module: String,
    /// Numa raiz, a definição; num filho, a instância. `(arquivo, linha)`.
    declared: (usize, u32),
    /// Num filho, a definição.
    defined: Option<(usize, u32)>,
    pub(crate) parent: Option<String>,
}

/// A árvore de módulos de um `.vvp`. Os blocos `generate`, `begin` e `fork`
/// não viram nós: as instâncias de dentro deles sobem para o módulo de cima,
/// com o caminho dos blocos no nome. Funções e tarefas ficam de fora.
/// Caminhos relativos da tabela de arquivos são relativos a `root`, o CWD
/// do `iverilog`.
fn parse_vvp(text: &str, root: &Utf8Path, library: Option<&Utf8Path>) -> Vec<ModuleInstance> {
    let mut scopes: Vec<Scope> = Vec::new();
    // As duas primeiras entradas são `N/A` e `<interactive>`.
    let mut files: Vec<Option<Utf8PathBuf>> = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if let Some(scope) = parse_scope(line) {
            scopes.push(scope);
        } else if let Some(count) = line
            .strip_prefix(":file_names ")
            .and_then(|rest| rest.trim_end_matches(';').trim().parse::<usize>().ok())
        {
            for entry in lines.by_ref().take(count) {
                let quoted = entry.trim().trim_end_matches(';');
                let name = quoted
                    .strip_prefix('"')
                    .and_then(|q| q.strip_suffix('"'))
                    .map(unescape)
                    .unwrap_or_default();
                let real = !name.is_empty() && name != "N/A" && !name.starts_with('<');
                // O `iverilog` recebe os fontes com `/` (`process::icarus_path`),
                // porque o `unescape` comeria a `\` de `C:\Users`; no
                // resultado, o separador volta a ser o do sistema.
                files.push(real.then(|| root.join(crate::paths::native_separators(&name))));
            }
        }
    }

    let index: HashMap<&str, usize> = scopes
        .iter()
        .enumerate()
        .map(|(i, scope)| (scope.label.as_str(), i))
        .collect();
    let file = |i: usize| files.get(i).cloned().flatten();
    let parent_of = |scope: &Scope| scope.parent.as_deref().and_then(|p| index.get(p).copied());

    // Cada módulo, com o módulo de cima e o caminho dos blocos entre eles.
    let mut nodes: Vec<Option<ModuleInstance>> = Vec::with_capacity(scopes.len());
    let mut owner: Vec<Option<usize>> = Vec::with_capacity(scopes.len());
    for scope in &scopes {
        let mut path = vec![scope.name.clone()];
        let mut up = parent_of(scope);
        let mut inside_code = false;
        while let Some(i) = up {
            match scopes[i].kind.as_str() {
                "module" => break,
                "generate" | "begin" | "fork" => path.push(scopes[i].name.clone()),
                _ => {
                    inside_code = true;
                    break;
                }
            }
            up = parent_of(&scopes[i]);
        }
        if scope.kind != "module" || inside_code {
            nodes.push(None);
            owner.push(None);
            continue;
        }
        path.reverse();
        let (definition, instance) = match scope.defined {
            Some(defined) => (defined, Some(scope.declared)),
            None => (scope.declared, None),
        };
        let definition_file = file(definition.0);
        nodes.push(Some(ModuleInstance {
            name: path.join("."),
            module: scope.module.clone(),
            library: match (&definition_file, library) {
                (Some(path), Some(library)) => path.starts_with(library),
                _ => false,
            },
            file: definition_file,
            line: Some(definition.1).filter(|&l| l > 0),
            instance_file: instance.and_then(|(f, _)| file(f)),
            instance_line: instance.map(|(_, l)| l).filter(|&l| l > 0),
            children: Vec::new(),
        }));
        owner.push(up);
    }

    // De baixo para cima: no .vvp um filho vem depois do pai, então quem
    // está no fim já tem os filhos quando sobe.
    let mut roots = Vec::new();
    for i in (0..nodes.len()).rev() {
        let Some(mut node) = nodes[i].take() else {
            continue;
        };
        node.children
            .sort_by_key(|c| (c.instance_line.unwrap_or(u32::MAX), c.name.clone()));
        match owner[i] {
            Some(p) if nodes[p].is_some() => {
                nodes[p].as_mut().expect("conferido").children.push(node);
            }
            _ => roots.push(node),
        }
    }
    roots.reverse();
    roots
}

/// `S_0x.. .scope module, "proc" "soma" 2 26, 3 1 0, S_0x..;`
pub(crate) fn parse_scope(line: &str) -> Option<Scope> {
    let (label, rest) = line.split_once(" .scope ")?;
    if !label.starts_with("S_") {
        return None;
    }
    let (kind, rest) = rest.split_once(',')?;
    let (name, rest) = quoted(rest.trim_start())?;
    let (module, rest) = quoted(rest.trim_start())?;
    let parts: Vec<&str> = rest.trim().trim_end_matches(';').split(',').collect();
    let numbers = |part: &str| -> Option<(usize, u32)> {
        let mut it = part.split_whitespace();
        Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
    };
    let declared = numbers(parts.first()?)?;
    let (defined, parent) = match parts.as_slice() {
        [_, def, parent] => (numbers(def), Some(parent.trim().to_owned())),
        _ => (None, None),
    };
    Some(Scope {
        label: label.trim().to_owned(),
        // `function.vec4.s14` é uma função; só a primeira palavra importa.
        kind: kind.trim().split('.').next().unwrap_or_default().to_owned(),
        name,
        module,
        declared,
        defined,
        parent,
    })
}

/// Uma string entre aspas no começo de `text`, sem os escapes, e o resto.
pub(crate) fn quoted(text: &str) -> Option<(String, &str)> {
    let body = text.strip_prefix('"')?;
    let mut escaped = false;
    for (i, c) in body.char_indices() {
        match c {
            '\\' if !escaped => escaped = true,
            '"' if !escaped => return Some((unescape(&body[..i]), &body[i + 1..])),
            _ => escaped = false,
        }
    }
    None
}

/// Desfaz os escapes de uma string do `.vvp`: `\\`, `\"` e octal (`\042`).
fn unescape(text: &str) -> String {
    if !text.contains('\\') {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let octal = &bytes[i + 1..bytes.len().min(i + 4)];
            if octal.len() == 3 && octal.iter().all(|b| (b'0'..=b'7').contains(b)) {
                out.push((octal[0] - b'0') * 64 + (octal[1] - b'0') * 8 + (octal[2] - b'0'));
                i += 4;
            } else {
                out.push(bytes[i + 1]);
                i += 2;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trechos de um `.vvp` real do Icarus 14 (o testbench do `soma`), com
    /// os escopos que importam: raiz, filhos, função, `generate` e escapes.
    const VVP: &str = r#"#! /usr/bin/vvp
:ivl_version "14.0 (devel)";
S_0x1 .scope module, "soma_tb" "soma_tb" 2 3;
 .timescale -9 -12;
S_0x2 .scope module, "proc" "soma" 2 26, 3 1 0, S_0x1;
S_0x3 .scope module, "p_soma" "processor" 3 56, 4 98 0, S_0x2;
S_0x4 .scope function.vec4.s14, "decode" "decode" 4 163, 4 163 0, S_0x3;
S_0x5 .scope generate, "op_add" "op_add" 4 14, 4 14 0, S_0x3;
S_0x6 .scope module, "my_add" "ula_add" 4 15, 4 418 0, S_0x5;
S_0x7 .scope module, "a\\b" "esc\042q" 3 10, 3 2 0, S_0x2;
    .scope S_0x3;
:file_names 5;
    "N/A";
    "<interactive>";
    "tb.v";
    "soma.v";
    "/lib/SAPHO/processor.v";
"#;

    #[test]
    fn module_tree_through_generate_blocks() {
        let roots = parse_vvp(VVP, Utf8Path::new("/p"), Some(Utf8Path::new("/lib/SAPHO")));
        assert_eq!(roots.len(), 1);
        let tb = &roots[0];
        assert_eq!(
            (tb.name.as_str(), tb.module.as_str()),
            ("soma_tb", "soma_tb")
        );
        assert_eq!(tb.file.as_deref(), Some(Utf8Path::new("/p/tb.v")));
        assert_eq!((tb.line, tb.instance_line), (Some(3), None));

        let proc = &tb.children[0];
        assert_eq!((proc.name.as_str(), proc.module.as_str()), ("proc", "soma"));
        assert_eq!(
            proc.instance_file.as_deref(),
            Some(Utf8Path::new("/p/tb.v"))
        );
        assert_eq!((proc.instance_line, proc.line), (Some(26), Some(1)));
        assert!(!proc.library);

        // Na ordem do fonte: a instância da linha 10 antes da 56.
        let names: Vec<&str> = proc.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["a\\b", "p_soma"]);
        assert_eq!(proc.children[0].module, "esc\"q");

        // A função some; o generate vira prefixo do nome.
        let processor = &proc.children[1];
        assert!(processor.library, "o processor vem da biblioteca SAPHO");
        assert_eq!(processor.children.len(), 1);
        assert_eq!(processor.children[0].name, "op_add.my_add");
        assert_eq!(processor.children[0].module, "ula_add");
    }

    #[test]
    fn scope_lines_and_escapes() {
        let root = parse_scope(r#"S_0x1 .scope module, "top" "top" 2 3;"#).unwrap();
        assert_eq!(
            (root.declared, root.defined, root.parent),
            ((2, 3), None, None)
        );
        assert!(parse_scope("    .scope S_0x3;").is_none());
        assert!(parse_scope(":file_names 5;").is_none());
        assert_eq!(unescape(r"a\\b"), r"a\b");
        assert_eq!(unescape(r"q\042"), "q\"");
        assert_eq!(quoted(r#""a\"b" rest"#), Some(("a\"b".to_owned(), " rest")));
    }
}
