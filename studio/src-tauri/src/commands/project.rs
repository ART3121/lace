//! Comandos do projeto que são do Studio: abrir, criar e fechar (o projeto
//! aberto, os recentes e a vigia da pasta), a pasta padrão, os padrões de um
//! processador novo, os módulos de um texto e a hierarquia. O que muda o
//! `.spf` (arquivos Verilog, topo, testbench, processadores, mover) são
//! métodos do protocolo, que a interface chama pelo servidor do Lace
//! ([`crate::server`]).
//!
//! O retrato ([`ProjectSnapshot`]) é o `lace status --json` com alguns
//! campos a mais que a interface usa para habilitar botões.

use lace_core::{Control, HierarchyOptions, HierarchyResult, Language, NewProcessor, Project};
use tauri::{AppHandle, Manager};

use crate::error::IpcResult;
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

/// Os padrões de um processador novo, para o formulário.
#[tauri::command]
pub fn project_processor_defaults() -> NewProcessor {
    NewProcessor::new("", Language::Cmm)
}

/// Os módulos declarados num texto Verilog, sem os comentados. A interface
/// usa para o contorno do arquivo.
#[tauri::command]
pub fn verilog_modules(text: String) -> Vec<String> {
    lace_core::verilog::modules_in(&text)
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
