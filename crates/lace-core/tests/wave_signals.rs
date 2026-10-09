//! A escolha dos sinais da onda: a árvore elaborada pelo Icarus, a escolha
//! em `wave/<testbench>.json`, a simulação que grava só ela, o layout que
//! mostra só ela e o layout salvo no projeto.
//!
//! Executam o Icarus do bundle e precisam de `LACE_TEST_BUNDLE` (ver
//! `common/mod.rs`).

mod common;

use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufReader;

use camino::{Utf8Path, Utf8PathBuf};
use fst_reader::{FstHierarchyEntry, FstReader};
use lace_core::{
    Control, FileRole, PortDirection, Project, ScopeKind, SimulationOptions, Simulator, Status,
    Tool, prepare_wave_layout, reset_saved_layout, simulate_project, wave_signals,
    write_selection,
};

const AND_GATE: &str = "\
module and_gate (
    input  wire a,
    input  wire b,
    output wire y
);
    wire t;
    assign t = a & b;
    assign y = t;
endmodule
";

const MUX2: &str = "\
module mux2 (
    input  wire a,
    input  wire b,
    input  wire sel,
    output wire y
);
    wire t0, t1;
    and_gate u0 (.a(a), .b(~sel), .y(t0));
    and_gate u1 (.a(b), .b(sel), .y(t1));
    assign y = t0 | t1;
endmodule
";

/// Sem `$dumpfile`: o Lace injeta o dump.
const MUX2_TB: &str = "\
module mux2_tb;
    reg a = 0, b = 1, sel = 0;
    wire y;
    mux2 dut (.a(a), .b(b), .sel(sel), .y(y));
    initial begin
        #5 sel = 1;
        #5 $finish;
    end
endmodule
";

fn project(root: &Utf8Path) -> Project {
    let mut project = Project::create(root, "mux").unwrap();
    for (role, name, text) in [
        (FileRole::Synthesizable, "and_gate.v", AND_GATE),
        (FileRole::Synthesizable, "mux2.v", MUX2),
        (FileRole::Testbench, "mux2_tb.v", MUX2_TB),
    ] {
        project.add_file(role, name, Some(text)).unwrap();
    }
    Project::open(project.root()).unwrap()
}

/// Os sinais gravados numa onda FST, pelo caminho.
fn dumped(path: &Utf8Path) -> BTreeSet<String> {
    let file = BufReader::new(File::open(path).unwrap());
    let mut fst = FstReader::open(file).unwrap();
    let mut stack: Vec<String> = Vec::new();
    let mut out = BTreeSet::new();
    fst.read_hierarchy(|entry| match entry {
        FstHierarchyEntry::Scope { name, .. } => stack.push(name),
        FstHierarchyEntry::UpScope => {
            stack.pop();
        }
        FstHierarchyEntry::Var { name, .. } => {
            out.insert(format!("{}.{name}", stack.join(".")));
        }
        _ => {}
    })
    .unwrap();
    out
}

fn simulate(toolchain: &lace_core::Toolchain, project: &Project) -> Utf8PathBuf {
    let sim = simulate_project(
        toolchain,
        project,
        &SimulationOptions::new(Simulator::Icarus),
        &Control::default(),
    )
    .unwrap();
    assert!(sim.succeeded(), "{sim:#?}");
    sim.waveform.unwrap().path
}

#[test]
fn the_choice_limits_what_the_simulation_records_and_the_layout_shows() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, dir) = common::tempdir();
    let project = project(&dir);
    let root = project.root().to_owned();

    // A árvore vem da elaboração, com as portas e o que há dentro de cada
    // instância.
    let tree = wave_signals(&toolchain, &project, &Control::default()).unwrap();
    assert_eq!(tree.status, Status::Succeeded, "{:#?}", tree.diagnostics);
    assert_eq!(tree.module, "mux2_tb");
    assert!(tree.selection.is_empty());
    assert_eq!(tree.selection_file, root.join("wave/mux2_tb.json"));
    let top = tree.root.unwrap();
    assert_eq!(top.path, "mux2_tb");
    let dut = &top.scopes[0];
    assert_eq!(dut.path, "mux2_tb.dut");
    assert_eq!(dut.kind, ScopeKind::Module);
    assert_eq!(dut.module.as_deref(), Some("mux2"));
    let sel = dut.signals.iter().find(|s| s.name == "sel").unwrap();
    assert_eq!(sel.direction, Some(PortDirection::Input));
    assert_eq!(sel.path, "mux2_tb.dut.sel");
    let paths: Vec<&str> = dut.scopes.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, ["mux2_tb.dut.u0", "mux2_tb.dut.u1"]);

    // Sem escolha, todos os sinais.
    let wave = simulate(&toolchain, &project);
    let all = dumped(&wave);
    assert!(all.contains("mux2_tb.dut.u1.t"), "{all:?}");

    // Com escolha: só ela.
    write_selection(
        &project,
        "mux2_tb",
        &["mux2_tb.sel".into(), "mux2_tb.dut.u0".into()],
    )
    .unwrap();
    let wave = simulate(&toolchain, &project);
    let chosen = dumped(&wave);
    assert!(chosen.contains("mux2_tb.sel"), "{chosen:?}");
    assert!(chosen.contains("mux2_tb.dut.u0.t"), "{chosen:?}");
    assert!(!chosen.contains("mux2_tb.dut.u1.t"), "{chosen:?}");
    assert!(!chosen.contains("mux2_tb.a"), "{chosen:?}");
    // O testbench do usuário não mudou.
    assert_eq!(std::fs::read_to_string(root.join("mux2_tb.v")).unwrap(), MUX2_TB);

    // O layout mostra os escolhidos, um grupo por escopo, e nasce em wave/.
    let prepared = prepare_wave_layout(&wave).unwrap().unwrap();
    assert_eq!(prepared.layout.testbench.as_deref(), Some("mux2_tb"));
    assert_eq!(prepared.layout.selection.len(), 2);
    let state = &prepared.layout.state;
    assert!(state.contains("\"Top-level\""), "{state}");
    assert!(state.contains("\"dut.u0\""), "{state}");
    assert!(!state.contains("\"dut.u1\""), "{state}");
    let saved = prepared.saved.unwrap();
    assert_eq!(saved.path, root.join("wave/mux2_tb.surf.ron"));
    assert!(!saved.customized);
    assert_eq!(prepared.state, saved.path);
    assert_eq!(std::fs::read_to_string(&saved.path).unwrap(), *state);
}

#[test]
fn the_saved_layout_is_regenerated_until_the_user_saves_it() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, dir) = common::tempdir();
    let project = project(&dir);
    let wave = simulate(&toolchain, &project);
    let first = prepare_wave_layout(&wave).unwrap().unwrap();
    let path = first.saved.as_ref().unwrap().path.clone();
    assert!(path.is_file());

    // A escolha muda o layout gerado; o arquivo, que o usuário não salvou,
    // acompanha.
    write_selection(&project, "mux2_tb", &["mux2_tb.dut".into()]).unwrap();
    let wave = simulate(&toolchain, &project);
    let second = prepare_wave_layout(&wave).unwrap().unwrap();
    assert_ne!(second.layout.state, first.layout.state);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), second.layout.state);

    // O Surfer salvou (Ctrl+S): o arquivo fica como está.
    let mine = second.layout.state.replace("\"dut\"", "\"meu grupo\"");
    std::fs::write(&path, &mine).unwrap();
    let third = prepare_wave_layout(&wave).unwrap().unwrap();
    let saved = third.saved.unwrap();
    assert!(saved.customized);
    assert_eq!(saved.state, mine);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), mine);

    // Voltar ao gerado.
    assert!(reset_saved_layout(&project, "mux2_tb").unwrap());
    assert!(!path.exists());
    let fourth = prepare_wave_layout(&wave).unwrap().unwrap();
    assert!(!fourth.saved.unwrap().customized);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), fourth.layout.state);
}
