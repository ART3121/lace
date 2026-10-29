//! O `aurora learn` de ponta a ponta, com a trilha de teste do aurora-learn
//! (`crates/aurora-learn/tests/fixtures`, por `AURORA_LEARN_DIR`). O `--json` de
//! cada subcomando confere com o schema dele em `docs/schema/`.
//!
//! Os que corrigem precisam do bundle em `AURORA_TEST_BUNDLE`, como os de
//! `cli.rs`; sem ele, avisam e passam.

use assert_cmd::Command;
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::Value;

fn fixtures() -> Utf8PathBuf {
    Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../aurora-learn/tests/fixtures")
}

/// `aurora` sem configuração herdada, com a trilha de teste, dentro de `dir`.
fn aurora(dir: &Utf8Path) -> Command {
    let mut cmd = Command::cargo_bin("aurora").unwrap();
    cmd.env_remove("AURORA_TOOLCHAIN")
        .env_remove("AURORA_COMPILER")
        .env_remove("AURORA_TEST_BUNDLE")
        .env("AURORA_LEARN_DIR", fixtures())
        .env("NO_COLOR", "1")
        .current_dir(dir);
    cmd
}

/// O mesmo, com o bundle de teste; `None` sem ele.
fn aurora_with_tools(dir: &Utf8Path) -> Option<Command> {
    let bundle = match std::env::var("AURORA_TEST_BUNDLE") {
        Ok(bundle) => bundle,
        Err(_) => {
            assert!(std::env::var_os("CI").is_none(), "CI sem AURORA_TEST_BUNDLE");
            eprintln!("PULADO: defina AURORA_TEST_BUNDLE");
            return None;
        }
    };
    let mut cmd = aurora(dir);
    cmd.env("AURORA_TOOLCHAIN", bundle);
    Some(cmd)
}

fn tempdir() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dunce::canonicalize(dir.path()).unwrap()).unwrap();
    (dir, path)
}

/// Roda com `--json`, confere o código de saída e o schema, e devolve o JSON.
fn json(cmd: &mut Command, args: &[&str], code: i32, schema: &str) -> Value {
    let out = cmd
        .arg("--json")
        .args(args)
        .assert()
        .code(code)
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&out)
        .unwrap_or_else(|e| panic!("{args:?}: {e}: {}", String::from_utf8_lossy(&out)));
    let path = Utf8Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/schema")
        .join(format!("{schema}.json"));
    let text = std::fs::read_to_string(&path).unwrap();
    let validator = jsonschema::validator_for(&serde_json::from_str(&text).unwrap()).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&value)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "{args:?} não confere com {schema}.json:\n{}\n\n{value:#}",
        errors.join("\n")
    );
    value
}

/// Uma pasta de exercícios da trilha de teste: `(guarda, pasta)`.
fn workspace() -> (tempfile::TempDir, Utf8PathBuf) {
    let (guard, dir) = tempdir();
    let report = json(
        &mut aurora(&dir),
        &["learn", "init", "ex", "--track", "teste"],
        0,
        "learn-init",
    );
    assert_eq!(report["exercises"], 4);
    assert_eq!(report["current"], "mux2");
    (guard, dir.join("ex"))
}

#[test]
fn without_the_component_the_error_says_how_to_install_it() {
    let (_guard, dir) = tempdir();
    let mut cmd = aurora(&dir);
    // Vazia: sem a trilha do repositório, que o `aurora` de teste (uma build
    // de desenvolvimento) acharia sozinho.
    cmd.env("AURORA_LEARN_DIR", "");
    let report = json(&mut cmd, &["learn", "init"], 2, "error");
    assert_eq!(report["error"]["code"], "learn_component_missing");
    assert_eq!(
        report["error"]["hint"],
        "Install it with: aurora install aurora-learn"
    );
}

#[test]
fn list_hint_and_reset_work_without_tools() {
    let (_guard, ex) = workspace();
    let list = json(&mut aurora(&ex), &["learn", "list"], 0, "learn-list");
    assert_eq!(list["total"], 4);
    assert_eq!(list["solved"], 0);
    assert_eq!(list["exercises"][0]["current"], true);
    assert_eq!(list["exercises"][3]["name"], "conta");

    // De uma subpasta, como os outros comandos.
    let deep = ex.join("exercises/01_comb/soma4");
    let hint = json(
        &mut aurora(&deep),
        &["learn", "hint", "soma4"],
        0,
        "learn-hint",
    );
    assert_eq!(hint["hints"][0], "O `cout` de um é o `cin` do próximo.");

    let file = ex.join("exercises/01_comb/mux2/mux2.v");
    std::fs::write(&file, "// meu\n").unwrap();
    // Sem --yes e sem terminal, recusa.
    aurora(&ex).args(["learn", "reset", "mux2"]).assert().code(2);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "// meu\n");
    json(
        &mut aurora(&ex),
        &["learn", "reset", "mux2", "--yes"],
        0,
        "learn-reset",
    );
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("module mux2")
    );

    let unknown = json(&mut aurora(&ex), &["learn", "hint", "nada"], 2, "error");
    assert_eq!(unknown["error"]["code"], "unknown_exercise");
}

#[test]
fn outside_a_workspace_the_error_says_how_to_create_one() {
    let (_guard, dir) = tempdir();
    let report = json(&mut aurora(&dir), &["learn", "list"], 2, "error");
    assert_eq!(report["error"]["code"], "no_learn_workspace");
}

#[test]
fn the_watch_mode_needs_a_terminal() {
    let (_guard, ex) = workspace();
    let Some(mut cmd) = aurora_with_tools(&ex) else {
        return;
    };
    let out = cmd
        .arg("learn")
        .assert()
        .code(2)
        .get_output()
        .stderr
        .clone();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("needs a terminal"), "{text}");
}

#[test]
fn check_records_solved_and_unsolved() {
    let (_guard, ex) = workspace();
    let Some(mut cmd) = aurora_with_tools(&ex) else {
        return;
    };
    // O arquivo inicial erra: saída sem atribuição.
    let report = json(&mut cmd, &["learn", "check"], 1, "learn-check");
    let result = &report["results"][0];
    assert_eq!(result["verdict"], "mismatch");
    assert_eq!(result["findings"][0]["kind"], "undriven_output");

    let file = ex.join("exercises/01_comb/mux2/mux2.v");
    std::fs::write(
        &file,
        "module mux2(input a, input b, input sel, output y);\n    assign y = sel ? b : a;\nendmodule\n",
    )
    .unwrap();
    let report = json(
        &mut aurora_with_tools(&ex).unwrap(),
        &["learn", "check", "mux2"],
        0,
        "learn-check",
    );
    assert_eq!(report["results"][0]["verdict"], "solved");
    assert_eq!(report["solved"], 1);
    assert!(ex.join("solutions/01_comb/mux2.v").is_file());

    // Em texto: o veredito e onde está a solução.
    let out = aurora_with_tools(&ex)
        .unwrap()
        .args(["learn", "check"])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("Solved."), "{text}");

    // Estragar depois de resolver desmarca.
    std::fs::write(
        &file,
        "module mux2(input a, input b, input sel, output y);\n    assign y = a;\nendmodule\n",
    )
    .unwrap();
    let report = json(
        &mut aurora_with_tools(&ex).unwrap(),
        &["learn", "check"],
        1,
        "learn-check",
    );
    assert_eq!(report["solved"], 0);
}

#[test]
fn dev_check_passes_on_the_test_track() {
    let (_guard, dir) = tempdir();
    let Some(mut cmd) = aurora_with_tools(&dir) else {
        return;
    };
    let track = fixtures().join("teste");
    let report = json(
        &mut cmd,
        &["learn", "dev", "check", track.as_str()],
        0,
        "learn-dev-check",
    );
    assert_eq!(report["exercises"].as_array().unwrap().len(), 4);
}
