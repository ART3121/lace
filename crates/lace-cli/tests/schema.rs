//! O `--json` de cada comando, de verdade, confere com o schema dele em
//! `docs/schema/`. O teste unitário de `report.rs` garante que os arquivos são
//! os que os tipos geram; este garante que a saída é a que os arquivos
//! descrevem.
//!
//! A primeira metade roda sem ferramentas. A segunda precisa do bundle em
//! `LACE_TEST_BUNDLE`, como os testes de `cli.rs`.

use assert_cmd::Command;
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::Value;

fn lace(dir: &Utf8Path) -> Command {
    let mut cmd = Command::cargo_bin("lace").unwrap();
    cmd.env_remove("LACE_TOOLCHAIN")
        .env_remove("LACE_COMPILER")
        .env_remove("LACE_TEST_BUNDLE")
        .current_dir(dir);
    cmd
}

fn tempdir() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dunce::canonicalize(dir.path()).unwrap()).unwrap();
    (dir, path)
}

/// Roda `args` com `--json`, confere o código de saída e devolve o JSON.
fn run(cmd: &mut Command, args: &[&str], code: i32) -> Value {
    let out = cmd
        .arg("--json")
        .args(args)
        .assert()
        .code(code)
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out)
        .unwrap_or_else(|e| panic!("{args:?}: {e}: {}", String::from_utf8_lossy(&out)))
}

/// As divergências entre `value` e `docs/schema/<schema>.json`.
fn violations(schema: &str, value: &Value) -> Vec<String> {
    let path = Utf8Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/schema")
        .join(format!("{schema}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let validator = jsonschema::validator_for(&serde_json::from_str(&text).unwrap()).unwrap();
    validator
        .iter_errors(value)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect()
}

/// Falha com cada divergência entre `value` e `docs/schema/<schema>.json`.
fn conforms(schema: &str, value: &Value) {
    let errors = violations(schema, value);
    assert!(
        errors.is_empty(),
        "a saída não confere com {schema}.json:\n{}\n\n{value:#}",
        errors.join("\n")
    );
}

fn bundle() -> Option<String> {
    let value = std::env::var("LACE_TEST_BUNDLE").ok();
    if value.is_none() {
        assert!(std::env::var_os("CI").is_none(), "CI sem LACE_TEST_BUNDLE");
        eprintln!("PULADO: defina LACE_TEST_BUNDLE");
    }
    value
}

#[test]
fn commands_without_tools_match_their_schemas() {
    let (_guard, dir) = tempdir();
    conforms("new", &run(&mut lace(&dir), &["new", "demo"], 0));
    let root = dir.join("demo");
    let at = || lace(&root);

    let status = run(&mut at(), &["status"], 0);
    conforms("status", &status);
    // O schema recusa o que não é a saída: sem isto, um schema vazio
    // passaria em tudo.
    let mut wrong = status.clone();
    wrong["processors"] = "nenhum".into();
    wrong.as_object_mut().unwrap().remove("spf");
    assert_eq!(violations("status", &wrong).len(), 2, "{wrong:#}");

    conforms("build", &run(&mut at(), &["build"], 0));
    conforms("top", &run(&mut at(), &["top"], 0));
    conforms("add", &run(&mut at(), &["add", "porta.v", "porta_tb.v"], 0));
    conforms("top", &run(&mut at(), &["top", "porta"], 0));
    conforms("remove", &run(&mut at(), &["remove", "porta_tb.v"], 0));
    conforms(
        "move",
        &run(&mut at(), &["move", "porta.v", "rtl/porta.v"], 0),
    );
    conforms("proc", &run(&mut at(), &["proc", "add", "soma"], 0));
    conforms(
        "proc",
        &run(&mut at(), &["proc", "set", "soma", "--freq", "50"], 0),
    );
    conforms("status", &run(&mut at(), &["status"], 0));
    conforms(
        "report-clean",
        &run(&mut at(), &["report", "clean", "--yes"], 0),
    );

    // Erros: do Core (projeto que não existe) e da própria CLI.
    let error = run(&mut at(), &["-C", "nao_existe", "status"], 2);
    conforms("error", &error);
    let error = run(&mut at(), &["remove", "nunca.v"], 2);
    assert_eq!(error["error"]["code"], "cli");
    conforms("error", &error);
}

#[test]
fn commands_with_tools_match_their_schemas() {
    let Some(tc) = bundle() else {
        return;
    };
    let (_guard, dir) = tempdir();
    let at = |root: &Utf8Path| {
        let mut cmd = lace(root);
        cmd.env("LACE_TOOLCHAIN", &tc);
        cmd
    };
    lace(&dir).args(["new", "demo"]).assert().success();
    let root = dir.join("demo");
    run(&mut at(&root), &["add", "porta.v", "porta_tb.v"], 0);

    conforms("tools", &run(&mut at(&root), &["tools"], 0));
    conforms("tools", &run(&mut at(&root), &["tools", "--verify"], 0));
    conforms("check", &run(&mut at(&root), &["check"], 0));
    conforms("hierarchy", &run(&mut at(&root), &["hierarchy"], 0));
    conforms("sim", &run(&mut at(&root), &["sim"], 0));
    conforms("synth", &run(&mut at(&root), &["synth", "--svg"], 0));

    // Um processador: compilado antes, com as saídas das portas.
    run(&mut at(&root), &["proc", "add", "soma"], 0);
    conforms("build", &run(&mut at(&root), &["build"], 0));
    conforms("sim", &run(&mut at(&root), &["sim", "-p", "soma"], 0));

    // Falhas também têm forma: verilog que não elabora, testbench sem fim.
    std::fs::write(
        root.join("porta.v"),
        "module porta(input a, output y);\n  assign y = ;\nendmodule\n",
    )
    .unwrap();
    let failed = run(&mut at(&root), &["check"], 1);
    assert_eq!(failed["check"]["status"], "failed");
    conforms("check", &failed);
    let failed = run(&mut at(&root), &["hierarchy"], 1);
    assert_eq!(failed["status"], "failed");
    conforms("hierarchy", &failed);
    std::fs::write(
        root.join("porta.v"),
        "module porta(input a, output y);\n  assign y = a;\nendmodule\n",
    )
    .unwrap();
    std::fs::write(
        root.join("infinito_tb.v"),
        "module infinito_tb;\n  reg clk = 0;\n  always #5 clk = ~clk;\nendmodule\n",
    )
    .unwrap();
    run(&mut at(&root), &["add", "infinito_tb.v"], 0);
    let stopped = run(
        &mut at(&root),
        &["sim", "infinito_tb.v", "--timeout", "1"],
        1,
    );
    assert_eq!(stopped["simulation"]["status"], "timed_out");
    conforms("sim", &stopped);

    // --events: cada linha confere com events.json, e o resultado da última,
    // com o schema do comando.
    let out = at(&root)
        .args(["--events", "sim", "porta_tb.v"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let lines: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(lines.len() > 1, "{lines:?}");
    for line in &lines {
        conforms("events", line);
    }
    conforms("sim", &lines.last().unwrap()["result"]);

    // O histórico que essas operações gravaram.
    conforms("report-list", &run(&mut at(&root), &["report", "list"], 0));
    conforms("report", &run(&mut at(&root), &["report"], 0));
    let clean = run(
        &mut at(&root),
        &["report", "clean", "--keep", "1", "--yes"],
        0,
    );
    assert!(!clean["removed"].as_array().unwrap().is_empty(), "{clean}");
    assert_eq!(clean["kept"], 1);
    conforms("report-clean", &clean);
}
