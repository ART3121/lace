//! Empacota um bundle pequeno e instala de verdade num diretório temporário.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use solar_installer::install::{self, Receipt, Target};
use solar_installer::pack::{self, Contents};
use solar_installer::payload::{Index, PAYLOAD_DIR};
use solar_installer::plan;

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn platform() -> &'static str {
    solar_core::Platform::current().unwrap().as_str()
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    installer: PathBuf,
}

/// Um bundle com três componentes: `icarus` e `yosys` dividem `oss/lib/libc`,
/// `graphviz` exige `yosys`.
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let toolchain = root.join("toolchain");
    let write = |rel: &str, data: &str| {
        let p = toolchain.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, data).unwrap();
    };
    fs::create_dir_all(&toolchain).unwrap();
    write(
        "bundle.json",
        &format!(
            r#"{{"schema": 2, "bundle": "teste", "platform": "{}"}}"#,
            platform()
        ),
    );
    let mut contents_components = Vec::new();
    for (name, bin, requires, recommended) in [
        ("icarus", "iverilog", vec![], true),
        ("yosys", "yosys", vec![], true),
        ("graphviz", "dot", vec!["yosys"], false),
    ] {
        let bin_rel = format!("oss/bin/{bin}");
        write(&bin_rel, bin);
        write("oss/lib/libc", "libc");
        let manifest = serde_json::json!({
            "name": name, "version": "1", "dir": "oss", "source": "teste",
            "files": { &bin_rel: sha256_hex(bin.as_bytes()) },
        });
        write(&format!("components/{name}.json"), &manifest.to_string());
        contents_components.push(serde_json::json!({
            "name": name, "label": name.to_uppercase(), "description": format!("o {name}"),
            "recommended": recommended, "requires": requires, "version": "1",
            "files": [bin_rel, "oss/lib/libc", format!("components/{name}.json")],
        }));
    }
    let contents = serde_json::json!({
        "schema": 1, "bundle": "teste", "platform": platform(),
        "common": ["bundle.json"], "components": contents_components,
    });
    fs::write(root.join("toolchain.contents.json"), contents.to_string()).unwrap();
    let solar = root.join("solar");
    fs::write(&solar, "#!/bin/sh\necho solar\n").unwrap();
    let installer = root.join("installer");
    fs::write(&installer, "instalador").unwrap();
    let contents = Contents::load(&root.join("toolchain.contents.json")).unwrap();
    let out = root.join("solar-teste");
    pack::tui(&toolchain, &contents, &solar, &installer, &out, 3).unwrap();
    Fixture {
        _dir: dir,
        installer: out,
        root,
    }
}

fn payload(f: &Fixture) -> (PathBuf, Index) {
    let dir = f.installer.join(PAYLOAD_DIR);
    let index = Index::load(&dir).unwrap();
    (dir, index)
}

fn files_under(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(
                    p.strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    out.sort();
    out
}

#[test]
fn installs_the_selection_and_replaces_it_later() {
    let f = fixture();
    let (payload_dir, index) = payload(&f);
    // Um pedaço sempre instalado, um compartilhado e um por componente.
    assert_eq!(index.chunks.len(), 5, "{:#?}", index.chunks);
    assert_eq!(index.chunks[0].file, "solar.tar.zst");
    assert!(f.installer.join("install").is_file());

    let prefix = f.root.join("instalacao");
    let target = Target {
        prefix: prefix.clone(),
        link: Some(f.root.join("bin/solar")),
    };
    let mut events = Vec::new();
    let selection = plan::recommended(&index);
    let report = install::install(&payload_dir, &index, &selection, &target, |e| {
        events.push(e)
    })
    .unwrap();
    assert_eq!(report.components, ["icarus", "yosys"]);
    assert_eq!(report.verified, 2);
    assert!(matches!(events.last(), Some(install::Event::Verifying)));
    assert_eq!(
        files_under(&prefix),
        [
            "bin/solar",
            "install.json",
            "toolchain/bundle.json",
            "toolchain/components/icarus.json",
            "toolchain/components/yosys.json",
            "toolchain/oss/bin/iverilog",
            "toolchain/oss/bin/yosys",
            "toolchain/oss/lib/libc",
            "uninstall.sh",
        ]
    );
    // O Solar instalado abre o bundle: só os componentes escolhidos.
    let tc = solar_core::Toolchain::open(prefix.join("toolchain").to_str().unwrap()).unwrap();
    assert!(tc.component("graphviz").is_none());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(prefix.join("bin/solar"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "bin/solar executável");
        assert_eq!(
            report.link,
            install::LinkOutcome::Created(f.root.join("bin/solar"))
        );
        assert_eq!(
            fs::read_link(f.root.join("bin/solar")).unwrap(),
            prefix.join("bin/solar")
        );
    }

    // Reinstalar com outra seleção troca tudo: o Icarus sai, o Graphviz entra.
    let (selection, added) = plan::from_names(&index, &["graphviz".into()]).unwrap();
    assert_eq!(added, ["yosys"]);
    let report = install::install(&payload_dir, &index, &selection, &target, |_| {}).unwrap();
    assert_eq!(report.replaced.unwrap().components, ["icarus", "yosys"]);
    let files = files_under(&prefix);
    assert!(
        !files.contains(&"toolchain/oss/bin/iverilog".to_owned()),
        "{files:?}"
    );
    assert!(
        files.contains(&"toolchain/oss/bin/dot".to_owned()),
        "{files:?}"
    );
    assert_eq!(
        Receipt::load(&prefix).unwrap().components,
        ["graphviz", "yosys"]
    );

    // O desinstalador remove a instalação e o atalho.
    #[cfg(unix)]
    {
        let status = std::process::Command::new("/bin/sh")
            .arg(prefix.join("uninstall.sh"))
            .output()
            .unwrap();
        assert!(status.status.success(), "{status:?}");
        assert!(!prefix.exists());
        assert!(fs::symlink_metadata(f.root.join("bin/solar")).is_err());
    }
}

#[test]
fn refuses_folders_that_are_not_solar_installations() {
    let f = fixture();
    let (payload_dir, index) = payload(&f);
    let prefix = f.root.join("ocupada");
    fs::create_dir_all(&prefix).unwrap();
    fs::write(prefix.join("tese.tex"), "não apagar").unwrap();
    let target = Target {
        prefix: prefix.clone(),
        link: None,
    };
    let err = install::install(
        &payload_dir,
        &index,
        &plan::recommended(&index),
        &target,
        |_| {},
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("não é uma instalação do Solar"), "{err}");
    assert_eq!(files_under(&prefix), ["tese.tex"]);
}

#[test]
fn a_corrupt_payload_leaves_the_previous_installation_alone() {
    let f = fixture();
    let (payload_dir, index) = payload(&f);
    let prefix = f.root.join("instalacao");
    let target = Target {
        prefix: prefix.clone(),
        link: None,
    };
    install::install(
        &payload_dir,
        &index,
        &plan::recommended(&index),
        &target,
        |_| {},
    )
    .unwrap();
    let before = files_under(&prefix);

    // Um pedaço trocado por lixo.
    let chunk = index
        .chunks
        .iter()
        .find(|c| c.components == ["yosys"])
        .unwrap();
    fs::write(payload_dir.join(&chunk.file), "lixo").unwrap();
    let err = install::install(
        &payload_dir,
        &index,
        &plan::recommended(&index),
        &target,
        |_| {},
    );
    assert!(err.is_err());
    assert_eq!(files_under(&prefix), before);
}
