//! Cancelamento, prazo e eventos das operações, com as ferramentas do
//! bundle. Precisam de `LACE_TEST_BUNDLE` (ver `common/mod.rs`).

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lace_core::{
    CancelToken, Control, Event, Project, SimulationOptions, Simulator, Status, Step, Stream,
    Termination, Tool, simulate_project,
};

/// Imprime uma linha e nunca chega ao `$finish`: o erro de testbench mais
/// comum de quem começa.
const ENDLESS_TB: &str = "\
module endless_tb;
    reg clk = 0;
    always #5 clk = ~clk;
    initial $display(\"comecou\");
endmodule
";

/// Imprime três linhas e termina. Os atrasos dão tempo à onda injetada de
/// abrir: um `$finish` no tempo 0 sai antes dela.
const COUNTING_TB: &str = "\
module counting_tb;
    integer i;
    initial begin
        for (i = 0; i < 3; i = i + 1)
            #1 $display(\"linha %0d\", i);
        #1 $finish;
    end
endmodule
";

/// Um projeto só com o testbench `name`, escolhido para simular.
fn project_with(name: &str, text: &str) -> (tempfile::TempDir, Project) {
    let (guard, dir) = common::tempdir();
    let mut project = Project::create(&dir, "p").unwrap();
    let path = project.root().join(name);
    std::fs::write(&path, text).unwrap();
    project.add_verilog(None, &path, true).unwrap();
    (guard, project)
}

/// Um `Control` que guarda os eventos numa lista.
fn recording() -> (Control, Arc<Mutex<Vec<Event>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let control = Control::new().on_event(move |e| sink.lock().unwrap().push(e.clone()));
    (control, events)
}

#[test]
fn timeout_stops_a_testbench_without_finish() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, project) = project_with("endless_tb.v", ENDLESS_TB);
    let mut options = SimulationOptions::new(Simulator::Icarus);
    options.timeout = Some(Duration::from_secs(1));

    let started = Instant::now();
    let result = simulate_project(&toolchain, &project, &options, &Control::default()).unwrap();
    assert!(started.elapsed() < Duration::from_secs(20));
    assert_eq!(result.status, Status::TimedOut, "{result:#?}");
    assert_eq!(result.failed_step, Some(Step::Simulate));
    let run = result.steps.last().unwrap();
    assert_eq!(run.termination, Termination::TimedOut);
    // O que o testbench escreveu antes do prazo continua no resultado.
    assert!(run.stdout.contains("comecou"), "{}", run.stdout);
    assert!(result.waveform.is_none());
}

#[test]
fn cancel_stops_the_running_simulation() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, project) = project_with("endless_tb.v", ENDLESS_TB);
    let cancel = CancelToken::new();
    // Cancela na primeira linha do testbench, que só chega aqui se a saída
    // do vvp vier ao vivo. Se não vier, o cancelamento de reserva, depois de
    // 10 s, impede o teste de rodar para sempre, e a asserção falha.
    let from_ui = cancel.clone();
    let saw_line = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&saw_line);
    let control = Control::new()
        .with_cancel(cancel.clone())
        .on_event(move |event| {
            if let Event::Output {
                step: Step::Simulate,
                line,
                ..
            } = event
                && line == "comecou"
            {
                seen.store(true, Ordering::SeqCst);
                from_ui.cancel();
            }
        });
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        cancel.cancel();
    });

    let started = Instant::now();
    let options = SimulationOptions::new(Simulator::Icarus);
    let result = simulate_project(&toolchain, &project, &options, &control).unwrap();
    assert!(
        saw_line.load(Ordering::SeqCst),
        "a linha do testbench só chegou no fim"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(result.status, Status::Cancelled, "{result:#?}");
    assert_eq!(result.failed_step, Some(Step::Simulate));
    assert_eq!(
        result.steps.last().unwrap().termination,
        Termination::Cancelled
    );
}

#[test]
fn cancelled_before_starting_runs_nothing() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, project) = project_with("counting_tb.v", COUNTING_TB);
    let cancel = CancelToken::new();
    cancel.cancel();
    let control = Control::new().with_cancel(cancel);

    let options = SimulationOptions::new(Simulator::Icarus);
    let result = simulate_project(&toolchain, &project, &options, &control).unwrap();
    assert_eq!(result.status, Status::Cancelled);
    assert_eq!(result.failed_step, Some(Step::Elaborate));
    assert!(result.steps.is_empty(), "{:#?}", result.steps);
}

#[test]
fn events_follow_the_steps_and_match_the_result() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, project) = project_with("counting_tb.v", COUNTING_TB);
    let (control, events) = recording();

    let options = SimulationOptions::new(Simulator::Icarus);
    let result = simulate_project(&toolchain, &project, &options, &control).unwrap();
    assert!(result.succeeded(), "{result:#?}");
    let events = events.lock().unwrap();

    // Começo e fim de cada passo, na ordem, com o mesmo comando e o mesmo
    // fim do resultado.
    let boundaries: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::StepStarted { step, command, .. } => Some((true, *step, Some(command), None)),
            Event::StepFinished {
                step, termination, ..
            } => Some((false, *step, None, Some(*termination))),
            _ => None,
        })
        .collect();
    assert_eq!(boundaries.len(), 4, "{events:#?}");
    for (i, report) in result.steps.iter().enumerate() {
        let (started, step, command, _) = &boundaries[2 * i];
        assert!(started);
        assert_eq!(*step, report.step);
        assert_eq!(*command, Some(&report.command));
        let (started, step, _, termination) = &boundaries[2 * i + 1];
        assert!(!started);
        assert_eq!(*step, report.step);
        assert_eq!(*termination, Some(report.termination));
    }

    // As linhas do testbench, uma por evento, iguais ao stdout do passo.
    let lines: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            Event::Output {
                step: Step::Simulate,
                stream: Stream::Stdout,
                line,
                ..
            } => Some(line.as_str()),
            _ => None,
        })
        .collect();
    let run = result.steps.last().unwrap();
    assert_eq!(lines, run.stdout.lines().collect::<Vec<_>>());
    assert!(lines.contains(&"linha 2"), "{lines:?}");
}
