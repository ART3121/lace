//! Os fluxos do Studio (`src/flows.rs`) rodando de verdade: o pedido que o
//! Studio monta vai ao servidor do Lace (`lace serve`), que roda o fluxo com
//! o bundle sobre cópias dos exemplos do Lace (`examples/`, na raiz do
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
use lace_core::{Simulator, Tool};
use lace_studio_lib::error::IpcError;
use lace_studio_lib::flows::FlowRequest;
use lace_studio_lib::jobs::JobMessage;
use lace_studio_lib::server::LaceServer;
use lace_studio_lib::settings::Settings;
use lace_studio_lib::toolchain;
use serde_json::Value;

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
    if let Err(error) = toolchain::resolve(&settings) {
        return skip(&format!(
            "sem bundle do Lace ({error}). Defina LACE_TEST_BUNDLE."
        ));
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
    Some(settings)
}

/// O bundle tem o componente?
fn has(settings: &Settings, component: &str) -> bool {
    toolchain::require(settings)
        .map(|t| t.component(component).is_some())
        .unwrap_or(false)
}

/// O Verilator roda: o componente e o compilador C++ (no Linux e no macOS,
/// o do sistema).
fn has_verilator(settings: &Settings) -> bool {
    toolchain::require(settings)
        .map(|t| t.tool(Tool::Verilator).is_ok() && t.tool(Tool::Perl).is_ok())
        .unwrap_or(false)
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
fn finish(
    server: &LaceServer,
    request: &FlowRequest,
    settings: &Settings,
    spf: &Utf8Path,
) -> JobMessage {
    let (operation, finisher) = request.plan(settings, spf);
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
fn run(server: &LaceServer, request: FlowRequest, settings: &Settings, spf: &Utf8Path) -> Value {
    match finish(server, &request, settings, spf) {
        JobMessage::Finished { outcome } => outcome,
        JobMessage::Failed { error } => panic!("o fluxo não rodou: {error}"),
        _ => panic!("outra mensagem"),
    }
}

/// O erro de um fluxo que o Lace não conseguiu rodar (`failed`).
fn fail(
    server: &LaceServer,
    request: &FlowRequest,
    settings: &Settings,
    spf: &Utf8Path,
) -> IpcError {
    match finish(server, request, settings, spf) {
        JobMessage::Failed { error } => error,
        JobMessage::Finished { outcome } => panic!("o fluxo rodou: {outcome}"),
        _ => panic!("outra mensagem"),
    }
}

fn succeeded(outcome: &Value) -> bool {
    outcome["succeeded"] == true
}

#[test]
fn verilog_project_checks_simulates_and_synthesizes() {
    let Some(settings) = settings() else { return };
    if !has(&settings, "icarus") || !has(&settings, "yosys") || !has(&settings, "graphviz") {
        eprintln!("PULADO: o bundle não tem icarus, yosys e graphviz");
        return;
    }
    let server = LaceServer::default();
    let (_dir, spf) = example("contador");

    let check = run(
        &server,
        FlowRequest::Check {
            file: None,
            processor: None,
            lint: false,
        },
        &settings,
        &spf,
    );
    assert!(
        succeeded(&check),
        "check: {}",
        check["check"]["diagnostics"]
    );
    assert!(!check["report"].is_null(), "o check grava relatório");

    let sim = run(
        &server,
        FlowRequest::Simulate {
            processor: None,
            testbench: None,
            simulator: Simulator::Icarus,
            fast: false,
            timeout_s: Some(60),
            open_wave: false,
        },
        &settings,
        &spf,
    );
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
    if has_verilator(&settings) {
        let fast = run(
            &server,
            FlowRequest::Simulate {
                processor: None,
                testbench: None,
                simulator: Simulator::Icarus,
                fast: true,
                timeout_s: Some(300),
                open_wave: true,
            },
            &settings,
            &spf,
        );
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

    let synth = run(
        &server,
        FlowRequest::Synthesize {
            processor: None,
            schematic: true,
            module: None,
            open_viewer: false,
        },
        &settings,
        &spf,
    );
    assert!(succeeded(&synth), "{}", synth["synthesis"]["diagnostics"]);
    let schematic = &synth["schematic"];
    let svg = schematic["svg"].as_str().expect("desenhou");
    assert!(Utf8Path::new(svg).is_file());
    assert!(
        !synth["synthesis"]["statistics"]["cells"].is_null(),
        "a síntese traz as estatísticas do stat"
    );

    // Desenhar outro módulo do mesmo netlist, sem sintetizar de novo.
    let netlist = synth["synthesis"]["netlist"].as_str().unwrap();
    let again = run(
        &server,
        FlowRequest::Schematic {
            netlist: netlist.into(),
            module: schematic["module"].as_str().unwrap().to_owned(),
            bus_widths: false,
        },
        &settings,
        &spf,
    );
    assert!(succeeded(&again));
    assert!(again["report"].is_null(), "redesenhar não grava relatório");
    server.stop();
}

#[test]
fn processor_builds_and_simulates_with_port_values() {
    let Some(settings) = settings() else { return };
    if !has(&settings, "yanc") || !has(&settings, "icarus") {
        eprintln!("PULADO: o bundle não tem yanc e icarus");
        return;
    }
    let server = LaceServer::default();
    let (_dir, spf) = example("soma");

    let build = run(
        &server,
        FlowRequest::Build {
            processors: vec!["soma".into()],
        },
        &settings,
        &spf,
    );
    assert!(succeeded(&build), "{}", build["builds"][0]["diagnostics"]);
    assert_eq!(build["builds"].as_array().map(Vec::len), Some(1));

    let sim = run(
        &server,
        FlowRequest::Simulate {
            processor: Some("soma".into()),
            testbench: None,
            simulator: Simulator::Icarus,
            fast: false,
            timeout_s: Some(60),
            open_wave: false,
        },
        &settings,
        &spf,
    );
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
    if has_verilator(&settings) {
        let fast = run(
            &server,
            FlowRequest::Simulate {
                processor: Some("soma".into()),
                testbench: None,
                simulator: Simulator::Icarus,
                fast: true,
                timeout_s: Some(300),
                open_wave: false,
            },
            &settings,
            &spf,
        );
        assert!(succeeded(&fast), "{}", fast["simulation"]["diagnostics"]);
        assert_eq!(last(&fast), Some(Value::from(55)));
        assert!(fast["simulation"]["waveform"].is_null());
    }

    // O alvo no F7: verifica o Verilog do soma, que o build acima compilou,
    // com o testbench do YANC, sem compilar de novo.
    let check = run(
        &server,
        FlowRequest::Check {
            file: None,
            processor: Some("soma".into()),
            lint: false,
        },
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
    if !has(&settings, "yanc") {
        eprintln!("PULADO: o bundle não tem yanc");
        return;
    }
    let server = LaceServer::default();
    let (_dir, spf) = example("com_erro");
    let build = run(
        &server,
        FlowRequest::Build {
            processors: vec!["conta".into()],
        },
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
    let build = run(
        &server,
        FlowRequest::Build { processors: vec![] },
        &settings,
        &spf,
    );
    assert!(succeeded(&build));
    assert_eq!(build["builds"].as_array().map(Vec::len), Some(0));
    server.stop();
}

#[test]
fn command_line_mirrors_the_cli() {
    let request = FlowRequest::Simulate {
        processor: Some("soma".into()),
        testbench: None,
        simulator: Simulator::Verilator,
        fast: false,
        timeout_s: Some(30),
        open_wave: true,
    };
    assert_eq!(
        request.command_line(),
        "lace-studio sim -p soma --verilator --timeout 30 --open"
    );
    // A Rápida não tem onda para abrir: o `--open` sai, como a CLI exige.
    let request = FlowRequest::Simulate {
        processor: None,
        testbench: None,
        simulator: Simulator::Icarus,
        fast: true,
        timeout_s: None,
        open_wave: true,
    };
    assert_eq!(request.command_line(), "lace-studio sim --fast");
    let request = FlowRequest::Check {
        file: Some("rtl/a b.v".into()),
        processor: None,
        lint: true,
    };
    assert_eq!(
        request.command_line(),
        "lace-studio check \"rtl/a b.v\" --lint"
    );
    let request = FlowRequest::Check {
        file: None,
        processor: Some("soma".into()),
        lint: false,
    };
    assert_eq!(request.command_line(), "lace-studio check -p soma");
}

/// O fluxo `learn`: corrige um exercício da trilha de teste do lace-learn
/// (`crates/lace-learn/tests/fixtures`, pela preferência `learn_dir`, que o
/// servidor recebe em `LACE_LEARN_DIR`), grava o resolvido e não grava
/// relatório.
#[test]
fn learn_flow_checks_an_exercise_and_records_it() {
    let Some(mut settings) = settings() else {
        return;
    };
    if !has(&settings, "icarus") {
        eprintln!("PULADO: o bundle não tem icarus");
        return;
    }
    let fixtures =
        Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/lace-learn/tests/fixtures");
    settings.learn_dir = Some(fixtures.to_string());
    let server = LaceServer::default();
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().join("ex")).unwrap();
    let track = lace_learn::load_track(&fixtures, "teste", None).unwrap();
    let workspace = lace_learn::Workspace::init(&root, track).unwrap();
    let mux2 = workspace.track().require("mux2").unwrap().clone();
    let spf = workspace.exercise_dir(&mux2).join("mux2.spf");

    let request = FlowRequest::Learn {
        root: root.clone(),
        exercise: "mux2".into(),
    };
    assert_eq!(request.command_line(), "lace-studio learn check mux2");
    // O arquivo inicial não atribui a saída.
    let wrong = run(&server, request.clone(), &settings, &spf);
    assert!(!succeeded(&wrong));
    assert_eq!(wrong["flow"], "learn");
    assert_eq!(wrong["learn"]["verdict"], "mismatch", "{}", wrong["learn"]);
    assert!(wrong["report"].is_null(), "a correção não grava relatório");

    std::fs::write(
        workspace.student_file(&mux2),
        "module mux2(input a, input b, input sel, output y);\n    assign y = sel ? b : a;\nendmodule\n",
    )
    .unwrap();
    let right = run(&server, request, &settings, &spf);
    assert!(succeeded(&right), "{}", right["learn"]);
    let reopened = lace_learn::Workspace::open(&root, &fixtures, None).unwrap();
    assert!(reopened.is_solved("mux2"));
    server.stop();
}

/// O fluxo `fpga_build` (`lace fpga build`): o pedido que a interface manda
/// e as recusas antes de compilar qualquer processador, como na CLI. Sem
/// Quartus na máquina de teste, a compilação em si não roda aqui.
#[test]
fn fpga_build_refuses_before_building() {
    let request: FlowRequest = serde_json::from_str(r#"{"flow":"fpga_build"}"#).unwrap();
    assert_eq!(request.command_line(), "lace-studio fpga build");
    let program: FlowRequest =
        serde_json::from_str(r#"{"flow":"fpga_program","cable":"USB-Blaster [USB-0]"}"#).unwrap();
    assert_eq!(
        program.command_line(),
        "lace-studio fpga program --cable \"USB-Blaster [USB-0]\""
    );
    let Some(mut settings) = settings() else {
        return;
    };
    // Uma pasta do Quartus sem o Quartus: nenhum é usado, nem o da máquina.
    let empty = tempfile::tempdir().unwrap();
    settings.quartus_dir = Some(empty.path().to_string_lossy().into_owned());
    let server = LaceServer::default();
    let (_dir, spf) = example("soma");
    // Com a placa desligada nesta máquina, o fluxo recusa antes de olhar o
    // projeto (o core confere o recurso experimental, como a CLI).
    if !lace_core::experimental::is_enabled(lace_core::experimental::Feature::Fpga) {
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
