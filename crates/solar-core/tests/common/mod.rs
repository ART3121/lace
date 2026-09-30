//! Apoio aos testes que executam as ferramentas de verdade.
//!
//! Precisam de um bundle montado por `scripts/bundle.py` em
//! `SOLAR_TEST_BUNDLE`. Sem ele, os testes avisam e passam, para que
//! `cargo test` funcione num clone recém-feito; com `CI` definido, a ausência
//! é falha. Um bundle parcial (`--only yanc`) basta para os testes de build.

#![allow(dead_code)]

use camino::{Utf8Path, Utf8PathBuf};
use solar_core::{Tool, Toolchain};

/// O bundle de teste.
pub fn toolchain() -> Option<Toolchain> {
    match std::env::var("SOLAR_TEST_BUNDLE") {
        Ok(dir) => Some(Toolchain::open(&dir).expect("SOLAR_TEST_BUNDLE não é um bundle válido")),
        Err(_) if std::env::var_os("CI").is_some() => {
            panic!("CI definido, mas SOLAR_TEST_BUNDLE não: estes testes não podem ser pulados")
        }
        Err(_) => {
            eprintln!("PULADO: defina SOLAR_TEST_BUNDLE com um bundle de scripts/bundle.py");
            None
        }
    }
}

/// Fonte do YANC, para os casos de teste do próprio YANC.
pub fn yanc_source() -> Utf8PathBuf {
    std::env::var("SOLAR_TEST_YANC_SRC")
        .map_or_else(|_| workspace().join("vendor/yanc"), Utf8PathBuf::from)
}

pub fn workspace() -> Utf8PathBuf {
    Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Diretório temporário com caminho canônico (no macOS `/tmp` é symlink).
pub fn tempdir() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dunce::canonicalize(dir.path()).unwrap()).unwrap();
    (dir, path)
}

pub fn copy_dir(from: &Utf8Path, to: &Utf8Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in from.read_dir_utf8().unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Copia um projeto de `examples/` para um diretório temporário, sem os
/// artefatos de builds anteriores.
pub fn example(name: &str) -> (tempfile::TempDir, Utf8PathBuf) {
    let (guard, dir) = tempdir();
    let target = dir.join(name);
    copy_dir(&workspace().join("examples").join(name), &target);
    let _ = std::fs::remove_dir_all(target.join(".solar"));
    for entry in target.read_dir_utf8().unwrap() {
        let proc_dir = entry.unwrap().into_path();
        let _ = std::fs::remove_dir_all(proc_dir.join("Hardware"));
        if let Ok(files) = proc_dir.join("Software").read_dir_utf8() {
            for file in files {
                let file = file.unwrap().into_path();
                if file.extension() == Some("asm") {
                    std::fs::remove_file(file).unwrap();
                }
            }
        }
    }
    (guard, target)
}

/// O bundle, se ele tiver todas as `tools`. Sem elas o teste avisa e passa;
/// com `CI` definido, falha.
pub fn toolchain_with(tools: &[Tool]) -> Option<Toolchain> {
    let toolchain = toolchain()?;
    let missing: Vec<_> = tools
        .iter()
        .filter(|&&t| toolchain.tool(t).is_err())
        .collect();
    if missing.is_empty() {
        return Some(toolchain);
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "CI sem as ferramentas {missing:?} no bundle"
    );
    eprintln!("PULADO: faltam {missing:?} no bundle");
    None
}
