//! Garantias da API pública que a documentação promete.

use solar_core::*;

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
    send_sync::<SolarError>();
    send::<RunningProcess>();
}

/// Os códigos de erro são estáveis e únicos.
#[test]
fn error_codes_are_snake_case() {
    let samples = [
        SolarError::ProjectExists("x".into()),
        SolarError::ProcessorExists("x".into()),
        SolarError::NoTestbench("x".into()),
        SolarError::ComponentMissing("x".into()),
        SolarError::SystemCompilerMissing,
    ];
    for error in &samples {
        let code = error.code();
        assert!(
            code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{code}"
        );
    }
}
