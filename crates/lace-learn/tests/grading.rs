//! A pasta de exercícios e a correção, com a trilha de `tests/fixtures/teste`.
//! Os testes que simulam precisam do bundle em `LACE_TEST_BUNDLE`, como os
//! do lace-core: sem ele, avisam e passam (com `CI`, falham).

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{Control, Severity, Toolchain};
use lace_learn::{Finding, Track, Verdict, Workspace, check_track, grade, load_track};

fn toolchain() -> Option<Toolchain> {
    match std::env::var("LACE_TEST_BUNDLE") {
        Ok(dir) => Some(Toolchain::open(&dir).expect("LACE_TEST_BUNDLE não é um bundle válido")),
        Err(_) if std::env::var_os("CI").is_some() => {
            panic!("CI definido, mas LACE_TEST_BUNDLE não: estes testes não podem ser pulados")
        }
        Err(_) => {
            eprintln!("PULADO: defina LACE_TEST_BUNDLE com um bundle de scripts/bundle.py");
            None
        }
    }
}

fn fixtures() -> Utf8PathBuf {
    Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn track() -> Track {
    load_track(&fixtures(), "teste", None).unwrap()
}

/// Uma pasta de exercícios nova numa pasta temporária.
fn workspace() -> (tempfile::TempDir, Workspace) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = Utf8PathBuf::from_path_buf(dunce::canonicalize(tmp.path()).unwrap()).unwrap();
    let workspace = Workspace::init(&dir.join("lace-learn"), track()).unwrap();
    (tmp, workspace)
}

fn write_student(workspace: &Workspace, name: &str, text: &str) {
    let exercise = workspace.track().require(name).unwrap();
    std::fs::write(workspace.student_file(exercise), text).unwrap();
}

fn grade_of(toolchain: &Toolchain, workspace: &Workspace, name: &str) -> lace_learn::Grade {
    let exercise = workspace.track().require(name).unwrap().clone();
    grade(toolchain, workspace, &exercise, &Control::new()).unwrap()
}

#[test]
fn the_track_is_read_in_order_with_texts_and_given_modules() {
    let track = track();
    let names: Vec<&str> = track.exercises().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["mux2", "soma16", "soma4", "conta"]);
    assert_eq!(track.title, "Teste");
    assert_eq!(track.chapters[1].title, "Sequencial");
    let mux2 = track.find("MUX2").unwrap();
    assert_eq!(mux2.title, "Multiplexador 2:1");
    assert_eq!(mux2.hints, ["Use o operador `?:`."]);
    assert_eq!(track.find("soma4").unwrap().given, ["somador_completo.v"]);
}

#[test]
fn init_creates_one_project_per_exercise_and_never_rewrites_the_student_file() {
    let (_tmp, workspace) = workspace();
    let mux2 = workspace.track().require("mux2").unwrap().clone();
    let dir = workspace.exercise_dir(&mux2);
    assert!(dir.join("mux2.spf").is_file());
    assert!(dir.join("README.md").is_file());
    assert!(dir.join(".lace-learn/tb_mux2.v").is_file());
    assert!(dir.join(".lace-learn/mux2_ref.v").is_file());
    let project = workspace.project(&mux2).unwrap();
    assert_eq!(project.top_level(), Some(workspace.student_file(&mux2)));
    assert_eq!(project.testbench(), Some(dir.join(".lace-learn/tb_mux2.v")));
    let soma4 = workspace.track().require("soma4").unwrap().clone();
    assert!(
        workspace
            .exercise_dir(&soma4)
            .join("somador_completo.v")
            .is_file()
    );

    // Reabrir põe em dia o que a trilha gera, e não toca no do aluno.
    std::fs::write(workspace.student_file(&mux2), "// meu\n").unwrap();
    std::fs::write(dir.join(".lace-learn/tb_mux2.v"), "estragado").unwrap();
    let root = workspace.root().to_owned();
    let reopened = Workspace::open(&root, &fixtures(), None).unwrap();
    assert_eq!(
        std::fs::read_to_string(reopened.student_file(&mux2)).unwrap(),
        "// meu\n"
    );
    assert!(
        std::fs::read_to_string(dir.join(".lace-learn/tb_mux2.v"))
            .unwrap()
            .contains("module tb_mux2")
    );

    // O reset é o único que volta o arquivo do aluno.
    reopened.reset(&mux2).unwrap();
    assert!(
        std::fs::read_to_string(reopened.student_file(&mux2))
            .unwrap()
            .contains("// Escreva aqui.")
    );
    assert!(matches!(
        Workspace::init(&root, track()),
        Err(lace_learn::LearnError::WorkspaceExists(_))
    ));
}

#[test]
fn progress_is_kept_and_the_solution_is_released_when_solved() {
    let (_tmp, mut workspace) = workspace();
    assert_eq!(workspace.current().name, "mux2");
    assert_eq!(workspace.next_pending().unwrap().name, "soma16");
    workspace.set_solved("soma16", true).unwrap();
    assert_eq!(workspace.next_pending().unwrap().name, "soma4");
    let soma16 = workspace.track().require("soma16").unwrap().clone();
    assert!(workspace.solution_path(&soma16).is_file());

    workspace.set_current("conta").unwrap();
    // Depois do último, volta ao primeiro por resolver.
    assert_eq!(workspace.next_pending().unwrap().name, "mux2");
    let root = workspace.root().to_owned();
    let reopened = Workspace::open(&root, &fixtures(), None).unwrap();
    assert_eq!(reopened.current().name, "conta");
    assert!(reopened.is_solved("soma16"));
    assert_eq!(reopened.solved_count(), 1);
    assert_eq!(Workspace::discover(&root.join("exercises")).unwrap(), root);
}

#[test]
fn the_fixture_track_passes_the_dev_check() {
    let Some(toolchain) = toolchain() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let scratch = Utf8PathBuf::from_path_buf(tmp.path().join("dev")).unwrap();
    let mut seen = Vec::new();
    let report = check_track(&toolchain, &track(), &scratch, &Control::new(), |e| {
        seen.push(e.name.clone())
    })
    .unwrap();
    assert!(report.ok(), "{report:#?}");
    assert_eq!(seen, ["mux2", "soma16", "soma4", "conta"]);
    for exercise in &report.exercises {
        assert_eq!(exercise.solution, Some(Verdict::Solved), "{exercise:?}");
        assert_ne!(exercise.start, Some(Verdict::Solved), "{exercise:?}");
    }
}

#[test]
fn a_wrong_answer_reports_each_output_and_the_first_mismatch() {
    let Some(toolchain) = toolchain() else { return };
    let (_tmp, workspace) = workspace();
    write_student(
        &workspace,
        "mux2",
        "module mux2(input a, input b, input sel, output y);\n    assign y = sel ? a : b;\nendmodule\n",
    );
    let result = grade_of(&toolchain, &workspace, "mux2");
    assert_eq!(result.verdict, Verdict::Mismatch, "{result:#?}");
    assert_eq!(result.samples, 8);
    assert_eq!(result.mismatched, 4);
    assert_eq!(result.outputs[0].name, "y");
    assert_eq!(result.outputs[0].mismatches, 4);
    // {a, b, sel} = 2: b = 1 e sel = 0, a amostra de 20 a 30 ns, comparada em 25.
    assert_eq!(result.outputs[0].first_ns, Some(25));
    assert!(result.findings.is_empty(), "{:?}", result.findings);
    assert!(result.waveform.as_ref().is_some_and(|w| w.is_file()));
    let layout = std::fs::read_to_string(result.layout.as_ref().unwrap()).unwrap();
    assert!(layout.contains("variable_add tb_mux2.y_ref"), "{layout}");
    assert!(layout.contains("marker_set_at 25ns mismatch"), "{layout}");
}

#[test]
fn the_solution_solves_and_nothing_is_reported() {
    let Some(toolchain) = toolchain() else { return };
    let (_tmp, workspace) = workspace();
    let soma4 = workspace.track().require("soma4").unwrap().clone();
    let solution = std::fs::read_to_string(soma4.solution_file()).unwrap();
    write_student(&workspace, "soma4", &solution);
    let result = grade_of(&toolchain, &workspace, "soma4");
    assert_eq!(result.verdict, Verdict::Solved, "{result:#?}");
    assert_eq!(result.samples, 256);
    // Nenhum aviso de timescale nem informação do simulador.
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.output.is_empty(), "{:?}", result.output);
}

#[test]
fn a_syntax_error_points_at_the_student_file() {
    let Some(toolchain) = toolchain() else { return };
    let (_tmp, workspace) = workspace();
    write_student(
        &workspace,
        "mux2",
        "module mux2(input a, input b, input sel, output y);\n    assign y = ;\nendmodule\n",
    );
    let result = grade_of(&toolchain, &workspace, "mux2");
    assert_eq!(result.verdict, Verdict::CompileError, "{result:#?}");
    let mux2 = workspace.track().require("mux2").unwrap();
    let error = result
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .expect("um erro");
    assert_eq!(
        error.file.as_deref(),
        Some(workspace.student_file(mux2).as_path())
    );
    assert_eq!(error.line, Some(2));
}

#[test]
fn renaming_a_port_is_an_interface_change() {
    let Some(toolchain) = toolchain() else { return };
    let (_tmp, workspace) = workspace();
    write_student(
        &workspace,
        "mux2",
        "module mux2(input a, input b, input sel, output out);\n    assign out = sel ? b : a;\nendmodule\n",
    );
    let result = grade_of(&toolchain, &workspace, "mux2");
    assert_eq!(result.verdict, Verdict::CompileError, "{result:#?}");
    assert_eq!(result.findings, [Finding::InterfaceChanged]);
}

#[test]
fn an_output_left_unassigned_is_undriven() {
    let Some(toolchain) = toolchain() else { return };
    let (_tmp, workspace) = workspace();
    let result = grade_of(&toolchain, &workspace, "mux2");
    assert_eq!(result.verdict, Verdict::Mismatch, "{result:#?}");
    assert_eq!(result.outputs[0].mismatches, 8);
    assert_eq!(
        result.findings,
        [Finding::UndrivenOutput { output: "y".into() }]
    );
}

#[test]
fn an_asynchronous_reset_where_a_synchronous_one_is_asked_errs_only_in_reset() {
    let Some(toolchain) = toolchain() else { return };
    let (_tmp, workspace) = workspace();
    write_student(
        &workspace,
        "conta",
        "module conta(input clk, input reset, input en, output reg [3:0] q);\n    always @(posedge clk or posedge reset)\n        if (reset) q <= 4'd0;\n        else if (en) q <= q + 4'd1;\nendmodule\n",
    );
    let result = grade_of(&toolchain, &workspace, "conta");
    assert_eq!(result.verdict, Verdict::Mismatch, "{result:#?}");
    assert!(result.mismatched > 0);
    assert_eq!(result.reset_mismatches, Some(result.mismatched));
    assert_eq!(result.findings, [Finding::ResetOnly]);
}

#[test]
fn a_combinational_loop_times_out() {
    let Some(toolchain) = toolchain() else { return };
    let (_tmp, workspace) = workspace();
    let soma16 = workspace.track().require("soma16").unwrap().clone();
    // Um laço que não termina: o testbench nunca chega ao resumo.
    write_student(
        &workspace,
        "soma16",
        "module soma16(input [15:0] a, input [15:0] b, output [16:0] s);\n    reg [16:0] r;\n    assign s = r;\n    always @(*) begin\n        r = a + b;\n        while (1) r = r + 1;\n    end\nendmodule\n",
    );
    let mut exercise = soma16.clone();
    exercise.spec.timeout_s = Some(2);
    let result = grade(&toolchain, &workspace, &exercise, &Control::new()).unwrap();
    assert_eq!(result.verdict, Verdict::TimedOut, "{result:#?}");
}
