//! A conversão de um projeto da estrutura de pastas de antes do YANC 7.1
//! para a de agora.
//!
//! Até a 6.x cada processador tinha três subpastas: `<proc>/Software/` (o
//! fonte e o `.asm`), `<proc>/Hardware/` (o Verilog e as memórias) e
//! `<proc>/Simulation/` (o testbench, `input_N.txt` e `output_N.txt`). Desde
//! a 7.1 a estrutura é a do próprio YANC: o que a pessoa escreve fica em
//! `<proc>/`, e tudo o que o YANC gera numa pasta de saída plana,
//! `<proc>/<proc>/` (ver o módulo `project`). Um projeto antigo não compila
//! sem a conversão: o fonte não está onde o Lace o procura.
//!
//! A conversão é a da AURORA (`js/project/estrutura_antiga.ts`), para que um
//! projeto convertido por uma abra igual na outra:
//!
//! - o que o YANC gerava (o `.asm`, o `.v`, o testbench, as memórias, as
//!   tabelas, as saídas da simulação, a onda) vai para a pasta de saída, onde
//!   o próximo build o sobrescreve; o resto, que é da pessoa (o fonte, os
//!   arquivos de arrays, `input_N.txt`, um `.v` que ela pôs em `Hardware/`,
//!   uma subpasta), vai para a pasta do processador;
//! - nada é apagado nem sobrescrito: um destino que já existe deixa o
//!   arquivo onde estava, e ele entra no relatório; a subpasta antiga só sai
//!   quando fica vazia;
//! - a linha `#PRNAME` (ou `#pragma yanc prname`) do fonte, que a 7.1
//!   recusa, sai;
//! - o `.spf` passa a apontar para os lugares novos (o topo, o testbench
//!   escolhido, as listas), e as entradas dos processadores perdem os campos
//!   de pasta da AURORA antiga (`softwarePath`, `hardwarePath`,
//!   `simulationPath`).

use camino::{Utf8Path, Utf8PathBuf};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

use crate::error::{LaceError, Result};
use crate::project::{Language, Processor, Project, validate_processor_name};

/// As subpastas que cada processador tinha antes do YANC 7.1.
pub(crate) const LEGACY_DIRS: [&str; 3] = ["Software", "Hardware", "Simulation"];

/// O que o sistema ou o git deixam sozinhos numa pasta: não é da pessoa nem
/// do YANC. A conversão não o move, e uma pasta antiga só com ele já não
/// conta como estrutura antiga.
pub(crate) fn is_system_file(name: &str) -> bool {
    matches!(name, ".DS_Store" | "Thumbs.db" | "desktop.ini" | ".gitkeep") || name.starts_with("._")
}

/// A pasta tem algo além do que [`is_system_file`] aceita.
pub(crate) fn has_content(dir: &Utf8Path) -> bool {
    dir.read_dir_utf8()
        .is_ok_and(|entries| entries.flatten().any(|e| !is_system_file(e.file_name())))
}

/// O que [`Project::convert_layout`] fez.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct LayoutConversion {
    /// Os processadores que tinham a estrutura antiga, na ordem do `.spf`.
    pub processors: Vec<String>,
    /// O que mudou de lugar.
    pub moved: Vec<ConvertedPath>,
    /// O que ficou onde estava, com o motivo.
    pub skipped: Vec<SkippedPath>,
    /// Os fontes que perderam a linha do nome (`#PRNAME`, `#pragma yanc
    /// prname`).
    pub name_lines: Vec<RemovedNameLine>,
    /// Os processadores cujo nome não serve mais: na 7.1 ele vira o módulo
    /// Verilog, e o YANC recusa um fonte com esse nome.
    pub invalid_names: Vec<String>,
    /// As subpastas antigas que ficaram com algo que não foi movido (o que o
    /// sistema deixa sozinho numa pasta, como o `.DS_Store`, não conta).
    #[schemars(with = "Vec<String>")]
    pub remaining_dirs: Vec<Utf8PathBuf>,
}

impl LayoutConversion {
    /// O projeto já estava na estrutura atual: nada a converter.
    pub fn is_empty(&self) -> bool {
        self.processors.is_empty()
    }
}

/// Um arquivo ou pasta que a conversão moveu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct ConvertedPath {
    /// De que processador.
    pub processor: String,
    /// Onde estava, absoluto.
    #[schemars(with = "String")]
    pub from: Utf8PathBuf,
    /// Onde ficou, absoluto.
    #[schemars(with = "String")]
    pub to: Utf8PathBuf,
    /// É algo que o YANC gera (foi para a pasta de saída).
    pub generated: bool,
}

/// Um arquivo ou pasta que a conversão deixou onde estava.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct SkippedPath {
    /// De que processador.
    pub processor: String,
    /// Onde está, absoluto.
    #[schemars(with = "String")]
    pub from: Utf8PathBuf,
    /// Para onde iria.
    #[schemars(with = "String")]
    pub to: Utf8PathBuf,
    /// Por quê: o destino já existe, ou o erro do sistema.
    pub reason: String,
}

/// Um fonte que perdeu a linha do nome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct RemovedNameLine {
    /// De que processador.
    pub processor: String,
    /// O fonte, já no lugar novo.
    #[schemars(with = "String")]
    pub file: Utf8PathBuf,
    /// O nome que a linha dava, que era o do módulo Verilog até a 6.x. Se
    /// difere do processador, o topo que instanciava o módulo pelo nome
    /// antigo precisa do novo.
    pub old_name: String,
}

impl Project {
    /// Os processadores que ainda têm a estrutura de antes do YANC 7.1
    /// (`Software/`, `Hardware/` ou `Simulation/` na pasta deles). Vazio num
    /// projeto na estrutura atual.
    pub fn legacy_processors(&self) -> Vec<&Processor> {
        self.processors()
            .iter()
            .filter(|p| !p.legacy_dirs().is_empty())
            .collect()
    }

    /// Converte os processadores da estrutura de antes do YANC 7.1 para a de
    /// agora, como a AURORA faz ao abrir um projeto antigo (ver o módulo
    /// `layout`), e grava o `.spf`. Devolve o que foi feito; num projeto já
    /// convertido, um relatório vazio, e nada é gravado.
    ///
    /// ```
    /// use lace_core::{Language, NewProcessor, Project};
    /// # let tmp = tempfile::tempdir()?;
    /// # let dir = camino::Utf8Path::from_path(tmp.path()).unwrap();
    /// let mut project = Project::create(dir, "p")?;
    /// let soma = project.add_processor(&NewProcessor::new("soma", Language::Cmm))?.clone();
    /// // Como a 6.x deixava: o fonte em Software/, com #PRNAME.
    /// std::fs::create_dir(soma.dir.join("Software"))?;
    /// std::fs::write(soma.dir.join("Software/soma.cmm"), "#PRNAME soma\n#NUBITS 23\n")?;
    /// std::fs::remove_file(&soma.source)?;
    ///
    /// let report = project.convert_layout()?;
    /// assert_eq!(report.processors, ["soma"]);
    /// assert_eq!(std::fs::read_to_string(&soma.source)?, "#NUBITS 23\n");
    /// assert!(!soma.dir.join("Software").exists());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Erros
    ///
    /// - [`LaceError::Io`] se a pasta de saída de um processador não puder
    ///   ser criada, ou o `.spf` não puder ser gravado. Um arquivo que não
    ///   pôde ser movido não é erro: fica no relatório, em `skipped`.
    pub fn convert_layout(&mut self) -> Result<LayoutConversion> {
        self.begin_write()?;
        let legacy: Vec<Processor> = self.legacy_processors().into_iter().cloned().collect();
        let mut report = LayoutConversion::default();
        if legacy.is_empty() {
            self.release_write();
            return Ok(report);
        }
        let mut changed = false;
        for p in &legacy {
            report.processors.push(p.name.clone());
            if validate_processor_name(&p.name).is_err() {
                report.invalid_names.push(p.name.clone());
            }
            if let Err(error) = std::fs::create_dir_all(&p.output_dir) {
                self.release_write();
                return Err(LaceError::io("Creating directory", &p.output_dir)(error));
            }
            let bases = file_bases(p);
            for dir in p.legacy_dirs() {
                let Ok(entries) = dir.read_dir_utf8() else {
                    continue;
                };
                let mut entries: Vec<_> = entries
                    .flatten()
                    .filter(|e| !is_system_file(e.file_name()))
                    .collect();
                entries.sort_by(|a, b| a.file_name().cmp(b.file_name()));
                for entry in entries {
                    let from = entry.path().to_owned();
                    let name = entry.file_name();
                    let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
                    let generated = !is_dir && is_generated(name, &bases);
                    let to = if generated { &p.output_dir } else { &p.dir }.join(name);
                    if std::fs::symlink_metadata(&to).is_ok() {
                        report.skipped.push(SkippedPath {
                            processor: p.name.clone(),
                            from,
                            to,
                            reason: "The destination already exists".into(),
                        });
                        continue;
                    }
                    if let Err(error) = std::fs::rename(&from, &to) {
                        report.skipped.push(SkippedPath {
                            processor: p.name.clone(),
                            from,
                            to,
                            reason: error.to_string(),
                        });
                        continue;
                    }
                    changed |= self.rewrite_moved(&from, &to);
                    if !generated && is_source(name) {
                        remove_name_line(p, &to, &mut report);
                    }
                    report.moved.push(ConvertedPath {
                        processor: p.name.clone(),
                        from,
                        to,
                        generated,
                    });
                }
                // Vazia, a pasta sai; com o que o sistema deixou, fica, sem
                // contar como antiga.
                let empty = dir.read_dir().is_ok_and(|mut d| d.next().is_none());
                if empty {
                    let _ = std::fs::remove_dir(&dir);
                }
                if has_content(&dir) {
                    report.remaining_dirs.push(dir);
                }
            }
        }
        changed |= self.drop_legacy_entry_fields();
        if changed {
            self.save()?;
        } else {
            self.release_write();
        }
        tracing::info!(
            processors = report.processors.len(),
            moved = report.moved.len(),
            skipped = report.skipped.len(),
            "Layout converted"
        );
        Ok(report)
    }

    /// Tira das entradas dos processadores no `.spf` os campos de pasta da
    /// AURORA de antes da 7.1, e deixa só o nome do arquivo num
    /// `sourceFile`/`cmmFile` com caminho. Diz se algo mudou; não grava.
    fn drop_legacy_entry_fields(&mut self) -> bool {
        let Some(list) = self
            .document
            .get_mut("structure")
            .and_then(|s| s.get_mut("processors"))
            .and_then(Value::as_array_mut)
        else {
            return false;
        };
        let mut changed = false;
        for entry in list.iter_mut().filter_map(Value::as_object_mut) {
            for key in ["softwarePath", "hardwarePath", "simulationPath"] {
                changed |= entry.remove(key).is_some();
            }
            for key in ["sourceFile", "cmmFile"] {
                if let Some(file) = entry.get(key).and_then(Value::as_str)
                    && file.contains(['/', '\\'])
                {
                    let name = file.rsplit(['/', '\\']).next().unwrap_or(file).to_owned();
                    entry.insert(key.to_owned(), name.into());
                    changed = true;
                }
            }
        }
        changed
    }
}

/// Os nomes que o processador dá aos arquivos gerados: o dele e o de cada
/// fonte em `Software/` (um `filtro_v2.cmm` gerava `filtro_v2.asm`).
fn file_bases(p: &Processor) -> Vec<String> {
    let mut bases = vec![p.name.clone()];
    if let Ok(entries) = p.dir.join("Software").read_dir_utf8() {
        for entry in entries.flatten() {
            let path = entry.path();
            if is_source(entry.file_name())
                && let Some(stem) = path.file_stem()
                && !bases.iter().any(|b| b == stem)
            {
                bases.push(stem.to_owned());
            }
        }
    }
    bases
}

/// `<nome>.cmm` ou `<nome>.cpp`.
fn is_source(name: &str) -> bool {
    Utf8Path::new(name)
        .extension()
        .is_some_and(|e| Language::from_extension(e).is_some())
}

/// O arquivo é algo que o YANC gera, ou gerava? As tabelas e os logs de
/// nome fixo, as saídas da simulação (`output_N.txt`, `toma_N.txt`), e o que
/// leva o nome do processador (`bases`): o `.asm`, o `.v`, o testbench, as
/// memórias, a tabela do PC e a onda. A mesma regra da AURORA.
fn is_generated(name: &str, bases: &[String]) -> bool {
    let name = name.to_ascii_lowercase();
    const TABLES: [&str; 6] = [
        "cmm_log.txt",
        "prog_log.txt",
        "trad_cmm.txt",
        "trad_opcode.txt",
        "src_lines.txt",
        "app_log.txt",
    ];
    if TABLES.contains(&name.as_str()) {
        return true;
    }
    let port_output = ["output_", "toma_"].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(".txt"))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    });
    if port_output {
        return true;
    }
    bases.iter().any(|base| {
        let b = base.to_ascii_lowercase();
        [
            format!("{b}.asm"),
            format!("{b}.v"),
            format!("{b}_tb.v"),
            format!("{b}_inst.mif"),
            format!("{b}_data.mif"),
            format!("pc_{b}_mem.txt"),
            format!("{b}_tb.vcd"),
            format!("{b}_tb.fst"),
            format!("{b}_tb.vvp"),
        ]
        .contains(&name)
    })
}

/// Tira do fonte a linha do nome e anota no relatório. Um fonte ilegível
/// fica como está: o build vai dizer o que falta.
fn remove_name_line(p: &Processor, file: &Utf8Path, report: &mut LayoutConversion) {
    let Ok(text) = std::fs::read_to_string(file) else {
        return;
    };
    let (text, old_name) = without_name_lines(&text);
    if let Some(old_name) = old_name
        && std::fs::write(file, text).is_ok()
    {
        report.name_lines.push(RemovedNameLine {
            processor: p.name.clone(),
            file: file.to_owned(),
            old_name,
        });
    }
}

/// O texto sem as linhas `#PRNAME <nome>` e `#pragma yanc prname <nome>`, e
/// o nome da primeira. Toda linha de nome sai, porque a 7.1 recusa qualquer
/// uma.
fn without_name_lines(text: &str) -> (String, Option<String>) {
    let mut old_name = None;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        match name_line(line) {
            Some(name) => {
                old_name.get_or_insert(name);
            }
            None => out.push_str(line),
        }
    }
    (out, old_name)
}

/// O nome que a linha declara, se ela for uma linha de nome.
fn name_line(line: &str) -> Option<String> {
    let rest = line.trim_start();
    let rest = if let Some(rest) = rest.strip_prefix("#PRNAME") {
        rest
    } else {
        let rest = rest.strip_prefix('#')?.trim_start();
        let rest = rest.strip_prefix("pragma")?;
        let rest = rest.trim_start().strip_prefix("yanc")?;
        rest.trim_start().strip_prefix("prname")?
    };
    if !rest.starts_with([' ', '\t']) {
        return None;
    }
    let mut words = rest.split_whitespace();
    let name = words.next()?;
    words.next().is_none().then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileRole, IssueKind, NewProcessor};

    /// Um projeto com o processador `soma` na estrutura da 7.1.
    fn project_with_soma(dir: &Utf8Path) -> (Project, Processor) {
        let mut project = Project::create(dir, "p").unwrap();
        let soma = project
            .add_processor(&NewProcessor::new("soma", Language::Cmm))
            .unwrap()
            .clone();
        (project, soma)
    }

    /// `soma` como a 6.x deixava: o fonte, com `#PRNAME`, em `Software/`, e
    /// o Verilog de um build antigo em `Hardware/`, registrado no `.spf`.
    fn make_legacy(project: &mut Project, soma: &Processor) {
        std::fs::create_dir_all(soma.dir.join("Software")).unwrap();
        std::fs::write(
            soma.dir.join("Software/soma.cmm"),
            "#PRNAME soma\n#NUBITS 23\n",
        )
        .unwrap();
        std::fs::remove_file(&soma.source).unwrap();
        std::fs::create_dir_all(soma.dir.join("Hardware")).unwrap();
        let old = soma.dir.join("Hardware/soma.v");
        std::fs::write(&old, "module soma; endmodule\n").unwrap();
        project
            .add_file(FileRole::Synthesizable, &old, None)
            .unwrap();
    }

    #[test]
    fn what_the_system_leaves_and_a_folder_of_the_person_are_not_the_old_layout() {
        let guard = tempfile::tempdir().unwrap();
        let (project, soma) = project_with_soma(Utf8Path::from_path(guard.path()).unwrap());
        std::fs::create_dir_all(soma.dir.join("Software")).unwrap();
        std::fs::write(soma.dir.join("Software/.DS_Store"), "x").unwrap();
        std::fs::write(soma.dir.join("Software/._soma.cmm"), "x").unwrap();
        std::fs::create_dir_all(soma.dir.join("Hardware")).unwrap();
        std::fs::write(soma.dir.join("Hardware/.gitkeep"), "").unwrap();
        // No Windows e no macOS, `Simulation/` abriria esta pasta.
        std::fs::create_dir_all(soma.dir.join("simulation")).unwrap();
        std::fs::write(soma.dir.join("simulation/estimulos.txt"), "1\n").unwrap();
        assert!(soma.legacy_dirs().is_empty(), "{:?}", soma.legacy_dirs());
        assert!(project.legacy_processors().is_empty());
        assert!(
            !project
                .issues()
                .iter()
                .any(|i| i.kind == IssueKind::LegacyLayout)
        );
    }

    #[test]
    fn what_the_conversion_could_not_move_does_not_block_the_build() {
        let guard = tempfile::tempdir().unwrap();
        let (mut project, soma) = project_with_soma(Utf8Path::from_path(guard.path()).unwrap());
        make_legacy(&mut project, &soma);
        // Antes da conversão, o build é tentado (e diz que falta convertê-la).
        assert!(soma.needs_conversion());
        assert_eq!(project.buildable_processors().len(), 1);
        // Um arquivo da pessoa cujo destino já existe fica em `Software/`.
        std::fs::write(soma.dir.join("Software/coef.txt"), "1\n").unwrap();
        std::fs::write(soma.dir.join("coef.txt"), "2\n").unwrap();
        let report = project.convert_layout().unwrap();
        assert_eq!(report.skipped.len(), 1, "{report:#?}");
        assert_eq!(report.remaining_dirs, [soma.dir.join("Software")]);
        assert!(!soma.needs_conversion());
        assert_eq!(soma.legacy_dirs(), [soma.dir.join("Software")]);
        let issue = project
            .issues()
            .into_iter()
            .find(|i| i.kind == IssueKind::LegacyLayout)
            .unwrap();
        assert!(
            issue.message.contains("builds from soma.cmm"),
            "{}",
            issue.message
        );
    }

    #[test]
    fn generated_files_follow_the_aurora_rule() {
        let bases = ["soma".to_owned(), "soma_v2".to_owned()];
        for name in [
            "soma.asm",
            "soma.v",
            "soma_tb.v",
            "soma_inst.mif",
            "soma_data.mif",
            "pc_soma_mem.txt",
            "soma_tb.vcd",
            "SOMA_V2.ASM",
            "cmm_log.txt",
            "trad_cmm.txt",
            "output_0.txt",
            "toma_12.txt",
        ] {
            assert!(is_generated(name, &bases), "{name}");
        }
        for name in [
            "soma.cmm",
            "input_0.txt",
            "cade_1.txt",
            "coef.txt",
            "outra.v",
            "output_.txt",
            "output_a.txt",
        ] {
            assert!(!is_generated(name, &bases), "{name}");
        }
    }

    #[test]
    fn name_lines_leave_the_source() {
        let (text, name) =
            without_name_lines("#PRNAME soma\n#NUBITS 23\n  #PRNAME outro\r\nint x;\n");
        assert_eq!(text, "#NUBITS 23\nint x;\n");
        assert_eq!(name.as_deref(), Some("soma"));
        let (text, name) = without_name_lines(
            "#pragma yanc prname filtro\n# pragma  yanc  prname x\n#pragma yanc nuioin 1\n",
        );
        assert_eq!(text, "#pragma yanc nuioin 1\n");
        assert_eq!(name.as_deref(), Some("filtro"));
        let (text, name) = without_name_lines("// #PRNAME soma\n#PRNAMEX y\nvoid main() {}");
        assert_eq!(text, "// #PRNAME soma\n#PRNAMEX y\nvoid main() {}");
        assert_eq!(name, None);
    }
}
