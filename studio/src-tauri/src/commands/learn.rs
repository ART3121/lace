//! Os exercícios do `lace learn` no Studio: abrir ou criar a pasta de
//! exercícios, escolher o exercício atual, restaurar o arquivo e ler a
//! solução liberada. Tudo pelo crate `lace-learn`, como a CLI; a correção é
//! uma operação (`flows.rs`, fluxo `learn`), com saída e cancelamento.

use camino::{Utf8Path, Utf8PathBuf};
use lace_learn::{Workspace, load_track, tracks_dir};
use serde::Serialize;
use tauri::AppHandle;

use crate::error::IpcResult;
use crate::settings::Settings;
use crate::state::blocking;
use crate::toolchain;

/// A pasta de exercícios aberta, como a vista Exercícios mostra.
#[derive(Debug, Clone, Serialize)]
pub struct LearnSnapshot {
    /// A pasta de exercícios.
    pub root: Utf8PathBuf,
    /// A trilha (`verilog`).
    pub track: String,
    /// O título da trilha.
    pub title: String,
    /// As boas-vindas, em markdown.
    pub welcome: String,
    /// A mensagem do fim, em markdown.
    pub farewell: Option<String>,
    /// Os capítulos, na ordem.
    pub chapters: Vec<LearnChapter>,
    /// O exercício atual.
    pub current: String,
    /// Quantos resolvidos.
    pub solved: usize,
    /// Quantos exercícios.
    pub total: usize,
}

/// Um capítulo.
#[derive(Debug, Clone, Serialize)]
pub struct LearnChapter {
    /// A pasta do capítulo (`01_primeiros_passos`).
    pub id: String,
    /// O título.
    pub title: String,
    /// A introdução, em markdown.
    pub intro: String,
    /// Os exercícios, na ordem.
    pub exercises: Vec<LearnExercise>,
}

/// Um exercício, com o que o aluno já fez.
#[derive(Debug, Clone, Serialize)]
pub struct LearnExercise {
    /// O nome.
    pub name: String,
    /// O título.
    pub title: String,
    /// O enunciado, em markdown.
    pub prompt: String,
    /// As dicas, em markdown.
    pub hints: Vec<String>,
    /// O módulo que o aluno escreve.
    pub module: String,
    /// O arquivo que o aluno edita.
    pub file: Utf8PathBuf,
    /// O `.spf` do projeto do exercício.
    pub spf: Utf8PathBuf,
    /// A onda da última correção.
    pub waveform: Utf8PathBuf,
    /// Resolvido.
    pub solved: bool,
    /// A solução liberada, depois de resolvido.
    pub solution: Option<Utf8PathBuf>,
}

/// O Studio usa o português dos enunciados; a tradução, quando houver, vem
/// pela interface.
fn lang(lang: Option<String>) -> Option<String> {
    lang.filter(|l| !l.is_empty())
}

/// A pasta das trilhas: a das preferências (`learn_dir`), ou a de
/// `LACE_LEARN_DIR`, ou a do componente `lace-learn`.
pub fn tracks(settings: &Settings) -> IpcResult<Utf8PathBuf> {
    if let Some(dir) = settings.learn_dir.as_deref().filter(|d| !d.is_empty()) {
        return Ok(Utf8PathBuf::from(dir));
    }
    let toolchain = toolchain::require(settings).ok();
    Ok(tracks_dir(toolchain.as_ref())?)
}

fn snapshot(workspace: &Workspace) -> LearnSnapshot {
    let track = workspace.track();
    let chapters = track
        .chapters
        .iter()
        .map(|chapter| LearnChapter {
            id: chapter.id.clone(),
            title: chapter.title.clone(),
            intro: chapter.intro.clone(),
            exercises: chapter
                .exercises
                .iter()
                .map(|e| {
                    let solved = workspace.is_solved(&e.name);
                    let dir = workspace.exercise_dir(e);
                    LearnExercise {
                        name: e.name.clone(),
                        title: e.title.clone(),
                        prompt: e.prompt.clone(),
                        hints: e.hints.clone(),
                        module: e.module.clone(),
                        file: workspace.student_file(e),
                        spf: dir.join(format!("{}.spf", e.name)),
                        waveform: workspace.waveform(e),
                        solved,
                        solution: solved
                            .then(|| workspace.solution_path(e))
                            .filter(|p| p.is_file()),
                    }
                })
                .collect(),
        })
        .collect();
    LearnSnapshot {
        root: workspace.root().to_owned(),
        track: track.id.clone(),
        title: track.title.clone(),
        welcome: track.welcome.clone(),
        farewell: track.farewell.clone(),
        chapters,
        current: workspace.current().name.clone(),
        solved: workspace.solved_count(),
        total: track.len(),
    }
}

fn open(settings: &Settings, root: &Utf8Path, lang: Option<&str>) -> IpcResult<Workspace> {
    let root = Workspace::discover(root)?;
    Ok(Workspace::open(&root, &tracks(settings)?, lang)?)
}

/// Abre a pasta de exercícios que contém `root` e a põe em dia com a trilha
/// (`lace learn` ao começar).
#[tauri::command]
pub async fn learn_open(
    app: AppHandle,
    root: String,
    lang: Option<String>,
) -> IpcResult<LearnSnapshot> {
    blocking(app, move |_, state| {
        let lang = self::lang(lang);
        let workspace = open(&state.settings.get(), Utf8Path::new(&root), lang.as_deref())?;
        Ok(snapshot(&workspace))
    })
    .await
}

/// Cria a pasta de exercícios em `dir` (`lace learn init`).
#[tauri::command]
pub async fn learn_init(
    app: AppHandle,
    dir: String,
    track: Option<String>,
    lang: Option<String>,
) -> IpcResult<LearnSnapshot> {
    blocking(app, move |_, state| {
        let lang = self::lang(lang);
        let tracks = tracks(&state.settings.get())?;
        let id = track.unwrap_or_else(|| lace_learn::DEFAULT_TRACK.to_owned());
        let track = load_track(&tracks, &id, lang.as_deref())?;
        let workspace = Workspace::init(Utf8Path::new(&dir), track)?;
        Ok(snapshot(&workspace))
    })
    .await
}

/// Torna atual o exercício `name`.
#[tauri::command]
pub async fn learn_set_current(
    app: AppHandle,
    root: String,
    name: String,
    lang: Option<String>,
) -> IpcResult<LearnSnapshot> {
    blocking(app, move |_, state| {
        let lang = self::lang(lang);
        let mut workspace = open(&state.settings.get(), Utf8Path::new(&root), lang.as_deref())?;
        workspace.set_current(&name)?;
        Ok(snapshot(&workspace))
    })
    .await
}

/// Volta o arquivo do exercício ao começo (`lace learn reset`). A interface
/// confirma antes.
#[tauri::command]
pub async fn learn_reset(app: AppHandle, root: String, name: String) -> IpcResult<()> {
    blocking(app, move |_, state| {
        let workspace = open(&state.settings.get(), Utf8Path::new(&root), None)?;
        let exercise = workspace.track().require(&name)?.clone();
        Ok(workspace.reset(&exercise)?)
    })
    .await
}

/// De onde vêm as trilhas, para a vista Exercícios saber antes de criar ou
/// abrir uma pasta.
#[derive(Debug, Clone, Serialize)]
pub struct LearnTracks {
    /// A pasta das trilhas ([`tracks`]).
    pub dir: Utf8PathBuf,
    /// As trilhas dela (`verilog`).
    pub tracks: Vec<String>,
}

/// As trilhas que o Studio acha. Sem preferência, sem `LACE_LEARN_DIR` e
/// sem o componente, o erro é `learn_component_missing`.
#[tauri::command]
pub async fn learn_tracks(app: AppHandle) -> IpcResult<LearnTracks> {
    blocking(app, |_, state| {
        let dir = tracks(&state.settings.get())?;
        let tracks = lace_learn::available_tracks(&dir)?;
        Ok(LearnTracks { dir, tracks })
    })
    .await
}
