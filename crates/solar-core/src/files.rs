//! Arquivos do projeto além dos processadores: Verilog sintetizável,
//! testbenches e os arquivos de entrada e saída da simulação.
//!
//! No `.spf` (AURORA, `js/project/file_mode.js` e `spf_store.ts`):
//!
//! - `structure.synthesizableFiles` e `structure.testbenchFiles`: listas de
//!   `{ "name", "path", "isTopLevel" }`. O caminho é relativo à raiz quando o
//!   arquivo está dentro dela, absoluto quando não.
//! - `structure.topLevelFile`: o módulo de topo do projeto (`-s` da checagem
//!   de sintaxe e da síntese).
//! - `structure.testbenchFile`: o testbench que a simulação do projeto roda.
//!
//! As listas são lidas do documento a cada chamada: o `.spf` é a única fonte,
//! e nada fica duplicado em memória para sair de sincronia.
//!
//! # Exemplo
//!
//! ```
//! use solar_core::{FileRole, Project};
//! # let tmp = tempfile::tempdir()?;
//! # let dir = camino::Utf8Path::from_path(tmp.path()).unwrap();
//!
//! let mut project = Project::create(dir, "p")?;
//! let top = project.add_file(FileRole::Synthesizable, "rtl/top.v", Some("module top; endmodule\n"))?;
//! let tb = project.add_file(FileRole::Testbench, "rtl/top_tb.v", Some("module top_tb; endmodule\n"))?;
//! project.set_top_level(&top)?;
//! project.set_testbench(&tb)?;
//! assert_eq!(project.top_level(), Some(top));
//! assert_eq!(project.testbench(), Some(tb));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::{Result, SolarError};
use crate::project::{Processor, Project};

/// Em qual lista do `.spf` um arquivo fica. Em JSON: `"synthesizable"` ou
/// `"testbench"`.
///
/// O Solar não classifica por conteúdo como a AURORA faz no navegador de
/// arquivos: quem registra diz o papel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FileRole {
    /// Entra na síntese e na simulação.
    Synthesizable,
    /// Só entra na simulação.
    Testbench,
}

impl FileRole {
    fn list_key(self) -> &'static str {
        match self {
            FileRole::Synthesizable => "synthesizableFiles",
            FileRole::Testbench => "testbenchFiles",
        }
    }

    fn selected_key(self) -> &'static str {
        match self {
            FileRole::Synthesizable => "topLevelFile",
            FileRole::Testbench => "testbenchFile",
        }
    }
}

/// Um arquivo registrado no projeto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct ProjectFile {
    /// Em qual lista ele está.
    pub role: FileRole,
    /// Caminho absoluto.
    pub path: Utf8PathBuf,
    /// Marcado como topo (sintetizável) ou como o testbench escolhido.
    pub top_level: bool,
}

impl Project {
    /// Os arquivos de um papel, na ordem do `.spf`.
    pub fn files(&self, role: FileRole) -> Vec<ProjectFile> {
        let Some(list) = self.document["structure"][role.list_key()].as_array() else {
            return Vec::new();
        };
        list.iter()
            .filter_map(|entry| {
                let path = entry["path"].as_str().filter(|p| !p.trim().is_empty())?;
                Some(ProjectFile {
                    role,
                    path: self.resolve_path(path),
                    top_level: entry["isTopLevel"].as_bool().unwrap_or(false),
                })
            })
            .collect()
    }

    /// O arquivo de topo: `topLevelFile`, senão o sintetizável marcado.
    pub fn top_level(&self) -> Option<Utf8PathBuf> {
        self.selected(FileRole::Synthesizable)
    }

    /// O testbench da simulação do projeto, na ordem da AURORA
    /// (`escolherTestbench`): `testbenchFile`; senão o testbench marcado; senão
    /// o primeiro da lista.
    pub fn testbench(&self) -> Option<Utf8PathBuf> {
        self.selected(FileRole::Testbench).or_else(|| {
            self.files(FileRole::Testbench)
                .into_iter()
                .next()
                .map(|f| f.path)
        })
    }

    fn selected(&self, role: FileRole) -> Option<Utf8PathBuf> {
        let declared = self.document["structure"][role.selected_key()]
            .as_str()
            .filter(|p| !p.trim().is_empty())
            .map(|p| self.resolve_path(p));
        declared.or_else(|| {
            self.files(role)
                .into_iter()
                .find(|f| f.top_level)
                .map(|f| f.path)
        })
    }

    /// Registra um arquivo no projeto e grava o `.spf`. Devolve o caminho
    /// absoluto.
    ///
    /// `path` pode ser relativo à raiz ou absoluto (inclusive fora do
    /// projeto). Com `contents`, cria o arquivo e os diretórios, recusando se
    /// ele já existir; com `None`, o arquivo precisa existir. Registrar de
    /// novo o mesmo caminho no mesmo papel não duplica a entrada. O `.spf`
    /// guarda o caminho relativo à raiz quando o arquivo está dentro dela.
    ///
    /// # Erros
    ///
    /// [`SolarError::InvalidProject`] se `contents` foi dado e o arquivo já
    /// existe, ou se não foi dado e o arquivo não existe.
    pub fn add_file(
        &mut self,
        role: FileRole,
        path: impl AsRef<Utf8Path>,
        contents: Option<&str>,
    ) -> Result<Utf8PathBuf> {
        let path = self.absolute(path.as_ref());
        match contents {
            Some(text) => {
                if path.exists() {
                    return Err(SolarError::InvalidProject {
                        path,
                        reason: "o arquivo já existe; o Solar não sobrescreve".into(),
                    });
                }
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(SolarError::io("criando diretório", parent))?;
                }
                std::fs::write(&path, text).map_err(SolarError::io("criando arquivo", &path))?;
            }
            None if !path.is_file() => {
                return Err(SolarError::InvalidProject {
                    path,
                    reason: "o arquivo não existe".into(),
                });
            }
            None => {}
        }

        if !self.files(role).iter().any(|f| f.path == path) {
            let entry = json!({
                "name": path.file_name().unwrap_or_default(),
                "path": self.stored_path(&path),
                "isTopLevel": false,
            });
            self.list_mut(role).push(entry);
            self.save()?;
        }
        tracing::info!(%path, ?role, "arquivo registrado");
        Ok(path)
    }

    /// Tira o arquivo do projeto (não apaga do disco). Diz se ele estava lá.
    pub fn remove_file(&mut self, role: FileRole, path: impl AsRef<Utf8Path>) -> Result<bool> {
        let path = self.absolute(path.as_ref());
        let root = self.root.clone();
        let list = self.list_mut(role);
        let before = list.len();
        list.retain(|entry| {
            entry["path"]
                .as_str()
                .map(|p| resolve(&root, p))
                .is_none_or(|p| p != path)
        });
        let removed = list.len() != before;
        let selected = self.document["structure"][role.selected_key()]
            .as_str()
            .map(|p| self.resolve_path(p));
        if selected.as_ref() == Some(&path) {
            self.document["structure"][role.selected_key()] = "".into();
        }
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// Marca o módulo de topo do projeto: grava `topLevelFile` e deixa só este
    /// arquivo com `isTopLevel` entre os sintetizáveis. O arquivo precisa
    /// existir; é registrado como sintetizável se ainda não estiver. O nome do
    /// módulo de topo é o nome do arquivo sem extensão.
    pub fn set_top_level(&mut self, path: impl AsRef<Utf8Path>) -> Result<()> {
        self.select(FileRole::Synthesizable, path.as_ref())
    }

    /// Escolhe o testbench de [`simulate_project`](crate::simulate_project):
    /// grava `testbenchFile` e deixa só este com `isTopLevel` entre os
    /// testbenches. O arquivo precisa existir; é registrado se ainda não
    /// estiver.
    pub fn set_testbench(&mut self, path: impl AsRef<Utf8Path>) -> Result<()> {
        self.select(FileRole::Testbench, path.as_ref())
    }

    fn select(&mut self, role: FileRole, path: &Utf8Path) -> Result<()> {
        let path = self.add_file(role, path, None)?;
        let stored = self.stored_path(&path);
        let root = self.root.clone();
        for entry in self.list_mut(role).iter_mut() {
            let is_it = entry["path"].as_str().map(|p| resolve(&root, p)) == Some(path.clone());
            entry["isTopLevel"] = is_it.into();
        }
        self.document["structure"][role.selected_key()] = stored.into();
        self.save()
    }

    fn list_mut(&mut self, role: FileRole) -> &mut Vec<Value> {
        let structure = &mut self.document["structure"];
        if !structure[role.list_key()].is_array() {
            structure[role.list_key()] = json!([]);
        }
        structure[role.list_key()]
            .as_array_mut()
            .expect("acabou de virar array")
    }

    /// Caminho como está gravado num `.spf` (relativo à raiz ou absoluto, com
    /// `\` ou `/`) para absoluto. Um caminho absoluto do Windows (`C:\...`)
    /// num `.spf` aberto no Linux fica como está.
    pub fn resolve_path(&self, stored: &str) -> Utf8PathBuf {
        resolve(&self.root, stored)
    }

    fn absolute(&self, path: &Utf8Path) -> Utf8PathBuf {
        if path.is_absolute() {
            path.to_owned()
        } else {
            self.root.join(path)
        }
    }

    /// Como a AURORA grava: relativo à raiz quando dentro dela.
    fn stored_path(&self, path: &Utf8Path) -> String {
        match path.strip_prefix(&self.root) {
            Ok(rel) => rel.as_str().replace('\\', "/"),
            Err(_) => path.as_str().to_owned(),
        }
    }
}

fn resolve(root: &Utf8Path, stored: &str) -> Utf8PathBuf {
    // `C:\x` num .spf aberto fora do Windows não é absoluto aqui, mas também
    // não faz sentido juntar à raiz; fica como está.
    if crate::paths::is_windows_absolute(stored) {
        return Utf8PathBuf::from(stored);
    }
    let normalized = if cfg!(windows) {
        stored.to_owned()
    } else {
        stored.replace('\\', "/")
    };
    let path = Utf8PathBuf::from(normalized);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

impl Processor {
    /// `Simulation/input_<porta>.txt`: um valor decimal por linha, lido pelo
    /// testbench com `$fscanf(..., "%d", ...)` quando o processador lê a porta.
    pub fn input_path(&self, port: u32) -> Utf8PathBuf {
        self.simulation_dir().join(format!("input_{port}.txt"))
    }

    /// `Simulation/output_<porta>.txt`: um valor decimal por linha, escrito
    /// pelo testbench a cada `out()` do programa.
    pub fn output_path(&self, port: u32) -> Utf8PathBuf {
        self.simulation_dir().join(format!("output_{port}.txt"))
    }

    /// Grava o arquivo de entrada de uma porta (substitui o anterior: é dado
    /// de estímulo, não código).
    pub fn write_input(&self, port: u32, text: &str) -> Result<Utf8PathBuf> {
        let path = self.input_path(port);
        std::fs::create_dir_all(self.simulation_dir())
            .map_err(SolarError::io("criando diretório", self.simulation_dir()))?;
        std::fs::write(&path, text).map_err(SolarError::io("gravando entrada", &path))?;
        Ok(path)
    }

    /// Grava a entrada de uma porta a partir de valores, um por linha, no
    /// formato que o testbench lê (`%d`).
    ///
    /// ```
    /// # use solar_core::{Language, NewProcessor, Project, read_data_file};
    /// # let tmp = tempfile::tempdir()?;
    /// # let dir = camino::Utf8Path::from_path(tmp.path()).unwrap();
    /// # let mut project = Project::create(dir, "p")?;
    /// let soma = project.add_processor(&NewProcessor::new("soma", Language::Cmm))?;
    /// let path = soma.write_input_values(0, &[5, -7, 21])?;
    /// assert_eq!(std::fs::read_to_string(&path)?, "5\n-7\n21\n");
    /// assert_eq!(read_data_file(&path)?, [5, -7, 21]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn write_input_values(&self, port: u32, values: &[i64]) -> Result<Utf8PathBuf> {
        let text: String = values.iter().map(|v| format!("{v}\n")).collect();
        self.write_input(port, &text)
    }

    /// Lê o que a simulação escreveu numa porta de saída, como texto.
    pub fn read_output(&self, port: u32) -> Result<String> {
        let path = self.output_path(port);
        std::fs::read_to_string(&path).map_err(SolarError::io("lendo saída", &path))
    }

    /// Lê a saída de uma porta como valores ([`read_data_file`]).
    pub fn read_output_values(&self, port: u32) -> Result<Vec<i64>> {
        read_data_file(&self.output_path(port))
    }
}

/// Lê um arquivo de dados da simulação (`input_<n>.txt`, `output_<n>.txt`):
/// um inteiro decimal com sinal por linha, que é o que o testbench gerado lê
/// com `%d` e escreve com `%0d`. Linhas vazias são ignoradas.
///
/// # Erros
///
/// - [`SolarError::InvalidDataFile`] na primeira linha que não for um inteiro
///   (a linha nunca é descartada em silêncio: um valor perdido muda o
///   resultado da simulação);
/// - [`SolarError::Io`] se o arquivo não puder ser lido.
pub fn read_data_file(path: &Utf8Path) -> Result<Vec<i64>> {
    let text = std::fs::read_to_string(path).map_err(SolarError::io("lendo dados", path))?;
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            line.trim()
                .parse::<i64>()
                .map_err(|_| SolarError::InvalidDataFile {
                    path: path.to_owned(),
                    line: u32::try_from(i + 1).unwrap_or(u32::MAX),
                    reason: format!("'{}' não é um inteiro decimal", line.trim()),
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Language, NewProcessor};

    fn project() -> (tempfile::TempDir, Project) {
        let dir = tempfile::tempdir().unwrap();
        let parent = Utf8PathBuf::from_path_buf(dunce::canonicalize(dir.path()).unwrap()).unwrap();
        let project = Project::create(&parent, "p").unwrap();
        (dir, project)
    }

    #[test]
    fn add_select_and_reopen() {
        let (_guard, mut project) = project();
        let top = project
            .add_file(
                FileRole::Synthesizable,
                "top/top.v",
                Some("module top; endmodule\n"),
            )
            .unwrap();
        let tb = project
            .add_file(
                FileRole::Testbench,
                "top/top_tb.v",
                Some("module top_tb; endmodule\n"),
            )
            .unwrap();
        assert!(top.is_file());
        assert_eq!(project.testbench(), Some(tb.clone()), "primeiro da lista");

        project.set_top_level(&top).unwrap();
        project.set_testbench(&tb).unwrap();
        // registrar de novo não duplica
        project
            .add_file(FileRole::Synthesizable, &top, None)
            .unwrap();

        let reopened = Project::open(project.spf_path()).unwrap();
        assert_eq!(reopened.files(FileRole::Synthesizable).len(), 1);
        assert_eq!(reopened.top_level(), Some(top.clone()));
        assert_eq!(reopened.testbench(), Some(tb));
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(reopened.spf_path()).unwrap()).unwrap();
        assert_eq!(doc["structure"]["topLevelFile"], "top/top.v");
        assert_eq!(
            doc["structure"]["synthesizableFiles"][0]["isTopLevel"],
            true
        );
    }

    #[test]
    fn never_overwrites_and_requires_existing() {
        let (_guard, mut project) = project();
        project
            .add_file(FileRole::Synthesizable, "a.v", Some("x"))
            .unwrap();
        assert!(
            project
                .add_file(FileRole::Synthesizable, "a.v", Some("y"))
                .is_err()
        );
        assert!(
            project
                .add_file(FileRole::Testbench, "nao_existe.v", None)
                .is_err()
        );
        assert!(project.remove_file(FileRole::Synthesizable, "a.v").unwrap());
        assert!(project.files(FileRole::Synthesizable).is_empty());
        assert!(
            project.root().join("a.v").is_file(),
            "remover do projeto não apaga do disco"
        );
    }

    #[test]
    fn windows_style_relative_paths_resolve() {
        let root = Utf8Path::new("/r");
        assert_eq!(
            resolve(root, "proc\\Hardware\\proc.v"),
            Utf8PathBuf::from("/r/proc/Hardware/proc.v")
        );
        assert_eq!(resolve(root, "C:\\x\\a.v"), Utf8PathBuf::from("C:\\x\\a.v"));
    }

    #[test]
    fn data_file_rejects_invalid_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("d.txt")).unwrap();
        std::fs::write(&path, "1\n\n-2\n3.5\n").unwrap();
        match read_data_file(&path) {
            Err(SolarError::InvalidDataFile { line, .. }) => assert_eq!(line, 4),
            other => panic!("{other:?}"),
        }
        std::fs::write(&path, " 1\n\n-2 \n").unwrap();
        assert_eq!(read_data_file(&path).unwrap(), [1, -2]);
    }

    #[test]
    fn input_and_output_files() {
        let (_guard, mut project) = project();
        let processor = project
            .add_processor(&NewProcessor::new("q", Language::Cmm))
            .unwrap()
            .clone();
        let path = processor.write_input(0, "1\n2\n").unwrap();
        assert_eq!(path, processor.simulation_dir().join("input_0.txt"));
        assert!(processor.read_output(0).is_err());
    }
}
