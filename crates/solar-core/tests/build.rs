//! Build de ponta a ponta com os compiladores YANC reais.

mod common;

use camino::Utf8Path;
use solar_core::{
    ArtifactKind, BuildOptions, BuildResult, Language, Project, Severity, SolarError, Status, Step,
    Toolchain, build,
};

/// O resultado em JSON, com os caminhos da máquina trocados por marcadores
/// e os fins de linha normalizados, para o snapshot ser o mesmo em Linux,
/// macOS e Windows. A troca é feita em
/// cada string do JSON já decodificada (no texto serializado, `\\` do Windows
/// aparece dobrado e não casaria com o caminho).
fn normalized(
    result: &BuildResult,
    project: &Utf8Path,
    toolchain: &Toolchain,
) -> serde_json::Value {
    fn walk(value: &mut serde_json::Value, project: &str, toolchain: &str) {
        match value {
            serde_json::Value::String(s) => {
                // No Windows as ferramentas escrevem CRLF (runtime C em modo
                // texto); o relatório guarda como veio, o snapshot compara LF.
                *s = s
                    .replace("\r\n", "\n")
                    .replace(project, "[PROJETO]")
                    .replace(toolchain, "[TOOLCHAIN]")
                    .replace('\\', "/")
                    .replace(".exe", "");
            }
            serde_json::Value::Array(items) => {
                items.iter_mut().for_each(|v| walk(v, project, toolchain))
            }
            serde_json::Value::Object(map) => {
                map.values_mut().for_each(|v| walk(v, project, toolchain))
            }
            _ => {}
        }
    }
    let mut value = serde_json::to_value(result).unwrap();
    walk(&mut value, project.as_str(), toolchain.root().as_str());
    value
}

#[test]
fn example_builds_in_both_languages() {
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, root) = common::example("soma");
    let project = Project::open(&root).unwrap();

    for processor in project.processors() {
        let result = build(&toolchain, processor, &BuildOptions::default()).unwrap();
        assert_eq!(result.status, Status::Succeeded, "{result:#?}");
        assert!(
            result
                .diagnostics
                .iter()
                .all(|d| d.severity == Severity::Info)
        );
        for artifact in result.artifacts.iter().filter(|a| a.required) {
            assert!(artifact.fresh, "{:?} não foi gerado", artifact.kind);
        }
        insta::assert_json_snapshot!(
            format!("build_ok_{}", processor.name),
            normalized(&result, &root, &toolchain),
            { ".steps[].duration_ms" => "[ms]" }
        );
    }
}

#[test]
fn verilog_embeds_the_final_simulation_path() {
    // Os artefatos não são relocáveis: o testbench abre
    // `<proc>/Simulation/input_0.txt` por caminho absoluto.
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, root) = common::example("soma");
    let project = Project::open(&root).unwrap();
    let processor = project.require_processor("soma").unwrap();
    let result = build(&toolchain, processor, &BuildOptions::default()).unwrap();
    let tb = result
        .artifacts
        .iter()
        .find(|a| a.kind == ArtifactKind::Testbench)
        .unwrap();
    // O asmcomp troca `\\` por `/` antes de embutir (hdl.c), então no Windows
    // o caminho aparece como `C:/.../Simulation/output_0.txt`.
    let text = std::fs::read_to_string(&tb.path)
        .unwrap()
        .replace('\\', "/");
    let expected = processor
        .simulation_dir()
        .join("output_0.txt")
        .as_str()
        .replace('\\', "/");
    assert!(text.contains(&expected), "{expected}");
}

#[test]
fn options_override_the_project() {
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, root) = common::example("soma");
    let project = Project::open(&root).unwrap();
    let mut options = BuildOptions::default();
    options.frequency_mhz = Some(50);
    options.clocks = Some(123);
    let result = build(
        &toolchain,
        project.require_processor("soma").unwrap(),
        &options,
    )
    .unwrap();
    let asm = result
        .steps
        .iter()
        .find(|s| s.step == Step::Assemble)
        .unwrap();
    let args = asm.command.args.join(" ");
    assert!(args.contains("-f 50 -c 123"), "{args}");
}

#[test]
fn cmm_error_points_to_source_file_and_line() {
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, root) = common::example("com_erro");
    let project = Project::open(&root).unwrap();
    let processor = project.require_processor("conta").unwrap();
    let result = build(&toolchain, processor, &BuildOptions::default()).unwrap();

    assert_eq!(result.status, Status::Failed);
    assert_eq!(result.failed_step, Some(Step::Compile));
    assert_eq!(result.steps.len(), 1, "não pode seguir para o appcomp");
    let error = result
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap();
    assert_eq!(error.file.as_deref(), Some(processor.source.as_path()));
    assert_eq!(error.line, Some(16));
    assert!(error.message.contains("'total'"));

    insta::assert_json_snapshot!(
        "build_erro_conta",
        normalized(&result, &root, &toolchain),
        { ".steps[].duration_ms" => "[ms]" }
    );
}

#[test]
fn cpp_error_points_to_preprocessed_file() {
    // O cpppp não emite marcadores de linha, então o cppcomp numera as linhas
    // do pp.cpp. Sem #include, as linhas coincidem com as do fonte.
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, root) = common::example("com_erro");
    let project = Project::open(&root).unwrap();
    let processor = project.require_processor("filtro").unwrap();
    assert_eq!(processor.language, Language::Cpp);
    let result = build(&toolchain, processor, &BuildOptions::default()).unwrap();

    assert_eq!(result.status, Status::Failed);
    assert_eq!(result.failed_step, Some(Step::Compile));
    let error = result
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap();
    assert_eq!(
        error.file.as_deref(),
        Some(processor.temp_dir.join("pp.cpp").as_path())
    );
    assert_eq!(error.line, Some(7));
}

#[test]
fn name_traps_are_caught_before_running() {
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, root) = common::example("soma");
    let project = Project::open(&root).unwrap();
    let processor = project.require_processor("soma").unwrap();
    let source = std::fs::read_to_string(&processor.source).unwrap();

    std::fs::write(
        &processor.source,
        source.replace("#PRNAME soma", "#PRNAME outro"),
    )
    .unwrap();
    let err = build(&toolchain, processor, &BuildOptions::default()).unwrap_err();
    assert!(
        matches!(&err, SolarError::InvalidSource { reason, .. } if reason.contains("'outro'")),
        "{err}"
    );

    std::fs::write(&processor.source, source.replace("#PRNAME soma", "")).unwrap();
    let err = build(&toolchain, processor, &BuildOptions::default()).unwrap_err();
    assert!(
        matches!(&err, SolarError::InvalidSource { reason, .. } if reason.contains("#PRNAME")),
        "{err}"
    );

    assert!(!processor.hardware_dir().join("outro.v").exists());
}

#[test]
fn build_processors_honors_failure_policy() {
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, root) = common::example("com_erro");
    let project = Project::open(&root).unwrap();
    assert_eq!(project.buildable_processors().len(), 2);

    let mut seen = Vec::new();
    let all = solar_core::build_processors(
        &toolchain,
        project.buildable_processors(),
        &BuildOptions::default(),
        solar_core::OnFailure::Continue,
        |r| seen.push(r.processor.clone()),
    )
    .unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(seen, ["conta", "filtro"], "on_result chamado a cada build");

    let first = solar_core::build_processors(
        &toolchain,
        project.buildable_processors(),
        &BuildOptions::default(),
        solar_core::OnFailure::Stop,
        |_| {},
    )
    .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].status, Status::Failed);
}

#[test]
fn new_processors_build_as_created() {
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, dir) = common::tempdir();
    let mut project = Project::create(&dir, "novo").unwrap();
    let mut sem_saida = solar_core::NewProcessor::new("sem_saida", Language::Cmm);
    sem_saida.output_ports = 0;
    for spec in [
        solar_core::NewProcessor::new("cmm", Language::Cmm),
        solar_core::NewProcessor::new("cpp", Language::Cpp),
        sem_saida,
    ] {
        project.add_processor(&spec).unwrap();
    }
    for processor in project.processors() {
        let result = build(&toolchain, processor, &BuildOptions::default()).unwrap();
        assert_eq!(
            result.status,
            Status::Succeeded,
            "{}: {:#?}",
            processor.name,
            result.diagnostics
        );
    }
}
