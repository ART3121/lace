//! Comandos do projeto que são do Studio: abrir, criar e fechar (o projeto
//! aberto, os recentes e a vigia da pasta), a pasta padrão, os padrões de um
//! processador novo, os módulos de um texto e a hierarquia. O que muda o
//! `.spf` (arquivos Verilog, topo, testbench, processadores, mover) são
//! métodos do protocolo, que a interface chama pelo servidor do Lace
//! ([`crate::server`]).
//!
//! O retrato ([`ProjectSnapshot`]) é o `lace status --json` com alguns
//! campos a mais que a interface usa para habilitar botões.

use camino::Utf8PathBuf;
use lace_core::{Control, HierarchyOptions, HierarchyResult, Language, NewProcessor};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::error::{IpcError, IpcResult, codes};
use crate::state::{AppState, blocking};
use crate::toolchain;
use crate::watcher;

/// O projeto inteiro, como a interface o mostra: o retrato do protocolo
/// (`project.status`), que é o `lace status --json` com os campos a mais
/// que a interface usa para habilitar botões.
pub type ProjectSnapshot = Value;

/// Torna o projeto do retrato `status` o projeto aberto: guarda o `.spf`,
/// põe nos recentes e vigia a pasta.
fn activate(app: &AppHandle, state: &AppState, status: Value) -> IpcResult<ProjectSnapshot> {
    let field = |name: &str| {
        status[name].as_str().map(Utf8PathBuf::from).ok_or_else(|| {
            IpcError::new(
                codes::INTERNAL,
                format!("The project status from the lace server has no {name}"),
            )
        })
    };
    let (spf, root) = (field("spf")?, field("root")?);
    let name = status["name"].as_str().unwrap_or_default();
    if let Err(error) = state.settings.touch_recent(&spf, name) {
        tracing::warn!("Could not update the recent projects: {error}");
    }
    *state.project.lock().expect("project lock") = Some(spf);
    watcher::watch(app, state, &root);
    Ok(status)
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

/// Abre um projeto: o `.spf` ou a pasta dele (o `project.status` do
/// servidor).
#[tauri::command]
pub async fn project_open(app: AppHandle, path: String) -> IpcResult<ProjectSnapshot> {
    blocking(app, move |app, state| {
        let status = state.server.call(
            &state.settings.get(),
            "project.status",
            json!({ "project": path }),
        )?;
        activate(app, state, status)
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
        let status = state.server.call(
            &state.settings.get(),
            "project.create",
            json!({ "parent": parent, "name": name }),
        )?;
        activate(app, state, status)
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
