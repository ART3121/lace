//! Empacota um bundle pequeno e instala de verdade num diretório temporário.

use std::fs;
use std::path::{Path, PathBuf};

use lace_installer::add;
use lace_installer::install::{self, Receipt, Target};
use lace_installer::pack::{self, Contents};
use lace_installer::payload::{Index, PAYLOAD_DIR};
use lace_installer::plan;
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
    let lace = root.join("lace");
    fs::write(&lace, "#!/bin/sh\necho lace\n").unwrap();
    let installer = root.join("installer");
    fs::write(&installer, "instalador").unwrap();
    let contents = Contents::load(&root.join("toolchain.contents.json")).unwrap();
    let out = root.join("lace-teste");
    pack::tui(&toolchain, &contents, &lace, &installer, &out, 3).unwrap();
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
    assert_eq!(index.chunks[0].file, "lace.tar.zst");
    assert!(f.installer.join("install").is_file());

    let prefix = f.root.join("instalacao");
    let target = Target {
        prefix: prefix.clone(),
        link: Some(f.root.join("bin/lace")),
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
            pack::lace_entry(platform()),
            "install.json",
            "toolchain/bundle.json",
            "toolchain/components/icarus.json",
            "toolchain/components/yosys.json",
            "toolchain/files.json",
            "toolchain/oss/bin/iverilog",
            "toolchain/oss/bin/yosys",
            "toolchain/oss/lib/libc",
            "uninstall.sh",
        ]
    );
    // O Lace instalado abre o bundle: só os componentes escolhidos.
    let tc = lace_core::Toolchain::open(prefix.join("toolchain").to_str().unwrap()).unwrap();
    assert!(tc.component("graphviz").is_none());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(prefix.join("bin/lace"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "bin/lace executável");
        assert_eq!(
            report.link,
            install::LinkOutcome::Created(f.root.join("bin/lace"))
        );
        assert_eq!(
            fs::read_link(f.root.join("bin/lace")).unwrap(),
            prefix.join("bin/lace")
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

    // O desinstalador remove a instalação, o atalho e as sobras de uma
    // instalação interrompida, que deixariam a pasta para trás.
    #[cfg(unix)]
    {
        fs::create_dir_all(prefix.join(".instalando-123/toolchain")).unwrap();
        fs::create_dir_all(prefix.join(".antigo-456/bin")).unwrap();
        let status = std::process::Command::new("/bin/sh")
            .arg(prefix.join("uninstall.sh"))
            .output()
            .unwrap();
        assert!(status.status.success(), "{status:?}");
        assert!(!prefix.exists());
        assert!(fs::symlink_metadata(f.root.join("bin/lace")).is_err());
    }
}

#[test]
fn refuses_folders_that_are_not_lace_installations() {
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
    assert!(err.contains("is not a Lace installation"), "{err}");
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

/// Acrescentar: só os pedaços dos componentes novos entram em `toolchain/`,
/// o `lace` não é reinstalado, e uma falha no meio desfaz tudo.
#[test]
fn adds_bundle_apps_without_reinstalling_lace() {
    let f = fixture();
    let (payload_dir, index) = payload(&f);
    let prefix = f.root.join("instalacao");
    let target = Target {
        prefix: prefix.clone(),
        link: None,
    };
    let only_icarus: plan::Selection = ["icarus".to_owned()].into();
    install::install(&payload_dir, &index, &only_icarus, &target, |_| {}).unwrap();
    // Um `lace` diferente do do payload: se o add o reinstalasse, voltaria
    // a ser o do payload.
    fs::write(prefix.join("bin/lace"), "o lace instalado").unwrap();
    let toolchain = prefix.join("toolchain");
    let before = files_under(&toolchain);

    // Um pedaço que falha no meio: nada fica.
    struct Failing(PathBuf);
    impl add::ChunkSource for Failing {
        fn fetch(
            &self,
            chunk: &lace_installer::payload::Chunk,
            _downloaded: &mut dyn FnMut(u64),
        ) -> anyhow::Result<PathBuf> {
            if chunk.components.iter().any(|c| c == "graphviz") {
                anyhow::bail!("a rede caiu");
            }
            Ok(self.0.join(&chunk.file))
        }
    }
    let installed = add::installed(&index, &toolchain).unwrap();
    let (_, new) = plan::adding(
        &index,
        &installed.iter().cloned().collect::<Vec<_>>(),
        &["graphviz".into()],
    )
    .unwrap();
    let new: plan::Selection = new.into_iter().collect();
    let err = add::add(&Failing(payload_dir.clone()), &index, &prefix, &new, |_| {}).unwrap_err();
    assert!(err.to_string().contains("a rede caiu"), "{err}");
    assert_eq!(files_under(&toolchain), before, "a falha deixou arquivos");

    // Um pedaço adulterado: o executável do Yosys não bate com o manifesto.
    // Ele entra em `toolchain/`, a conferência o recusa, e tudo sai.
    let tampered = f.root.join("adulterado");
    fs::create_dir_all(&tampered).unwrap();
    for chunk in &index.chunks {
        let from = payload_dir.join(&chunk.file);
        let to = tampered.join(&chunk.file);
        if chunk.components == ["yosys"] {
            let unpacked = f.root.join("aberto");
            let decoder = zstd::Decoder::new(fs::File::open(&from).unwrap()).unwrap();
            tar::Archive::new(decoder).unpack(&unpacked).unwrap();
            fs::write(unpacked.join("toolchain/oss/bin/yosys"), "outro yosys").unwrap();
            let encoder = zstd::Encoder::new(fs::File::create(&to).unwrap(), 3).unwrap();
            let mut builder = tar::Builder::new(encoder);
            builder.append_dir_all(".", &unpacked).unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        } else {
            fs::copy(&from, &to).unwrap();
        }
    }
    let err = add::add(&add::LocalPayload(tampered), &index, &prefix, &new, |_| {}).unwrap_err();
    assert!(
        err.to_string().contains("do not match the manifest"),
        "{err}"
    );
    assert_eq!(
        files_under(&toolchain),
        before,
        "a conferência falhou e sobrou arquivo"
    );
    assert_eq!(
        Receipt::load(&prefix).unwrap().components,
        ["icarus"],
        "o recibo mudou sem a instalação mudar"
    );

    let report = add::add(
        &add::LocalPayload(payload_dir),
        &index,
        &prefix,
        &new,
        |_| {},
    )
    .unwrap();
    assert_eq!(report.added, ["yosys", "graphviz"]);
    assert_eq!(report.components, ["graphviz", "icarus", "yosys"]);
    for bin in ["iverilog", "yosys", "dot"] {
        assert!(toolchain.join("oss/bin").join(bin).is_file(), "{bin}");
    }
    assert_eq!(
        fs::read_to_string(prefix.join("bin/lace")).unwrap(),
        "o lace instalado",
        "o add reinstalou o lace"
    );
    assert_eq!(
        Receipt::load(&prefix).unwrap().components,
        ["graphviz", "icarus", "yosys"]
    );
    assert!(
        !fs::read_dir(&prefix).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')),
        "sobrou a pasta provisória"
    );

    // Outro bundle: acrescentar misturaria versões.
    let mut other = index.clone();
    other.bundle = "outro".into();
    let err = add::installed(&other, &toolchain).unwrap_err();
    assert!(err.to_string().contains("update Lace"), "{err}");
}
