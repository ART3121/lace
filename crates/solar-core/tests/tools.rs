//! Simulação, checagem, síntese, esquemático e visualizador, com as
//! ferramentas do bundle. Precisam de `SOLAR_TEST_BUNDLE` com um bundle
//! completo (ver `common/mod.rs`).

mod common;

use std::time::Duration;

use solar_core::{
    BuildOptions, DesignTarget, FileRole, Project, SchematicOptions, Severity, SimulationOptions,
    Simulator, Status, Step, Tool, ViewerOptions, WaveformFormat, build, check_syntax,
    open_waveform, render_schematic, simulate, simulate_project, synthesize,
};

fn built(name: &str) -> (tempfile::TempDir, Project) {
    let (guard, root) = common::example("soma");
    let project = Project::open(&root).unwrap();
    let toolchain = common::toolchain().unwrap();
    let result = build(
        &toolchain,
        project.require_processor(name).unwrap(),
        &BuildOptions::default(),
    )
    .unwrap();
    assert!(result.succeeded(), "{result:#?}");
    (guard, project)
}

fn is_fst(path: &camino::Utf8Path) -> bool {
    // Cabeçalho FST: bloco 0 (FST_BL_HDR) seguido do tamanho de 8 bytes.
    let bytes = std::fs::read(path).unwrap();
    bytes.first() == Some(&0) && !bytes.starts_with(b"$")
}

#[test]
fn icarus_simulates_both_languages() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    for name in ["soma", "filtro"] {
        let (_guard, project) = built(name);
        let processor = project.require_processor(name).unwrap();
        let result = simulate(
            &toolchain,
            processor,
            &SimulationOptions::new(Simulator::Icarus),
        )
        .unwrap();
        assert_eq!(result.status, Status::Succeeded, "{result:#?}");
        assert_eq!(processor.read_output(0).unwrap().trim(), "55", "{name}");
        assert_eq!(result.outputs, [processor.output_path(0)]);
        let wave = result.waveform.unwrap();
        assert_eq!(wave.format, WaveformFormat::Fst);
        assert!(is_fst(&wave.path), "{} não é FST", wave.path);
        // O `$finish called` do Icarus é informação, não aviso.
        assert!(
            result
                .diagnostics
                .iter()
                .all(|d| d.severity == Severity::Info),
            "{:#?}",
            result.diagnostics
        );
    }
}

#[test]
fn verilator_simulates_processor() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Verilator]) else {
        return;
    };
    let (_guard, project) = built("soma");
    let processor = project.require_processor("soma").unwrap();
    let result = simulate(
        &toolchain,
        processor,
        &SimulationOptions::new(Simulator::Verilator),
    )
    .unwrap();
    assert_eq!(
        result.status,
        Status::Succeeded,
        "{:#?}",
        result.diagnostics
    );
    assert_eq!(processor.read_output(0).unwrap().trim(), "55");
    assert!(result.waveform.unwrap().path.is_file());
}

#[test]
fn simulation_requires_a_build() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, root) = common::example("soma");
    let project = Project::open(&root).unwrap();
    let err = simulate(
        &toolchain,
        project.require_processor("soma").unwrap(),
        &SimulationOptions::new(Simulator::Icarus),
    )
    .unwrap_err();
    assert!(
        matches!(err, solar_core::SolarError::NotBuilt { .. }),
        "{err}"
    );
}

#[test]
fn project_simulation_with_processor_testbench() {
    // O modo projeto da AURORA com o testbench padrão do processador: roda na
    // raiz, com o pc_soma_mem.txt copiado para lá.
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, mut project) = built("soma");
    let processor = project.require_processor("soma").unwrap().clone();
    let generated = processor.temp_dir.join("soma_tb.v");
    let tb = processor.simulation_dir().join("soma_tb.v");
    std::fs::copy(&generated, &tb).unwrap();
    project.set_testbench(&tb).unwrap();

    let result = simulate_project(
        &toolchain,
        &project,
        &SimulationOptions::new(Simulator::Icarus),
    )
    .unwrap();
    assert_eq!(result.status, Status::Succeeded, "{result:#?}");
    assert_eq!(result.top, "soma_tb");
    assert!(project.root().join("pc_soma_mem.txt").is_file());
    assert_eq!(
        result.waveform.unwrap().path,
        project.root().join("soma_tb.vcd")
    );
    assert_eq!(processor.read_output(0).unwrap().trim(), "55");
}

#[test]
fn project_simulation_injects_dump_when_missing() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, root) = common::example("contador");
    let project = Project::open(&root).unwrap();
    let result = simulate_project(
        &toolchain,
        &project,
        &SimulationOptions::new(Simulator::Icarus),
    )
    .unwrap();
    assert_eq!(result.status, Status::Succeeded, "{result:#?}");
    let vvp = result
        .steps
        .iter()
        .find(|s| s.step == Step::Simulate)
        .unwrap();
    assert!(vvp.stdout.contains("q = 10"), "{}", vvp.stdout);
    assert_eq!(result.waveform.unwrap().path, root.join("contador_tb.vcd"));
    // O testbench do usuário não é alterado; a cópia instrumentada fica no Temp.
    assert!(
        !std::fs::read_to_string(root.join("rtl/contador_tb.v"))
            .unwrap()
            .contains("$dumpvars")
    );
}

#[test]
fn project_simulation_with_verilator() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Verilator]) else {
        return;
    };
    let (_guard, root) = common::example("contador");
    let project = Project::open(&root).unwrap();
    let result = simulate_project(
        &toolchain,
        &project,
        &SimulationOptions::new(Simulator::Verilator),
    )
    .unwrap();
    assert_eq!(
        result.status,
        Status::Succeeded,
        "{:#?}",
        result.diagnostics
    );
    let run = result
        .steps
        .iter()
        .find(|s| s.step == Step::Simulate)
        .unwrap();
    assert!(run.stdout.contains("q = 10"), "{}", run.stdout);
}

#[test]
fn syntax_check_reports_file_and_line() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog]) else {
        return;
    };
    let (_guard, root) = common::example("contador");
    let mut project = Project::open(&root).unwrap();
    assert!(check_syntax(&toolchain, &project).unwrap().succeeded());

    let bad = project
        .add_file(
            FileRole::Synthesizable,
            "rtl/quebrado.v",
            Some("module quebrado(input a, output b);\n  assign b = a &;\nendmodule\n"),
        )
        .unwrap();
    let result = check_syntax(&toolchain, &project).unwrap();
    assert_eq!(result.status, Status::Failed);
    let error = result
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap();
    assert_eq!(error.file.as_deref(), Some(bad.as_path()));
    assert_eq!(error.line, Some(2));
}

#[test]
fn synthesis_and_schematic_of_processor_and_top_level() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Yosys, Tool::Dot]) else {
        return;
    };

    let (_guard, project) = built("soma");
    let synth = synthesize(
        &toolchain,
        &project,
        &DesignTarget::Processor("soma".into()),
    )
    .unwrap();
    assert_eq!(synth.status, Status::Succeeded, "{:#?}", synth.diagnostics);
    assert!(
        synth.modules.iter().any(|m| m == "soma"),
        "{:?}",
        synth.modules
    );
    let svg = render_schematic(
        &toolchain,
        synth.netlist.as_ref().unwrap(),
        "soma",
        &SchematicOptions::default(),
    )
    .unwrap();
    assert_eq!(svg.status, Status::Succeeded, "{:#?}", svg.diagnostics);
    assert!(
        std::fs::read_to_string(svg.svg.unwrap())
            .unwrap()
            .contains("<svg")
    );
    // No Linux, o dot usa as fontes do bundle, não a configuração do
    // sistema (nos outros, o pacote não traz fontes).
    if cfg!(target_os = "linux") {
        let render = svg.steps.iter().find(|s| s.step == Step::Render).unwrap();
        assert!(!render.stderr.contains("Fontconfig"), "{}", render.stderr);
        let conf = synth.netlist.as_ref().unwrap().with_file_name("fonts.conf");
        assert!(
            std::fs::read_to_string(conf)
                .unwrap()
                .contains("oss-cad-suite/share/fonts")
        );
    }

    let (_guard, root) = common::example("contador");
    let project = Project::open(&root).unwrap();
    let synth = synthesize(&toolchain, &project, &DesignTarget::TopLevel).unwrap();
    assert_eq!(synth.status, Status::Succeeded, "{:#?}", synth.diagnostics);
    let svg = render_schematic(
        &toolchain,
        synth.netlist.as_ref().unwrap(),
        "contador",
        &SchematicOptions::default(),
    )
    .unwrap();
    assert!(svg.succeeded(), "{:#?}", svg.diagnostics);
}

#[test]
fn synthesis_error_is_reported() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Yosys]) else {
        return;
    };
    let (_guard, root) = common::example("contador");
    let mut project = Project::open(&root).unwrap();
    let bad = project
        .add_file(
            FileRole::Synthesizable,
            "rtl/quebrado.v",
            Some("module quebrado(input a, output b);\n  assign b = a &;\nendmodule\n"),
        )
        .unwrap();
    let result = synthesize(&toolchain, &project, &DesignTarget::TopLevel).unwrap();
    assert_eq!(result.status, Status::Failed);
    let error = result
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap();
    assert_eq!(error.file.as_deref(), Some(bad.as_path()));
    assert_eq!(error.line, Some(2));
}

/// Abre uma janela de verdade: só roda com `SOLAR_TEST_GUI=1`.
#[test]
fn surfer_opens_and_closes() {
    if std::env::var_os("SOLAR_TEST_GUI").is_none() {
        eprintln!("PULADO: defina SOLAR_TEST_GUI=1 para abrir o Surfer");
        return;
    }
    let Some(toolchain) = common::toolchain_with(&[Tool::Surfer, Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, root) = common::example("contador");
    let project = Project::open(&root).unwrap();
    let sim = simulate_project(
        &toolchain,
        &project,
        &SimulationOptions::new(Simulator::Icarus),
    )
    .unwrap();
    let wave = sim.waveform.unwrap();
    let mut surfer = open_waveform(&toolchain, &wave.path, &ViewerOptions::default()).unwrap();
    let early = surfer.wait_timeout(Duration::from_secs(3)).unwrap();
    let log = std::fs::read_to_string(surfer.log_file()).unwrap_or_default();
    assert!(early.is_none(), "o Surfer fechou sozinho: {early:?}\n{log}");
    surfer.kill().unwrap();
}
