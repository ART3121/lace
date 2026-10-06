//! Comandos do projeto: abrir, criar, fechar, o retrato do projeto e o que
//! muda o `.spf` (arquivos Verilog, topo, testbench, processadores).
//!
//! Cada comando chama uma função do Core e nada mais. O
//! retrato ([`ProjectSnapshot`]) é o `lace status --json` com alguns campos a
//! mais que a interface usa para habilitar botões.

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{
    AddedFile, Control, FileRole, HierarchyOptions, HierarchyResult, LaceError, Language,
    ListPosition, MovedPath, NewProcessor, Processor, ProcessorConfig, Project, ProjectFile,
    ProjectIssue,
};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::{IpcError, IpcResult, codes};
use crate::state::{AppState, blocking};
use crate::toolchain;
use crate::watcher;

/// O projeto inteiro, como a interface o mostra.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectSnapshot {
    /// O nome do projeto.
    pub name: String,
    /// O `.spf`.
    pub spf: Utf8PathBuf,
    /// A pasta do projeto.
    pub root: Utf8PathBuf,
    /// Os módulos registrados (sintetizáveis).
    pub synthesizable: Vec<ProjectFile>,
    /// Os testbenches registrados.
    pub testbenches: Vec<ProjectFile>,
    /// O arquivo de topo.
    pub top_level: Option<Utf8PathBuf>,
    /// O módulo de topo, quando dá para saber.
    pub top_module: Option<String>,
    /// Por que o módulo de topo não dá para saber (arquivo com vários
    /// módulos, nenhum com o nome dele).
    pub top_module_error: Option<IpcError>,
    /// O testbench que a simulação do projeto roda.
    pub selected_testbench: Option<Utf8PathBuf>,
    /// O módulo desse testbench.
    pub testbench_module: Option<String>,
    /// Os `.v` e `.sv` da pasta que não estão registrados.
    pub unregistered: Vec<Utf8PathBuf>,
    /// Os processadores SAPHO.
    pub processors: Vec<ProcessorStatus>,
    /// A onda da simulação do projeto, se já existe no disco.
    pub waveform: Option<Utf8PathBuf>,
    /// Os Verilog conhecidos que podem ser o topo: os registrados, os de
    /// fora do `.spf` e o gerado de cada processador, menos os com nome de
    /// testbench (a regra de `Project::set_top_level` do Core).
    pub top_candidates: Vec<Utf8PathBuf>,
    /// O que está estranho no `.spf` sem impedir de abrir
    /// (`Project::issues`): vai para o painel de problemas.
    pub issues: Vec<ProjectIssue>,
}

/// Um processador, com o que a interface precisa saber dele.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessorStatus {
    /// Os campos do `Processor` do Core (nome, linguagem, caminhos,
    /// frequência, clocks, arrays).
    #[serde(flatten)]
    pub processor: Processor,
    /// O Verilog e o testbench gerados existem.
    pub built: bool,
    /// Os arquivos de entrada (`Simulation/input_<n>.txt`) que existem.
    pub inputs: Vec<PortFile>,
    /// Os arquivos de saída (`Simulation/output_<n>.txt`) que existem.
    pub outputs: Vec<PortFile>,
    /// Entradas que o testbench lê e que não existem (só depois do build).
    pub missing_inputs: Vec<Utf8PathBuf>,
    /// A onda da simulação do processador, se já existe.
    pub waveform: Option<Utf8PathBuf>,
    /// O que o build gerou e existe no disco.
    pub generated: GeneratedFiles,
}

/// Os arquivos que o build de um processador gera, separados pelo papel.
/// Só entram os que existem.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GeneratedFiles {
    /// `Hardware/<nome>.v`: o processador em Verilog, um módulo do design.
    pub verilog: Option<Utf8PathBuf>,
    /// `Simulation/<nome>_tb.v`: o testbench que o `asmcomp` gera.
    pub testbench: Option<Utf8PathBuf>,
    /// `Software/<nome>.asm`: o assembly que o `cmmcomp` gera.
    pub assembly: Option<Utf8PathBuf>,
    /// `Hardware/*.mif`: as memórias de dados e de instruções.
    pub memories: Vec<Utf8PathBuf>,
    /// Os `.txt` e `.log` da pasta temporária: logs e traduções do YANC.
    pub intermediates: Vec<Utf8PathBuf>,
}

impl GeneratedFiles {
    fn of(processor: &Processor) -> Self {
        let existing = |path: Utf8PathBuf| path.is_file().then_some(path);
        let name = &processor.name;
        Self {
            verilog: existing(processor.hardware_dir().join(format!("{name}.v"))),
            testbench: existing(processor.testbench_path()),
            assembly: existing(processor.software_dir().join(format!("{name}.asm"))),
            memories: files_with(&processor.hardware_dir(), &["mif"]),
            intermediates: files_with(&processor.temp_dir, &["txt", "log"]),
        }
    }
}

/// Os arquivos de uma pasta com uma das extensões, em ordem.
fn files_with(dir: &Utf8Path, extensions: &[&str]) -> Vec<Utf8PathBuf> {
    let Ok(entries) = dir.read_dir_utf8() else {
        return Vec::new();
    };
    let mut files: Vec<Utf8PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.into_path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| extensions.contains(&e)))
        .collect();
    files.sort();
    files
}

/// Um arquivo de porta de entrada ou de saída.
#[derive(Debug, Clone, Serialize)]
pub struct PortFile {
    /// A porta (`n` de `in(n)` e `out(n)`).
    pub port: u32,
    /// O arquivo.
    pub path: Utf8PathBuf,
}

/// Monta o retrato do projeto.
pub fn snapshot(project: &Project) -> ProjectSnapshot {
    let (top_module, top_module_error) = match project.top_module() {
        Ok(module) => (module, None),
        Err(error) => (None, Some(IpcError::from(error))),
    };
    let processors: Vec<ProcessorStatus> = project
        .processors()
        .iter()
        .map(|p| {
            let built = p.is_built();
            ProcessorStatus {
                processor: p.clone(),
                built,
                inputs: port_files(&p.simulation_dir(), "input_"),
                outputs: port_files(&p.simulation_dir(), "output_"),
                missing_inputs: if built {
                    lace_core::missing_inputs(p).unwrap_or_default()
                } else {
                    Vec::new()
                },
                waveform: lace_core::waveform_path(project, Some(p))
                    .ok()
                    .filter(|w| w.is_file()),
                generated: GeneratedFiles::of(p),
            }
        })
        .collect();
    ProjectSnapshot {
        name: project.name().to_owned(),
        spf: project.spf_path().to_owned(),
        root: project.root().to_owned(),
        synthesizable: project.files(FileRole::Synthesizable),
        testbenches: project.files(FileRole::Testbench),
        top_level: project.top_level(),
        top_module,
        top_module_error,
        selected_testbench: project.testbench(),
        testbench_module: project.testbench_module().ok().flatten(),
        top_candidates: top_candidates(project, &processors),
        unregistered: project.unregistered_verilog(),
        processors,
        waveform: lace_core::waveform_path(project, None)
            .ok()
            .filter(|w| w.is_file()),
        issues: project.issues(),
    }
}

/// Os candidatos a topo, na ordem: registrados (módulos e testbenches), o
/// Verilog gerado dos processadores e os de fora do `.spf`. Um testbench
/// cocotb (`.py`) nunca é topo.
fn top_candidates(project: &Project, processors: &[ProcessorStatus]) -> Vec<Utf8PathBuf> {
    let mut candidates: Vec<Utf8PathBuf> = Vec::new();
    let registered = [FileRole::Synthesizable, FileRole::Testbench]
        .into_iter()
        .flat_map(|role| project.files(role))
        .map(|f| f.path);
    let generated = processors
        .iter()
        .filter_map(|p| p.generated.verilog.clone());
    for path in registered
        .chain(generated)
        .chain(project.unregistered_verilog())
    {
        let name = path.file_name().unwrap_or_default();
        if !lace_core::verilog::is_testbench_name(name)
            && !lace_core::cocotb::is_testbench(&path)
            && !candidates.contains(&path)
        {
            candidates.push(path);
        }
    }
    candidates
}

/// Os `<prefixo><n>.txt` de uma pasta, por porta.
fn port_files(dir: &Utf8Path, prefix: &str) -> Vec<PortFile> {
    let Ok(entries) = dir.read_dir_utf8() else {
        return Vec::new();
    };
    let mut files: Vec<PortFile> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let port = entry
                .file_name()
                .strip_prefix(prefix)?
                .strip_suffix(".txt")?
                .parse()
                .ok()?;
            Some(PortFile {
                port,
                path: entry.path().to_owned(),
            })
        })
        .collect();
    files.sort_by_key(|f| f.port);
    files
}

/// Torna `project` o projeto aberto: guarda o `.spf`, põe nos recentes e
/// vigia a pasta.
fn activate(app: &AppHandle, state: &AppState, project: &Project) -> IpcResult<ProjectSnapshot> {
    *state.project.lock().expect("project lock") = Some(project.spf_path().to_owned());
    if let Err(error) = state
        .settings
        .touch_recent(project.spf_path(), project.name())
    {
        tracing::warn!("Could not update the recent projects: {error}");
    }
    watcher::watch(app, state, project.root());
    Ok(snapshot(project))
}

/// Abre um projeto: o `.spf` ou a pasta dele.
#[tauri::command]
pub async fn project_open(app: AppHandle, path: String) -> IpcResult<ProjectSnapshot> {
    blocking(app, move |app, state| {
        let project = Project::open(&path)?;
        activate(app, state, &project)
    })
    .await
}

/// Cria `<pai>/<nome>/<nome>.spf` e o abre. O nome segue a regra do Core
/// (`validate_project_name`): letras sem acento, dígitos, `_` e `-`,
/// começando por letra.
#[tauri::command]
pub async fn project_create(
    app: AppHandle,
    parent: String,
    name: String,
) -> IpcResult<ProjectSnapshot> {
    blocking(app, move |app, state| {
        let project = Project::create(&parent, &name)?;
        activate(app, state, &project)
    })
    .await
}

/// Confere um nome de processador novo enquanto o usuário digita, com a
/// regra do Core (`validate_processor_name`): `None` se ele serve, ou o erro
/// `invalid_name` com o motivo.
#[tauri::command]
pub fn processor_check_name(name: String) -> Option<IpcError> {
    lace_core::validate_processor_name(&name)
        .err()
        .map(IpcError::from)
}

/// Confere um nome de projeto novo enquanto o usuário digita: `None` se ele
/// serve, ou o erro `invalid_name` do Core com o motivo.
#[tauri::command]
pub fn project_check_name(name: String) -> Option<IpcError> {
    lace_core::validate_project_name(&name)
        .err()
        .map(IpcError::from)
}

/// Fecha o projeto: para de vigiar a pasta e esquece o `.spf`.
#[tauri::command]
pub async fn project_close(app: AppHandle) -> IpcResult<()> {
    blocking(app, |_, state| {
        *state.project.lock().expect("project lock") = None;
        watcher::stop(state);
        Ok(())
    })
    .await
}

/// O retrato do projeto aberto, relido do disco.
#[tauri::command]
pub async fn project_snapshot(app: AppHandle) -> IpcResult<ProjectSnapshot> {
    blocking(app, |_, state| Ok(snapshot(&state.project()?))).await
}

/// Registra um arquivo Verilog; cria a partir do modelo se não existir
/// (`lace add`).
#[tauri::command]
pub async fn project_add_verilog(
    app: AppHandle,
    path: String,
    testbench: bool,
) -> IpcResult<AddedFile> {
    blocking(app, move |_, state| {
        let mut project = state.project()?;
        // Sem bundle o arquivo é criado do mesmo jeito: o Core lê as portas
        // com o leitor embutido em vez do Yosys, como na CLI.
        let toolchain = toolchain::require(&state.settings.get()).ok();
        Ok(project.add_verilog(toolchain.as_ref(), Utf8PathBuf::from(path), testbench)?)
    })
    .await
}

/// Tira um arquivo do projeto, sem apagar do disco (`lace remove`).
#[tauri::command]
pub async fn project_remove_verilog(app: AppHandle, path: String) -> IpcResult<bool> {
    blocking(app, move |_, state| {
        Ok(state.project()?.remove_verilog(Utf8PathBuf::from(path))?)
    })
    .await
}

/// Muda a posição de um arquivo registrado na lista dele (`lace order`):
/// a ordem é a dos compiladores, e um `` `define `` só vale para os que vêm
/// depois. Devolve a lista na ordem nova.
#[tauri::command]
pub async fn project_reorder(
    app: AppHandle,
    path: String,
    position: ListPosition,
) -> IpcResult<Vec<ProjectFile>> {
    blocking(app, move |_, state| {
        Ok(state
            .project()?
            .reorder_file(Utf8PathBuf::from(path), &position)?)
    })
    .await
}

/// Escolhe o topo por arquivo ou por nome de módulo (`lace top`).
#[tauri::command]
pub async fn project_set_top(app: AppHandle, target: String) -> IpcResult<Utf8PathBuf> {
    blocking(app, move |_, state| {
        Ok(state.project()?.set_top(&target)?)
    })
    .await
}

/// Escolhe o testbench da simulação do projeto (`lace sim <testbench>`).
#[tauri::command]
pub async fn project_set_testbench(app: AppHandle, path: String) -> IpcResult<()> {
    blocking(app, move |_, state| {
        Ok(state.project()?.set_testbench(Utf8PathBuf::from(path))?)
    })
    .await
}

/// O pedido de um processador novo (`lace proc add`). Campo ausente fica
/// com o padrão da AURORA (`NewProcessor::new`).
#[derive(Debug, Clone, Deserialize)]
pub struct NewProcessorRequest {
    /// Nome: `[A-Za-z_][A-Za-z0-9_]*`.
    pub name: String,
    /// `cmm` (C±) ou `cpp` (C).
    pub language: Language,
    /// `#NUIOIN`.
    pub input_ports: Option<u32>,
    /// `#NUIOOU`.
    pub output_ports: Option<u32>,
    /// `#NUBITS`.
    pub nubits: Option<u32>,
    /// `#NBMANT`.
    pub nbmant: Option<u32>,
    /// `#NBEXPO`.
    pub nbexpo: Option<u32>,
    /// `#NUGAIN`.
    pub nugain: Option<u32>,
    /// `#NDSTAC`.
    pub ndstac: Option<u32>,
    /// `#SDEPTH`.
    pub sdepth: Option<u32>,
}

/// Os padrões de um processador novo, para o formulário.
#[tauri::command]
pub fn project_processor_defaults() -> NewProcessor {
    NewProcessor::new("", Language::Cmm)
}

/// Cria um processador: pastas, fonte-modelo e entrada no `.spf`.
#[tauri::command]
pub async fn project_add_processor(
    app: AppHandle,
    request: NewProcessorRequest,
) -> IpcResult<Processor> {
    blocking(app, move |_, state| {
        let mut project = state.project()?;
        let mut spec = NewProcessor::new(&request.name, request.language);
        for (value, field) in [
            (request.input_ports, &mut spec.input_ports),
            (request.output_ports, &mut spec.output_ports),
            (request.nubits, &mut spec.nubits),
            (request.nbmant, &mut spec.nbmant),
            (request.nbexpo, &mut spec.nbexpo),
            (request.nugain, &mut spec.nugain),
            (request.ndstac, &mut spec.ndstac),
            (request.sdepth, &mut spec.sdepth),
        ] {
            if let Some(value) = value {
                *field = value;
            }
        }
        Ok(project.add_processor(&spec)?.clone())
    })
    .await
}

/// Grava frequência, clocks e exportação de arrays de um processador
/// (`lace proc set`). Campo `None` fica como está.
#[tauri::command]
pub async fn project_configure_processor(
    app: AppHandle,
    name: String,
    frequency_mhz: Option<u32>,
    clocks: Option<u32>,
    show_arrays: Option<bool>,
) -> IpcResult<Processor> {
    blocking(app, move |_, state| {
        let mut project = state.project()?;
        let mut config = ProcessorConfig::default();
        config.frequency_mhz = frequency_mhz;
        config.clocks = clocks;
        config.show_arrays = show_arrays;
        Ok(project.configure_processor(&name, &config)?.clone())
    })
    .await
}

/// Os valores que a simulação escreveu numa porta de saída.
#[tauri::command]
pub async fn project_output_values(
    app: AppHandle,
    processor: String,
    port: u32,
) -> IpcResult<Vec<i64>> {
    blocking(app, move |_, state| {
        let project = state.project()?;
        Ok(project
            .require_processor(&processor)?
            .read_output_values(port)?)
    })
    .await
}

/// Os módulos declarados num texto Verilog, sem os comentados. A interface
/// usa para o contorno do arquivo.
#[tauri::command]
pub fn verilog_modules(text: String) -> Vec<String> {
    lace_core::verilog::modules_in(&text)
}

/// Move ou renomeia um arquivo ou pasta do projeto: o arrastar e o
/// renomear da árvore (`Project::move_path`). `to` é o caminho final. O Core
/// mantém o `.spf` em dia e recusa o que fica no lugar (`cannot_move`), o
/// que está fora do projeto (`outside_project`) e o destino que existe
/// (`path_exists`). Com `overwrite`, o que está no destino vai para a
/// lixeira e o movimento é tentado de novo: substituir é do Studio, o Core
/// nunca sobrescreve.
#[tauri::command]
pub async fn project_move(
    app: AppHandle,
    from: String,
    to: String,
    overwrite: bool,
) -> IpcResult<MovedPath> {
    blocking(app, move |_, state| {
        let mut project = state.project()?;
        match project.move_path(&from, &to) {
            Err(LaceError::PathExists(target)) if overwrite => {
                trash::delete(&target)
                    .map_err(|e| IpcError::new(codes::IO, format!("{target}: {e}")))?;
                Ok(project.move_path(&from, &to)?)
            }
            result => Ok(result?),
        }
    })
    .await
}

/// A hierarquia do design e dos testbenches, elaborada pelo Icarus
/// (`lace_core::hierarchy`). Não compila os processadores nem grava
/// relatório.
#[tauri::command]
pub async fn project_hierarchy(app: AppHandle) -> IpcResult<HierarchyResult> {
    blocking(app, |_, state| {
        let project = state.project()?;
        let toolchain = toolchain::require(&state.settings.get())?;
        Ok(lace_core::hierarchy(
            &toolchain,
            &project,
            &HierarchyOptions::default(),
            &Control::default(),
        )?)
    })
    .await
}
