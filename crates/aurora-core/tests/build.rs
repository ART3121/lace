//! Build de ponta a ponta com os compiladores YANC reais.

mod common;

use camino::Utf8Path;
use aurora_core::{
    BuildOptions, BuildResult, Control, FileRole, AuroraError, Language, NewProcessor, Project,
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
        for artifact in &result.artifacts {
            assert!(
                artifact.path.starts_with(&processor.output_dir),
                "{:?} fora da pasta de saída: {}",
                artifact.kind,
                artifact.path
            );
            if artifact.required {
                assert!(artifact.fresh, "{:?} não foi gerado", artifact.kind);
            }
        }
        insta::assert_json_snapshot!(
            format!("build_ok_{}", processor.name),
            normalized(&result, &root, &toolchain),
            { ".steps[].duration_ms" => "[ms]", ".duration_ms" => "[ms]" }
        );
    }
}

#[test]
fn the_generated_verilog_names_its_files_relative_to_the_output_folder() {
    // A estrutura do YANC 7.1: o Verilog e o testbench citam as memórias,
    // a tabela do PC e as saídas pelo nome, e as entradas na pasta do fonte,
    // e o projeto pode mudar de pasta sem compilar de novo.
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
    assert!(result.succeeded());
    let verilog = std::fs::read_to_string(processor.verilog_path()).unwrap();
    assert!(verilog.contains(".DFILE(\"soma_data.mif\")"), "{verilog}");
    assert!(verilog.contains(".IFILE(\"soma_inst.mif\")"), "{verilog}");
    let testbench = std::fs::read_to_string(processor.testbench_path()).unwrap();
    assert!(testbench.contains("$fopen(\"output_0.txt\""), "{testbench}");
    assert!(
        !testbench.contains(root.as_str()),
        "caminho absoluto no testbench"
    );
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
    assert_eq!(
        result.steps.len(),
        1,
        "não pode seguir para o próximo passo"
    );
    let error = result
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap();
    assert_eq!(error.file.as_deref(), Some(processor.source.as_path()));
    assert_eq!(error.line, Some(15));
    assert!(error.message.contains("'total'"));

    insta::assert_json_snapshot!(
        "build_erro_conta",
        normalized(&result, &root, &toolchain),
        { ".steps[].duration_ms" => "[ms]", ".duration_ms" => "[ms]" }
    );
}

#[test]
fn cpp_error_points_to_the_file_the_compiler_read() {
    // O cppcomp do YANC 7.1 pré-processa na memória e cita o arquivo e a
    // linha escritos (o fonte, ou o cabeçalho incluído).
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
    let error = result.first_error().unwrap();
    assert_eq!(error.file.as_deref(), Some(processor.source.as_path()));
    assert_eq!(error.line, Some(5));

    // Um erro dentro de um cabeçalho da pasta do processador cita o
    // cabeçalho, que o cppcomp acha sem `-I`.
    std::fs::write(
        processor.dir.join("util.h"),
        "int dobro(int x)\n{\n    return x + y;\n}\n",
    )
    .unwrap();
    std::fs::write(
        &processor.source,
        "#include \"util.h\"\nvoid main(void)\n{\n    out(0, dobro(in(0)));\n}\n",
    )
    .unwrap();
    let result = build(
        &toolchain,
        processor,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
    let error = result.first_error().unwrap();
    assert_eq!(
        error.file.as_deref(),
        Some(processor.dir.join("util.h").as_path())
    );
    assert_eq!(error.line, Some(3));
}

#[test]
fn a_name_line_left_from_yanc_6_is_the_compiler_error() {
    // Na 7.1 o processador se chama como o arquivo, e o `#PRNAME` que sobrou
    // de um fonte da 6.x é erro do cmmcomp, na linha dele.
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, root) = common::example("soma");
    let project = Project::open(&root).unwrap();
    let processor = project.require_processor("soma").unwrap();
    let source = std::fs::read_to_string(&processor.source).unwrap();
    std::fs::write(&processor.source, format!("#PRNAME outro\n{source}")).unwrap();
    let result = build(
        &toolchain,
        processor,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(result.status, Status::Failed);
    assert_eq!(result.failed_step, Some(Step::Compile));
    let error = result.first_error().unwrap();
    assert_eq!(error.file.as_deref(), Some(processor.source.as_path()));
    assert_eq!(error.line, Some(1));
    assert!(error.message.contains("#PRNAME"), "{error:?}");
    assert!(!processor.output_dir.join("outro.v").exists());
}

/// A lista de módulos SAPHO da AURORA acompanha a do YANC do bundle: um
/// processador com o nome de um módulo da biblioteca se instanciaria.
#[test]
fn no_processor_takes_the_name_of_a_sapho_module() {
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let sapho = toolchain.hdl_dir().unwrap();
    let mut modules = Vec::new();
    for entry in sapho.read_dir_utf8().unwrap() {
        let path = entry.unwrap().into_path();
        if path.extension() != Some("v") {
            continue;
        }
        let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
        for line in text.lines() {
            if let Some(rest) = line.trim_start().strip_prefix("module ") {
                let name: String = rest
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                modules.push(name);
            }
        }
    }
    assert!(modules.len() > 20, "poucos módulos em {sapho}: {modules:?}");
    let accepted: Vec<_> = modules
        .iter()
        .filter(|name| aurora_core::validate_processor_name(name).is_ok())
        .collect();
    assert!(accepted.is_empty(), "nomes de módulo SAPHO aceitos: {accepted:?}");
}

#[test]
fn the_source_must_carry_the_processor_name() {
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, root) = common::example("soma");
    std::fs::rename(root.join("soma/soma.cmm"), root.join("soma/outro.cmm")).unwrap();
    let spf = root.join("soma.spf");
    let text = std::fs::read_to_string(&spf).unwrap();
    std::fs::write(
        &spf,
        text.replacen(
            "\"name\": \"soma\"",
            "\"name\": \"soma\", \"sourceFile\": \"outro.cmm\"",
            1,
        ),
    )
    .unwrap();
    let project = Project::open(&root).unwrap();
    let err = build(
        &toolchain,
        project.require_processor("soma").unwrap(),
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(
        matches!(&err, AuroraError::InvalidSource { reason, .. } if reason.contains("soma.cmm")),
        "{err}"
    );
}

#[test]
fn a_project_in_the_old_layout_is_refused_until_converted() {
    let Some(toolchain) = common::toolchain() else {
        return;
    };
    let (_guard, root) = common::example("soma");
    // Como a 6.x deixava: o fonte, com #PRNAME, em Software/, e um build
    // antigo em Hardware/ e Simulation/.
    for (from, to) in [
        ("soma/soma.cmm", "soma/Software/soma.cmm"),
        ("filtro/filtro.cpp", "filtro/Software/filtro.cpp"),
    ] {
        std::fs::create_dir_all(root.join(to).parent().unwrap()).unwrap();
        let text = std::fs::read_to_string(root.join(from)).unwrap();
        let name = if from.ends_with(".cmm") {
            "#PRNAME soma\n"
        } else {
            "#pragma yanc prname filtro\n"
        };
        std::fs::write(root.join(to), format!("{name}{text}")).unwrap();
        std::fs::remove_file(root.join(from)).unwrap();
    }
    std::fs::create_dir_all(root.join("soma/Hardware")).unwrap();
    std::fs::write(
        root.join("soma/Hardware/soma.v"),
        "module soma; endmodule\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("soma/Simulation")).unwrap();
    std::fs::write(root.join("soma/Simulation/input_0.txt"), "1\n").unwrap();
    std::fs::write(root.join("soma/Simulation/output_0.txt"), "55\n").unwrap();
    let mut project = Project::open(&root).unwrap();
    project
        .add_file(
            FileRole::Synthesizable,
            root.join("soma/Hardware/soma.v"),
            None,
        )
        .unwrap();
    project
        .set_top_level(root.join("soma/Hardware/soma.v"))
        .unwrap();
    assert_eq!(project.legacy_processors().len(), 2);
    assert!(
        project
            .issues()
            .iter()
            .any(|i| i.kind == aurora_core::IssueKind::LegacyLayout)
    );
    let err = build(
        &toolchain,
        project.require_processor("soma").unwrap(),
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(matches!(err, AuroraError::LegacyLayout { .. }), "{err}");

    let report = project.convert_layout().unwrap();
    assert_eq!(report.processors, ["soma", "filtro"]);
    assert!(report.skipped.is_empty(), "{report:#?}");
    assert_eq!(report.name_lines.len(), 2);
    assert!(root.join("soma/soma.cmm").is_file());
    assert!(root.join("soma/input_0.txt").is_file());
    assert!(root.join("soma/soma/soma.v").is_file());
    assert!(root.join("soma/soma/output_0.txt").is_file());
    for old in ["Software", "Hardware", "Simulation"] {
        assert!(!root.join("soma").join(old).exists(), "{old}");
    }
    // O .spf segue o arquivo.
    let reopened = Project::open(&root).unwrap();
    assert_eq!(reopened.top_level(), Some(root.join("soma/soma/soma.v")));
    assert!(reopened.legacy_processors().is_empty());
    assert!(project.convert_layout().unwrap().is_empty());

    for processor in reopened.processors() {
        let result = build(
            &toolchain,
            processor,
            &BuildOptions::default(),
            &Control::default(),
        )
        .unwrap();
        assert!(result.succeeded(), "{:#?}", result.diagnostics);
    }
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
    let all = aurora_core::build_processors(
        &toolchain,
        project.buildable_processors(),
        &BuildOptions::default(),
        aurora_core::OnFailure::Continue,
        &Control::default(),
        |r| seen.push(r.processor.clone()),
    )
    .unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(seen, ["conta", "filtro"], "on_result chamado a cada build");

    let first = aurora_core::build_processors(
        &toolchain,
        project.buildable_processors(),
        &BuildOptions::default(),
        aurora_core::OnFailure::Stop,
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
    let mut sem_saida = NewProcessor::new("sem_saida", Language::Cmm);
    sem_saida.output_ports = 0;
    let mut pilhas = NewProcessor::new("pilhas", Language::Cmm);
    pilhas.ndstac = Some(8);
    pilhas.sdepth = Some(4);
    pilhas.nugain = 64;
    for spec in [
        NewProcessor::new("cmm", Language::Cmm),
        NewProcessor::new("cpp", Language::Cpp),
        sem_saida,
        pilhas,
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
            .add_processor(&NewProcessor::new(name, Language::Cmm))
            .unwrap();
    }
    let builds = aurora_core::build_processors(
        &toolchain,
        project.buildable_processors(),
        &BuildOptions::default(),
        aurora_core::OnFailure::Continue,
        &Control::default(),
        |_| {},
    )
    .unwrap();
    let root = project.root().to_owned();
    let soma_v = root.join("soma/soma/soma.v");
    let soma_tb = root.join("soma/soma/soma_tb.v");
    let filtro_v = root.join("filtro/filtro/filtro.v");

    // Sem RTL e sem testbench: o primeiro processador vira o topo e o
    // testbench escolhido; o segundo só entra nas listas.
    let added = project.register_generated(&builds).unwrap();
    let summary: Vec<_> = added
        .iter()
        .map(|f| {
            (
                // Com `/`, também no Windows.
                f.path
                    .strip_prefix(&root)
                    .unwrap()
                    .as_str()
                    .replace('\\', "/"),
                f.role,
                f.selected,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("soma/soma/soma.v".into(), FileRole::Synthesizable, true),
            ("soma/soma/soma_tb.v".into(), FileRole::Testbench, true),
            (
                "filtro/filtro/filtro.v".into(),
                FileRole::Synthesizable,
                false
            ),
            (
                "filtro/filtro/filtro_tb.v".into(),
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
            .add_processor(&NewProcessor::new(name, Language::Cmm))
            .unwrap();
    }
    let ruim = project.require_processor("ruim").unwrap().source.clone();
    let source = std::fs::read_to_string(&ruim).unwrap();
    std::fs::write(&ruim, source.replace("void main()", "void main(")).unwrap();
    let builds = aurora_core::build_processors(
        &toolchain,
        project.buildable_processors(),
        &BuildOptions::default(),
        aurora_core::OnFailure::Continue,
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
