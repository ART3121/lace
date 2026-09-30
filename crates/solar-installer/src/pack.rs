//! Montar os instaladores a partir de um bundle feito por
//! `scripts/bundle.py`.
//!
//! O `bundle.py` grava, ao lado do bundle, o índice `<bundle>.contents.json`:
//! que arquivo é de que componente. Aqui cada arquivo vai para o grupo do
//! conjunto exato de componentes que o usa. Um grupo vira um pedaço
//! `.tar.zst` do payload da TUI ([`tui`]) ou uma entrada `[Files]` do Inno
//! Setup ([`inno`]).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;

use crate::payload::{Chunk, ComponentInfo, INDEX_FILE, INDEX_SCHEMA, Index, PAYLOAD_DIR};

/// O índice `<bundle>.contents.json` do `bundle.py`.
#[derive(Debug, Clone, Deserialize)]
pub struct Contents {
    /// Versão do formato.
    pub schema: u32,
    /// Identificador do bundle.
    pub bundle: String,
    /// Plataforma.
    pub platform: String,
    /// Arquivos que vão com qualquer seleção (o `bundle.json`).
    pub common: Vec<String>,
    /// Os componentes, com os arquivos de cada um.
    pub components: Vec<ContentsComponent>,
}

/// Um componente no índice de conteúdo.
#[derive(Debug, Clone, Deserialize)]
pub struct ContentsComponent {
    /// Nome no manifesto.
    pub name: String,
    /// Nome para mostrar.
    pub label: String,
    /// O que ele faz.
    pub description: String,
    /// Faz parte do perfil recomendado?
    pub recommended: bool,
    /// Dependências.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Versão.
    pub version: String,
    /// Todos os arquivos que ele usa, relativos ao bundle, inclusive os que
    /// divide com outros.
    pub files: Vec<String>,
}

impl Contents {
    /// Lê o índice.
    pub fn load(path: &Path) -> anyhow::Result<Contents> {
        let text = fs::read_to_string(path).with_context(|| format!("lendo {}", path.display()))?;
        let contents: Contents =
            serde_json::from_str(&text).with_context(|| format!("{} inválido", path.display()))?;
        if contents.schema != 1 {
            bail!(
                "{}: formato {} desconhecido",
                path.display(),
                contents.schema
            );
        }
        Ok(contents)
    }

    fn infos(&self) -> Vec<ComponentInfo> {
        self.components
            .iter()
            .map(|c| ComponentInfo {
                name: c.name.clone(),
                label: c.label.clone(),
                description: c.description.clone(),
                recommended: c.recommended,
                requires: c.requires.clone(),
                version: c.version.clone(),
            })
            .collect()
    }
}

/// Arquivos usados pelo mesmo conjunto de componentes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// O conjunto; vazio para os arquivos comuns.
    pub components: Vec<String>,
    /// Os arquivos, relativos ao bundle.
    pub files: Vec<String>,
}

/// Agrupa os arquivos pelo conjunto de componentes que os usa. O grupo
/// comum (sem componentes) vem primeiro; os outros, dos mais
/// compartilhados para os de um componente só.
pub fn groups(contents: &Contents) -> Vec<Group> {
    let mut owners: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for f in &contents.common {
        owners.entry(f).or_default();
    }
    for c in &contents.components {
        for f in &c.files {
            if !contents.common.contains(f) {
                owners.entry(f).or_default().insert(&c.name);
            }
        }
    }
    let order: Vec<&str> = contents
        .components
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    let mut by_set: BTreeMap<(usize, Vec<usize>), Vec<String>> = BTreeMap::new();
    for (file, set) in owners {
        let mut ranks: Vec<usize> = set
            .iter()
            .map(|n| {
                order
                    .iter()
                    .position(|o| o == n)
                    .expect("componente do índice")
            })
            .collect();
        ranks.sort_unstable();
        let key = (
            if ranks.is_empty() {
                0
            } else {
                usize::MAX - ranks.len()
            },
            ranks,
        );
        by_set.entry(key).or_default().push(file.to_owned());
    }
    by_set
        .into_iter()
        .map(|((_, ranks), files)| Group {
            components: ranks.iter().map(|&r| order[r].to_owned()).collect(),
            files,
        })
        .collect()
}

fn size(path: &Path) -> anyhow::Result<u64> {
    let meta = fs::symlink_metadata(path).with_context(|| format!("lendo {}", path.display()))?;
    Ok(if meta.file_type().is_symlink() {
        0
    } else {
        meta.len()
    })
}

fn ensure_empty(out: &Path) -> anyhow::Result<()> {
    if out.exists() && fs::read_dir(out)?.next().is_some() {
        bail!("{} já existe e não está vazio", out.display());
    }
    fs::create_dir_all(out).with_context(|| format!("criando {}", out.display()))
}

/// Um arquivo do pedaço: origem, nome no tar, e se precisa sair executável
/// seja qual for o modo da origem (o `bin/solar`).
struct Entry {
    src: PathBuf,
    name: String,
    executable: bool,
}

fn write_chunk(out: &Path, entries: &[Entry], level: i32) -> anyhow::Result<()> {
    let file = fs::File::create(out).with_context(|| format!("criando {}", out.display()))?;
    let mut encoder = zstd::Encoder::new(file, level)?;
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get() as u32);
    encoder.multithread(threads)?;
    let mut builder = tar::Builder::new(encoder);
    builder.follow_symlinks(false);
    builder.mode(tar::HeaderMode::Deterministic);
    for e in entries {
        if e.executable {
            let data =
                fs::File::open(&e.src).with_context(|| format!("abrindo {}", e.src.display()))?;
            let mut header = tar::Header::new_gnu();
            header.set_size(data.metadata()?.len());
            header.set_mode(0o755);
            header.set_mtime(0);
            header.set_cksum();
            builder
                .append_data(&mut header, &e.name, data)
                .with_context(|| format!("empacotando {}", e.src.display()))?;
        } else {
            builder
                .append_path_with_name(&e.src, &e.name)
                .with_context(|| format!("empacotando {}", e.src.display()))?;
        }
    }
    builder.into_inner()?.finish()?;
    Ok(())
}

/// Texto que acompanha o instalador de Linux e macOS.
const README: &str = "\
Instalador do Solar {version} ({platform}), bundle de ferramentas {bundle}

  ./install              instalação guiada no terminal
  ./install --yes        perfil recomendado, sem perguntas
  ./install --list       os componentes e o tamanho de cada um
  ./install --help       todas as opções

Padrão: ~/.local/share/solar, com o atalho ~/.local/bin/solar.
Para remover: <pasta da instalação>/uninstall.sh
";

/// Monta o instalador de Linux e macOS em `out`: o executável `install`, o
/// `payload/` e o `LEIA-ME.txt`.
pub fn tui(
    toolchain: &Path,
    contents: &Contents,
    solar: &Path,
    installer: &Path,
    out: &Path,
    level: i32,
) -> anyhow::Result<Index> {
    ensure_empty(out)?;
    let payload = out.join(PAYLOAD_DIR);
    fs::create_dir_all(&payload)?;
    let install = out.join("install");
    fs::copy(installer, &install).with_context(|| format!("copiando {}", installer.display()))?;
    set_executable(&install)?;

    let mut chunks = Vec::new();
    let mut numbered = 0;
    for group in groups(contents) {
        let mut entries: Vec<Entry> = group
            .files
            .iter()
            .map(|f| Entry {
                src: toolchain.join(f),
                name: format!("toolchain/{f}"),
                executable: false,
            })
            .collect();
        let file = if group.components.is_empty() {
            entries.insert(
                0,
                Entry {
                    src: solar.to_owned(),
                    name: "bin/solar".to_owned(),
                    executable: true,
                },
            );
            "solar.tar.zst".to_owned()
        } else {
            numbered += 1;
            format!("c{numbered:02}.tar.zst")
        };
        let mut bytes = 0;
        for e in &entries {
            bytes += size(&e.src)?;
        }
        write_chunk(&payload.join(&file), &entries, level)?;
        chunks.push(Chunk {
            file,
            components: group.components,
            size: bytes,
            entries: entries.len() as u64,
        });
    }

    let index = Index {
        schema: INDEX_SCHEMA,
        solar_version: crate::SOLAR_VERSION.to_owned(),
        bundle: contents.bundle.clone(),
        platform: contents.platform.clone(),
        components: contents.infos(),
        chunks,
    };
    fs::write(
        payload.join(INDEX_FILE),
        serde_json::to_string_pretty(&index)? + "\n",
    )?;
    fs::write(
        out.join("LEIA-ME.txt"),
        README
            .replace("{version}", &index.solar_version)
            .replace("{platform}", &index.platform)
            .replace("{bundle}", &index.bundle),
    )?;
    Ok(index)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

/// Empacota o diretório `dir` num `.tar.gz`, com o nome dele como pasta de
/// topo.
pub fn tar_gz(dir: &Path, out: &Path) -> anyhow::Result<()> {
    let name = dir.file_name().context("diretório sem nome")?;
    let file = fs::File::create(out).with_context(|| format!("criando {}", out.display()))?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.follow_symlinks(false);
    builder.mode(tar::HeaderMode::Deterministic);
    builder.append_dir_all(name, dir)?;
    builder.into_inner()?.finish()?;
    Ok(())
}

/// Nome de componente no Inno Setup: sem hífen (o Inno lê `Components:`
/// como expressão).
pub fn inno_name(name: &str) -> String {
    name.replace('-', "_")
}

/// "a, b e c".
fn human_list(items: &[&str]) -> String {
    match items {
        [] => "nenhum componente".into(),
        [one] => (*one).into(),
        [rest @ .., last] => format!("{} e {last}", rest.join(", ")),
    }
}

fn inno_quote(text: &str) -> String {
    text.replace('"', "\"\"")
}

/// Monta o estágio do instalador de Windows em `out`: `bin/solar.exe`,
/// `common/` e `chunks/NN/` com os arquivos, e `components.iss` com as
/// seções `[Components]` e `[Files]` que o `installer/windows/solar.iss`
/// inclui.
pub fn inno(toolchain: &Path, contents: &Contents, solar: &Path, out: &Path) -> anyhow::Result<()> {
    ensure_empty(out)?;
    fs::create_dir_all(out.join("bin"))?;
    fs::copy(solar, out.join("bin/solar.exe"))
        .with_context(|| format!("copiando {}", solar.display()))?;

    // Um componente que exige exatamente um outro fica embaixo dele na
    // árvore: marcar o filho marca o pai, desmarcar o pai desmarca o filho.
    let names: Vec<&str> = contents
        .components
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    let mut parent: BTreeMap<&str, &str> = BTreeMap::new();
    for c in &contents.components {
        match c.requires.as_slice() {
            [] => {}
            [p] if names.contains(&p.as_str()) => {
                parent.insert(&c.name, p);
            }
            _ => bail!(
                "{}: o Inno Setup só expressa a dependência de um componente só",
                c.name
            ),
        }
    }
    let full = |name: &str| -> String {
        match parent.get(name) {
            Some(p) => format!("{}\\{}", inno_name(p), inno_name(name)),
            None => inno_name(name),
        }
    };

    let recommended: Vec<&str> = contents
        .components
        .iter()
        .filter(|c| c.recommended)
        .map(|c| c.label.as_str())
        .collect();
    let mut iss = format!(
        "; Gerado por solar-pack a partir do índice do bundle. Não editar.\n\n#define Recommended \"{}\"\n\n[Components]\n",
        inno_quote(&human_list(&recommended))
    );
    iss.push_str(
        "Name: \"solar\"; Description: \"Solar (linha de comando)\"; Types: recomendada avancada; Flags: fixed\n",
    );
    let mut ordered: Vec<&ContentsComponent> = Vec::new();
    for c in contents
        .components
        .iter()
        .filter(|c| !parent.contains_key(c.name.as_str()))
    {
        ordered.push(c);
        ordered.extend(
            contents
                .components
                .iter()
                .filter(|k| parent.get(k.name.as_str()) == Some(&c.name.as_str())),
        );
    }
    for c in ordered {
        let mut line = format!(
            "Name: \"{}\"; Description: \"{}: {}\"",
            full(&c.name),
            inno_quote(&c.label),
            inno_quote(&c.description)
        );
        if c.recommended {
            line.push_str("; Types: recomendada");
        }
        if parent.values().any(|p| *p == c.name) {
            line.push_str("; Flags: checkablealone");
        }
        iss.push_str(&line);
        iss.push('\n');
    }

    iss.push_str("\n[Files]\nSource: \"{#Stage}\\bin\\solar.exe\"; DestDir: \"{app}\\bin\"; Components: solar; Flags: ignoreversion\n");
    let mut numbered = 0;
    for group in groups(contents) {
        let (dir, expr) = if group.components.is_empty() {
            ("common".to_owned(), "solar".to_owned())
        } else {
            numbered += 1;
            (
                format!("chunks\\{numbered:02}"),
                group
                    .components
                    .iter()
                    .map(|c| full(c))
                    .collect::<Vec<_>>()
                    .join(" or "),
            )
        };
        let base = out.join(dir.replace('\\', "/")).join("toolchain");
        for f in &group.files {
            let dest = base.join(f);
            fs::create_dir_all(dest.parent().expect("tem pai"))?;
            fs::copy(toolchain.join(f), &dest).with_context(|| format!("copiando {f}"))?;
        }
        iss.push_str(&format!(
            "Source: \"{{#Stage}}\\{dir}\\*\"; DestDir: \"{{app}}\"; Components: {expr}; Flags: ignoreversion recursesubdirs createallsubdirs\n"
        ));
    }
    // O Inno Setup lê UTF-8 com BOM; as descrições têm acentos.
    let mut bytes = b"\xef\xbb\xbf".to_vec();
    bytes.extend(iss.replace('\n', "\r\n").into_bytes());
    fs::write(out.join("components.iss"), bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents() -> Contents {
        let c = |name: &str, requires: &[&str], files: &[&str]| ContentsComponent {
            name: name.into(),
            label: name.to_uppercase(),
            description: format!("o \"{name}\""),
            recommended: name != "verilator",
            requires: requires.iter().map(|s| s.to_string()).collect(),
            version: "1".into(),
            files: files.iter().map(|s| s.to_string()).collect(),
        };
        Contents {
            schema: 1,
            bundle: "teste".into(),
            platform: "linux-x64".into(),
            common: vec!["bundle.json".into()],
            components: vec![
                c(
                    "icarus",
                    &[],
                    &["oss/lib/libc", "oss/bin/iverilog", "components/icarus.json"],
                ),
                c(
                    "verilator",
                    &[],
                    &[
                        "oss/lib/libc",
                        "oss/bin/verilator",
                        "components/verilator.json",
                    ],
                ),
                c(
                    "yosys",
                    &[],
                    &[
                        "oss/lib/libc",
                        "oss/lib/libtcl",
                        "oss/bin/yosys",
                        "components/yosys.json",
                    ],
                ),
                c(
                    "graphviz",
                    &["yosys"],
                    &[
                        "oss/lib/libc",
                        "oss/lib/libtcl",
                        "oss/bin/dot",
                        "components/graphviz.json",
                    ],
                ),
                c(
                    "surfer-aurora",
                    &[],
                    &[
                        "surfer-aurora/surfer-aurora",
                        "components/surfer-aurora.json",
                    ],
                ),
            ],
        }
    }

    #[test]
    fn files_are_grouped_by_the_components_that_use_them() {
        let groups = groups(&contents());
        let summary: Vec<(Vec<&str>, Vec<&str>)> = groups
            .iter()
            .map(|g| {
                (
                    g.components.iter().map(String::as_str).collect(),
                    g.files.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(summary[0], (vec![], vec!["bundle.json"]));
        assert_eq!(
            summary[1],
            (
                vec!["icarus", "verilator", "yosys", "graphviz"],
                vec!["oss/lib/libc"]
            )
        );
        assert_eq!(
            summary[2],
            (vec!["yosys", "graphviz"], vec!["oss/lib/libtcl"])
        );
        // Cada arquivo está em exatamente um grupo.
        let all: Vec<&String> = groups.iter().flat_map(|g| &g.files).collect();
        let unique: BTreeSet<&&String> = all.iter().collect();
        assert_eq!(all.len(), unique.len());
        assert_eq!(all.len(), 13);
    }

    #[test]
    fn inno_script_nests_dependencies_and_quotes_text() {
        let dir = tempfile::tempdir().unwrap();
        let toolchain = dir.path().join("toolchain");
        let c = contents();
        for f in c
            .common
            .iter()
            .chain(c.components.iter().flat_map(|c| &c.files))
        {
            let p = toolchain.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, f).unwrap();
        }
        let solar = dir.path().join("solar.exe");
        fs::write(&solar, "exe").unwrap();
        let out = dir.path().join("stage");
        inno(&toolchain, &c, &solar, &out).unwrap();
        let iss = fs::read_to_string(out.join("components.iss")).unwrap();
        assert!(iss.starts_with('\u{feff}'));
        let iss = iss.replace("\r\n", "\n");
        assert!(iss.contains("Name: \"yosys\"; Description: \"YOSYS: o \"\"yosys\"\"\"; Types: recomendada; Flags: checkablealone"), "{iss}");
        assert!(iss.contains("Name: \"yosys\\graphviz\""), "{iss}");
        assert!(
            iss.contains("#define Recommended \"ICARUS, YOSYS, GRAPHVIZ e SURFER-AURORA\""),
            "{iss}"
        );
        assert!(iss.contains("Name: \"surfer_aurora\""), "{iss}");
        assert!(
            iss.contains("Components: icarus or verilator or yosys or yosys\\graphviz;"),
            "{iss}"
        );
        assert!(
            !iss.contains(
                "Name: \"verilator\"; Description: \"VERILATOR: o \"\"verilator\"\"\"; Types"
            ),
            "{iss}"
        );
        assert_eq!(
            fs::read_to_string(out.join("common/toolchain/bundle.json")).unwrap(),
            "bundle.json"
        );
        assert!(out.join("chunks/01/toolchain/oss/lib/libc").is_file());
    }
}
