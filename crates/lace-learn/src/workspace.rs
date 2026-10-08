//! A pasta de exercícios do aluno: um projeto Lace por exercício, o estado
//! (o exercício atual e os resolvidos) e as soluções liberadas.
//!
//! ```text
//! lace-learn/                          a pasta que o `init` cria
//!   .lace-learn.json                   o estado
//!   .gitignore
//!   exercises/02_basico/mux2/          um projeto Lace por exercício
//!     mux2.spf
//!     mux2.v                           o que o aluno edita
//!     README.md                        o enunciado
//!     .lace-learn/tb_mux2.v            o testbench (gerado)
//!     .lace-learn/mux2_ref.v           a referência (gerada)
//!     .lace-learn/wave.fst             a onda da última correção
//!   solutions/02_basico/mux2.v         a solução, depois de resolvido
//! ```
//!
//! O arquivo do aluno só é escrito ao criar o exercício e no
//! [`reset`](Workspace::reset), que é pedido dele. O que está em
//! `.lace-learn/` e o `README.md` são da trilha: [`Workspace::open`] os
//! grava de novo quando a trilha muda, e cria os exercícios novos.

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{FileRole, Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{LearnError, Result};
use crate::reference;
use crate::testbench;
use crate::track::{self, Exercise, Track};

/// O estado, na raiz da pasta de exercícios.
pub const STATE_FILE: &str = ".lace-learn.json";
/// O formato do estado que este Lace grava.
pub const STATE_FORMAT: u32 = 1;
/// A pasta, em cada exercício, do que o `lace learn` gera.
pub const HIDDEN_DIR: &str = ".lace-learn";
/// A pasta dos exercícios.
pub const EXERCISES_DIR: &str = "exercises";
/// A pasta das soluções liberadas.
pub const SOLUTIONS_DIR: &str = "solutions";
/// O enunciado, em cada exercício.
pub const PROMPT_FILE: &str = "README.md";

/// O `.lace-learn.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
    format: u32,
    track: String,
    #[serde(default)]
    current: Option<String>,
    #[serde(default)]
    solved: Vec<String>,
}

/// Uma pasta de exercícios aberta, com a trilha dela.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: Utf8PathBuf,
    track: Track,
    state: State,
}

/// Um exercício na lista, com o que o aluno já fez.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct ExerciseStatus {
    /// O nome.
    pub name: String,
    /// O capítulo (a pasta dele).
    pub chapter: String,
    /// O título.
    pub title: String,
    /// Já resolvido.
    pub solved: bool,
    /// É o exercício atual.
    pub current: bool,
    /// O arquivo que o aluno edita.
    #[schemars(with = "String")]
    pub file: Utf8PathBuf,
}

impl Workspace {
    /// Cria a pasta de exercícios em `dir` com todos os exercícios da
    /// trilha, e o primeiro como atual. `dir` pode existir, vazia.
    ///
    /// # Erros
    ///
    /// - [`LearnError::WorkspaceExists`] se `dir` existe e tem alguma
    ///   coisa;
    /// - os de criar os projetos e de gerar os testbenches
    ///   ([`LearnError::InvalidTrack`] com um exercício que não serve).
    pub fn init(dir: &Utf8Path, track: Track) -> Result<Workspace> {
        if dir.exists() {
            let mut entries = dir
                .read_dir_utf8()
                .map_err(LearnError::io("Reading", dir))?;
            if entries.next().is_some() {
                return Err(LearnError::WorkspaceExists(dir.to_owned()));
            }
        }
        std::fs::create_dir_all(dir).map_err(LearnError::io("Creating", dir))?;
        let root = canonical(dir)?;
        let first = track.exercises().next().map(|e| e.name.clone());
        let mut workspace = Workspace {
            root,
            state: State {
                format: STATE_FORMAT,
                track: track.id.clone(),
                current: first,
                solved: Vec::new(),
            },
            track,
        };
        write_if_changed(&workspace.root.join(".gitignore"), GITIGNORE)?;
        workspace.save()?;
        workspace.sync()?;
        Ok(workspace)
    }

    /// Abre a pasta de exercícios `root` (a que tem o `.lace-learn.json`),
    /// carregando a trilha dela da pasta das trilhas, e a põe em dia com a
    /// trilha: cria os exercícios novos e grava de novo o que mudou em
    /// `.lace-learn/` e nos `README.md`.
    ///
    /// # Erros
    ///
    /// [`LearnError::NoWorkspace`] sem o `.lace-learn.json`;
    /// [`LearnError::InvalidState`] se ele não se entende; os de
    /// [`track::load_track`] e os de pôr em dia.
    pub fn open(root: &Utf8Path, tracks: &Utf8Path, lang: Option<&str>) -> Result<Workspace> {
        let root = canonical(root)?;
        let state = read_state(&root)?;
        let track = track::load_track(tracks, &state.track, lang)?;
        let mut workspace = Workspace { root, track, state };
        let valid = workspace
            .state
            .current
            .as_deref()
            .is_some_and(|c| workspace.track.find(c).is_some());
        if !valid {
            workspace.state.current = workspace.track.exercises().next().map(|e| e.name.clone());
        }
        workspace.sync()?;
        Ok(workspace)
    }

    /// A pasta de exercícios que contém `start`: a primeira, de `start` para
    /// cima, com o `.lace-learn.json`.
    ///
    /// # Erros
    ///
    /// [`LearnError::NoWorkspace`] se nenhuma tem.
    pub fn discover(start: &Utf8Path) -> Result<Utf8PathBuf> {
        let start = canonical(start)?;
        start
            .ancestors()
            .find(|dir| dir.join(STATE_FILE).is_file())
            .map(Utf8Path::to_owned)
            .ok_or(LearnError::NoWorkspace(start))
    }

    /// A trilha que esta pasta segue, já lida do estado. Para abrir sem
    /// ter a trilha (saber qual carregar).
    ///
    /// # Erros
    ///
    /// Os de ler o `.lace-learn.json`.
    pub fn track_of(root: &Utf8Path) -> Result<String> {
        Ok(read_state(root)?.track)
    }

    /// A pasta de exercícios.
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// A trilha.
    pub fn track(&self) -> &Track {
        &self.track
    }

    /// O exercício atual.
    pub fn current(&self) -> &Exercise {
        self.state
            .current
            .as_deref()
            .and_then(|c| self.track.find(c))
            .or_else(|| self.track.exercises().next())
            .expect("a loaded track has exercises")
    }

    /// Torna atual o exercício `name` e grava o estado.
    ///
    /// # Erros
    ///
    /// [`LearnError::UnknownExercise`]; os de gravar o estado.
    pub fn set_current(&mut self, name: &str) -> Result<()> {
        let name = self.track.require(name)?.name.clone();
        self.state.current = Some(name);
        self.save()
    }

    /// Se o exercício `name` já foi resolvido.
    pub fn is_solved(&self, name: &str) -> bool {
        self.state
            .solved
            .iter()
            .any(|s| s.eq_ignore_ascii_case(name))
    }

    /// Quantos exercícios da trilha estão resolvidos.
    pub fn solved_count(&self) -> usize {
        self.track
            .exercises()
            .filter(|e| self.is_solved(&e.name))
            .count()
    }

    /// Marca o exercício como resolvido ou não, e grava o estado. Resolvido,
    /// a solução dele vai para `solutions/` e fica lá.
    ///
    /// # Erros
    ///
    /// [`LearnError::UnknownExercise`]; os de gravar.
    pub fn set_solved(&mut self, name: &str, solved: bool) -> Result<()> {
        let exercise = self.track.require(name)?.clone();
        let known = self.is_solved(&exercise.name);
        if solved {
            self.write_solution(&exercise)?;
            if !known {
                self.state.solved.push(exercise.name.clone());
            }
        } else if known {
            self.state
                .solved
                .retain(|s| !s.eq_ignore_ascii_case(&exercise.name));
        }
        self.save()
    }

    /// Guarda o que uma correção disse: resolvido marca e libera a solução;
    /// uma correção que roda e não passa desmarca um exercício resolvido (o
    /// aluno mexeu depois); cancelada não muda nada. É o que o `lace learn`
    /// e o Studio fazem depois de cada [`grade`](fn@crate::grade).
    ///
    /// # Erros
    ///
    /// Os de [`Workspace::set_solved`].
    pub fn record(&mut self, grade: &crate::Grade) -> Result<()> {
        match grade.verdict {
            crate::Verdict::Solved => self.set_solved(&grade.exercise, true),
            crate::Verdict::Cancelled => Ok(()),
            _ if self.is_solved(&grade.exercise) => self.set_solved(&grade.exercise, false),
            _ => Ok(()),
        }
    }

    /// O próximo exercício por resolver depois do atual, voltando ao
    /// começo da trilha; `None` com todos resolvidos.
    pub fn next_pending(&self) -> Option<&Exercise> {
        let all: Vec<&Exercise> = self.track.exercises().collect();
        let at = self
            .track
            .position(&self.current().name)
            .unwrap_or_default();
        all[at + 1..]
            .iter()
            .chain(all[..=at].iter())
            .find(|e| !self.is_solved(&e.name))
            .copied()
    }

    /// A lista dos exercícios, na ordem, com o que o aluno já fez.
    pub fn statuses(&self) -> Vec<ExerciseStatus> {
        let current = self.current().name.clone();
        self.track
            .exercises()
            .map(|e| ExerciseStatus {
                name: e.name.clone(),
                chapter: e.chapter.clone(),
                title: e.title.clone(),
                solved: self.is_solved(&e.name),
                current: e.name == current,
                file: self.student_file(e),
            })
            .collect()
    }

    /// A pasta do exercício (o projeto Lace dele).
    pub fn exercise_dir(&self, exercise: &Exercise) -> Utf8PathBuf {
        self.root
            .join(EXERCISES_DIR)
            .join(&exercise.chapter)
            .join(&exercise.name)
    }

    /// O arquivo que o aluno edita: `<módulo>.v` na pasta do exercício.
    pub fn student_file(&self, exercise: &Exercise) -> Utf8PathBuf {
        self.exercise_dir(exercise)
            .join(format!("{}.v", exercise.module))
    }

    /// Os arquivos do exercício que, mudando, pedem nova correção: o do
    /// aluno e os módulos dados.
    pub fn watched_files(&self, exercise: &Exercise) -> Vec<Utf8PathBuf> {
        let dir = self.exercise_dir(exercise);
        std::iter::once(self.student_file(exercise))
            .chain(exercise.given.iter().map(|g| dir.join(g)))
            .collect()
    }

    /// A onda da última correção do exercício.
    pub fn waveform(&self, exercise: &Exercise) -> Utf8PathBuf {
        self.exercise_dir(exercise).join(testbench::WAVE_FILE)
    }

    /// O projeto Lace do exercício.
    ///
    /// # Erros
    ///
    /// Os de [`Project::open`].
    pub fn project(&self, exercise: &Exercise) -> Result<Project> {
        Ok(Project::open(self.exercise_dir(exercise))?)
    }

    /// Onde fica a solução liberada do exercício.
    pub fn solution_path(&self, exercise: &Exercise) -> Utf8PathBuf {
        self.root
            .join(SOLUTIONS_DIR)
            .join(&exercise.chapter)
            .join(format!("{}.v", exercise.module))
    }

    /// Volta o arquivo do aluno ao que a trilha dá (`start.v`). É o único
    /// jeito de o `lace learn` sobrescrever o que o aluno escreveu: quem
    /// chama confirma antes.
    ///
    /// # Erros
    ///
    /// Os de ler a trilha e gravar o arquivo.
    pub fn reset(&self, exercise: &Exercise) -> Result<()> {
        let start = read(&exercise.start_file())?;
        let file = self.student_file(exercise);
        std::fs::write(&file, start).map_err(LearnError::io("Writing", &file))
    }

    /// Cria os exercícios que faltam e grava de novo o que a trilha gera e
    /// mudou.
    fn sync(&mut self) -> Result<()> {
        let exercises: Vec<Exercise> = self.track.exercises().cloned().collect();
        for exercise in &exercises {
            self.prepare(exercise)?;
        }
        Ok(())
    }

    /// Um exercício em dia com a trilha.
    fn prepare(&self, exercise: &Exercise) -> Result<()> {
        let dir = self.exercise_dir(exercise);
        let hidden = dir.join(HIDDEN_DIR);
        let module = &exercise.module;
        let testbench_text = if exercise.custom_testbench {
            read(&exercise.source.join(track::TESTBENCH_FILE))?
        } else {
            testbench::generate(exercise, &exercise.interface()?)?
        };
        let reference_text = reference::reference_text(&read(&exercise.solution_file())?);
        let testbench_file = hidden.join(format!("{}.v", testbench::testbench_module(module)));
        let reference_file = hidden.join(format!("{}.v", testbench::reference_module(module)));

        let new = !dir.join(format!("{}.spf", exercise.name)).is_file();
        let mut project = if new {
            let chapter = dir.parent().expect("an exercise folder has a parent");
            std::fs::create_dir_all(chapter).map_err(LearnError::io("Creating", chapter))?;
            Some(Project::create(chapter, &exercise.name)?)
        } else {
            None
        };

        std::fs::create_dir_all(&hidden).map_err(LearnError::io("Creating", &hidden))?;
        write_if_changed(&testbench_file, &testbench_text)?;
        write_if_changed(&reference_file, &reference_text)?;
        write_if_changed(&dir.join(PROMPT_FILE), &prompt_text(exercise))?;
        // O do aluno e os dados só quando faltam: depois são dele.
        let student = self.student_file(exercise);
        if !student.exists() {
            let start = read(&exercise.start_file())?;
            std::fs::write(&student, start).map_err(LearnError::io("Writing", &student))?;
        }
        for given in &exercise.given {
            let path = dir.join(given);
            if !path.exists() {
                let text = read(&exercise.source.join(given))?;
                std::fs::write(&path, text).map_err(LearnError::io("Writing", &path))?;
            }
        }

        if let Some(project) = project.as_mut() {
            project.add_file(FileRole::Synthesizable, &student, None)?;
            project.set_top_level(&student)?;
            for given in &exercise.given {
                project.add_file(FileRole::Synthesizable, dir.join(given), None)?;
            }
            project.set_testbench(&testbench_file)?;
            tracing::info!(exercise = %exercise.name, %dir, "Exercise created");
        }
        Ok(())
    }

    fn write_solution(&self, exercise: &Exercise) -> Result<()> {
        let path = self.solution_path(exercise);
        let parent = path.parent().expect("a solution file has a parent");
        std::fs::create_dir_all(parent).map_err(LearnError::io("Creating", parent))?;
        write_if_changed(&path, &read(&exercise.solution_file())?)
    }

    /// Grava o estado: num arquivo ao lado, depois troca, para nunca ficar
    /// pela metade.
    fn save(&self) -> Result<()> {
        let path = self.root.join(STATE_FILE);
        let text = serde_json::to_string_pretty(&self.state).expect("the state serializes") + "\n";
        let tmp = self.root.join(format!("{STATE_FILE}.tmp"));
        std::fs::write(&tmp, text).map_err(LearnError::io("Writing", &tmp))?;
        std::fs::rename(&tmp, &path).map_err(LearnError::io("Writing", &path))
    }
}

/// O `.gitignore` da pasta: o que o Lace grava ao corrigir e simular.
const GITIGNORE: &str =
    "# Gerado pelo lace learn: o que o Lace grava ao corrigir e simular.\n.lace/\n.lace-learn/\n";

/// O `README.md` de um exercício: o título e o enunciado, sem as dicas.
fn prompt_text(exercise: &Exercise) -> String {
    format!("# {}\n\n{}\n", exercise.title, exercise.prompt)
}

fn read_state(root: &Utf8Path) -> Result<State> {
    let path = root.join(STATE_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(LearnError::NoWorkspace(root.to_owned()));
        }
        Err(e) => {
            return Err(LearnError::Io {
                action: "Reading",
                path,
                source: e,
            });
        }
    };
    let state: State = serde_json::from_str(&text).map_err(|e| LearnError::InvalidState {
        path: path.clone(),
        reason: e.to_string(),
    })?;
    if state.format != STATE_FORMAT {
        return Err(LearnError::InvalidState {
            path,
            reason: format!(
                "format {} is not supported by this Lace (it reads format {STATE_FORMAT})",
                state.format
            ),
        });
    }
    Ok(state)
}

fn read(path: &Utf8Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(LearnError::io("Reading", path))
}

/// Grava `text` em `path` se o conteúdo for outro (ou se não existir).
fn write_if_changed(path: &Utf8Path, text: &str) -> Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    std::fs::write(path, text).map_err(LearnError::io("Writing", path))
}

/// O caminho absoluto e sem `..`; no Windows, sem o prefixo `\\?\`, que os
/// compiladores não aceitam (como no lace-core).
fn canonical(path: &Utf8Path) -> Result<Utf8PathBuf> {
    let canonical = dunce::canonicalize(path).map_err(LearnError::io("Reading", path))?;
    Utf8PathBuf::from_path_buf(canonical)
        .map_err(|p| LearnError::Lace(lace_core::LaceError::NonUtf8Path(p.display().to_string())))
}
