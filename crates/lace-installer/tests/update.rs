//! Duas versões de um bundle pequeno: instala a primeira e atualiza para a
//! segunda trocando só o que mudou.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lace_installer::add::{ChunkSource, LocalPayload};
use lace_installer::files::{self, FilesManifest};
use lace_installer::install::{self, Event, Receipt, Target};
use lace_installer::pack::{self, Contents};
use lace_installer::payload::{Chunk, Index, PAYLOAD_DIR};
use lace_installer::plan;
use lace_installer::update;
use sha2::{Digest, Sha256};

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn platform() -> &'static str {
    lace_core::Platform::current().unwrap().as_str()
}

/// Um componente: nome, o que exige, o executável e os arquivos (caminho no
/// bundle e conteúdo). O executável vai no manifesto dele com o hash do
/// conteúdo, ou com um hash errado para forçar a conferência a falhar.
struct Spec<'a> {
    name: &'a str,
    requires: &'a [&'a str],
    exe: &'a str,
    files: &'a [(&'a str, &'a str)],
}

/// Monta o bundle `specs` e empacota o instalador em `root/<name>`.
fn build(root: &Path, name: &str, lace: &str, specs: &[Spec], wrong_hash: Option<&str>) -> PathBuf {
    let toolchain = root.join(format!("{name}-toolchain"));
    let write = |rel: &str, data: &str| {
        let p = toolchain.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, data).unwrap();
    };
    write(
        "bundle.json",
        &format!(
            r#"{{"schema": 2, "bundle": "{name}", "platform": "{}"}}"#,
            platform()
        ),
    );
    let mut contents_components = Vec::new();
    for s in specs {
        let mut paths = Vec::new();
        let mut exe_hash = String::new();
        for (rel, data) in s.files {
            write(rel, data);
            paths.push(rel.to_string());
            if *rel == s.exe {
                exe_hash = sha256_hex(data.as_bytes());
            }
        }
        if wrong_hash == Some(s.name) {
            exe_hash = "0".repeat(64);
        }
        let manifest = serde_json::json!({
            "name": s.name, "version": "1", "dir": "oss", "source": "teste",
            "files": { s.exe: exe_hash },
        });
        write(&format!("components/{}.json", s.name), &manifest.to_string());
        paths.push(format!("components/{}.json", s.name));
        contents_components.push(serde_json::json!({
            "name": s.name, "label": s.name.to_uppercase(), "description": format!("o {}", s.name),
            "recommended": true, "requires": s.requires, "version": "1",
            "files": paths,
        }));
    }
    let contents = serde_json::json!({
        "schema": 1, "bundle": name, "platform": platform(),
        "common": ["bundle.json"], "components": contents_components,
    });
    let contents_path = root.join(format!("{name}.contents.json"));
    fs::write(&contents_path, contents.to_string()).unwrap();
    let lace_path = root.join(format!("{name}-lace"));
    fs::write(&lace_path, lace).unwrap();
    let installer = root.join(format!("{name}-installer"));
    fs::write(&installer, "instalador").unwrap();
    let out = root.join(name);
    let contents = Contents::load(&contents_path).unwrap();
    pack::tui(&toolchain, &contents, &lace_path, &installer, &out, 3).unwrap();
    out
}

fn v1(root: &Path) -> PathBuf {
    build(
        root,
        "v1",
        "lace 1",
        &[
            Spec {
                name: "icarus",
                requires: &[],
                exe: "oss/bin/iverilog",
                files: &[
                    ("oss/bin/iverilog", "iverilog 1"),
                    ("oss/lib/libc", "libc"),
                    ("oss/share/icarus/old.txt", "sai na v2"),
                ],
            },
            Spec {
                name: "yosys",
                requires: &[],
                exe: "oss/bin/yosys",
                files: &[("oss/bin/yosys", "yosys 1"), ("oss/lib/libc", "libc")],
            },
            Spec {
                name: "graphviz",
                requires: &["yosys"],
                exe: "oss/bin/dot",
                files: &[("oss/bin/dot", "dot 1"), ("oss/lib/libc", "libc")],
            },
        ],
        None,
    )
}

/// A v2: o Icarus muda (o executável, um arquivo novo e um que sai), o Yosys
/// e a biblioteca dividida ficam iguais, e o `lace` muda.
fn v2(root: &Path, name: &str, wrong_hash: Option<&str>) -> PathBuf {
    build(
        root,
        name,
        "lace 2",
        &[
            Spec {
                name: "icarus",
                requires: &[],
                exe: "oss/bin/iverilog",
                files: &[
                    ("oss/bin/iverilog", "iverilog 2"),
                    ("oss/lib/libc", "libc"),
                    ("oss/share/icarus/new.txt", "entra na v2"),
                ],
            },
            Spec {
                name: "yosys",
                requires: &[],
                exe: "oss/bin/yosys",
                files: &[("oss/bin/yosys", "yosys 1"), ("oss/lib/libc", "libc")],
            },
            Spec {
                name: "graphviz",
                requires: &["yosys"],
                exe: "oss/bin/dot",
                files: &[("oss/bin/dot", "dot 1"), ("oss/lib/libc", "libc")],
            },
        ],
        wrong_hash,
    )
}

/// Os pedaços do payload, contando quais foram pedidos.
struct Counting {
    inner: LocalPayload,
    fetched: Mutex<Vec<String>>,
}

impl ChunkSource for Counting {
    fn fetch(&self, chunk: &Chunk, downloaded: &mut dyn FnMut(u64)) -> anyhow::Result<PathBuf> {
        self.fetched.lock().unwrap().push(chunk.file.clone());
        self.inner.fetch(chunk, downloaded)
    }
}

fn counting(installer: &Path) -> Counting {
    Counting {
        inner: LocalPayload(installer.join(PAYLOAD_DIR)),
        fetched: Mutex::new(Vec::new()),
    }
}

/// Todos os arquivos sob `dir`, com o conteúdo.
fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, fs::read(&p).unwrap());
            }
        }
    }
    out
}

struct Installed {
    _dir: tempfile::TempDir,
    root: PathBuf,
    prefix: PathBuf,
}

/// Instala a v1 com o Icarus e o Yosys.
fn install_v1() -> Installed {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let installer = v1(&root);
    let payload = installer.join(PAYLOAD_DIR);
    let index = Index::load(&payload).unwrap();
    let prefix = root.join("instalacao");
    let target = Target {
        prefix: prefix.clone(),
        link: None,
    };
    let selection = plan::from_names(&index, &["icarus".into(), "yosys".into()])
        .unwrap()
        .0;
    install::install(&payload, &index, &selection, &target, |_| {}).unwrap();
    Installed {
        _dir: dir,
        root,
        prefix,
    }
}

fn load_new(installer: &Path) -> (Index, FilesManifest) {
    let payload = installer.join(PAYLOAD_DIR);
    (
        Index::load(&payload).unwrap(),
        FilesManifest::load(&payload.join(files::FILES_FILE)).unwrap(),
    )
}

#[test]
fn the_installation_keeps_the_manifest_of_its_version() {
    let i = install_v1();
    let manifest = FilesManifest::load(&i.prefix.join(files::INSTALLED)).unwrap();
    // Cada arquivo instalado (menos o próprio manifesto e os do instalador)
    // está no manifesto, com o hash certo.
    let installed = snapshot(&i.prefix);
    let selection: plan::Selection = ["icarus".into(), "yosys".into()].into();
    let listed = manifest.files_for(&selection);
    for (rel, data) in &installed {
        if [files::INSTALLED, "install.json", "uninstall.sh"].contains(&rel.as_str()) {
            continue;
        }
        let (hash, _) = listed.get(rel.as_str()).unwrap_or_else(|| panic!("{rel}"));
        assert_eq!(*hash, sha256_hex(data), "{rel}");
    }
    assert_eq!(
        listed.len(),
        installed.len() - 3,
        "{:?}",
        listed.keys().collect::<Vec<_>>()
    );
    // O `lace` tem o nome da plataforma.
    let lace = if cfg!(windows) {
        "bin/lace.exe"
    } else {
        "bin/lace"
    };
    assert!(listed.contains_key(lace), "{listed:?}");
}

#[test]
fn only_the_chunks_with_changed_files_are_downloaded() {
    let i = install_v1();
    let installer = v2(&i.root, "v2", None);
    let (index, new) = load_new(&installer);
    let installed = FilesManifest::load(&i.prefix.join(files::INSTALLED)).unwrap();
    let plan = update::plan(&i.prefix, &installed, &index, &new).unwrap();
    assert_eq!(plan.components, ["icarus", "yosys"]);
    assert_eq!(plan.changed, ["lace", "icarus"]);
    assert!(plan.added.is_empty() && plan.removed.is_empty());
    assert_eq!(plan.stale, ["toolchain/oss/share/icarus/old.txt"]);
    let chunks: Vec<&str> = plan.chunks.iter().map(|c| c.chunk.file.as_str()).collect();
    // O pedaço sempre instalado e o só do Icarus; não o da biblioteca
    // dividida, nem o do Yosys.
    let icarus_chunk = index
        .chunks
        .iter()
        .find(|c| c.components == ["icarus"])
        .unwrap();
    assert_eq!(chunks, ["lace.tar.zst", icarus_chunk.file.as_str()]);
    assert!(plan.download() > 0);

    let source = counting(&installer);
    let mut events = Vec::new();
    let applied = update::apply(&source, &plan, &new, &i.prefix, |e| events.push(e)).unwrap();
    assert_eq!(*source.fetched.lock().unwrap(), chunks);
    assert_eq!(applied.removed, 1);
    assert_eq!(applied.verified, 2);
    assert!(matches!(events.last(), Some(Event::Verifying)));

    let after = snapshot(&i.prefix);
    let text = |rel: &str| String::from_utf8(after[rel].clone()).unwrap();
    assert_eq!(text("toolchain/oss/bin/iverilog"), "iverilog 2");
    assert_eq!(text("toolchain/oss/share/icarus/new.txt"), "entra na v2");
    assert!(!after.contains_key("toolchain/oss/share/icarus/old.txt"));
    assert!(!i.prefix.join("toolchain/oss/share/icarus").join("old.txt").exists());
    assert_eq!(text("toolchain/oss/bin/yosys"), "yosys 1");
    let lace = if cfg!(windows) {
        "bin/lace.exe"
    } else {
        "bin/lace"
    };
    assert_eq!(text(lace), "lace 2");
    assert!(text("toolchain/bundle.json").contains("\"v2\""));
    // O manifesto instalado é o da v2, e nada ficou para trás.
    assert_eq!(
        FilesManifest::load(&i.prefix.join(files::INSTALLED)).unwrap(),
        new
    );
    let leftovers: Vec<String> = fs::read_dir(&i.prefix)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.'))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert_eq!(Receipt::load(&i.prefix).unwrap().bundle, "v2");

    // De novo: nada a fazer, a não ser que um arquivo tenha sumido do disco.
    let installed = FilesManifest::load(&i.prefix.join(files::INSTALLED)).unwrap();
    let again = update::plan(&i.prefix, &installed, &index, &new).unwrap();
    assert!(again.chunks.is_empty() && again.stale.is_empty(), "{again:?}");
    fs::remove_file(i.prefix.join("toolchain/oss/bin/yosys")).unwrap();
    let repair = update::plan(&i.prefix, &installed, &index, &new).unwrap();
    assert_eq!(repair.changed, ["yosys"]);
    assert_eq!(repair.files(), 1);
}

#[test]
fn a_failure_after_swapping_puts_everything_back() {
    let i = install_v1();
    // A v2 diz que o executável do Icarus tem outro hash: os arquivos batem
    // com o manifesto de arquivos, mas o bundle não passa na conferência
    // depois da troca.
    let installer = v2(&i.root, "v2-ruim", Some("icarus"));
    let (index, new) = load_new(&installer);
    let installed = FilesManifest::load(&i.prefix.join(files::INSTALLED)).unwrap();
    let before = snapshot(&i.prefix);
    let plan = update::plan(&i.prefix, &installed, &index, &new).unwrap();
    let source = counting(&installer);
    let error = update::apply(&source, &plan, &new, &i.prefix, |_| {}).unwrap_err();
    assert!(
        format!("{error:#}").contains("do not match the manifest"),
        "{error:#}"
    );
    assert_eq!(snapshot(&i.prefix), before);
}

#[test]
fn a_chunk_that_does_not_match_the_manifest_changes_nothing() {
    let i = install_v1();
    let installer = v2(&i.root, "v2", None);
    let (index, mut new) = load_new(&installer);
    // O manifesto espera outro conteúdo para o executável novo.
    for g in &mut new.groups {
        if let Some(h) = g.files.get_mut("toolchain/oss/bin/iverilog") {
            *h = "1".repeat(64);
        }
    }
    let installed = FilesManifest::load(&i.prefix.join(files::INSTALLED)).unwrap();
    let before = snapshot(&i.prefix);
    let plan = update::plan(&i.prefix, &installed, &index, &new).unwrap();
    let error = update::apply(&counting(&installer), &plan, &new, &i.prefix, |_| {}).unwrap_err();
    assert!(
        format!("{error:#}").contains("does not match the file manifest"),
        "{error:#}"
    );
    assert_eq!(snapshot(&i.prefix), before);
}

#[test]
fn a_component_the_new_version_requires_comes_in() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let installer = v1(&root);
    let payload = installer.join(PAYLOAD_DIR);
    let index = Index::load(&payload).unwrap();
    let prefix = root.join("instalacao");
    let target = Target {
        prefix: prefix.clone(),
        link: None,
    };
    let selection = plan::from_names(&index, &["icarus".into()]).unwrap().0;
    install::install(&payload, &index, &selection, &target, |_| {}).unwrap();

    // Na v3, o Icarus passa a exigir o Yosys.
    let installer = build(
        &root,
        "v3",
        "lace 3",
        &[
            Spec {
                name: "icarus",
                requires: &["yosys"],
                exe: "oss/bin/iverilog",
                files: &[("oss/bin/iverilog", "iverilog 1"), ("oss/lib/libc", "libc")],
            },
            Spec {
                name: "yosys",
                requires: &[],
                exe: "oss/bin/yosys",
                files: &[("oss/bin/yosys", "yosys 1"), ("oss/lib/libc", "libc")],
            },
        ],
        None,
    );
    let (index, new) = load_new(&installer);
    let installed = FilesManifest::load(&prefix.join(files::INSTALLED)).unwrap();
    let plan = update::plan(&prefix, &installed, &index, &new).unwrap();
    assert_eq!(plan.components, ["icarus", "yosys"]);
    assert_eq!(plan.added, ["yosys"]);
    update::apply(&counting(&installer), &plan, &new, &prefix, |_| {}).unwrap();
    let tc = lace_core::Toolchain::open(prefix.join("toolchain").to_str().unwrap()).unwrap();
    assert!(tc.component("yosys").is_some());
    assert!(!prefix.join("toolchain/oss/share/icarus/old.txt").exists());
}
