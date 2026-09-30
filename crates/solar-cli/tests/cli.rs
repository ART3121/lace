//! A CLI de ponta a ponta: códigos de saída, texto e JSON.
//!
//! Os testes que rodam ferramentas precisam de um bundle em `SOLAR_TEST_BUNDLE`
//! (parcial com o YANC basta para os de build; completo para os demais; ver
//! `crates/solar-core/tests/common/mod.rs`). Sem elas, avisam e passam.

use assert_cmd::Command;
use camino::{Utf8Path, Utf8PathBuf};
use predicates::prelude::*;
use serde_json::Value;

/// `solar` sem nenhuma configuração herdada do ambiente de quem roda o teste.
fn solar(config_dir: &Utf8Path) -> Command {
    let mut cmd = Command::cargo_bin("solar").unwrap();
    cmd.env_remove("SOLAR_TOOLCHAIN")
        .env_remove("SOLAR_TEST_BUNDLE")
        .env("SOLAR_CONFIG", config_dir.join("config.json"))
        .env("NO_COLOR", "1");
    cmd
}

fn tempdir() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    // dunce: no Windows, `std::fs::canonicalize` devolve `\\?\C:\...`, e o Solar
    // trabalha com `C:\...`.
    let path = Utf8PathBuf::from_path_buf(dunce::canonicalize(dir.path()).unwrap()).unwrap();
    (dir, path)
}

fn copy_dir(from: &Utf8Path, to: &Utf8Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in from.read_dir_utf8().unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == ".solar" || name == "Hardware" || name.ends_with(".asm") {
            continue;
        }
        let target = to.join(name);
        if entry.file_type().unwrap().is_dir() {
            copy_dir(entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn example(name: &str) -> (tempfile::TempDir, Utf8PathBuf) {
    let (guard, dir) = tempdir();
    let from = Utf8Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    copy_dir(&from, &dir.join(name));
    (guard, dir.join(name))
}

fn json(cmd: &mut Command, code: i32) -> Value {
    let out = cmd.assert().code(code).get_output().stdout.clone();
    serde_json::from_slice(&out)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out)))
}

fn env_or_skip(var: &str) -> Option<String> {
    let value = std::env::var(var).ok();
    if value.is_none() {
        assert!(std::env::var_os("CI").is_none(), "CI sem {var}");
        eprintln!("PULADO: defina {var}");
    }
    value
}

/// Um bundle mínimo (só o manifesto, sem componentes) para testar como a CLI
/// acha e lê o bundle sem precisar das ferramentas.
fn fake_bundle(dir: &Utf8Path) {
    std::fs::create_dir_all(dir.join("yanc/bin")).unwrap();
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x64",
        ("macos", "aarch64") => "darwin-arm64",
        ("windows", "x86_64") => "windows-x64",
        _ => "desconhecida",
    };
    std::fs::write(dir.join("yanc/bin/arquivo"), "abc").unwrap();
    let header = serde_json::json!({ "schema": 2, "bundle": "teste", "platform": platform });
    std::fs::write(dir.join("bundle.json"), header.to_string()).unwrap();
    let yanc = serde_json::json!({
        "name": "yanc", "version": "v0", "dir": "yanc", "source": "teste",
        // sha256 de "abc"
        "files": { "yanc/bin/arquivo": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" },
    });
    std::fs::create_dir_all(dir.join("components")).unwrap();
    std::fs::write(dir.join("components/yanc.json"), yanc.to_string()).unwrap();
}

#[test]
fn new_proc_and_file_commands_build_the_skeleton() {
    let (_guard, dir) = tempdir();
    solar(&dir)
        .args(["new", "proj", "--dir", dir.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains("proj.spf"));
    let project = dir.join("proj");
    solar(&dir)
        .args([
            "-C",
            project.as_str(),
            "proc",
            "add",
            "alu",
            "--lang",
            "cpp",
            "--inputs",
            "2",
        ])
        .assert()
        .success();
    let source = std::fs::read_to_string(project.join("alu/Software/alu.cpp")).unwrap();
    assert!(source.contains("#pragma yanc nuioin 2"));
    assert!(project.join("alu/Hardware").is_dir());

    // Nome repetido é erro do Solar, não do build: código 2, com o código estável.
    let err = json(
        solar(&dir).args(["-C", project.as_str(), "--json", "proc", "add", "alu"]),
        2,
    );
    assert_eq!(err["error"]["code"], "processor_exists");

    solar(&dir)
        .args([
            "-C",
            project.as_str(),
            "file",
            "add",
            "rtl/top.v",
            "--create",
        ])
        .assert()
        .success();
    solar(&dir)
        .args(["-C", project.as_str(), "file", "top", "rtl/top.v"])
        .assert()
        .success();
    let status = json(
        solar(&dir).args(["-C", project.as_str(), "--json", "status"]),
        0,
    );
    assert_eq!(status["name"], "proj");
    assert_eq!(status["processors"][0]["name"], "alu");
    assert_eq!(status["processors"][0]["built"], false);
    assert_eq!(
        Utf8Path::new(status["top_level"].as_str().unwrap()),
        project.join("rtl").join("top.v")
    );
}

#[test]
fn proc_set_persists_simulation_parameters() {
    let (_guard, root) = example("soma");
    let (_cfg, cfg) = tempdir();
    solar(&cfg)
        .args([
            "-C",
            root.as_str(),
            "proc",
            "set",
            "soma",
            "--freq",
            "50",
            "--clocks",
            "300",
            "--show-arrays",
            "true",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("50 MHz, 300 clocks, arrays"));
    let list = json(
        solar(&cfg).args(["-C", root.as_str(), "--json", "proc", "list"]),
        0,
    );
    assert_eq!(list[0]["frequency_mhz"], 50);
    assert_eq!(list[0]["clocks"], 300);
    assert_eq!(list[0]["show_arrays"], true);

    solar(&cfg)
        .args(["-C", root.as_str(), "proc", "set", "soma"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("nada para mudar"));
}

#[test]
fn input_accepts_negative_values_and_files() {
    let (_guard, root) = example("soma");
    let (_cfg, cfg) = tempdir();
    solar(&cfg)
        .args(["-C", root.as_str(), "input", "soma", "0", "3", "-4", "5"])
        .assert()
        .success();
    let path = root.join("soma/Simulation/input_0.txt");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "3\n-4\n5\n");

    let from = cfg.join("dados.txt");
    std::fs::write(&from, "7\n8\n").unwrap();
    solar(&cfg)
        .args([
            "-C",
            root.as_str(),
            "input",
            "soma",
            "1",
            "--from",
            from.as_str(),
        ])
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(root.join("soma/Simulation/input_1.txt")).unwrap(),
        "7\n8\n"
    );
}

#[test]
fn missing_bundle_is_exit_code_2() {
    let (_guard, root) = example("soma");
    let (_cfg, cfg) = tempdir();
    // O binário de teste não tem bundle ao lado dele.
    let err = json(
        solar(&cfg).args(["-C", root.as_str(), "--json", "build"]),
        2,
    );
    assert_eq!(err["error"]["code"], "bundle_not_found");

    let err = json(
        solar(&cfg).args([
            "-C",
            root.as_str(),
            "--json",
            "build",
            "--toolchain",
            "/nao/existe",
        ]),
        2,
    );
    assert_eq!(err["error"]["code"], "invalid_bundle");
}

#[test]
fn bundle_next_to_the_executable_is_found_and_verified() {
    let (_guard, dir) = tempdir();
    let exe = assert_cmd::cargo::cargo_bin("solar");
    let install = dir.join("instalacao");
    std::fs::create_dir_all(install.join("bin")).unwrap();
    let installed = install
        .join("bin")
        .join(exe.file_name().unwrap().to_str().unwrap());
    std::fs::copy(&exe, &installed).unwrap();
    fake_bundle(&install.join("toolchain"));

    let run = |args: &[&str], code: i32| {
        let mut cmd = std::process::Command::new(installed.as_std_path());
        cmd.args(args)
            .env_remove("SOLAR_TOOLCHAIN")
            .env("SOLAR_CONFIG", dir.join("config.json"))
            .env("NO_COLOR", "1");
        let out = cmd.output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<Value>(&out.stdout).unwrap()
    };
    let tools = run(&["--json", "tools", "--verify"], 0);
    assert_eq!(
        Utf8Path::new(tools["root"].as_str().unwrap()),
        install.join("toolchain")
    );
    assert_eq!(tools["bundle"], "teste");
    assert_eq!(tools["verify"], serde_json::json!([]));
    assert_eq!(
        tools["tools"]["yosys"]["error"]["code"],
        "component_missing"
    );

    // Um executável alterado aparece no --verify, com código 1.
    std::fs::write(install.join("toolchain/yanc/bin/arquivo"), "mudou").unwrap();
    let tools = run(&["--json", "tools", "--verify"], 1);
    assert_eq!(tools["verify"][0]["path"], "yanc/bin/arquivo");
}

#[test]
fn config_set_compiler_validates_the_directory() {
    let (_guard, dir) = tempdir();
    fake_bundle(&dir.join("bundle"));
    let bundle = dir.join("bundle");
    let empty = dir.join("vazio");
    std::fs::create_dir_all(&empty).unwrap();
    solar(&dir)
        .args(["config", "set-compiler", empty.as_str()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("não tem perl, make"));

    // Um diretório com os três programas (vazios bastam para a conferência).
    let fake = dir.join("compilador");
    std::fs::create_dir_all(&fake).unwrap();
    let exe = if cfg!(windows) { ".exe" } else { "" };
    if cfg!(windows) {
        std::fs::create_dir_all(fake.join("usr/bin")).unwrap();
        std::fs::create_dir_all(fake.join("ucrt64/bin")).unwrap();
        for f in ["usr/bin/perl.exe", "usr/bin/make.exe", "ucrt64/bin/g++.exe"] {
            std::fs::write(fake.join(f), "").unwrap();
        }
    } else {
        for f in ["perl", "make", "g++"] {
            std::fs::write(fake.join(format!("{f}{exe}")), "").unwrap();
        }
    }
    solar(&dir)
        .args(["config", "set-compiler", fake.as_str()])
        .assert()
        .success();
    let tools = json(
        solar(&dir).args(["--json", "tools", "--toolchain", bundle.as_str()]),
        0,
    );
    assert!(
        tools["system_compiler"]["cxx"]
            .as_str()
            .unwrap()
            .contains("g++")
    );
    assert!(Utf8Path::new(tools["tools"]["perl"]["path"].as_str().unwrap()).starts_with(&fake));

    solar(&dir)
        .args(["config", "unset-compiler"])
        .assert()
        .success();
    let shown = json(solar(&dir).args(["--json", "config", "show"]), 0);
    assert!(shown["config"]["compiler_dir"].is_null());
}

#[test]
fn build_ok_is_exit_code_0() {
    let Some(tc) = env_or_skip("SOLAR_TEST_BUNDLE") else {
        return;
    };
    let (_guard, root) = example("soma");
    let (_cfg, cfg) = tempdir();
    let report = json(
        solar(&cfg)
            .args(["-C", root.as_str(), "--json", "build"])
            .env("SOLAR_TOOLCHAIN", &tc),
        0,
    );
    let results = report["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r["status"] == "succeeded"));
}

#[test]
fn build_error_is_exit_code_1_with_file_and_line() {
    let Some(tc) = env_or_skip("SOLAR_TEST_BUNDLE") else {
        return;
    };
    let (_guard, root) = example("com_erro");
    let (_cfg, cfg) = tempdir();
    solar(&cfg)
        .args(["-C", root.as_str(), "build", "-p", "conta"])
        .env("SOLAR_TOOLCHAIN", &tc)
        .assert()
        .code(1)
        .stdout(predicate::str::contains("conta.cmm:16: erro:"));
}

#[test]
fn unknown_processor_lists_available_ones() {
    let Some(tc) = env_or_skip("SOLAR_TEST_BUNDLE") else {
        return;
    };
    let (_guard, root) = example("soma");
    let (_cfg, cfg) = tempdir();
    solar(&cfg)
        .args(["-C", root.as_str(), "build", "-p", "nada"])
        .env("SOLAR_TOOLCHAIN", &tc)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("soma, filtro"));
}

#[test]
fn processor_simulation_writes_output() {
    let Some(tc) = env_or_skip("SOLAR_TEST_BUNDLE") else {
        return;
    };
    let (_guard, root) = example("soma");
    let (_cfg, cfg) = tempdir();
    let report = json(
        solar(&cfg)
            .args(["-C", root.as_str(), "--json", "sim", "-p", "soma"])
            .env("SOLAR_TOOLCHAIN", &tc),
        0,
    );
    assert_eq!(report["builds"][0]["status"], "succeeded");
    assert_eq!(report["simulation"]["status"], "succeeded");
    let output = json(
        solar(&cfg).args(["-C", root.as_str(), "--json", "output", "soma", "0"]),
        0,
    );
    assert_eq!(output["values"], serde_json::json!([55]));
}

#[test]
fn project_simulation_check_and_synthesis() {
    let Some(tc) = env_or_skip("SOLAR_TEST_BUNDLE") else {
        return;
    };
    let (_guard, root) = example("contador");
    let (_cfg, cfg) = tempdir();
    let run = |args: &[&str], code: i32| {
        let mut cmd = solar(&cfg);
        cmd.args(["-C", root.as_str()])
            .args(args)
            .env("SOLAR_TOOLCHAIN", &tc);
        json(&mut cmd, code)
    };

    let sim = run(&["--json", "sim"], 0);
    assert_eq!(sim["simulation"]["waveform"]["format"], "fst");

    let check = run(&["--json", "check"], 0);
    assert_eq!(check["status"], "succeeded");

    let synth = run(&["--json", "synth", "--svg"], 0);
    assert_eq!(
        synth["synthesis"]["modules"],
        serde_json::json!(["contador"])
    );
    assert!(Utf8Path::new(synth["schematic"]["svg"].as_str().unwrap()).is_file());
}

#[test]
fn completions_are_generated() {
    let (_cfg, cfg) = tempdir();
    solar(&cfg)
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("solar"));
}

#[test]
fn build_reports_all_failures_but_sim_stops_at_first() {
    let Some(tc) = env_or_skip("SOLAR_TEST_BUNDLE") else {
        return;
    };
    let (_guard, root) = example("com_erro");
    let (_cfg, cfg) = tempdir();
    let build = json(
        solar(&cfg)
            .args(["-C", root.as_str(), "--json", "build"])
            .env("SOLAR_TOOLCHAIN", &tc),
        1,
    );
    assert_eq!(build["results"].as_array().unwrap().len(), 2);

    let sim = json(
        solar(&cfg)
            .args(["-C", root.as_str(), "--json", "sim"])
            .env("SOLAR_TOOLCHAIN", &tc),
        1,
    );
    assert_eq!(sim["builds"].as_array().unwrap().len(), 1);
    assert!(sim["simulation"].is_null());
}

#[test]
fn input_from_file_rejects_garbage() {
    let (_guard, root) = example("soma");
    let (_cfg, cfg) = tempdir();
    let from = cfg.join("lixo.txt");
    std::fs::write(&from, "1\ndois\n").unwrap();
    let err = json(
        solar(&cfg).args([
            "-C",
            root.as_str(),
            "--json",
            "input",
            "soma",
            "0",
            "--from",
            from.as_str(),
        ]),
        2,
    );
    assert_eq!(err["error"]["code"], "invalid_data_file");
    assert!(err["error"]["message"].as_str().unwrap().contains(":2:"));
    assert!(
        !root.join("soma/Simulation/input_0.txt").exists(),
        "nada gravado"
    );
}

/// Sem display o Surfer não abre janela nenhuma: morre logo, e o comando
/// precisa dizer isso em vez de "aberto". Só no Linux (e outros Unix com X11
/// ou Wayland): no macOS e no Windows a interface gráfica não depende de
/// `DISPLAY`, e o Surfer abriria de verdade.
#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn wave_without_display_reports_early_exit() {
    let Some(tc) = env_or_skip("SOLAR_TEST_BUNDLE") else {
        return;
    };
    let (_guard, root) = example("contador");
    let (_cfg, cfg) = tempdir();
    let sim = json(
        solar(&cfg)
            .args(["-C", root.as_str(), "--json", "sim"])
            .env("SOLAR_TOOLCHAIN", &tc),
        0,
    );
    let wave = sim["simulation"]["waveform"]["path"]
        .as_str()
        .unwrap()
        .to_owned();
    let err = json(
        solar(&cfg)
            .args(["--json", "wave", &wave])
            .env("SOLAR_TOOLCHAIN", &tc)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY"),
        2,
    );
    assert_eq!(err["error"]["code"], "process_exited_early");
}
