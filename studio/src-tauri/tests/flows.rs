//! Os fluxos do Studio (`src/flows.rs`) rodando de verdade, com o bundle do
//! Lace, sobre cópias dos exemplos do Lace (`examples/`, na raiz do
//! repositório).
//!
//! O bundle é o que o Studio acharia sozinho (a instalação do Lace) ou o de
//! `LACE_TEST_BUNDLE`. Sem bundle, os testes avisam e passam, como os do
//! Lace; com a variável `CI` definida, falham, porque lá não podem pular.
//!
//! ```sh
//! cargo test --test flows                                 # instalação do Lace
//! LACE_TEST_BUNDLE=/caminho/do/toolchain cargo test --test flows
//! ```

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{Control, Simulator, Status};
use lace_studio_lib::flows::{self, FlowOutcome, FlowRequest, Progress};
use lace_studio_lib::settings::Settings;
use lace_studio_lib::toolchain;

/// As preferências apontando para o bundle de teste, ou `None` sem bundle.
fn settings() -> Option<Settings> {
    let mut settings = Settings::default();
    if let Ok(dir) = std::env::var("LACE_TEST_BUNDLE") {
        settings.toolchain_dir = Some(dir);
    }
    match toolchain::resolve(&settings) {
        Ok(_) => Some(settings),
        Err(error) if std::env::var_os("CI").is_some() => {
            panic!("CI definido, mas sem bundle do Lace: {error}")
        }
        Err(error) => {
            eprintln!("PULADO: sem bundle do Lace ({error}). Defina LACE_TEST_BUNDLE.");
            None
        }
    }
}

/// O bundle tem o componente?
fn has(settings: &Settings, component: &str) -> bool {
    toolchain::require(settings)
        .map(|t| t.component(component).is_some())
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

fn run(request: FlowRequest, settings: &Settings, spf: &Utf8Path) -> FlowOutcome {
    let progress = |_: Progress| {};
    flows::run(&request, settings, spf, &Control::default(), &progress).expect("o fluxo rodou")
}

#[test]
fn verilog_project_checks_simulates_and_synthesizes() {
    let Some(settings) = settings() else { return };
    if !has(&settings, "icarus") || !has(&settings, "yosys") || !has(&settings, "graphviz") {
        eprintln!("PULADO: o bundle não tem icarus, yosys e graphviz");
        return;
    }
    let (_dir, spf) = example("contador");

    let check = run(
        FlowRequest::Check {
            file: None,
            processor: None,
            lint: false,
        },
        &settings,
        &spf,
    );
    assert!(
        check.succeeded,
        "check: {:?}",
        check.check.map(|c| c.diagnostics)
    );
    assert!(check.report.is_some(), "o check grava relatório");

    let sim = run(
        FlowRequest::Simulate {
            processor: None,
            testbench: None,
            simulator: Simulator::Icarus,
            timeout_s: Some(60),
            open_wave: false,
        },
        &settings,
        &spf,
    );
    let simulation = sim.simulation.expect("simulou");
    assert_eq!(
        simulation.status,
        Status::Succeeded,
        "{:?}",
        simulation.diagnostics
    );
    assert!(
        simulation
            .waveform
            .as_ref()
            .is_some_and(|w| w.path.is_file())
    );
    assert!(sim.wave.is_none(), "open_wave falso não abre o Surfer");

    let synth = run(
        FlowRequest::Synthesize {
            processor: None,
            schematic: true,
            module: None,
        },
        &settings,
        &spf,
    );
    assert!(
        synth.succeeded,
        "{:?}",
        synth.synthesis.map(|s| s.diagnostics)
    );
    let schematic = synth.schematic.expect("desenhou");
    assert!(schematic.svg.as_ref().is_some_and(|svg| svg.is_file()));
    assert!(
        synth
            .synthesis
            .as_ref()
            .and_then(|s| s.statistics.as_ref())
            .is_some_and(|s| s.cells.is_some()),
        "a síntese traz as estatísticas do stat"
    );

    // Desenhar outro módulo do mesmo netlist, sem sintetizar de novo.
    let netlist = synth.synthesis.unwrap().netlist.unwrap();
    let again = run(
        FlowRequest::Schematic {
            netlist,
            module: schematic.module.clone(),
            bus_widths: false,
        },
        &settings,
        &spf,
    );
    assert!(again.succeeded);
    assert!(again.report.is_none(), "redesenhar não grava relatório");
}

#[test]
fn processor_builds_and_simulates_with_port_values() {
    let Some(settings) = settings() else { return };
    if !has(&settings, "yanc") || !has(&settings, "icarus") {
        eprintln!("PULADO: o bundle não tem yanc e icarus");
        return;
    }
    let (_dir, spf) = example("soma");

    let build = run(
        FlowRequest::Build {
            processors: vec!["soma".into()],
        },
        &settings,
        &spf,
    );
    assert!(
        build.succeeded,
        "{:?}",
        build.builds.first().map(|b| &b.diagnostics)
    );
    assert_eq!(build.builds.len(), 1);

    let sim = run(
        FlowRequest::Simulate {
            processor: Some("soma".into()),
            testbench: None,
            simulator: Simulator::Icarus,
            timeout_s: Some(60),
            open_wave: false,
        },
        &settings,
        &spf,
    );
    assert!(sim.succeeded, "{:?}", sim.simulation.map(|s| s.diagnostics));
    // soma.cmm escreve 1 + 2 + ... + 10 na porta 0.
    let port0 = sim.outputs.iter().find(|p| p.port == 0).expect("porta 0");
    assert_eq!(port0.values.last(), Some(&55));

    // O alvo no F7: verifica o Verilog do soma, que o build acima compilou,
    // com o testbench do YANC, sem compilar de novo.
    let check = run(
        FlowRequest::Check {
            file: None,
            processor: Some("soma".into()),
            lint: false,
        },
        &settings,
        &spf,
    );
    assert!(check.succeeded, "{:?}", check.check.map(|c| c.diagnostics));
    assert!(check.builds.is_empty(), "o F7 não compila");
    assert_eq!(check.command, "lace-studio check -p soma");
    let targets = &check.check.as_ref().unwrap().targets;
    assert!(targets.iter().any(|t| t == "soma_tb"), "{targets:?}");
}

#[test]
fn build_error_comes_back_as_diagnostic_not_err() {
    let Some(settings) = settings() else { return };
    if !has(&settings, "yanc") {
        eprintln!("PULADO: o bundle não tem yanc");
        return;
    }
    let (_dir, spf) = example("com_erro");
    let build = run(
        FlowRequest::Build {
            processors: vec!["conta".into()],
        },
        &settings,
        &spf,
    );
    assert!(!build.succeeded);
    let result = &build.builds[0];
    assert_eq!(result.status, Status::Failed);
    let error = result.first_error().expect("diagnóstico de erro");
    assert_eq!(error.line, Some(16), "{error:?}");
    assert!(
        error
            .file
            .as_ref()
            .is_some_and(|f| f.as_str().ends_with("conta.cmm"))
    );
}

#[test]
fn verilog_only_project_build_is_a_no_op() {
    let Some(settings) = settings() else { return };
    let (_dir, spf) = example("contador");
    let build = run(FlowRequest::Build { processors: vec![] }, &settings, &spf);
    assert!(build.succeeded);
    assert!(build.builds.is_empty());
}

#[test]
fn command_line_mirrors_the_cli() {
    let request = FlowRequest::Simulate {
        processor: Some("soma".into()),
        testbench: None,
        simulator: Simulator::Verilator,
        timeout_s: Some(30),
        open_wave: true,
    };
    assert_eq!(
        request.command_line(),
        "lace-studio sim -p soma --verilator --timeout 30 --open"
    );
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
