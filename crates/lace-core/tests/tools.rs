//! Simulação, checagem, síntese, esquemático e visualizador, com as
//! ferramentas do bundle. Precisam de `LACE_TEST_BUNDLE` com um bundle
//! completo (ver `common/mod.rs`).

mod common;

use std::time::Duration;

use lace_core::{
    BuildOptions, CheckOptions, Control, DesignTarget, FileRole, LaceError, Language, NewProcessor,
    Project, SchematicOptions, Severity, SimulationOptions, Simulator, Status, Step, Tool,
    ViewerOptions, WaveformFormat, build, check, open_waveform, render_schematic, simulate,
    simulate_project, synthesize, wave_layout,
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
        // O testbench do asmcomp pede `<nome>_tb.vcd`; o Icarus grava sempre
        // FST, com a extensão trocada numa cópia do testbench.
        let wave = result.waveform.unwrap();
        assert_eq!(wave.path, processor.temp_dir.join(format!("{name}_tb.fst")));
        assert_eq!(wave.format, WaveformFormat::Fst);
        assert!(common::is_fst(&wave.path), "{} não é FST", wave.path);
        assert!(processor.testbench_path().is_file());
        // O layout do Surfer acha o processador na onda FST e as tabelas que
        // o YANC deixou na pasta temporária dele.
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

/// A simulação rápida de um processador: no Verilator, mesmo pedindo o
/// Icarus, as mesmas saídas e nenhuma onda.
#[test]
fn fast_simulation_of_a_processor() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Verilator]) else {
        return;
    };
    let (_guard, project) = built("soma");
    let processor = project.require_processor("soma").unwrap();
    let mut options = SimulationOptions::new(Simulator::Icarus);
    options.fast = true;
    let run = || simulate(&toolchain, processor, &options, &Control::default()).unwrap();
    let result = run();
    assert_eq!(
        result.status,
        Status::Succeeded,
        "{:#?}",
        result.diagnostics
    );
    assert!(result.fast);
    assert_eq!(result.simulator, Simulator::Verilator);
    assert!(result.waveform.is_none());
    assert_eq!(processor.read_output(0).unwrap().trim(), "55");
    let verilate = result
        .steps
        .iter()
        .find(|s| s.step == Step::Verilate)
        .unwrap();
    assert!(
        !verilate.command.args.iter().any(|a| a == "--trace"),
        "{:?}",
        verilate.command.args
    );
    // Nenhuma onda no disco, e o modelo fica à parte do da simulação com onda.
    for wave in ["soma_tb.vcd", "soma_tb.fst"] {
        assert!(!processor.temp_dir.join(wave).exists(), "{wave}");
    }
    assert!(processor.temp_dir.join("obj_dir_fast_soma_tb").is_dir());
    assert!(!processor.temp_dir.join("obj_dir_soma_tb").exists());
    // De novo, com o modelo em dia: o executável não é refeito, e a
    // simulação continua completa.
    let again = run();
    assert_eq!(again.status, Status::Succeeded, "{again:#?}");
    assert_eq!(processor.read_output(0).unwrap().trim(), "55");
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
    // O `$dumpfile("soma_tb.vcd")` do testbench vira `.fst` na cópia
    // simulada; o testbench do usuário não muda.
    assert_eq!(
        result.waveform.unwrap().path,
        project.root().join("soma_tb.fst")
    );
    assert!(
        std::fs::read_to_string(&tb)
            .unwrap()
            .contains("soma_tb.vcd")
    );
    assert_eq!(processor.read_output(0).unwrap().trim(), "55");
}

/// Um programa que deixa um complexo na memória: o layout lê o valor dele
/// da onda FST e o traduz.
const COMPLEX_PROGRAM: &str = "\
#PRNAME cx
#NUBITS 32
#NBMANT 23
#NBEXPO 8
#NDSTAC 8
#SDEPTH 2
#NUIOIN 1
#NUIOOU 1

void main()
{
    comp z;
    z = complex(1.5, -2.0);
    fout(0, abs(z));
}
";

#[test]
fn the_layout_translates_complexes_from_an_fst_wave() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, dir) = common::tempdir();
    let mut project = Project::create(&dir, "cx").unwrap();
    let processor = project
        .add_processor(&NewProcessor::new("cx", Language::Cmm))
        .unwrap()
        .clone();
    std::fs::write(&processor.source, COMPLEX_PROGRAM).unwrap();
    let built = build(
        &toolchain,
        &processor,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(built.succeeded(), "{built:#?}");
    let result = simulate(
        &toolchain,
        &processor,
        &SimulationOptions::new(Simulator::Icarus),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(result.status, Status::Succeeded, "{result:#?}");
    let wave = result.waveform.unwrap();
    assert!(common::is_fst(&wave.path), "{} não é FST", wave.path);
    let layout = wave_layout(&wave.path).unwrap().expect("a processor wave");
    let complex = layout
        .mappings
        .iter()
        .find(|m| m.name == "lace_complex")
        .expect("the complex translator");
    assert!(
        complex.content.contains("1.500 -2.000i"),
        "{}",
        complex.content
    );
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
    // O Verilator grava VCD: o dump injetado leva a extensão `.vcd`.
    let wave = result.waveform.unwrap();
    assert_eq!(wave.path, root.join("contador_tb.vcd"));
    assert_eq!(wave.format, WaveformFormat::Vcd);
    assert!(!common::is_fst(&wave.path), "{} não é VCD", wave.path);
}

/// A simulação rápida do testbench do projeto: no Verilator, sem o dump que
/// o Lace injetaria e sem gravar o `$dumpfile` do testbench.
#[test]
fn fast_project_simulation_writes_no_waveform() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Verilator]) else {
        return;
    };
    let (_guard, root) = common::example("contador");
    let project = Project::open(&root).unwrap();
    let mut options = SimulationOptions::new(Simulator::Icarus);
    options.fast = true;
    let run = || simulate_project(&toolchain, &project, &options, &Control::default()).unwrap();
    let result = run();
    assert_eq!(
        result.status,
        Status::Succeeded,
        "{:#?}",
        result.diagnostics
    );
    assert!(result.fast);
    assert_eq!(result.simulator, Simulator::Verilator);
    assert!(result.waveform.is_none());
    let simulated = result
        .steps
        .iter()
        .find(|s| s.step == Step::Simulate)
        .unwrap();
    assert!(simulated.stdout.contains("q = 10"), "{}", simulated.stdout);
    // Sem `$dumpfile`, nada é injetado: o Verilator recebe o testbench do
    // usuário, e não sai onda.
    let verilate = result
        .steps
        .iter()
        .find(|s| s.step == Step::Verilate)
        .unwrap();
    let args = &verilate.command.args;
    assert!(!args.iter().any(|a| a == "--trace"), "{args:?}");
    assert!(
        args.iter()
            .any(|a| a.ends_with("contador_tb.v") && !a.contains("instr_")),
        "{args:?}"
    );
    for wave in ["contador_tb.vcd", "contador_tb.fst"] {
        assert!(!root.join(wave).exists(), "{wave}");
    }
    assert!(project.temp_dir().join("obj_dir_fast_contador_tb").is_dir());

    // Com `$dumpfile` e `$dumpvars`, o Verilator sem `--trace` os ignora.
    let tb = root.join("rtl/contador_tb.v");
    let text = std::fs::read_to_string(&tb).unwrap().replace(
        "    initial begin\n        #12",
        "    initial begin\n        $dumpfile(\"onda.vcd\");\n        $dumpvars(0, contador_tb);\n        #12",
    );
    assert!(text.contains("$dumpfile"));
    std::fs::write(&tb, text).unwrap();
    let result = run();
    assert_eq!(result.status, Status::Succeeded, "{result:#?}");
    assert!(result.waveform.is_none());
    assert!(!root.join("onda.vcd").exists());
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning),
        "{:#?}",
        result.diagnostics
    );
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

/// Um somador com dois testes cocotb; o segundo confere `y` contra
/// `expected` na linha 19.
fn adder_tests(expected: u32) -> String {
    format!(
        "# aurora-toplevel: somador\n\
         import cocotb\n\
         from cocotb.triggers import Timer\n\
         \n\
         \n\
         @cocotb.test()\n\
         async def soma(dut):\n\
         \x20   dut.a.value = 3\n\
         \x20   dut.b.value = 4\n\
         \x20   await Timer(1, \"ns\")\n\
         \x20   assert dut.y.value == 7\n\
         \n\
         \n\
         @cocotb.test()\n\
         async def vai_um(dut):\n\
         \x20   dut.a.value = 15\n\
         \x20   dut.b.value = 1\n\
         \x20   await Timer(1, \"ns\")\n\
         \x20   assert dut.y.value == {expected}, f\"y = {{int(dut.y.value)}}\"\n"
    )
}

#[test]
fn cocotb_testbench_runs_its_tests_on_icarus() {
    let Some(toolchain) = common::toolchain_with_cocotb() else {
        return;
    };
    let (_guard, dir) = common::tempdir();
    let mut project = Project::create(&dir, "cocotb").unwrap();
    let root = project.root().to_owned();
    std::fs::create_dir_all(root.join("rtl")).unwrap();
    std::fs::write(
        root.join("rtl/somador.v"),
        "module somador(input [3:0] a, input [3:0] b, output [4:0] y);\n    assign y = a + b;\nendmodule\n",
    )
    .unwrap();
    project
        .add_verilog(Some(&toolchain), "rtl/somador.v", false)
        .unwrap();
    // Um .py novo sai do modelo, como testbench, com o módulo do nome na
    // diretiva; um nome que não é módulo Python é recusado.
    let err = project
        .check_add_verilog("rtl/test-somador.py")
        .unwrap_err();
    assert_eq!(err.code(), "invalid_name");
    let added = project
        .add_verilog(Some(&toolchain), "rtl/test_somador.py", false)
        .unwrap();
    assert_eq!(added.role, FileRole::Testbench);
    assert!(added.created && added.selected, "{added:?}");
    let tb = added.path;
    let template = std::fs::read_to_string(&tb).unwrap();
    assert_eq!(
        lace_core::cocotb::toplevel_directive(&template).as_deref(),
        Some("somador")
    );
    let icarus = SimulationOptions::new(Simulator::Icarus);
    let run = |project: &Project| {
        simulate_project(&toolchain, project, &icarus, &Control::default()).unwrap()
    };
    let result = run(&project);
    assert_eq!(result.status, Status::Succeeded, "{result:#?}");
    assert_eq!(result.top, "somador");
    let tests = result.tests.as_ref().unwrap();
    assert_eq!((tests.passed, tests.failed), (1, 0), "{tests:?}");

    // Um teste que falha reprova a simulação, com o erro na linha do assert
    // no .py, e a onda vem mesmo assim: é nela que se vê a falha.
    std::fs::write(&tb, adder_tests(17)).unwrap();
    let result = run(&project);
    assert_eq!(result.status, Status::Failed, "{result:#?}");
    assert_eq!(result.failed_step, Some(Step::Simulate));
    let tests = result.tests.as_ref().unwrap();
    assert_eq!((tests.passed, tests.failed), (1, 1), "{tests:?}");
    let error = result
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap();
    assert!(error.message.contains("test_somador.vai_um"), "{error:?}");
    assert_eq!(error.line, Some(19), "{error:?}");
    let wave = result
        .waveform
        .as_ref()
        .expect("a onda do teste que falhou");
    assert_eq!(wave.path, root.join("test_somador.fst"));
    assert_eq!(wave.format, WaveformFormat::Fst);
    assert!(common::is_fst(&wave.path));
    assert_eq!(lace_core::waveform_path(&project, None).unwrap(), wave.path);

    // Corrigido, passa. Sem a diretiva, vale o topo do projeto, com aviso.
    std::fs::write(
        &tb,
        adder_tests(16).replace("# aurora-toplevel: somador\n", ""),
    )
    .unwrap();
    let result = run(&project);
    assert_eq!(result.status, Status::Succeeded, "{result:#?}");
    assert_eq!(result.tests.as_ref().unwrap().passed, 2);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains("aurora-toplevel")),
        "{:#?}",
        result.diagnostics
    );
    // Nem o projeto nem o bundle ganham __pycache__.
    assert!(!root.join("rtl/__pycache__").exists());

    // A simulação rápida roda os testes no Icarus sem gravar a onda.
    std::fs::remove_file(root.join("test_somador.fst")).unwrap();
    let mut fast = SimulationOptions::new(Simulator::Icarus);
    fast.fast = true;
    let result = simulate_project(&toolchain, &project, &fast, &Control::default()).unwrap();
    assert_eq!(result.status, Status::Succeeded, "{result:#?}");
    assert!(result.fast && result.waveform.is_none());
    assert_eq!(result.simulator, Simulator::Icarus);
    assert_eq!(result.tests.as_ref().unwrap().passed, 2);
    assert!(!root.join("test_somador.fst").exists());
    let vvp = result
        .steps
        .iter()
        .find(|s| s.step == Step::Simulate)
        .unwrap();
    assert!(
        vvp.command.args.iter().any(|a| a == "-none"),
        "{:?}",
        vvp.command.args
    );

    // O check verifica o Verilog e deixa o .py de fora, e não o aceita como
    // arquivo.
    let checked = check(
        &toolchain,
        &project,
        &CheckOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(checked.status, Status::Succeeded, "{checked:#?}");
    let mut options = CheckOptions::default();
    options.file = Some(tb.clone());
    let err = check(&toolchain, &project, &options, &Control::default()).unwrap_err();
    assert_eq!(err.code(), "invalid_name");
}

#[test]
fn cocotb_testbench_runs_its_tests_on_verilator() {
    let Some(toolchain) = common::toolchain_with_cocotb() else {
        return;
    };
    let Some(toolchain) = toolchain.tool(Tool::Verilator).is_ok().then_some(toolchain) else {
        eprintln!("PULADO: falta o Verilator no bundle");
        return;
    };
    let (_guard, dir) = common::tempdir();
    let mut project = Project::create(&dir, "cocotb").unwrap();
    let root = project.root().to_owned();
    std::fs::write(
        root.join("somador.v"),
        "module somador(input [3:0] a, input [3:0] b, output [4:0] y);\n    assign y = a + b;\nendmodule\n",
    )
    .unwrap();
    project
        .add_verilog(Some(&toolchain), "somador.v", false)
        .unwrap();
    let tb = root.join("test_somador.py");
    std::fs::write(&tb, adder_tests(17)).unwrap();
    project
        .add_verilog(Some(&toolchain), "test_somador.py", false)
        .unwrap();

    // O teste que falha reprova, com o erro na linha do assert, e a onda vem
    // em VCD, a do Verilator.
    let verilator = SimulationOptions::new(Simulator::Verilator);
    let result = simulate_project(&toolchain, &project, &verilator, &Control::default()).unwrap();
    assert_eq!(result.status, Status::Failed, "{result:#?}");
    assert_eq!(result.simulator, Simulator::Verilator);
    let tests = result.tests.as_ref().unwrap();
    assert_eq!((tests.passed, tests.failed), (1, 1), "{tests:?}");
    let error = result
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap();
    assert_eq!(error.tool, Tool::Verilator);
    assert_eq!(error.line, Some(19), "{error:?}");
    let wave = result.waveform.clone().expect("a onda do teste que falhou");
    assert_eq!(wave.path, root.join("test_somador.vcd"));
    assert_eq!(wave.format, WaveformFormat::Vcd);
    assert_eq!(lace_core::waveform_path(&project, None).unwrap(), wave.path);
    // Os avisos do compilador sobre a biblioteca do Verilator e a VPI do
    // cocotb, do bundle, não são do projeto.
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning),
        "{:#?}",
        result.diagnostics
    );

    // Corrigido, na simulação rápida: os testes passam e não sai onda.
    std::fs::write(&tb, adder_tests(16)).unwrap();
    std::fs::remove_file(&wave.path).unwrap();
    let mut fast = SimulationOptions::new(Simulator::Verilator);
    fast.fast = true;
    let result = simulate_project(&toolchain, &project, &fast, &Control::default()).unwrap();
    assert_eq!(result.status, Status::Succeeded, "{result:#?}");
    assert!(result.fast && result.waveform.is_none());
    assert_eq!(result.tests.as_ref().unwrap().passed, 2);
    assert!(!root.join("test_somador.vcd").exists());
    assert!(
        project
            .temp_dir()
            .join("cocotb/test_somador/obj_dir_fast_somador")
            .is_dir()
    );
}
