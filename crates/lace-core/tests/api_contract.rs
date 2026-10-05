//! Garantias da API pública que a documentação promete.

use lace_core::*;

fn send_sync<T: Send + Sync>() {}
fn send<T: Send>() {}

/// Uma GUI roda as operações numa thread e manda o resultado para a thread
/// da interface: os tipos precisam atravessar threads.
#[test]
fn types_cross_threads() {
    send_sync::<Toolchain>();
    send_sync::<BundleManifest>();
    send_sync::<SystemCompiler>();
    send_sync::<Project>();
    send_sync::<Processor>();
    send_sync::<BuildResult>();
    send_sync::<SimulationResult>();
    send_sync::<SynthesisResult>();
    send_sync::<SchematicResult>();
    send_sync::<CheckResult>();
    send_sync::<HierarchyResult>();
    send_sync::<MovedPath>();
    send_sync::<SynthesisStatistics>();
    send_sync::<history::RunRecord>();
    send_sync::<history::RunSummary>();
    send_sync::<history::RunComparison>();
    send_sync::<history::Operation<'static>>();
    send_sync::<LaceError>();
    send::<RunningProcess>();
    // O botão de parar fica na thread da interface com um clone do pedido;
    // a operação roda em outra com o Control.
    send_sync::<CancelToken>();
    send_sync::<Control>();
    send_sync::<Event>();
}

/// Os códigos de erro são estáveis e únicos.
#[test]
fn error_codes_are_snake_case() {
    let samples = [
        LaceError::ProjectExists("x".into()),
        LaceError::ProcessorExists("x".into()),
        LaceError::NoTestbench("x".into()),
        LaceError::ComponentMissing("x".into()),
        LaceError::SystemCompilerMissing,
        LaceError::ProjectNotFound("x".into()),
        LaceError::NoReports("x".into()),
        LaceError::ReportNotFound("x".into()),
        LaceError::NotComparable("x".into()),
        LaceError::OutsideProject("x".into()),
        LaceError::CannotMove {
            path: "x".into(),
            reason: "y".into(),
        },
        LaceError::PathExists("x".into()),
    ];
    for error in &samples {
        let code = error.code();
        assert!(
            code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{code}"
        );
    }
}
