//! Perfis de instalação e a seleção de componentes.

use std::collections::BTreeSet;

use anyhow::bail;

use crate::payload::Index;

/// Os componentes escolhidos, pelo nome.
pub type Selection = BTreeSet<String>;

/// Como o usuário escolhe os componentes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// O perfil padrão: os componentes marcados como recomendados.
    Recommended,
    /// O usuário marca exatamente o que quer.
    Advanced,
}

/// Os componentes do perfil recomendado.
pub fn recommended(index: &Index) -> Selection {
    index
        .components
        .iter()
        .filter(|c| c.recommended)
        .map(|c| c.name.clone())
        .collect()
}

/// Marca ou desmarca `name`, mantendo as dependências: marcar um componente
/// marca o que ele exige; desmarcar um desmarca quem o exige.
pub fn toggle(index: &Index, selection: &mut Selection, name: &str) {
    if selection.contains(name) {
        selection.remove(name);
        // Quem depende dele, direta ou indiretamente, sai junto.
        loop {
            let dependents: Vec<String> = index
                .components
                .iter()
                .filter(|c| selection.contains(&c.name))
                .filter(|c| c.requires.iter().any(|r| !selection.contains(r)))
                .map(|c| c.name.clone())
                .collect();
            if dependents.is_empty() {
                break;
            }
            for d in dependents {
                selection.remove(&d);
            }
        }
    } else {
        add_with_requirements(index, selection, name);
    }
}

fn add_with_requirements(index: &Index, selection: &mut Selection, name: &str) {
    if !selection.insert(name.to_owned()) {
        return;
    }
    if let Some(c) = index.component(name) {
        for r in c.requires.clone() {
            add_with_requirements(index, selection, &r);
        }
    }
}

/// A seleção de uma lista de nomes (da linha de comando). Nome desconhecido
/// é erro; as dependências entram, e são devolvidas à parte para avisar.
pub fn from_names(index: &Index, names: &[String]) -> anyhow::Result<(Selection, Vec<String>)> {
    let unknown: Vec<&String> = names
        .iter()
        .filter(|n| index.component(n).is_none())
        .collect();
    if !unknown.is_empty() {
        let known: Vec<&str> = index.components.iter().map(|c| c.name.as_str()).collect();
        bail!(
            "componente desconhecido: {} (os deste instalador: {})",
            unknown
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            known.join(", ")
        );
    }
    let mut selection = Selection::new();
    for n in names {
        add_with_requirements(index, &mut selection, n);
    }
    let added = selection
        .iter()
        .filter(|s| !names.contains(s))
        .cloned()
        .collect();
    Ok((selection, added))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::payload::{Chunk, ComponentInfo, INDEX_SCHEMA};

    pub(crate) fn index() -> Index {
        let c = |name: &str, recommended: bool, requires: &[&str]| ComponentInfo {
            name: name.into(),
            label: name.to_uppercase(),
            description: format!("o {name}"),
            recommended,
            requires: requires.iter().map(|s| s.to_string()).collect(),
            version: "1".into(),
        };
        let chunk = |file: &str, components: &[&str], size: u64| Chunk {
            file: file.into(),
            components: components.iter().map(|s| s.to_string()).collect(),
            size,
            entries: 1,
        };
        Index {
            schema: INDEX_SCHEMA,
            solar_version: "0.1.0".into(),
            bundle: "teste".into(),
            platform: "linux-x64".into(),
            components: vec![
                c("yanc", true, &[]),
                c("icarus", true, &[]),
                c("verilator", false, &[]),
                c("yosys", true, &[]),
                c("graphviz", true, &["yosys"]),
            ],
            chunks: vec![
                chunk("solar.tar.zst", &[], 10),
                chunk(
                    "c01.tar.zst",
                    &["icarus", "verilator", "yosys", "graphviz"],
                    100,
                ),
                chunk("c02.tar.zst", &["icarus"], 20),
                chunk("c03.tar.zst", &["verilator"], 30),
                chunk("c04.tar.zst", &["yosys"], 40),
                chunk("c05.tar.zst", &["graphviz"], 5),
                chunk("c06.tar.zst", &["yanc"], 1),
            ],
        }
    }

    fn set(names: &[&str]) -> Selection {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn recommended_profile_and_sizes() {
        let index = index();
        let rec = recommended(&index);
        assert_eq!(rec, set(&["graphviz", "icarus", "yanc", "yosys"]));
        // As bibliotecas em comum contam uma vez só.
        assert_eq!(index.size_of(&rec), 10 + 100 + 20 + 40 + 5 + 1);
        assert_eq!(index.size_of(&Selection::new()), 10);
        assert_eq!(index.component_size("icarus"), 120);
    }

    #[test]
    fn toggling_keeps_requirements() {
        let index = index();
        let mut sel = Selection::new();
        toggle(&index, &mut sel, "graphviz");
        assert_eq!(sel, set(&["graphviz", "yosys"]));
        toggle(&index, &mut sel, "yosys");
        assert!(sel.is_empty(), "desmarcar o Yosys desmarca o Graphviz");
        toggle(&index, &mut sel, "icarus");
        toggle(&index, &mut sel, "icarus");
        assert!(sel.is_empty());
    }

    #[test]
    fn names_from_the_command_line() {
        let index = index();
        let (sel, added) = from_names(&index, &["graphviz".into()]).unwrap();
        assert_eq!(sel, set(&["graphviz", "yosys"]));
        assert_eq!(added, ["yosys"]);
        let err = from_names(&index, &["gtkwave".into()])
            .unwrap_err()
            .to_string();
        assert!(err.contains("gtkwave") && err.contains("icarus"), "{err}");
    }
}
