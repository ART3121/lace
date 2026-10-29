//! Garantias da API pública que a documentação promete.

use aurora_core::*;

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
    send_sync::<AuroraError>();
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
        AuroraError::ProjectExists("x".into()),
        AuroraError::ProcessorExists("x".into()),
        AuroraError::NoTestbench("x".into()),
        AuroraError::ComponentMissing("x".into()),
        AuroraError::SystemCompilerMissing,
        AuroraError::ProjectNotFound("x".into()),
        AuroraError::NoReports("x".into()),
        AuroraError::ReportNotFound("x".into()),
        AuroraError::NotComparable("x".into()),
        AuroraError::OutsideProject("x".into()),
        AuroraError::CannotMove {
            path: "x".into(),
            reason: "y".into(),
        },
        AuroraError::PathExists("x".into()),
    ];
    for error in &samples {
        let code = error.code();
        assert!(
            code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{code}"
        );
    }
}
