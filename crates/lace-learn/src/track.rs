//! A trilha: os capítulos e os exercícios, lidos da pasta dela.
//!
//! ```text
//! verilog/                     a trilha (o nome da pasta é o id)
//!   track.json                 {"format": 1}
//!   track.md                   título e boas-vindas
//!   final.md                   a mensagem do fim (opcional)
//!   01_primeiros_passos/       um capítulo; o número dá a ordem
//!     chapter.md               título e introdução
//!     01_um/                   um exercício; o nome é o da pasta sem o número
//!       exercise.json          como o testbench testa (Spec)
//!       prompt.md              enunciado e dicas (prompt.en.md, em inglês)
//!       start.v                o que o aluno recebe
//!       solution.v             a referência
//!       tb.v                   testbench escrito à mão (opcional)
//!       <outro>.v              módulos dados ao aluno (opcional)
//! ```
//!
//! Os textos (título, enunciado, dicas) saem dos `.md`; o testbench gerado
//! do `exercise.json` está em [`testbench`](crate::testbench).

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::verilog::{ModuleInterface, read_interfaces};
use lace_core::{LaceError, Toolchain};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{LearnError, Result};
use crate::text;

/// A variável de ambiente que aponta a pasta das trilhas, no lugar do
/// componente instalado: é como quem escreve exercícios testa a trilha do
/// repositório (`lace-learn/`).
pub const TRACKS_ENV: &str = "LACE_LEARN_DIR";

/// A trilha que o `lace learn init` usa quando nenhuma é pedida.
pub const DEFAULT_TRACK: &str = "verilog";

/// O formato de trilha que este Lace entende (`track.json`).
pub const TRACK_FORMAT: u32 = 1;

/// O arquivo que marca a pasta de uma trilha.
pub const TRACK_FILE: &str = "track.json";
/// O arquivo de cada exercício.
pub const EXERCISE_FILE: &str = "exercise.json";
/// O que o aluno recebe.
pub const START_FILE: &str = "start.v";
/// A solução de referência.
pub const SOLUTION_FILE: &str = "solution.v";
/// O testbench escrito à mão, quando o gerado não serve.
pub const TESTBENCH_FILE: &str = "tb.v";

/// Uma trilha carregada.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Track {
    /// O nome da pasta (`verilog`).
    pub id: String,
    /// A pasta da trilha.
    #[schemars(with = "String")]
    pub dir: Utf8PathBuf,
    /// O título (`track.md`).
    pub title: String,
    /// A mensagem de boas-vindas, em markdown.
    pub welcome: String,
    /// A mensagem do fim (`final.md`), em markdown.
    pub farewell: Option<String>,
    /// Os capítulos, na ordem.
    pub chapters: Vec<Chapter>,
}

/// Um capítulo.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Chapter {
    /// O nome da pasta, com o número (`01_primeiros_passos`).
    pub id: String,
    /// O título (`chapter.md`).
    pub title: String,
    /// A introdução, em markdown.
    pub intro: String,
    /// Os exercícios, na ordem.
    pub exercises: Vec<Exercise>,
}

/// Um exercício da trilha.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct Exercise {
    /// O nome: a pasta sem o número (`mux2`). Único na trilha; é também o
    /// nome do projeto na pasta de exercícios.
    pub name: String,
    /// O capítulo (o nome da pasta dele).
    pub chapter: String,
    /// A pasta do exercício na trilha.
    #[schemars(with = "String")]
    pub source: Utf8PathBuf,
    /// O título (`prompt.md`).
    pub title: String,
    /// O enunciado, em markdown, sem o título e sem as dicas.
    pub prompt: String,
    /// As dicas, na ordem.
    pub hints: Vec<String>,
    /// O módulo que o aluno escreve: o `module` do `exercise.json`, ou o
    /// nome do exercício.
    pub module: String,
    /// Como o testbench gerado testa.
    pub spec: Spec,
    /// O exercício tem `tb.v` escrito à mão.
    pub custom_testbench: bool,
    /// Os módulos dados ao aluno: os `.v` da pasta que não são `start.v`,
    /// `solution.v` nem `tb.v`, pelo nome.
    pub given: Vec<String>,
}

/// O `exercise.json`: como o testbench gerado testa o módulo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Spec {
    /// Combinacional ou sequencial.
    pub kind: Kind,
    /// O nome do módulo, quando não é o do exercício.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// Combinacional: todas as combinações das entradas, ou amostras
    /// aleatórias. Sem, todas quando as entradas somam até
    /// [`EXHAUSTIVE_BITS`](crate::testbench::EXHAUSTIVE_BITS) bits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stimulus: Option<Stimulus>,
    /// Amostras aleatórias (padrão 200).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub samples: Option<u32>,
    /// A semente do `$random` (padrão 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
    /// Sequencial: a entrada de clock (padrão `clk`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<String>,
    /// Sequencial: a entrada de reset, se o módulo tiver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset: Option<Reset>,
    /// Sequencial: quantos ciclos (padrão 200).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycles: Option<u32>,
    /// Quanto a simulação pode durar, em segundos (padrão 10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_s: Option<u64>,
}

/// O tipo de circuito.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Kind {
    /// As saídas dependem só das entradas.
    Combinational,
    /// Tem clock: as entradas mudam a cada ciclo.
    Sequential,
}

/// Como escolher as entradas de um circuito combinacional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Stimulus {
    /// Todas as combinações.
    Exhaustive,
    /// Amostras aleatórias, depois de tudo 0 e tudo 1.
    Random,
}

/// A entrada de reset de um circuito sequencial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Reset {
    /// O nome da porta.
    pub name: String,
    /// O nível que reseta: 1 (padrão) ou 0.
    #[serde(default = "active_high")]
    pub active: u8,
    /// Depois dos dois primeiros ciclos, o reset volta em média uma vez a
    /// cada tantos ciclos (padrão 16; 0 desliga).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every: Option<u32>,
}

fn active_high() -> u8 {
    1
}

impl Track {
    /// Carrega a trilha da pasta `dir`. `lang` escolhe os textos
    /// (`prompt.en.md` com `"en"`); sem, ou sem a tradução, os em português.
    ///
    /// # Erros
    ///
    /// [`LearnError::InvalidTrack`] com o arquivo que falta ou que não se
    /// entende; [`LearnError::Io`] se não der para ler.
    pub fn load(dir: &Utf8Path, lang: Option<&str>) -> Result<Track> {
        let id = dir
            .file_name()
            .ok_or_else(|| LearnError::track(dir, "not a folder name"))?
            .to_owned();
        let manifest = dir.join(TRACK_FILE);
        let format: TrackFile = read_json(&manifest)?;
        if format.format != TRACK_FORMAT {
            return Err(LearnError::track(
                &manifest,
                format!(
                    "format {} is not supported by this Lace (it reads format {TRACK_FORMAT})",
                    format.format
                ),
            ));
        }
        let intro = text::read(dir, "track", lang)?;
        let farewell = match text::localized(dir, "final", lang) {
            Some(_) => Some(text::read(dir, "final", lang)?.body),
            None => None,
        };

        let mut chapters = Vec::new();
        let mut names: Vec<String> = Vec::new();
        for (chapter_id, chapter_dir) in numbered_dirs(dir)? {
            let doc = text::read(&chapter_dir, "chapter", lang)?;
            let mut exercises = Vec::new();
            for (dir_name, exercise_dir) in numbered_dirs(&chapter_dir)? {
                let exercise = load_exercise(&chapter_id, &dir_name, &exercise_dir, lang)?;
                if names.iter().any(|n| n.eq_ignore_ascii_case(&exercise.name)) {
                    return Err(LearnError::track(
                        &exercise_dir,
                        format!("another exercise is already named '{}'", exercise.name),
                    ));
                }
                names.push(exercise.name.clone());
                exercises.push(exercise);
            }
            chapters.push(Chapter {
                id: chapter_id,
                title: doc.title,
                intro: doc.body,
                exercises,
            });
        }
        if names.is_empty() {
            return Err(LearnError::track(dir, "the track has no exercises"));
        }
        Ok(Track {
            id,
            dir: dir.to_owned(),
            title: intro.title,
            welcome: intro.body,
            farewell,
            chapters,
        })
    }

    /// Todos os exercícios, na ordem da trilha.
    pub fn exercises(&self) -> impl Iterator<Item = &Exercise> {
        self.chapters.iter().flat_map(|c| c.exercises.iter())
    }

    /// Quantos exercícios a trilha tem.
    pub fn len(&self) -> usize {
        self.chapters.iter().map(|c| c.exercises.len()).sum()
    }

    /// Se a trilha não tem exercício (uma trilha carregada sempre tem).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// O exercício com esse nome (sem diferenciar maiúsculas).
    pub fn find(&self, name: &str) -> Option<&Exercise> {
        self.exercises().find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// O exercício com esse nome, ou [`LearnError::UnknownExercise`].
    pub fn require(&self, name: &str) -> Result<&Exercise> {
        self.find(name)
            .ok_or_else(|| LearnError::UnknownExercise(name.to_owned()))
    }

    /// A posição do exercício na trilha, a partir de 0.
    pub fn position(&self, name: &str) -> Option<usize> {
        self.exercises()
            .position(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// O capítulo de um exercício.
    pub fn chapter_of(&self, exercise: &Exercise) -> Option<&Chapter> {
        self.chapters.iter().find(|c| c.id == exercise.chapter)
    }
}

impl Exercise {
    /// O `start.v` na trilha.
    pub fn start_file(&self) -> Utf8PathBuf {
        self.source.join(START_FILE)
    }

    /// O `solution.v` na trilha.
    pub fn solution_file(&self) -> Utf8PathBuf {
        self.source.join(SOLUTION_FILE)
    }

    /// As portas do módulo, lidas da solução, na ordem da declaração.
    ///
    /// # Erros
    ///
    /// [`LearnError::InvalidTrack`] se a solução não declara o módulo.
    pub fn interface(&self) -> Result<ModuleInterface> {
        let solution = self.solution_file();
        interface_in(&solution, &self.module)
    }
}

/// As portas do módulo `module` no arquivo `file`, pelo leitor de texto do
/// lace-core (sem o Yosys: é rápido e não roda ferramenta).
pub(crate) fn interface_in(file: &Utf8Path, module: &str) -> Result<ModuleInterface> {
    read_interfaces(None, &[file.to_owned()])?
        .into_iter()
        .find(|m| m.name == module)
        .ok_or_else(|| LearnError::track(file, format!("does not declare module '{module}'")))
}

/// A pasta das trilhas, na ordem:
///
/// 1. a de [`TRACKS_ENV`], se definida e não vazia;
/// 2. numa build de desenvolvimento (`cargo build`, `tauri dev`), a pasta
///    `lace-learn/` do repositório de onde o Lace foi compilado, se ela
///    estiver lá: quem roda o Lace do fonte vê a trilha do fonte, sem
///    instalar o componente. [`TRACKS_ENV`] definida e vazia desliga isto;
/// 3. a do componente `lace-learn` do bundle.
///
/// # Erros
///
/// [`LearnError::ComponentMissing`] quando nenhuma das três existe.
pub fn tracks_dir(toolchain: Option<&Toolchain>) -> Result<Utf8PathBuf> {
    match std::env::var_os(TRACKS_ENV) {
        Some(dir) if !dir.is_empty() => {
            return Utf8PathBuf::from_path_buf(dir.into())
                .map_err(|p| LaceError::NonUtf8Path(p.display().to_string()).into());
        }
        Some(_) => {}
        None => {
            if let Some(dir) = repository_tracks() {
                return Ok(dir);
            }
        }
    }
    let toolchain = toolchain.ok_or(LearnError::ComponentMissing)?;
    let component = toolchain
        .component(lace_core::component::LACE_LEARN)
        .ok_or(LearnError::ComponentMissing)?;
    Ok(toolchain.root().join(&component.dir))
}

/// As trilhas do repositório, só numa build de desenvolvimento e só se a
/// pasta ainda estiver onde estava ao compilar.
fn repository_tracks() -> Option<Utf8PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let dir = Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lace-learn");
    let canonical = dunce::canonicalize(&dir).ok()?;
    let dir = Utf8PathBuf::from_path_buf(canonical).ok()?;
    dir.join(DEFAULT_TRACK)
        .join(TRACK_FILE)
        .is_file()
        .then_some(dir)
}

/// Carrega a trilha `id` da pasta das trilhas.
///
/// # Erros
///
/// [`LearnError::TrackNotFound`] se a pasta não tem a trilha; os de
/// [`Track::load`].
pub fn load_track(tracks: &Utf8Path, id: &str, lang: Option<&str>) -> Result<Track> {
    let dir = tracks.join(id);
    if !dir.join(TRACK_FILE).is_file() {
        return Err(LearnError::TrackNotFound {
            track: id.to_owned(),
            dir: tracks.to_owned(),
        });
    }
    Track::load(&dir, lang)
}

/// As trilhas da pasta das trilhas: as subpastas com `track.json`, em ordem.
pub fn available_tracks(tracks: &Utf8Path) -> Result<Vec<String>> {
    let mut ids: Vec<String> = read_dir(tracks)?
        .into_iter()
        .filter(|(_, path)| path.join(TRACK_FILE).is_file())
        .map(|(name, _)| name)
        .collect();
    ids.sort();
    Ok(ids)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackFile {
    format: u32,
}

fn load_exercise(
    chapter: &str,
    dir_name: &str,
    dir: &Utf8Path,
    lang: Option<&str>,
) -> Result<Exercise> {
    let name = strip_number(dir_name).to_owned();
    lace_core::validate_project_name(&name)
        .map_err(|e| LearnError::track(dir, format!("the exercise name: {e}")))?;
    let spec: Spec = read_json(&dir.join(EXERCISE_FILE))?;
    let module = spec.module.clone().unwrap_or_else(|| name.clone());
    if !is_identifier(&module) {
        return Err(LearnError::track(
            &dir.join(EXERCISE_FILE),
            format!("'{module}' is not a Verilog module name"),
        ));
    }
    let doc = text::read(dir, "prompt", lang)?;
    for required in [START_FILE, SOLUTION_FILE] {
        if !dir.join(required).is_file() {
            return Err(LearnError::track(&dir.join(required), "missing"));
        }
    }
    let mut given: Vec<String> = read_dir(dir)?
        .into_iter()
        .filter(|(file, path)| {
            path.is_file()
                && file.ends_with(".v")
                && ![START_FILE, SOLUTION_FILE, TESTBENCH_FILE].contains(&file.as_str())
        })
        .map(|(file, _)| file)
        .collect();
    given.sort();
    if given.iter().any(|g| *g == format!("{module}.v")) {
        return Err(LearnError::track(
            dir,
            format!("{module}.v is the student's file; a given module needs another name"),
        ));
    }
    Ok(Exercise {
        name,
        chapter: chapter.to_owned(),
        source: dir.to_owned(),
        title: doc.title,
        prompt: doc.body,
        hints: doc.hints,
        module,
        spec,
        custom_testbench: dir.join(TESTBENCH_FILE).is_file(),
        given,
    })
}

/// Lê um JSON da trilha.
fn read_json<T: serde::de::DeserializeOwned>(path: &Utf8Path) -> Result<T> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            LearnError::track(path, "missing")
        } else {
            LearnError::Io {
                action: "Reading",
                path: path.to_owned(),
                source: e,
            }
        }
    })?;
    serde_json::from_str(&text).map_err(|e| LearnError::track(path, e.to_string()))
}

/// As entradas de uma pasta, com o nome e o caminho.
fn read_dir(dir: &Utf8Path) -> Result<Vec<(String, Utf8PathBuf)>> {
    let entries = dir
        .read_dir_utf8()
        .map_err(LearnError::io("Reading", dir))?;
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(LearnError::io("Reading", dir))?;
        found.push((entry.file_name().to_owned(), entry.path().to_owned()));
    }
    Ok(found)
}

/// As subpastas numeradas (`01_nome`), em ordem.
fn numbered_dirs(dir: &Utf8Path) -> Result<Vec<(String, Utf8PathBuf)>> {
    let mut dirs: Vec<(String, Utf8PathBuf)> = read_dir(dir)?
        .into_iter()
        .filter(|(name, path)| path.is_dir() && strip_number(name) != name)
        .collect();
    dirs.sort();
    Ok(dirs)
}

/// O nome sem o número da frente (`01_mux2` vira `mux2`); o mesmo nome sem
/// número.
fn strip_number(name: &str) -> &str {
    let digits = name.bytes().take_while(u8::is_ascii_digit).count();
    match name[digits..].strip_prefix('_') {
        Some(rest) if digits > 0 && !rest.is_empty() => rest,
        _ => name,
    }
}

/// Um identificador simples do Verilog.
pub(crate) fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_number_prefix_orders_and_is_stripped() {
        assert_eq!(strip_number("01_mux2"), "mux2");
        assert_eq!(strip_number("12_a_b"), "a_b");
        assert_eq!(strip_number("mux2"), "mux2");
        assert_eq!(strip_number("01_"), "01_");
        assert_eq!(strip_number("_x"), "_x");
    }

    #[test]
    fn identifiers() {
        assert!(is_identifier("mux2"));
        assert!(is_identifier("_a1"));
        assert!(!is_identifier("2mux"));
        assert!(!is_identifier("a-b"));
        assert!(!is_identifier(""));
    }
}
