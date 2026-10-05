//! Comandos do histórico de relatórios (`lace report`). Leem e apagam
//! `.lace/reports/`; nenhuma ferramenta roda.

use camino::Utf8PathBuf;
use lace_core::history::{self, Cleanup, RunComparison, RunRecord, RunSummary};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::IpcResult;
use crate::state::blocking;

/// Os relatórios, do mais novo para o mais antigo (`lace report list`).
#[tauri::command]
pub async fn history_list(app: AppHandle) -> IpcResult<Vec<RunSummary>> {
    blocking(app, |_, state| Ok(history::list(&state.project()?)?)).await
}

/// Um relatório aberto.
#[derive(Debug, Clone, Serialize)]
pub struct ReportView {
    /// O identificador (`run-000042`).
    pub id: String,
    /// O `report.txt`.
    pub path: Utf8PathBuf,
    /// O que a comparação usa.
    pub record: RunRecord,
    /// O texto do relatório.
    pub text: String,
}

/// Um relatório (`lace report show ID`); sem `id`, o mais novo.
#[tauri::command]
pub async fn history_show(app: AppHandle, id: Option<String>) -> IpcResult<ReportView> {
    blocking(app, move |_, state| {
        let project = state.project()?;
        let record = match id {
            Some(id) => history::load(&project, &id)?,
            None => history::latest(&project)?,
        };
        Ok(ReportView {
            text: history::report_text(&project, &record.id)?,
            path: history::report_path(&project, &record.id)?,
            id: record.id.clone(),
            record,
        })
    })
    .await
}

/// Compara dois relatórios (`lace report compare [ID] [--against ID]`).
#[tauri::command]
pub async fn history_compare(
    app: AppHandle,
    id: Option<String>,
    against: Option<String>,
) -> IpcResult<RunComparison> {
    blocking(app, move |_, state| {
        let project = state.project()?;
        Ok(history::compare_reports(
            &project,
            id.as_deref(),
            against.as_deref(),
        )?)
    })
    .await
}

/// Quais relatórios apagar (`lace report clean`): todos, todos menos os
/// mais novos (`--keep N`) ou os escolhidos.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CleanupRequest {
    All,
    KeepLatest { keep: usize },
    Reports { ids: Vec<String> },
}

impl From<CleanupRequest> for Cleanup {
    fn from(request: CleanupRequest) -> Self {
        match request {
            CleanupRequest::All => Cleanup::All,
            CleanupRequest::KeepLatest { keep } => Cleanup::KeepLatest(keep),
            CleanupRequest::Reports { ids } => Cleanup::Reports(ids),
        }
    }
}

/// Os relatórios que a limpeza escolhe, do mais antigo para o mais novo,
/// sem apagar nada (`history::plan_cleanup`). A interface mostra a lista,
/// confirma e passa a mesma lista para [`history_clean`]: um relatório
/// gravado entre as duas chamadas não sai sem ter sido mostrado.
#[tauri::command]
pub async fn history_plan_cleanup(
    app: AppHandle,
    cleanup: CleanupRequest,
) -> IpcResult<Vec<String>> {
    blocking(app, move |_, state| {
        Ok(history::plan_cleanup(&state.project()?, &cleanup.into())?)
    })
    .await
}

/// O que [`history_clean`] fez: o `report-clean.json` da CLI.
#[derive(Debug, Clone, Serialize)]
pub struct CleanReport {
    /// Os apagados, do mais antigo para o mais novo.
    pub removed: Vec<String>,
    /// Quantos ficaram.
    pub kept: usize,
}

/// Apaga os relatórios `ids`, os de [`history_plan_cleanup`]
/// (`history::remove`). O número de um relatório apagado não volta.
#[tauri::command]
pub async fn history_clean(app: AppHandle, ids: Vec<String>) -> IpcResult<CleanReport> {
    blocking(app, move |_, state| {
        let project = state.project()?;
        let removed = history::remove(&project, &ids)?;
        let kept = history::list(&project)?.len();
        Ok(CleanReport { removed, kept })
    })
    .await
}
