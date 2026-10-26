//! Os fluxos do Studio (`src/flows.rs`) rodando de verdade: o pedido que a
//! interface manda vai ao servidor do Lace (`lace serve`), que roda o fluxo
//! com o bundle sobre cópias dos exemplos do Lace (`examples/`, na raiz do
//! repositório), e a resposta volta como a interface a lê.
//!
//! O servidor é o `lace` de `LACE_STUDIO_LACE` ou, sem ela, o build de
//! desenvolvimento em `target/debug` da raiz do repositório. O bundle é o
//! que o Studio acharia sozinho (a instalação do Lace) ou o de
//! `LACE_TEST_BUNDLE`. Sem um dos dois, os testes avisam e passam, como os
//! do Lace; com a variável `CI` definida, falham, porque lá não podem
//! pular.
//!
//! ```sh
//! cargo build -p lace-cli                                  # na raiz
//! cargo test --test flows                                  # instalação do Lace
//! LACE_TEST_BUNDLE=/caminho/do/toolchain cargo test --test flows
//! ```

use camino::{Utf8Path, Utf8PathBuf};
use lace_studio_lib::error::IpcError;
use lace_studio_lib::flows;
use lace_studio_lib::jobs::JobMessage;
use lace_studio_lib::server::LaceServer;
use lace_studio_lib::settings::Settings;
use lace_studio_lib::toolchain;
use serde_json::{Value, json};

fn skip(reason: &str) -> Option<Settings> {
    if std::env::var_os("CI").is_some() {
        panic!("CI definido, mas {reason}");
    }
    eprintln!("PULADO: {reason}");
    None
}

/// As preferências apontando para o bundle de teste e para o `lace` do
/// servidor, ou `None` sem um dos dois.
fn settings() -> Option<Settings> {
    let mut settings = Settings::default();
    if let Ok(dir) = std::env::var("LACE_TEST_BUNDLE") {
        settings.toolchain_dir = Some(dir);
    }
    if std::env::var_os("LACE_STUDIO_LACE").is_none() {
        let exe = if cfg!(windows) { "lace.exe" } else { "lace" };
        let dev = Utf8Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/debug")
            .join(exe);
        if !dev.is_file() {
            return skip(
                "sem o lace do servidor: rode cargo build -p lace-cli ou defina LACE_STUDIO_LACE",
            );
        }
        settings.lace_path = Some(dev.to_string());
    }
    let info = toolchain::info(&LaceServer::default(), &settings);
    if info["found"] != true {
        return skip(&format!(
            "sem bundle do Lace ({}). Defina LACE_TEST_BUNDLE.",
            info["error"]["message"]
        ));
    }
    Some(settings)
}

/// O `toolchain.info` do servidor.
fn info(server: &LaceServer, settings: &Settings) -> Value {
    toolchain::info(server, settings)
}

/// O bundle tem o componente?
fn has(server: &LaceServer, settings: &Settings, component: &str) -> bool {
    info(server, settings)["components"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|c| c["name"] == component || c["id"] == component)
}

/// A ferramenta está disponível?
fn tool(info: &Value, name: &str) -> bool {
    info["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|t| t["name"] == name && !t["path"].is_null())
}

/// O Verilator roda: a ferramenta e o Perl (no Linux e no macOS, o do
/// sistema).
fn has_verilator(server: &LaceServer, settings: &Settings) -> bool {
    let info = info(server, settings);
    tool(&info, "verilator") && tool(&info, "perl")
}

fn copy_dir(from: &Utf8Path, to: &Utf8Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in from.read_dir_utf8().unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Copia `examples/<nome>` (da raiz do repositório) para uma pasta
/// temporária e devolve o `.spf` da cópia.
fn example(name: &str) -> (tempfile::TempDir, Utf8PathBuf) {
    let examples = Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().join(name)).unwrap();
    copy_dir(&examples.join(name), &root);
    let spf = root.join(format!("{name}.spf"));
    (dir, spf)
}

/// O fluxo pelo servidor, como `jobs.rs` o roda: a mensagem final.
fn finish(server: &LaceServer, request: &Value, settings: &Settings, spf: &Utf8Path) -> JobMessage {
    let (operation, finisher) =
        flows::plan(server, settings, spf, request.clone()).expect("o plano do fluxo");
    let reply = server.call_with(
        settings,
        operation.method,
        operation.params,
        Box::new(|_, _| {}),
        |_| {},
    );
    finisher.finish(reply)
}

/// O resultado de um fluxo que rodou (`finished`), como a interface o lê.
fn run(server: &LaceServer, request: Value, settings: &Settings, spf: &Utf8Path) -> Value {
    match finish(server, &request, settings, spf) {
        JobMessage::Finished { outcome } => outcome,
        JobMessage::Failed { error } => panic!("o fluxo não rodou: {error}"),
        _ => panic!("outra mensagem"),
    }
}

/// O erro de um fluxo que o Lace não conseguiu rodar (`failed`).
fn fail(server: &LaceServer, request: &Value, settings: &Settings, spf: &Utf8Path) -> IpcError {
    match finish(server, request, settings, spf) {
        JobMessage::Failed { error } => error,
        JobMessage::Finished { outcome } => panic!("o fluxo rodou: {outcome}"),
        _ => panic!("outra mensagem"),
    }
}

fn succeeded(outcome: &Value) -> bool {
    outcome["succeeded"] == true
}

/// O comando de um fluxo, como o console e o histórico o mostram.
fn command(server: &LaceServer, settings: &Settings, request: Value) -> String {
    server
        .call(
            settings,
            "flow.command",
            json!({ "request": request, "program": "lace-studio" }),
        )
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn verilog_project_checks_simulates_and_synthesizes() {
    let Some(settings) = settings() else { return };
    let server = LaceServer::default();
    if !["icarus", "yosys", "graphviz"]
        .iter()
        .all(|c| has(&server, &settings, c))
    {
        eprintln!("PULADO: o bundle não tem icarus, yosys e graphviz");
        return;
    }
    let (_dir, spf) = example("contador");

    let check = run(&server, json!({ "flow": "check" }), &settings, &spf);
    assert!(
        succeeded(&check),
        "check: {}",
        check["check"]["diagnostics"]
    );
    assert!(!check["report"].is_null(), "o check grava relatório");

    let simulate = json!({ "flow": "simulate", "simulator": "icarus", "timeout_s": 60 });
    let sim = run(&server, simulate, &settings, &spf);
    let simulation = &sim["simulation"];
    assert_eq!(
        simulation["status"], "succeeded",
        "{}",
        simulation["diagnostics"]
    );
    let waveform = simulation["waveform"]["path"].as_str().expect("onda");
    assert!(Utf8Path::new(waveform).is_file());
    assert!(sim["wave"].is_null(), "open_wave falso não abre o Surfer");

    // A Rápida (F9): o testbench Verilog no Verilator, mesmo com o Icarus
    // escolhido, e sem onda, então nada abre.
    if has_verilator(&server, &settings) {
        let fast = json!({
            "flow": "simulate", "simulator": "icarus", "fast": true,
            "timeout_s": 300, "open_wave": true,
        });
        let fast = run(&server, fast, &settings, &spf);
        assert_eq!(fast["command"], "lace-studio sim --fast --timeout 300");
        let simulation = &fast["simulation"];
        assert_eq!(
            simulation["status"], "succeeded",
            "{}",
            simulation["diagnostics"]
        );
        assert_eq!(simulation["fast"], true);
        assert_eq!(simulation["simulator"], "verilator");
        assert!(simulation["waveform"].is_null());
        assert!(fast["wave"].is_null() && fast["wave_tab"].is_null());
    }

    let synthesize = json!({ "flow": "synthesize", "schematic": true });
    let synth = run(&server, synthesize, &settings, &spf);
    assert!(succeeded(&synth), "{}", synth["synthesis"]["diagnostics"]);
    let schematic = &synth["schematic"];
    let svg = schematic["svg"].as_str().expect("desenhou");
    assert!(Utf8Path::new(svg).is_file());
    assert!(
        !synth["synthesis"]["statistics"]["cells"].is_null(),
        "a síntese traz as estatísticas do stat"
    );

    // Desenhar outro módulo do mesmo netlist, sem sintetizar de novo.
    let again = json!({
        "flow": "schematic",
        "netlist": synth["synthesis"]["netlist"],
        "module": schematic["module"],
        "bus_widths": false,
    });
    let again = run(&server, again, &settings, &spf);
    assert!(succeeded(&again));
    assert!(again["report"].is_null(), "redesenhar não grava relatório");
    server.stop();
}

#[test]
fn processor_builds_and_simulates_with_port_values() {
    let Some(settings) = settings() else { return };
    let server = LaceServer::default();
    if !has(&server, &settings, "yanc") || !has(&server, &settings, "icarus") {
        eprintln!("PULADO: o bundle não tem yanc e icarus");
        return;
    }
    let (_dir, spf) = example("soma");

    let build = run(
        &server,
        json!({ "flow": "build", "processors": ["soma"] }),
        &settings,
        &spf,
    );
    assert!(succeeded(&build), "{}", build["builds"][0]["diagnostics"]);
    assert_eq!(build["builds"].as_array().map(Vec::len), Some(1));

    let simulate =
        json!({ "flow": "simulate", "processor": "soma", "simulator": "icarus", "timeout_s": 60 });
    let sim = run(&server, simulate, &settings, &spf);
    assert!(succeeded(&sim), "{}", sim["simulation"]["diagnostics"]);
    // soma.cmm escreve 1 + 2 + ... + 10 na porta 0.
    let last = |outcome: &Value| {
        outcome["outputs"]
            .as_array()
            .and_then(|ports| ports.iter().find(|p| p["port"] == 0))
            .and_then(|port| port["values"].as_array()?.last().cloned())
    };
    assert_eq!(last(&sim), Some(Value::from(55)));

    // A Rápida no processador: as mesmas saídas, sem onda.
    if has_verilator(&server, &settings) {
        let fast = json!({
            "flow": "simulate", "processor": "soma", "simulator": "icarus",
            "fast": true, "timeout_s": 300,
        });
        let fast = run(&server, fast, &settings, &spf);
        assert!(succeeded(&fast), "{}", fast["simulation"]["diagnostics"]);
        assert_eq!(last(&fast), Some(Value::from(55)));
        assert!(fast["simulation"]["waveform"].is_null());
    }

    // O alvo no F7: verifica o Verilog do soma, que o build acima compilou,
    // com o testbench do YANC, sem compilar de novo.
    let check = run(
        &server,
        json!({ "flow": "check", "processor": "soma" }),
        &settings,
        &spf,
    );
    assert!(succeeded(&check), "{}", check["check"]["diagnostics"]);
    assert_eq!(
        check["builds"].as_array().map(Vec::len),
        Some(0),
        "o F7 não compila"
    );
    assert_eq!(check["command"], "lace-studio check -p soma");
    let targets = check["check"]["targets"].as_array().unwrap();
    assert!(targets.iter().any(|t| t == "soma_tb"), "{targets:?}");
    server.stop();
}

#[test]
fn build_error_comes_back_as_diagnostic_not_err() {
    let Some(settings) = settings() else { return };
    let server = LaceServer::default();
    if !has(&server, &settings, "yanc") {
        eprintln!("PULADO: o bundle não tem yanc");
        return;
    }
    let (_dir, spf) = example("com_erro");
    let build = run(
        &server,
        json!({ "flow": "build", "processors": ["conta"] }),
        &settings,
        &spf,
    );
    assert!(!succeeded(&build));
    let result = &build["builds"][0];
    assert_eq!(result["status"], "failed");
    let error = result["diagnostics"]
        .as_array()
        .and_then(|d| d.iter().find(|d| d["severity"] == "error"))
        .expect("diagnóstico de erro");
    assert_eq!(error["line"], 15, "{error}");
    assert!(
        error["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("conta.cmm"))
    );
    server.stop();
}

#[test]
fn verilog_only_project_build_is_a_no_op() {
    let Some(settings) = settings() else { return };
    let server = LaceServer::default();
    let (_dir, spf) = example("contador");
    let build = run(&server, json!({ "flow": "build" }), &settings, &spf);
    assert!(succeeded(&build));
    assert_eq!(build["builds"].as_array().map(Vec::len), Some(0));
    server.stop();
}

#[test]
fn command_line_mirrors_the_cli() {
    let Some(settings) = settings() else { return };
    let server = LaceServer::default();
    let simulate = json!({
        "flow": "simulate", "processor": "soma", "simulator": "verilator",
        "timeout_s": 30, "open_wave": true,
    });
    assert_eq!(
        command(&server, &settings, simulate),
        "lace-studio sim -p soma --verilator --timeout 30 --open"
    );
    // A Rápida não tem onda para abrir: o `--open` sai, como a CLI exige.
    let fast =
        json!({ "flow": "simulate", "simulator": "icarus", "fast": true, "open_wave": true });
    assert_eq!(command(&server, &settings, fast), "lace-studio sim --fast");
    let check = json!({ "flow": "check", "file": "rtl/a b.v", "lint": true });
    assert_eq!(
        command(&server, &settings, check),
        "lace-studio check \"rtl/a b.v\" --lint"
    );
    let check = json!({ "flow": "check", "processor": "soma" });
    assert_eq!(
        command(&server, &settings, check),
        "lace-studio check -p soma"
    );
    server.stop();
}

/// O fluxo `learn`: corrige um exercício da trilha de teste do lace-learn
/// (`crates/lace-learn/tests/fixtures`, pela preferência `learn_dir`, que o
/// servidor recebe em `LACE_LEARN_DIR`), grava o resolvido e não grava
/// relatório. A pasta de exercícios também é criada pelo servidor.
#[test]
fn learn_flow_checks_an_exercise_and_records_it() {
    let Some(mut settings) = settings() else {
        return;
    };
    let fixtures =
        Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/lace-learn/tests/fixtures");
    settings.learn_dir = Some(fixtures.to_string());
    let server = LaceServer::default();
    if !has(&server, &settings, "icarus") {
        eprintln!("PULADO: o bundle não tem icarus");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().join("ex")).unwrap();
    let snapshot = server
        .call(
            &settings,
            "learn.init",
            json!({ "dir": root, "track": "teste" }),
        )
        .unwrap();
    let mux2 = |snapshot: &Value| {
        snapshot["chapters"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|c| c["exercises"].as_array().cloned().unwrap_or_default())
            .find(|e| e["name"] == "mux2")
            .expect("o exercício mux2")
    };
    let exercise = mux2(&snapshot);
    let spf = Utf8PathBuf::from(exercise["spf"].as_str().unwrap());
    let file = Utf8PathBuf::from(exercise["file"].as_str().unwrap());

    let request = json!({ "flow": "learn", "root": root, "exercise": "mux2" });
    assert_eq!(
        command(&server, &settings, request.clone()),
        "lace-studio learn check mux2"
    );
    // O arquivo inicial não atribui a saída.
    let wrong = run(&server, request.clone(), &settings, &spf);
    assert!(!succeeded(&wrong));
    assert_eq!(wrong["flow"], "learn");
    assert_eq!(wrong["learn"]["verdict"], "mismatch", "{}", wrong["learn"]);
    assert!(wrong["report"].is_null(), "a correção não grava relatório");

    std::fs::write(
        &file,
        "module mux2(input a, input b, input sel, output y);\n    assign y = sel ? b : a;\nendmodule\n",
    )
    .unwrap();
    let right = run(&server, request, &settings, &spf);
    assert!(succeeded(&right), "{}", right["learn"]);
    let reopened = server
        .call(&settings, "learn.status", json!({ "root": root }))
        .unwrap();
    assert_eq!(mux2(&reopened)["solved"], true);
    server.stop();
}

/// O fluxo `fpga_build` (`lace fpga build`): o pedido que a interface manda
/// e as recusas antes de compilar qualquer processador, como na CLI. Sem
/// Quartus na máquina de teste, a compilação em si não roda aqui.
#[test]
fn fpga_build_refuses_before_building() {
    let Some(mut settings) = settings() else {
        return;
    };
    // Uma pasta do Quartus sem o Quartus: nenhum é usado, nem o da máquina.
    let empty = tempfile::tempdir().unwrap();
    settings.quartus_dir = Some(empty.path().to_string_lossy().into_owned());
    let server = LaceServer::default();
    let request = json!({ "flow": "fpga_build" });
    assert_eq!(
        command(&server, &settings, request.clone()),
        "lace-studio fpga build"
    );
    let program = json!({ "flow": "fpga_program", "cable": "USB-Blaster [USB-0]" });
    assert_eq!(
        command(&server, &settings, program),
        "lace-studio fpga program --cable \"USB-Blaster [USB-0]\""
    );
    let (_dir, spf) = example("soma");
    // Com a placa desligada nesta máquina, o fluxo recusa antes de olhar o
    // projeto (o Lace confere o recurso experimental, como a CLI).
    let experimental = server
        .call(&settings, "experimental.list", Value::Null)
        .unwrap();
    let fpga = experimental["components"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|c| c["feature"] == "fpga" && c["enabled"] == true);
    if !fpga {
        let error = fail(&server, &request, &settings, &spf);
        assert_eq!(error.code, "experimental_disabled");
        server.stop();
        return;
    }
    let error = fail(&server, &request, &settings, &spf);
    assert_eq!(error.code, "no_fpga_config");

    let root = spf.parent().unwrap();
    std::fs::write(
        root.join("fpga.json"),
        r#"{"board": "de2-115", "connect": {"clk": "CLOCK_50", "rst": "!KEY[0]"}}"#,
    )
    .unwrap();
    let error = fail(&server, &request, &settings, &spf);
    assert_eq!(error.code, "quartus_missing");
    // Nenhum processador foi compilado.
    assert!(!root.join("soma/soma/soma.v").exists());
    server.stop();
}
