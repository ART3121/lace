//! Build de ponta a ponta com os compiladores YANC reais.

mod common;

use camino::Utf8Path;
use lace_core::{
    ArtifactKind, BuildOptions, BuildResult, Control, FileRole, LaceError, Language, Project,
    Severity, Status, Step, Toolchain, build,
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
        let result = build(
            &toolchain,
            processor,
            &BuildOptions::default(),
            &Control::default(),
        )
        .unwrap();
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
            { ".steps[].duration_ms" => "[ms]", ".duration_ms" => "[ms]" }
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
    let result = build(
        &toolchain,
        processor,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
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
        &Control::default(),
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
    let result = build(
        &toolchain,
        processor,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();

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
        { ".steps[].duration_ms" => "[ms]", ".duration_ms" => "[ms]" }
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
    let result = build(
        &toolchain,
        processor,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();

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
    let err = build(
        &toolchain,
        processor,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(
        matches!(&err, LaceError::InvalidSource { reason, .. } if reason.contains("'outro'")),
        "{err}"
    );

    std::fs::write(&processor.source, source.replace("#PRNAME soma", "")).unwrap();
    let err = build(
        &toolchain,
        processor,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(
        matches!(&err, LaceError::InvalidSource { reason, .. } if reason.contains("#PRNAME")),
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
    let all = lace_core::build_processors(
        &toolchain,
        project.buildable_processors(),
        &BuildOptions::default(),
        lace_core::OnFailure::Continue,
        &Control::default(),
        |r| seen.push(r.processor.clone()),
    )
    .unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(seen, ["conta", "filtro"], "on_result chamado a cada build");

    let first = lace_core::build_processors(
        &toolchain,
        project.buildable_processors(),
        &BuildOptions::default(),
        lace_core::OnFailure::Stop,
        &Control::default(),
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
    let mut sem_saida = lace_core::NewProcessor::new("sem_saida", Language::Cmm);
    sem_saida.output_ports = 0;
    for spec in [
        lace_core::NewProcessor::new("cmm", Language::Cmm),
        lace_core::NewProcessor::new("cpp", Language::Cpp),
        sem_saida,
    ] {
        project.add_processor(&spec).unwrap();
    }
    for processor in project.processors() {
        let result = build(
            &toolchain,
            processor,
            &BuildOptions::default(),
            &Control::default(),
        )
        .unwrap();
        assert_eq!(
            result.status,
            Status::Succeeded,
            "{}: {:#?}",
            processor.name,
            result.diagnostics
        );
    }
}

#[test]
fn register_generated_adds_the_verilog_and_the_testbench() {
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, dir) = common::tempdir();
    let mut project = Project::create(&dir, "gerados").unwrap();
    for name in ["soma", "filtro"] {
        project
            .add_processor(&lace_core::NewProcessor::new(name, Language::Cmm))
            .unwrap();
    }
    let builds = lace_core::build_processors(
        &toolchain,
        project.buildable_processors(),
        &BuildOptions::default(),
        lace_core::OnFailure::Continue,
        &Control::default(),
        |_| {},
    )
    .unwrap();
    let root = project.root().to_owned();
    let soma_v = root.join("soma/Hardware/soma.v");
    let soma_tb = root.join("soma/Simulation/soma_tb.v");
    let filtro_v = root.join("filtro/Hardware/filtro.v");

    // Sem RTL e sem testbench: o primeiro processador vira o topo e o
    // testbench escolhido; o segundo só entra nas listas.
    let added = project.register_generated(&builds).unwrap();
    let summary: Vec<_> = added
        .iter()
        .map(|f| {
            (
                f.path.strip_prefix(&root).unwrap().to_string(),
                f.role,
                f.selected,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("soma/Hardware/soma.v".into(), FileRole::Synthesizable, true),
            (
                "soma/Simulation/soma_tb.v".into(),
                FileRole::Testbench,
                true
            ),
            (
                "filtro/Hardware/filtro.v".into(),
                FileRole::Synthesizable,
                false
            ),
            (
                "filtro/Simulation/filtro_tb.v".into(),
                FileRole::Testbench,
                false
            ),
        ]
    );
    let reopened = Project::open(project.spf_path()).unwrap();
    assert_eq!(reopened.top_level(), Some(soma_v.clone()));
    assert_eq!(reopened.testbench(), Some(soma_tb.clone()));

    // O build seguinte não repete; o que foi tirado volta, sem virar topo.
    assert!(project.register_generated(&builds).unwrap().is_empty());
    project
        .remove_file(FileRole::Synthesizable, &filtro_v)
        .unwrap();
    let again = project.register_generated(&builds).unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].path, filtro_v);
    assert!(!again[0].selected);
    assert_eq!(project.top_level(), Some(soma_v));
}

#[test]
fn register_generated_keeps_the_top_and_skips_failed_builds() {
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, dir) = common::tempdir();
    let mut project = Project::create(&dir, "com_rtl").unwrap();
    let root = project.root().to_owned();
    // Um RTL do usuário já é o topo.
    std::fs::write(
        root.join("cpu.v"),
        "module cpu(input a, output y);\n  assign y = a;\nendmodule\n",
    )
    .unwrap();
    project
        .add_verilog(None, root.join("cpu.v"), false)
        .unwrap();
    for name in ["soma", "ruim"] {
        project
            .add_processor(&lace_core::NewProcessor::new(name, Language::Cmm))
            .unwrap();
    }
    let ruim = project.require_processor("ruim").unwrap().source.clone();
    let source = std::fs::read_to_string(&ruim).unwrap();
    std::fs::write(&ruim, source.replace("void main()", "void main(")).unwrap();
    let builds = lace_core::build_processors(
        &toolchain,
        project.buildable_processors(),
        &BuildOptions::default(),
        lace_core::OnFailure::Continue,
        &Control::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(builds[1].status, Status::Failed, "ruim não compila");

    let added = project.register_generated(&builds).unwrap();
    let paths: Vec<_> = added.iter().map(|f| f.path.file_name().unwrap()).collect();
    assert_eq!(
        paths,
        ["soma.v", "soma_tb.v"],
        "só o build que terminou bem"
    );
    assert!(!added[0].selected, "o topo do usuário fica");
    assert_eq!(project.top_level(), Some(root.join("cpu.v")));
    assert!(
        added[1].selected,
        "sem testbench, o do processador é o escolhido"
    );
}
