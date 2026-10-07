//! Os casos de teste do próprio YANC, rodados pelo Lace.
//!
//! - `CMMComp/Tests/<nome>/Software/<nome>.cmm`: precisa compilar até o fim, e
//!   o `.asm` tem que ser idêntico ao `golden.asm` (o mesmo critério do
//!   `Scripts/regress.sh`). Isso confere que o Lace chama o `cmmcomp` do
//!   mesmo jeito que o YANC se testa.
//! - `CMMComp/NegTests/manifest.txt`: cada fixture tem que ser recusada com a
//!   mensagem esperada, e nunca por crash.

mod common;

use lace_core::{BuildOptions, Control, LaceError, Project, Severity, Status, build};

#[test]
fn cmm_positive_tests_match_golden_asm() {
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let tests = common::yanc_source().join("Compilers/CMMComp/Tests");
    let mut names: Vec<_> = tests
        .read_dir_utf8()
        .unwrap()
        .map(|e| e.unwrap().file_name().to_owned())
        .filter(|name| {
            tests
                .join(name)
                .join("Software")
                .join(format!("{name}.cmm"))
                .is_file()
        })
        .collect();
    names.sort();
    assert!(names.len() > 50, "só {} casos em {tests}", names.len());

    let mut failures = Vec::new();
    for name in &names {
        let (_guard, root) = common::tempdir();
        common::copy_dir(
            &tests.join(name).join("Software"),
            &root.join(name).join("Software"),
        );
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
        let asm = lf(std::fs::read(processor.software_dir().join(format!("{name}.asm"))).unwrap());
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
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let neg = common::yanc_source().join("Compilers/CMMComp/NegTests");
    let manifest = std::fs::read_to_string(neg.join("manifest.txt")).unwrap();

    let mut checked = 0;
    let mut failures = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (fixture, expected) = line.split_once('|').unwrap();
        checked += 1;

        // Como o regress.sh: a fixture vira <nome>/Software/<nome>.cmm, com o
        // nome do #PRNAME dela, porque o Lace recusa um nome diferente do
        // processador antes de compilar (`recursion.cmm`, do YANC 5.7, declara
        // `recursion`); sem #PRNAME, `p`.
        let source = std::fs::read_to_string(neg.join(fixture)).unwrap();
        let name = source
            .lines()
            .find_map(|line| line.trim().strip_prefix("#PRNAME"))
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or("p")
            .to_owned();
        let (_guard, root) = common::tempdir();
        let software = root.join(&name).join("Software");
        std::fs::create_dir_all(&software).unwrap();
        std::fs::write(software.join(format!("{name}.cmm")), &source).unwrap();
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
            // Fixture sem #PRNAME: o Lace recusa antes do cmmcomp, de propósito.
            Err(LaceError::InvalidSource { reason, .. }) if reason.contains("#PRNAME") => {}
            Err(e) => failures.push(format!("{fixture}: {e}")),
        }
    }
    assert!(checked > 5, "manifesto com só {checked} casos");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
