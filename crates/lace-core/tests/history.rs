//! O histórico de relatórios: gravar depois de sintetizar e simular, listar,
//! ler e comparar.
//!
//! Os testes de erro não executam ferramenta nenhuma e rodam sempre. O do
//! fluxo inteiro executa o Yosys e o Icarus do bundle e precisa de
//! `LACE_TEST_BUNDLE` (ver `common/mod.rs`).

mod common;

use std::time::SystemTime;

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::history::{self, Availability, Change, Cleanup, Operation};
use lace_core::{
    Control, DesignTarget, LaceError, Project, SimulationOptions, Simulator, SynthesisMetric, Tool,
    simulate_project, synthesize,
};

fn project() -> (tempfile::TempDir, Project) {
    let (guard, dir) = common::tempdir();
    let project = Project::create(&dir, "p").unwrap();
    (guard, project)
}

fn write(path: impl AsRef<Utf8Path>, text: &str) -> Utf8PathBuf {
    let path = path.as_ref().to_owned();
    std::fs::write(&path, text).unwrap();
    path
}

fn counter(width: u32) -> String {
    format!(
        "module counter (input clk, input rst, output reg [{}:0] q);\n\
         \x20 always @(posedge clk) if (rst) q <= 0; else q <= q + 1;\n\
         endmodule\n",
        width - 1
    )
}

const TESTBENCH: &str = "\
`timescale 1ns / 1ps
module counter_tb;
  reg clk = 0, rst = 1;
  wire [7:0] q;
  counter dut (.clk(clk), .rst(rst), .q(q));
  always #5 clk = ~clk;
  initial begin
    #12 rst = 0;
    #100 $display(\"q = %0d\", q);
    $finish;
  end
endmodule
";

#[test]
fn an_empty_history_says_so() {
    let (_guard, project) = project();
    assert!(history::list(&project).unwrap().is_empty());
    assert!(matches!(
        history::latest(&project),
        Err(LaceError::NoReports(_))
    ));
    assert!(matches!(
        history::load(&project, "7"),
        Err(LaceError::ReportNotFound(id)) if id == "run-000007"
    ));
    assert!(matches!(
        history::load(&project, "build-000007"),
        Err(LaceError::ReportNotFound(_))
    ));
    assert!(matches!(
        history::compare_reports(&project, None, None),
        Err(LaceError::NoReports(_))
    ));
}

#[test]
fn a_damaged_record_is_listed_as_unreadable() {
    let (_guard, project) = project();
    let dir = project.root().join(history::REPORTS_DIR).join("run-000001");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("record.json"), "{ not json").unwrap();
    let list = history::list(&project).unwrap();
    assert_eq!(list.len(), 1);
    assert!(!list[0].readable);
    assert!(matches!(
        history::load(&project, "1"),
        Err(LaceError::InvalidReport { .. })
    ));
    // Um formato que este Lace não conhece também não se lê.
    std::fs::write(dir.join("record.json"), r#"{"schema": 99}"#).unwrap();
    assert!(matches!(
        history::load(&project, "1"),
        Err(LaceError::InvalidReport { reason, .. }) if reason.contains("format 99")
    ));
}

#[test]
fn cleanup_chooses_removes_and_never_reuses_numbers() {
    let (_guard, project) = project();
    let dir = project.root().join(history::REPORTS_DIR);
    for n in 1..=5 {
        std::fs::create_dir_all(dir.join(format!("run-{n:06}"))).unwrap();
    }
    // Uma pasta de uma limpeza interrompida, e nenhum `sequence`.
    std::fs::create_dir_all(dir.join(".removing-run-000009-1")).unwrap();

    assert_eq!(
        history::plan_cleanup(&project, &Cleanup::KeepLatest(2)).unwrap(),
        ["run-000001", "run-000002", "run-000003"]
    );
    assert!(
        history::plan_cleanup(&project, &Cleanup::KeepLatest(9))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        history::plan_cleanup(
            &project,
            &Cleanup::Reports(vec!["4".into(), "run-000002".into(), "4".into()])
        )
        .unwrap(),
        ["run-000002", "run-000004"]
    );
    assert!(matches!(
        history::plan_cleanup(&project, &Cleanup::Reports(vec!["2".into(), "7".into()])),
        Err(LaceError::ReportNotFound(id)) if id == "run-000007"
    ));

    let all = history::plan_cleanup(&project, &Cleanup::All).unwrap();
    assert_eq!(all.len(), 5);
    // Um que já saiu é pulado.
    std::fs::remove_dir(dir.join("run-000003")).unwrap();
    let removed = history::remove(&project, &all).unwrap();
    assert_eq!(
        removed,
        ["run-000001", "run-000002", "run-000004", "run-000005"]
    );
    assert!(history::list(&project).unwrap().is_empty());
    let left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(left, ["sequence"]);
    assert_eq!(
        std::fs::read_to_string(dir.join("sequence"))
            .unwrap()
            .trim(),
        "5"
    );
}

#[test]
fn synthesis_and_simulation_are_recorded_and_compared() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Yosys, Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let module = write(root.join("counter.v"), &counter(8));
    project
        .add_verilog(Some(&toolchain), module.clone(), false)
        .unwrap();
    project
        .add_verilog(
            Some(&toolchain),
            write(root.join("counter_tb.v"), TESTBENCH),
            true,
        )
        .unwrap();
    let control = Control::default();
    let synth = |project: &Project| {
        let started = SystemTime::now();
        let result = synthesize(&toolchain, project, &DesignTarget::TopLevel, &control).unwrap();
        assert!(result.succeeded(), "{:#?}", result.diagnostics);
        let operation = Operation::new("lace synth", started).with_synthesis(&result);
        history::record(project, &toolchain, &operation).unwrap()
    };

    // A síntese grava as estatísticas do Yosys.
    let first = synth(&project);
    assert_eq!(first.id, "run-000001");
    let statistics = first.synthesis.as_ref().expect("estatísticas do Yosys");
    assert_eq!(statistics.top, "counter");
    assert!(statistics.cells.unwrap() > 0);
    assert!(
        statistics.cell_types.iter().any(|c| c.cell_type == "$add"),
        "{statistics:#?}"
    );
    assert!(first.simulation.is_none());

    // A simulação grava os tempos e o tempo simulado do $finish.
    let started = SystemTime::now();
    let sim = simulate_project(
        &toolchain,
        &project,
        &SimulationOptions::new(Simulator::Icarus),
        &control,
    )
    .unwrap();
    assert!(sim.succeeded(), "{:#?}", sim.diagnostics);
    let operation = Operation::new("lace sim", started).with_simulation(&sim);
    let simulated = history::record(&project, &toolchain, &operation).unwrap();
    let timings = simulated.simulation.as_ref().unwrap();
    assert!(timings.succeeded);
    assert!(timings.compile_ms.is_some() && timings.execution_ms.is_some());
    // 12 ns + 100 ns, com precisão de 1 ps.
    assert_eq!(timings.simulated_fs, Some(112_000_000));

    // Um contador mais largo: mais bits de fio, mesmas células.
    write(&module, &counter(16));
    let third = synth(&project);
    assert_eq!(third.id, "run-000003");

    let list = history::list(&project).unwrap();
    let ids: Vec<_> = list.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["run-000003", "run-000002", "run-000001"]);
    assert_eq!(list[1].simulation, Availability::Available);
    assert!(list[0].synthesis && !list[1].synthesis);

    // Sem referência, a comparação pula a simulação e pega a síntese
    // anterior.
    let comparison = history::compare_reports(&project, None, None).unwrap();
    assert_eq!(comparison.current_id, "run-000003");
    assert_eq!(comparison.baseline_id, "run-000001");
    let synthesis = comparison.synthesis.unwrap();
    let wire_bits = synthesis.metric(SynthesisMetric::WireBits).unwrap();
    assert_eq!(wire_bits.change, Change::Increased);
    assert!(comparison.simulation.is_none());
    assert!(
        comparison
            .warnings
            .iter()
            .any(|w| w == "synthesis sources differ"),
        "{:?}",
        comparison.warnings
    );

    // O texto guardado tem as seções do relatório.
    let text = history::report_text(&project, "3").unwrap();
    for section in [
        "LACE OPERATION REPORT",
        "HOST ENVIRONMENT",
        "EDA TOOLCHAIN",
        "TIMING BREAKDOWN",
        "GENERIC SYNTHESIS STATISTICS",
        "ARTIFACTS",
        "run-000003",
    ] {
        assert!(text.contains(section), "{section}:\n{text}");
    }
    assert!(!text.contains("FAILURE DIAGNOSTIC"));
}
