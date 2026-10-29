//! Os casos de teste do próprio YANC, rodados pela AURORA.
//!
//! - `CMMComp/Tests/<nome>/<nome>.cmm`: precisa compilar até o fim, e o
//!   `.asm` tem que ser idêntico ao `golden.asm` (o mesmo critério do
//!   `Scripts/regress.sh`). Isso confere que a AURORA chama o `cmmcomp` do
//!   mesmo jeito que o YANC se testa. Os fontes vêm de `AURORA_TEST_YANC_SRC`
//!   (padrão: `vendor/yanc`), que tem que ser da versão do YANC do bundle.
//! - `CMMComp/NegTests/manifest.txt`: cada fixture tem que ser recusada com a
//!   mensagem esperada, e nunca por crash.

mod common;

use camino::Utf8Path;
use aurora_core::{BuildOptions, Control, Project, Severity, Status, build};

/// Monta o caso como um processador da AURORA em `processor`: o caso do YANC
/// 7.1 já tem a estrutura dele, com o fonte e os arquivos (os de arrays, as
/// entradas) lado a lado. O que o regress compara (`golden*`) fica de fora.
fn copy_case(case: &Utf8Path, processor: &Utf8Path) {
    std::fs::create_dir_all(processor).unwrap();
    for entry in case.read_dir_utf8().unwrap() {
        let entry = entry.unwrap();
        let file = entry.file_name();
        if entry.file_type().unwrap().is_file() && !file.starts_with("golden") {
            std::fs::copy(entry.path(), processor.join(file)).unwrap();
        }
    }
}

#[test]
fn cmm_positive_tests_match_golden_asm() {
    let (Some(toolchain), Some(yanc)) = (common::toolchain(), common::yanc_source()) else {
        return;
    };
    let tests = yanc.join("Compilers/CMMComp/Tests");
    let mut names: Vec<_> = tests
        .read_dir_utf8()
        .unwrap()
        .map(|e| e.unwrap().file_name().to_owned())
        .filter(|name| tests.join(name).join(format!("{name}.cmm")).is_file())
        .collect();
    names.sort();
    assert!(names.len() > 50, "só {} casos em {tests}", names.len());

    let mut failures = Vec::new();
    for name in &names {
        let (_guard, root) = common::tempdir();
        copy_case(&tests.join(name), &root.join(name));
        std::fs::write(
            root.join("t.spf"),
            format!(r#"{{"structure": {{"processors": [{{"name": "{name}"}}]}}}}"#),
        )
        .unwrap();
        let project = Project::open(root.join("t.spf")).unwrap();
        let processor = project.require_processor(name).unwrap();

        let result = match build(
            &toolchain,
            processor,
            &BuildOptions::default(),
            &Control::default(),
        ) {
            Ok(result) => result,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        if result.status != Status::Succeeded {
            let errors: Vec<_> = result
                .diagnostics
                .iter()
                .filter(|d| d.severity != Severity::Info)
                .map(|d| d.raw.as_str())
                .collect();
            failures.push(format!(
                "{name}: {:?} em {:?}: {errors:?}",
                result.status, result.failed_step
            ));
            continue;
        }
        // No Windows o runtime C do mingw grava em modo texto, com CRLF; o
        // conteúdo tem que ser o mesmo byte a byte fora isso.
        let lf =
            |bytes: Vec<u8>| -> Vec<u8> { bytes.into_iter().filter(|&b| b != b'\r').collect() };
        let asm = lf(std::fs::read(processor.assembly_path()).unwrap());
        let golden = lf(std::fs::read(tests.join(name).join("golden.asm")).unwrap());
        if asm != golden {
            failures.push(format!("{name}: .asm difere do golden.asm"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} de {} falharam:\n{}",
        failures.len(),
        names.len(),
        failures.join("\n")
    );
}

#[test]
fn cmm_negative_tests_are_rejected_with_expected_message() {
    let (Some(toolchain), Some(yanc)) = (common::toolchain(), common::yanc_source()) else {
        return;
    };
    let neg = yanc.join("Compilers/CMMComp/NegTests");
    let manifest = std::fs::read_to_string(neg.join("manifest.txt")).unwrap();

    let mut checked = 0;
    let mut failures = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (fixture, expected) = line.split_once('|').unwrap();
        checked += 1;

        // A fixture vira <nome>/<nome>.cmm, com o nome do arquivo dela, que é
        // o do processador, como no regress.sh (há fixtures do próprio nome:
        // `begin.cmm`, `bad-name.cmm`).
        let source = std::fs::read_to_string(neg.join(fixture)).unwrap();
        let name = fixture.trim_end_matches(".cmm").to_owned();
        // Um nome que a AURORA não aceita para processador: recusado antes do
        // cmmcomp, que o recusaria também.
        if aurora_core::validate_processor_name(&name).is_err() {
            continue;
        }
        let (_guard, root) = common::tempdir();
        let dir = root.join(&name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.cmm")), &source).unwrap();
        std::fs::write(
            root.join("t.spf"),
            format!(r#"{{"structure": {{"processors": ["{name}"]}}}}"#),
        )
        .unwrap();
        let project = Project::open(root.join("t.spf")).unwrap();
        let processor = project.require_processor(&name).unwrap();

        match build(
            &toolchain,
            processor,
            &BuildOptions::default(),
            &Control::default(),
        ) {
            Ok(result) => {
                let found = result
                    .diagnostics
                    .iter()
                    .any(|d| d.severity == Severity::Error && d.raw.contains(expected));
                if result.status != Status::Failed || !found {
                    failures.push(format!(
                        "{fixture}: {:?}, esperava erro com '{expected}'",
                        result.status
                    ));
                }
            }
            Err(e) => failures.push(format!("{fixture}: {e}")),
        }
    }
    assert!(checked > 5, "manifesto com só {checked} casos");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
