//! Simulação, checagem, síntese, esquemático e visualizador, com as
//! ferramentas do bundle. Precisam de `LACE_TEST_BUNDLE` com um bundle
//! completo (ver `common/mod.rs`).

mod common;

use std::time::Duration;

use lace_core::{
    BuildOptions, CheckOptions, Control, DesignTarget, FileRole, LaceError, Project,
    SchematicOptions, Severity, SimulationOptions, Simulator, Status, Step, Tool, ViewerOptions,
    WaveformFormat, build, check, open_waveform, render_schematic, simulate, simulate_project,
    synthesize, wave_layout,
};

fn built(name: &str) -> (tempfile::TempDir, Project) {
    let (guard, root) = common::example("soma");
    let project = Project::open(&root).unwrap();
    let toolchain = common::toolchain().unwrap();
    let result = build(
        &toolchain,
        project.require_processor(name).unwrap(),
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(result.succeeded(), "{result:#?}");
    (guard, project)
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
            &Control::default(),
        )
        .unwrap();
        assert_eq!(result.status, Status::Succeeded, "{result:#?}");
        assert_eq!(processor.read_output(0).unwrap().trim(), "55", "{name}");
        assert_eq!(result.outputs, [processor.output_path(0)]);
        // O testbench do asmcomp grava `<nome>_tb.vcd`, e o formato segue a
        // extensão do `$dumpfile`: só `.fst` liga o `-fst` do vvp.
        let wave = result.waveform.unwrap();
        assert_eq!(wave.path, processor.temp_dir.join(format!("{name}_tb.vcd")));
        assert_eq!(wave.format, WaveformFormat::Vcd);
        assert!(!common::is_fst(&wave.path), "{} não é VCD", wave.path);
        // O layout do Surfer acha o processador na onda e as tabelas que o
        // YANC deixou na pasta temporária dele.
        let layout = wave_layout(&wave.path).unwrap().expect("a processor wave");
        let found = &layout.processors[..];
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].processor, name);
        assert!(found[0].assembly && found[0].source, "{found:#?}");
        assert!(found[0].variables > 0, "{found:#?}");
        let asm = &layout.mappings[0];
        assert_eq!(asm.name, format!("lace_asm_{name}"));
        assert!(asm.content.contains("\n0 NOP"), "{}", asm.content);
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
        &Control::default(),
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
        &Control::default(),
    )
    .unwrap_err();
    assert!(
        matches!(err, lace_core::LaceError::NotBuilt { .. }),
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
        &Control::default(),
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
        &Control::default(),
    )
    .unwrap();
    assert_eq!(result.status, Status::Succeeded, "{result:#?}");
    let vvp = result
        .steps
        .iter()
        .find(|s| s.step == Step::Simulate)
        .unwrap();
    assert!(vvp.stdout.contains("q = 10"), "{}", vvp.stdout);
    // Sem `$dumpfile`, o Lace injeta `<testbench>.fst` na raiz, com
    // `$dumpvars(0, ...)`: o testbench e o contador.
    let wave = result.waveform.unwrap();
    assert_eq!(wave.path, root.join("contador_tb.fst"));
    assert_eq!(wave.format, WaveformFormat::Fst);
    assert!(common::is_fst(&wave.path));
    assert!(
        common::dumped_scopes(&wave.path) >= 2,
        "onda sem o módulo testado"
    );
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
        &Control::default(),
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

/// Só um processador: o Verilog dele e o testbench que o build gerou, sem o
/// resto do projeto (o `filtro`, não compilado, não entra).
#[test]
fn check_of_a_single_processor() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog]) else {
        return;
    };
    let (_guard, project) = built("soma");
    let mut options = CheckOptions::default();
    options.processor = Some("soma".into());
    let result = check(&toolchain, &project, &options, &Control::default()).unwrap();
    assert!(result.succeeded(), "{:#?}", result.diagnostics);
    assert_eq!(result.targets, ["soma", "soma_tb"]);
    assert_eq!(result.steps.len(), 2);

    options.processor = Some("filtro".into());
    assert!(matches!(
        check(&toolchain, &project, &options, &Control::default()),
        Err(LaceError::NotBuilt { processor, .. }) if processor == "filtro"
    ));
    options.processor = Some("nenhum".into());
    assert!(matches!(
        check(&toolchain, &project, &options, &Control::default()),
        Err(LaceError::ProcessorNotFound { .. })
    ));
}

#[test]
fn syntax_check_reports_file_and_line() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog]) else {
        return;
    };
    let (_guard, root) = common::example("contador");
    let mut project = Project::open(&root).unwrap();
    let options = CheckOptions::default();
    assert!(
        check(&toolchain, &project, &options, &Control::default())
            .unwrap()
            .succeeded()
    );

    let bad = project
        .add_file(
            FileRole::Synthesizable,
            "rtl/quebrado.v",
            Some("module quebrado(input a, output b);\n  assign b = a &;\nendmodule\n"),
        )
        .unwrap();
    let result = check(&toolchain, &project, &options, &Control::default()).unwrap();
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
        &Control::default(),
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
        &Control::default(),
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
    let synth = synthesize(
        &toolchain,
        &project,
        &DesignTarget::TopLevel,
        &Control::default(),
    )
    .unwrap();
    assert_eq!(synth.status, Status::Succeeded, "{:#?}", synth.diagnostics);
    let svg = render_schematic(
        &toolchain,
        synth.netlist.as_ref().unwrap(),
        "contador",
        &SchematicOptions::default(),
        &Control::default(),
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
    let result = synthesize(
        &toolchain,
        &project,
        &DesignTarget::TopLevel,
        &Control::default(),
    )
    .unwrap();
    assert_eq!(result.status, Status::Failed);
    let error = result
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap();
    assert_eq!(error.file.as_deref(), Some(bad.as_path()));
    assert_eq!(error.line, Some(2));
}

/// Abre uma janela de verdade: só roda com `LACE_TEST_GUI=1`.
#[test]
fn surfer_opens_and_closes() {
    if std::env::var_os("LACE_TEST_GUI").is_none() {
        eprintln!("PULADO: defina LACE_TEST_GUI=1 para abrir o Surfer");
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
        &Control::default(),
    )
    .unwrap();
    let wave = sim.waveform.unwrap();
    let mut surfer = open_waveform(&toolchain, &wave.path, &ViewerOptions::default()).unwrap();
    let early = surfer.wait_timeout(Duration::from_secs(3)).unwrap();
    let log = std::fs::read_to_string(surfer.log_file()).unwrap_or_default();
    assert!(early.is_none(), "o Surfer fechou sozinho: {early:?}\n{log}");
    surfer.kill().unwrap();
}

/// Vários processadores num projeto: um topo do usuário instancia o `soma`
/// (C±, 23 bits), o `filtro` (C, 32 bits) e o `soma` de novo, e a simulação
/// e a síntese do projeto levam o Verilog e o `pc_<nome>_mem.txt` de cada um.
#[test]
fn several_processors_simulate_and_synthesize_together() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp, Tool::Yosys]) else {
        return;
    };
    let (_guard, mut project) = built("soma");
    let filtro = project.require_processor("filtro").unwrap();
    let result = build(
        &toolchain,
        filtro,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(result.succeeded(), "{result:#?}");

    let root = project.root().to_owned();
    std::fs::create_dir_all(root.join("rtl")).unwrap();
    let top = root.join("rtl/duo.v");
    std::fs::write(
        &top,
        "module duo (input clk, rst,
             output signed [22:0] a, output a_en,
             output signed [31:0] b, output b_en,
             output signed [22:0] c, output c_en);
             soma   p0 (clk, rst, a, a_en);
             filtro p1 (clk, rst, b, b_en);
             soma   p2 (clk, rst, c, c_en);
         endmodule\n",
    )
    .unwrap();
    let tb = root.join("rtl/duo_tb.v");
    std::fs::write(
        &tb,
        "`timescale 1ns/1ps
         module duo_tb;
             reg clk = 0, rst = 1;
             wire signed [22:0] a, c; wire signed [31:0] b; wire a_en, b_en, c_en;
             duo dut (clk, rst, a, a_en, b, b_en, c, c_en);
             always #5 clk = ~clk;
             initial #10 rst = 0;
             always @(posedge clk) begin
                 if (a_en) $display(\"a = %0d\", a);
                 if (b_en) $display(\"b = %0d\", b);
                 if (c_en) $display(\"c = %0d\", c);
             end
             initial #50000 $finish;
         endmodule\n",
    )
    .unwrap();
    project
        .add_file(FileRole::Synthesizable, &top, None)
        .unwrap();
    project.add_file(FileRole::Testbench, &tb, None).unwrap();
    project.set_top_level(&top).unwrap();
    project.set_testbench(&tb).unwrap();

    let sim = simulate_project(
        &toolchain,
        &project,
        &SimulationOptions::new(Simulator::Icarus),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(sim.status, Status::Succeeded, "{sim:#?}");
    let run = sim.steps.iter().find(|s| s.step == Step::Simulate).unwrap();
    for line in ["a = 55", "b = 55", "c = 55"] {
        assert!(run.stdout.contains(line), "{line}:\n{}", run.stdout);
    }
    assert!(root.join("pc_soma_mem.txt").is_file());
    assert!(root.join("pc_filtro_mem.txt").is_file());

    let synth = synthesize(
        &toolchain,
        &project,
        &DesignTarget::TopLevel,
        &Control::default(),
    )
    .unwrap();
    assert!(synth.succeeded(), "{:#?}", synth.diagnostics);
    for module in ["duo", "soma", "filtro"] {
        assert!(
            synth.modules.iter().any(|m| m == module),
            "{module}: {:?}",
            synth.modules
        );
    }
}
