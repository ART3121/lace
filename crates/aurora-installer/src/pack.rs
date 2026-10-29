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

use crate::files::{self, FILES_FILE, FILES_SCHEMA, FileGroup, FilesManifest};
use crate::payload::{
    Chunk, ComponentInfo, INDEX_FILE, INDEX_SCHEMA, Index, PAYLOAD_DIR, ProfileInfo,
};

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
    /// Os perfis de instalação, o padrão primeiro; vazio num índice
    /// anterior a eles ([`Contents::profiles`]).
    #[serde(default)]
    pub profiles: Vec<ProfileInfo>,
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
    /// Faz parte do perfil padrão? O `bundle.py` grava; a AURORA anterior aos
    /// perfis precisa dele no índice.
    pub recommended: bool,
    /// Dependências.
    #[serde(default)]
    pub requires: Vec<String>,
    /// A descrição em outros idiomas, por código (`pt`).
    #[serde(default)]
    pub translations: BTreeMap<String, ComponentText>,
    /// Versão.
    pub version: String,
    /// Todos os arquivos que ele usa, relativos ao bundle, inclusive os que
    /// divide com outros.
    pub files: Vec<String>,
}

/// A descrição de um componente num idioma.
#[derive(Debug, Clone, Deserialize)]
pub struct ComponentText {
    /// O que ele faz.
    pub description: String,
}

impl Contents {
    /// Lê o índice.
    pub fn load(path: &Path) -> anyhow::Result<Contents> {
        let text =
            fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
        let contents: Contents =
            serde_json::from_str(&text).with_context(|| format!("Invalid {}", path.display()))?;
        if contents.schema != 1 {
            bail!("{}: unknown format {}", path.display(), contents.schema);
        }
        Ok(contents)
    }

    /// Os perfis, como [`Index::profiles`]: os do índice, ou um,
    /// `recommended`, num índice anterior a eles.
    pub fn profiles(&self) -> Vec<ProfileInfo> {
        self.index_without_chunks().profiles()
    }

    fn index_without_chunks(&self) -> Index {
        Index {
            schema: INDEX_SCHEMA,
            aurora_version: crate::AURORA_VERSION.to_owned(),
            bundle: self.bundle.clone(),
            platform: self.platform.clone(),
            profiles: self.profiles.clone(),
            components: self.infos(),
            chunks: Vec::new(),
        }
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

/// O nome do pedaço de cada grupo de [`groups`], na mesma ordem: o comum é
/// `aurora.tar.zst`; os outros, `c01.tar.zst`, `c02.tar.zst`...
fn chunk_names(groups: &[Group]) -> Vec<String> {
    let mut numbered = 0;
    groups
        .iter()
        .map(|g| {
            if g.components.is_empty() {
                "aurora.tar.zst".to_owned()
            } else {
                numbered += 1;
                format!("c{numbered:02}.tar.zst")
            }
        })
        .collect()
}

/// Onde fica o `aurora` na instalação, e o nome dele no pedaço sempre
/// instalado e no manifesto de arquivos: `bin/aurora.exe` no Windows,
/// `bin/aurora` nos outros.
pub fn aurora_entry(platform: &str) -> &'static str {
    if platform == "windows-x64" {
        "bin/aurora.exe"
    } else {
        "bin/aurora"
    }
}

/// O manifesto de arquivos ([`crate::files`]) do bundle em `toolchain`,
/// com o `aurora` em `aurora`: o SHA-256 de cada arquivo, nos grupos de
/// [`groups`], com o pedaço de cada um.
pub fn files_manifest(
    toolchain: &Path,
    contents: &Contents,
    aurora: &Path,
) -> anyhow::Result<FilesManifest> {
    let groups = groups(contents);
    let names = chunk_names(&groups);
    let mut out = Vec::new();
    for (group, chunk) in groups.into_iter().zip(names) {
        let mut entries = BTreeMap::new();
        if group.components.is_empty() {
            entries.insert(
                aurora_entry(&contents.platform).to_owned(),
                files::digest(aurora)?,
            );
        }
        for f in &group.files {
            entries.insert(format!("toolchain/{f}"), files::digest(&toolchain.join(f))?);
        }
        out.push(FileGroup {
            chunk,
            components: group.components,
            files: entries,
        });
    }
    Ok(FilesManifest {
        schema: FILES_SCHEMA,
        aurora_version: crate::AURORA_VERSION.to_owned(),
        bundle: contents.bundle.clone(),
        platform: contents.platform.clone(),
        groups: out,
    })
}

fn size(path: &Path) -> anyhow::Result<u64> {
    let meta = fs::symlink_metadata(path).with_context(|| format!("Reading {}", path.display()))?;
    Ok(if meta.file_type().is_symlink() {
        0
    } else {
        meta.len()
    })
}

fn ensure_empty(out: &Path) -> anyhow::Result<()> {
    if out.exists() && fs::read_dir(out)?.next().is_some() {
        bail!("{} already exists and is not empty", out.display());
    }
    fs::create_dir_all(out).with_context(|| format!("Creating {}", out.display()))
}

/// Um arquivo do pedaço: origem, nome no tar, e se precisa sair executável
/// seja qual for o modo da origem (o `bin/aurora`).
struct Entry {
    src: PathBuf,
    name: String,
    executable: bool,
}

fn write_chunk(out: &Path, entries: &[Entry], level: i32) -> anyhow::Result<()> {
    let file = fs::File::create(out).with_context(|| format!("Creating {}", out.display()))?;
    let mut encoder = zstd::Encoder::new(file, level)?;
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get() as u32);
    encoder.multithread(threads)?;
    let mut builder = tar::Builder::new(encoder);
    builder.follow_symlinks(false);
    builder.mode(tar::HeaderMode::Deterministic);
    for e in entries {
        if e.executable {
            let data =
                fs::File::open(&e.src).with_context(|| format!("Opening {}", e.src.display()))?;
            let mut header = tar::Header::new_gnu();
            header.set_size(data.metadata()?.len());
            header.set_mode(0o755);
            header.set_mtime(0);
            header.set_cksum();
            builder
                .append_data(&mut header, &e.name, data)
                .with_context(|| format!("Packing {}", e.src.display()))?;
        } else {
            builder
                .append_path_with_name(&e.src, &e.name)
                .with_context(|| format!("Packing {}", e.src.display()))?;
        }
    }
    builder.into_inner()?.finish()?;
    Ok(())
}

/// Texto que acompanha o instalador de Linux e macOS.
const README: &str = "\
AURORA {version} installer ({platform}), tool bundle {bundle}

  ./install                      Guided installation in the terminal
  ./install --yes                The default profile, no questions
  ./install --yes --profile NAME Another profile (see --list)
  ./install --list               The profiles, the components and the size of each
  ./install --help               All options

Default: ~/.local/share/aurora, with the link ~/.local/bin/aurora.
To remove it, with the bundle: aurora uninstall (or <installation folder>/uninstall.sh)
";

/// Monta o instalador de Linux e macOS em `out`: o executável `install`, o
/// `payload/` e o `LEIA-ME.txt`. O payload leva também o manifesto de
/// arquivos (`files.json`), que vai junto no pedaço sempre instalado.
pub fn tui(
    toolchain: &Path,
    contents: &Contents,
    aurora: &Path,
    installer: &Path,
    out: &Path,
    level: i32,
) -> anyhow::Result<Index> {
    ensure_empty(out)?;
    let payload = out.join(PAYLOAD_DIR);
    fs::create_dir_all(&payload)?;
    let install = out.join("install");
    fs::copy(installer, &install).with_context(|| format!("Copying {}", installer.display()))?;
    set_executable(&install)?;

    let manifest = files_manifest(toolchain, contents, aurora)?;
    let manifest_path = payload.join(FILES_FILE);
    manifest.save(&manifest_path)?;

    let groups = groups(contents);
    let names = chunk_names(&groups);
    let mut chunks = Vec::new();
    for (group, file) in groups.into_iter().zip(names) {
        let mut entries: Vec<Entry> = group
            .files
            .iter()
            .map(|f| Entry {
                src: toolchain.join(f),
                name: format!("toolchain/{f}"),
                executable: false,
            })
            .collect();
        if group.components.is_empty() {
            entries.insert(
                0,
                Entry {
                    src: aurora.to_owned(),
                    name: aurora_entry(&contents.platform).to_owned(),
                    executable: true,
                },
            );
            entries.push(Entry {
                src: manifest_path.clone(),
                name: files::INSTALLED.to_owned(),
                executable: false,
            });
        }
        let mut bytes = 0;
        for e in &entries {
            bytes += size(&e.src)?;
        }
        let path = payload.join(&file);
        write_chunk(&path, &entries, level)?;
        chunks.push(Chunk {
            file,
            components: group.components,
            size: bytes,
            entries: entries.len() as u64,
            download: size(&path)?,
        });
    }

    let index = Index {
        chunks,
        ..contents.index_without_chunks()
    };
    fs::write(
        payload.join(INDEX_FILE),
        serde_json::to_string_pretty(&index)? + "\n",
    )?;
    fs::write(
        out.join("LEIA-ME.txt"),
        README
            .replace("{version}", &index.aurora_version)
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
    let name = dir.file_name().context("Directory has no name")?;
    let file = fs::File::create(out).with_context(|| format!("Creating {}", out.display()))?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.follow_symlinks(false);
    builder.mode(tar::HeaderMode::Deterministic);
    builder.append_dir_all(name, dir)?;
    builder.into_inner()?.finish()?;
    Ok(())
}

/// Copia o payload do instalador em `installer` para `dir` com os nomes que a
/// release publica ([`crate::payload::release_asset`]): o índice, o
/// manifesto de arquivos e cada pedaço. Devolve os arquivos gravados.
pub fn release_assets(installer: &Path, dir: &Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let payload = installer.join(PAYLOAD_DIR);
    let index = Index::load(&payload)?;
    fs::create_dir_all(dir).with_context(|| format!("Creating {}", dir.display()))?;
    let files = [crate::payload::INDEX_FILE, FILES_FILE]
        .into_iter()
        .map(str::to_owned)
        .chain(index.chunks.iter().map(|c| c.file.clone()));
    let mut written = Vec::new();
    for file in files {
        let dest = dir.join(crate::payload::release_asset(
            &index.aurora_version,
            &index.platform,
            &file,
        ));
        fs::copy(payload.join(&file), &dest)
            .with_context(|| format!("Copying {file} to {}", dest.display()))?;
        written.push(dest);
    }
    Ok(written)
}

/// Nome de componente no Inno Setup: sem hífen (o Inno lê `Components:`
/// como expressão).
pub fn inno_name(name: &str) -> String {
    name.replace('-', "_")
}

/// O componente de cima de cada um, na árvore do Inno Setup: quem exige
/// exatamente um outro fica embaixo dele (marcar o filho marca o pai,
/// desmarcar o pai desmarca o filho). Os pares são `(nome, requires)`. O Inno
/// não expressa dependência de mais de um componente.
fn inno_parents<'a>(
    components: &[(&'a str, &'a [String])],
) -> anyhow::Result<BTreeMap<&'a str, &'a str>> {
    let mut parent = BTreeMap::new();
    for &(name, requires) in components {
        match requires {
            [] => {}
            [p] if components.iter().any(|&(n, _)| n == p.as_str()) => {
                parent.insert(name, p.as_str());
            }
            _ => bail!("{name}: Inno Setup can only express a dependency on a single component"),
        }
    }
    Ok(parent)
}

fn inno_full_name(parent: &BTreeMap<&str, &str>, name: &str) -> String {
    match parent.get(name) {
        Some(p) => format!("{}\\{}", inno_name(p), inno_name(name)),
        None => inno_name(name),
    }
}

/// O nome completo de cada componente no Inno Setup, com o de cima quando
/// há: `icarus\cocotb`, `surfer_aurora`. É o nome que `[Components]` declara
/// e que o `/COMPONENTS=` do assistente aceita. Os pares são `(nome,
/// requires)`, como no índice do payload.
pub fn inno_component_names<'a>(
    components: &[(&'a str, &'a [String])],
) -> anyhow::Result<BTreeMap<&'a str, String>> {
    let parent = inno_parents(components)?;
    Ok(components
        .iter()
        .map(|&(name, _)| (name, inno_full_name(&parent, name)))
        .collect())
}

/// O tipo de instalação Custom no Inno Setup. Por dentro é `avancada`, o
/// nome da instalação Advanced de antes dos perfis, que o `aurora update` das
/// versões anteriores passa ao assistente novo em `/TYPE=`.
pub const CUSTOM_TYPE: &str = "avancada";

/// Os idiomas do assistente do Windows: o código das traduções de
/// `bundle/components.json` e o nome do idioma no `[Languages]` do
/// `installer/windows/aurora.iss`, que são o mesmo. O inglês é o texto base.
pub const INNO_LANGUAGES: [&str; 2] = ["en", "pt"];

/// O nome da AURORA na lista de componentes, sempre marcado.
const AURORA_LABEL: &str = "AURORA (CLI & Core)";

/// Os textos fixos do `components.iss` em cada idioma.
struct InnoTexts {
    recommended: &'static str,
    custom_type: &'static str,
    /// O perfil escolhido: o nome, quantos aplicativos marca e de quantos.
    profile: fn(&str, usize, usize) -> String,
    /// Para quem ele é, depois do [`profile`](Self::profile).
    audience: fn(&str) -> String,
    /// O Custom, com o total de aplicativos.
    custom: fn(usize) -> String,
}

fn inno_texts(lang: &str) -> InnoTexts {
    match lang {
        "pt" => InnoTexts {
            recommended: "Recomendado",
            custom_type: "Personalizada (avançada): você escolhe os componentes",
            profile: |label, marked, total| {
                format!(
                    "O {label} instala a {AURORA_LABEL} e os {marked} aplicativos marcados \
                     abaixo, de {total}."
                )
            },
            audience: |audience| format!(" Para {}.", crate::lowercase_first(audience)),
            custom: |total| {
                format!(
                    "A instalação personalizada é para uso avançado, para quem sabe de quais \
                     componentes precisa: marque abaixo os aplicativos a instalar, de {total}. \
                     A {AURORA_LABEL} é sempre instalada."
                )
            },
        },
        _ => InnoTexts {
            recommended: "Recommended",
            custom_type: "Custom (advanced): choose the components yourself",
            profile: |label, marked, total| {
                format!(
                    "{label} installs {AURORA_LABEL} and the {marked} apps marked below, of {total}."
                )
            },
            audience: |audience| format!(" For {}.", crate::lowercase_first(audience)),
            custom: |total| {
                format!(
                    "The Custom installation is for advanced use, for users who know which \
                     components they need: mark below the apps to install, of the {total}. \
                     {AURORA_LABEL} is always installed."
                )
            },
        },
    }
}

/// A seção `[CustomMessages]` do `components.iss`, com os textos dos tipos,
/// dos componentes e do perfil em cada idioma de [`INNO_LANGUAGES`] (o que
/// não tem tradução fica em inglês), e a função `ProfileDetail`: para o
/// nome de um tipo de instalação, a linha que a página de componentes
/// mostra entre a caixa dos perfis e a lista.
fn inno_messages(profiles: &[ProfileInfo], contents: &Contents) -> String {
    // Uma mensagem vai até o fim da linha; o `%` abre um argumento.
    let one_line = |text: &str| text.replace(['\r', '\n'], " ").replace('%', "%%");
    let total = contents.components.len();
    let mut msgs = String::from("\n[CustomMessages]\n");
    for lang in INNO_LANGUAGES {
        let texts = inno_texts(lang);
        let mut put = |key: &str, value: &str| {
            msgs.push_str(&format!("{lang}.{key}={}\n", one_line(value)));
        };
        for (i, p) in profiles.iter().enumerate() {
            let (label, description, audience) = match p.translations.get(lang) {
                Some(t) => (&t.label, &t.description, &t.audience),
                None => (&p.label, &p.description, &p.audience),
            };
            let shown = if i == 0 {
                format!("{label} ({})", texts.recommended)
            } else {
                label.clone()
            };
            put(
                &format!("Type_{}", inno_name(&p.name)),
                &format!("{shown}: {}", crate::lowercase_first(description)),
            );
            let marked = contents
                .components
                .iter()
                .filter(|c| p.components.contains(&c.name))
                .count();
            let mut detail = (texts.profile)(&shown, marked, total);
            if !audience.is_empty() {
                detail.push_str(&(texts.audience)(audience));
            }
            put(&format!("Detail_{}", inno_name(&p.name)), &detail);
        }
        put("Type_Custom", texts.custom_type);
        put("Detail_Custom", &(texts.custom)(total));
        put("Comp_aurora", AURORA_LABEL);
        for c in &contents.components {
            let description = c
                .translations
                .get(lang)
                .map_or(&c.description, |t| &t.description);
            put(
                &format!("Comp_{}", inno_name(&c.name)),
                &format!("{}: {description}", c.label),
            );
        }
    }
    msgs.push_str(
        "\n[Code]\n{ O texto da página de componentes para o tipo escolhido. }\n\
         function ProfileDetail(const TypeName: String): String;\nbegin\n  \
         Result := CustomMessage('Detail_Custom');\n",
    );
    for p in profiles {
        let name = inno_name(&p.name);
        msgs.push_str(&format!(
            "  if TypeName = '{name}' then\n    Result := CustomMessage('Detail_{name}');\n"
        ));
    }
    msgs.push_str("end;\n");
    msgs
}

fn inno_quote(text: &str) -> String {
    text.replace('"', "\"\"")
}

/// Monta o estágio do instalador de Windows em `out`: `bin/aurora.exe`,
/// `common/` e `chunks/NN/` com os arquivos, e `components.iss` com as
/// seções `[Components]` e `[Files]` que o `installer/windows/aurora.iss`
/// inclui. O `common/` leva o manifesto de arquivos (`toolchain/files.json`),
/// o mesmo que o payload de [`tui`] gera desse bundle.
pub fn inno(
    toolchain: &Path,
    contents: &Contents,
    aurora: &Path,
    out: &Path,
    web: Option<&Path>,
) -> anyhow::Result<()> {
    ensure_empty(out)?;
    fs::create_dir_all(out.join("bin"))?;
    fs::copy(aurora, out.join("bin/aurora.exe"))
        .with_context(|| format!("Copying {}", aurora.display()))?;

    let pairs: Vec<(&str, &[String])> = contents
        .components
        .iter()
        .map(|c| (c.name.as_str(), c.requires.as_slice()))
        .collect();
    let parent = inno_parents(&pairs)?;
    let full = |name: &str| inno_full_name(&parent, name);

    // Os tipos de instalação: um por perfil, o padrão primeiro, e o Custom,
    // de marcar à mão.
    let profiles = contents.profiles();
    let mut iss = format!(
        "; Gerado por aurora-pack a partir do índice do bundle. Não editar.\n\n#define DefaultProfile \"{}\"\n\n[Types]\n",
        inno_quote(&profiles.first().map_or(String::new(), |p| p.label.clone()))
    );
    for p in &profiles {
        let name = inno_name(&p.name);
        iss.push_str(&format!(
            "Name: \"{name}\"; Description: \"{{cm:Type_{name}}}\"\n"
        ));
    }
    iss.push_str(&format!(
        "Name: \"{CUSTOM_TYPE}\"; Description: \"{{cm:Type_Custom}}\"; Flags: iscustom\n\n[Components]\n"
    ));
    let all_types: Vec<String> = profiles
        .iter()
        .map(|p| inno_name(&p.name))
        .chain([CUSTOM_TYPE.to_owned()])
        .collect();
    iss.push_str(&format!(
        "Name: \"aurora\"; Description: \"{{cm:Comp_aurora}}\"; Types: {}; Flags: fixed\n",
        all_types.join(" ")
    ));
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
    for c in &ordered {
        let mut line = format!(
            "Name: \"{}\"; Description: \"{{cm:Comp_{}}}\"",
            full(&c.name),
            inno_name(&c.name)
        );
        let types: Vec<String> = profiles
            .iter()
            .filter(|p| p.components.contains(&c.name))
            .map(|p| inno_name(&p.name))
            .collect();
        if !types.is_empty() {
            line.push_str(&format!("; Types: {}", types.join(" ")));
        }
        if parent.values().any(|p| *p == c.name) {
            line.push_str("; Flags: checkablealone");
        }
        // No assistente que baixa os aplicativos (`/DWeb`), o espaço de cada
        // um não vem do [Files]: vai declarado.
        let mut bytes = 0;
        for f in &c.files {
            bytes += size(&toolchain.join(f))?;
        }
        iss.push_str(&format!(
            "#ifdef Web\n{line}; ExtraDiskSpaceRequired: {bytes}\n#else\n{line}\n#endif\n"
        ));
    }

    // Os aplicativos que a pasta escolhida já tem (do assistente ou do
    // `aurora install`) ficam marcados numa reinstalação.
    iss.push_str("\n[Code]\nprocedure SelectInstalledComponents(const Dir: String);\nbegin\n");
    for c in &ordered {
        iss.push_str(&format!(
            "  if FileExists(Dir + '\\toolchain\\components\\{}.json') then\n    WizardSelectComponents('{}');\n",
            c.name,
            full(&c.name)
        ));
    }
    iss.push_str("end;\n");
    iss.push_str(&inno_messages(&profiles, contents));

    iss.push_str("\n[Files]\nSource: \"{#Stage}\\bin\\aurora.exe\"; DestDir: \"{app}\\bin\"; Components: aurora; Flags: ignoreversion\n");
    // No assistente que baixa os aplicativos, o bundle não vai dentro dele.
    iss.push_str("#ifndef Web\n");
    let mut numbered = 0;
    for group in groups(contents) {
        let (dir, expr) = if group.components.is_empty() {
            ("common".to_owned(), "aurora".to_owned())
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
            fs::copy(toolchain.join(f), &dest).with_context(|| format!("Copying {f}"))?;
        }
        iss.push_str(&format!(
            "Source: \"{{#Stage}}\\{dir}\\*\"; DestDir: \"{{app}}\"; Components: {expr}; Flags: ignoreversion recursesubdirs createallsubdirs\n"
        ));
    }
    iss.push_str("#endif\n");
    let manifest = files_manifest(toolchain, contents, aurora)?;
    let manifest_path = out.join("common").join(files::INSTALLED);
    fs::create_dir_all(manifest_path.parent().expect("tem pai"))?;
    manifest.save(&manifest_path)?;
    write_iss(&out.join("components.iss"), &iss)?;
    if let Some(installer) = web {
        write_iss(&out.join("web.iss"), &web_script(installer, &full)?)?;
    }
    Ok(())
}

/// Grava um script do Inno Setup, que lê UTF-8 com BOM (as descrições têm
/// acentos) e fins de linha do Windows.
fn write_iss(path: &Path, text: &str) -> anyhow::Result<()> {
    let mut bytes = b"\xef\xbb\xbf".to_vec();
    bytes.extend(text.replace('\n', "\r\n").into_bytes());
    fs::write(path, bytes).with_context(|| format!("Writing {}", path.display()))
}

/// O `web.iss` do assistente que baixa os aplicativos (`/DWeb`), a partir do
/// instalador de [`tui`] em `installer`, com os pedaços que a release
/// publica: `AddDownloads` põe na página de download o índice, o pedaço
/// sempre instalado e os pedaços dos componentes marcados, cada um com o
/// SHA-256 dele; `SelectedApps` dá os componentes marcados, com os nomes do
/// AURORA, para o `aurora setup`.
fn web_script(installer: &Path, full: &dyn Fn(&str) -> String) -> anyhow::Result<String> {
    let payload = installer.join(PAYLOAD_DIR);
    let index = Index::load(&payload)?;
    let asset =
        |file: &str| crate::payload::release_asset(&index.aurora_version, &index.platform, file);
    let mut iss = String::from(
        "; Gerado por aurora-pack a partir do payload da release. Não editar.\n\n[Code]\n\
         { O índice, o pedaço sempre instalado e os dos componentes marcados. }\n\
         procedure AddDownloads(Page: TDownloadWizardPage; const Base: String);\nbegin\n",
    );
    let line = |file: &str| -> anyhow::Result<String> {
        let name = asset(file);
        let sha = files::digest(&payload.join(file))?;
        Ok(format!("Page.Add(Base + '{name}', '{name}', '{sha}');"))
    };
    iss.push_str(&format!("  {}\n", line(INDEX_FILE)?));
    for chunk in &index.chunks {
        if chunk.components.is_empty() {
            iss.push_str(&format!("  {}\n", line(&chunk.file)?));
        } else {
            let expr = chunk
                .components
                .iter()
                .map(|c| full(c))
                .collect::<Vec<_>>()
                .join(" or ");
            iss.push_str(&format!(
                "  if WizardIsComponentSelected('{expr}') then\n    {}\n",
                line(&chunk.file)?
            ));
        }
    }
    iss.push_str(
        "end;\n\n{ Os componentes marcados, com os nomes da AURORA, separados por vírgula. }\n\
         function SelectedApps: String;\nbegin\n  Result := '';\n",
    );
    for c in &index.components {
        iss.push_str(&format!(
            "  if WizardIsComponentSelected('{}') then\n    Result := Result + ',{}';\n",
            full(&c.name),
            c.name
        ));
    }
    iss.push_str("  if Result <> '' then\n    Delete(Result, 1, 1);\nend;\n");
    Ok(iss)
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
            // Só o Yosys tem tradução: os outros ficam em inglês no `pt`.
            translations: if name == "yosys" {
                [(
                    "pt".to_owned(),
                    ComponentText {
                        description: "a síntese".into(),
                    },
                )]
                .into()
            } else {
                BTreeMap::new()
            },
            version: "1".into(),
            files: files.iter().map(|s| s.to_string()).collect(),
        };
        Contents {
            schema: 1,
            bundle: "teste".into(),
            platform: "linux-x64".into(),
            common: vec!["bundle.json".into()],
            profiles: vec![
                ProfileInfo {
                    name: "tudo".into(),
                    label: "Tudo".into(),
                    description: "todos os \"apps\"".into(),
                    audience: "quem usa o SAPHO".into(),
                    translations: [(
                        "pt".to_owned(),
                        crate::payload::ProfileText {
                            label: "Tudo".into(),
                            description: "todos os apps".into(),
                            audience: "quem usa o SAPHO, em português".into(),
                        },
                    )]
                    .into(),
                    components: ["icarus", "verilator", "yosys", "graphviz", "surfer-aurora"]
                        .map(String::from)
                        .to_vec(),
                },
                ProfileInfo {
                    name: "sim".into(),
                    label: "Sim".into(),
                    description: "só simular".into(),
                    audience: "quem só simula, na aula d'A AURORA".into(),
                    translations: Default::default(),
                    components: ["icarus", "surfer-aurora"].map(String::from).to_vec(),
                },
            ],
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
        let aurora = dir.path().join("aurora.exe");
        fs::write(&aurora, "exe").unwrap();
        let out = dir.path().join("stage");
        inno(&toolchain, &c, &aurora, &out, None).unwrap();
        let iss = fs::read_to_string(out.join("components.iss")).unwrap();
        assert!(iss.starts_with('\u{feff}'));
        let iss = iss.replace("\r\n", "\n");
        // Um tipo por perfil, o padrão primeiro, e o Custom; os textos vêm
        // das mensagens de cada idioma.
        assert!(
            iss.contains(
                "#define DefaultProfile \"Tudo\"\n\n[Types]\n\
                 Name: \"tudo\"; Description: \"{cm:Type_tudo}\"\n\
                 Name: \"sim\"; Description: \"{cm:Type_sim}\"\n\
                 Name: \"avancada\"; Description: \"{cm:Type_Custom}\"; Flags: iscustom\n"
            ),
            "{iss}"
        );
        assert!(
            iss.contains("Name: \"aurora\"; Description: \"{cm:Comp_aurora}\"; Types: tudo sim avancada; Flags: fixed"),
            "{iss}"
        );
        assert!(
            iss.contains("Name: \"yosys\"; Description: \"{cm:Comp_yosys}\"; Types: tudo; Flags: checkablealone"),
            "{iss}"
        );
        assert!(iss.contains("Name: \"yosys\\graphviz\""), "{iss}");
        assert!(
            iss.contains("Name: \"surfer_aurora\"; Description: \"{cm:Comp_surfer_aurora}\"; Types: tudo sim\n"),
            "{iss}"
        );
        assert!(
            iss.contains("Components: icarus or verilator or yosys or yosys\\graphviz;"),
            "{iss}"
        );
        // O padrão é o recomendado; o perfil diz quantos aplicativos marca e
        // para quem é; o Custom é o uso avançado.
        for line in [
            "en.Type_tudo=Tudo (Recommended): todos os \"apps\"",
            "pt.Type_tudo=Tudo (Recomendado): todos os apps",
            "en.Type_sim=Sim: só simular",
            "en.Detail_sim=Sim installs AURORA (CLI & Core) and the 2 apps marked below, of 5. For quem só simula, na aula d'A AURORA.",
            "pt.Detail_tudo=O Tudo (Recomendado) instala a AURORA (CLI & Core) e os 5 aplicativos marcados abaixo, de 5. Para quem usa o SAPHO, em português.",
            "en.Type_Custom=Custom (advanced): choose the components yourself",
            "pt.Type_Custom=Personalizada (avançada): você escolhe os componentes",
            "en.Comp_aurora=AURORA (CLI & Core)",
            "en.Comp_yosys=YOSYS: o \"yosys\"",
            "pt.Comp_yosys=YOSYS: a síntese",
            "pt.Comp_icarus=ICARUS: o \"icarus\"",
        ] {
            assert!(iss.contains(&format!("{line}\n")), "{line}\n{iss}");
        }
        assert!(
            iss.contains("en.Detail_Custom=The Custom installation is for advanced use"),
            "{iss}"
        );
        assert!(
            iss.contains(
                "  if TypeName = 'sim' then\n    Result := CustomMessage('Detail_sim');\n"
            ),
            "{iss}"
        );
        assert_eq!(
            fs::read_to_string(out.join("common/toolchain/bundle.json")).unwrap(),
            "bundle.json"
        );
        assert!(out.join("chunks/01/toolchain/oss/lib/libc").is_file());
    }

    #[test]
    fn inno_component_names_are_the_ones_the_script_declares() {
        let c = contents();
        let pairs: Vec<(&str, &[String])> = c
            .components
            .iter()
            .map(|c| (c.name.as_str(), c.requires.as_slice()))
            .collect();
        let names = inno_component_names(&pairs).unwrap();
        assert_eq!(names["icarus"], "icarus");
        assert_eq!(names["graphviz"], "yosys\\graphviz");
        assert_eq!(names["surfer-aurora"], "surfer_aurora");

        let two = vec!["icarus".to_owned(), "yosys".to_owned()];
        let invalid = [
            ("icarus", &[][..]),
            ("yosys", &[][..]),
            ("x", two.as_slice()),
        ];
        assert!(inno_component_names(&invalid).is_err());
    }
}
