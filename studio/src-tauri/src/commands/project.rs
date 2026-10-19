//! Comandos do projeto: abrir, criar, fechar, o retrato do projeto e o que
//! muda o `.spf` (arquivos Verilog, topo, testbench, processadores).
//!
//! Cada comando chama uma função do Core e nada mais. O
//! retrato ([`ProjectSnapshot`]) é o `lace status --json` com alguns campos a
//! mais que a interface usa para habilitar botões.

use camino::Utf8PathBuf;
use lace_core::{
    AddedFile, Control, HierarchyOptions, HierarchyResult, LaceError, Language, LayoutConversion,
    ListPosition, MovedPath, NewProcessor, Processor, ProcessorConfig, Project, ProjectFile,
    RemovedProcessor,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::{IpcError, IpcResult, codes};
use crate::state::{AppState, blocking};
use crate::toolchain;
use crate::watcher;

/// O projeto inteiro, como a interface o mostra: o retrato do core
/// (`Project::status`), que é o `lace status --json` com os campos a mais
/// que a interface usa para habilitar botões.
pub type ProjectSnapshot = lace_core::ProjectStatus;

/// Monta o retrato do projeto.
pub fn snapshot(project: &Project) -> ProjectSnapshot {
    project.status()
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

/// A pasta padrão para criar e abrir projetos: `Projetos` na pasta do
/// usuário (`~/Projetos` no Linux e no macOS, `C:\Users\<nome>\Projetos` no
/// Windows). É criada aqui, porque o `Project::create` pede a pasta de cima
/// existente e o `fs_create_dir` só cria dentro do projeto aberto.
#[tauri::command]
pub fn project_default_parent(app: AppHandle) -> String {
    let Ok(home) = app.path().home_dir() else {
        return "Projetos".to_owned();
    };
    let dir = home.join("Projetos");
    if let Err(error) = std::fs::create_dir_all(&dir) {
        tracing::warn!("Could not create {}: {error}", dir.display());
    }
    dir.to_string_lossy().into_owned()
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
/// regra do Core (`validate_new_processor_name`): `None` se ele serve, ou o
/// erro `invalid_name` com o motivo.
#[tauri::command]
pub fn processor_check_name(name: String) -> Option<IpcError> {
    lace_core::validate_new_processor_name(&name)
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
    /// `#NDSTAC`; ausente, o YANC calcula pelo programa.
    pub ndstac: Option<u32>,
    /// `#SDEPTH`; ausente, o YANC calcula pelo programa.
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
        // Sem as duas, o YANC calcula as pilhas pelo programa.
        spec.ndstac = request.ndstac;
        spec.sdepth = request.sdepth;
        for (value, field) in [
            (request.input_ports, &mut spec.input_ports),
            (request.output_ports, &mut spec.output_ports),
            (request.nubits, &mut spec.nubits),
            (request.nbmant, &mut spec.nbmant),
            (request.nbexpo, &mut spec.nbexpo),
            (request.nugain, &mut spec.nugain),
        ] {
            if let Some(value) = value {
                *field = value;
            }
        }
        Ok(project.add_processor(&spec)?.clone())
    })
    .await
}

/// O que `project_remove_processor` fez, como o `lace proc remove --json`.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessorRemoval {
    /// O que saiu: o nome, a pasta e os arquivos que saíram das listas.
    pub removed: RemovedProcessor,
    /// A pasta foi para a lixeira do sistema; `false`, ela continua no disco
    /// (ou não existia).
    pub trashed: bool,
}

/// Tira um processador do projeto (`lace proc remove`): a entrada no `.spf`,
/// os arquivos dele nas listas e o temporário do Lace. Com `to_trash`, a
/// pasta dele vai para a lixeira do sistema, como o apagar do Explorador;
/// sem, fica no disco.
#[tauri::command]
pub async fn project_remove_processor(
    app: AppHandle,
    name: String,
    to_trash: bool,
) -> IpcResult<ProcessorRemoval> {
    blocking(app, move |_, state| {
        let removed = state.project()?.remove_processor(&name)?;
        let trashed = to_trash && removed.dir.exists();
        if trashed {
            trash::delete(&removed.dir).map_err(|e| {
                IpcError::new(
                    codes::IO,
                    format!(
                        "{name} is out of the project, but its folder did not go to the trash ({e}): {}",
                        removed.dir
                    ),
                )
            })?;
        }
        Ok(ProcessorRemoval { removed, trashed })
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
/// nunca sobrescreve. Menos o que o Core mantém no lugar, a pasta do projeto
/// e uma pasta que contém a origem (`Project::replace_refusal`): aí o erro é
/// `cannot_replace`, e nada vai para a lixeira.
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
                if let Some(reason) = project.replace_refusal(&from, &target) {
                    return Err(IpcError::new(
                        codes::CANNOT_REPLACE,
                        format!("{target} is not replaced: {reason}"),
                    ));
                }
                trash::delete(&target)
                    .map_err(|e| IpcError::new(codes::IO, format!("{target}: {e}")))?;
                Ok(project.move_path(&from, &to)?)
            }
            result => Ok(result?),
        }
    })
    .await
}

/// Converte os processadores da estrutura de pastas do YANC 6.x
/// (`Software/`, `Hardware/`, `Simulation/`) para a do 7.1
/// (`Project::convert_layout`, o `lace convert`). Nada é apagado nem
/// sobrescrito; o relatório diz o que mudou de lugar e o que ficou.
#[tauri::command]
pub async fn project_convert_layout(app: AppHandle) -> IpcResult<LayoutConversion> {
    blocking(app, |_, state| Ok(state.project()?.convert_layout()?)).await
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
