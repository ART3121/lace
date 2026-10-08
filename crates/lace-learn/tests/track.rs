//! As trilhas do repositório (`lace-learn/`, que viram o componente
//! `lace-learn`): carregam, e cada exercício passa no `dev check` (o
//! `start.v` não passa, a `solution.v` passa). O `dev check` precisa do
//! bundle em `LACE_TEST_BUNDLE`; sem ele, avisa e passa.

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::{Control, Toolchain};
use lace_learn::{available_tracks, check_track, load_track};

fn tracks() -> Utf8PathBuf {
    Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lace-learn")
}

#[test]
fn the_tracks_of_this_repository_load() {
    let ids = available_tracks(&tracks()).unwrap();
    assert!(ids.contains(&"verilog".to_owned()), "{ids:?}");
    for id in ids {
        let track = load_track(&tracks(), &id, None).unwrap();
        for exercise in track.exercises() {
            assert!(!exercise.hints.is_empty(), "{} sem dica", exercise.name);
        }
    }
}

#[test]
fn every_exercise_of_this_repository_passes_the_dev_check() {
    let toolchain = match std::env::var("LACE_TEST_BUNDLE") {
        Ok(dir) => Toolchain::open(&dir).expect("LACE_TEST_BUNDLE não é um bundle válido"),
        Err(_) if std::env::var_os("CI").is_some() => {
            panic!("CI definido, mas LACE_TEST_BUNDLE não: estes testes não podem ser pulados")
        }
        Err(_) => {
            eprintln!("PULADO: defina LACE_TEST_BUNDLE com um bundle de scripts/bundle.py");
            return;
        }
    };
    for id in available_tracks(&tracks()).unwrap() {
        let track = load_track(&tracks(), &id, None).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let scratch = Utf8PathBuf::from_path_buf(tmp.path().join(&id)).unwrap();
        let report = check_track(&toolchain, &track, &scratch, &Control::new(), |_| {}).unwrap();
        let problems: Vec<String> = report
            .exercises
            .iter()
            .filter(|e| !e.problems.is_empty())
            .map(|e| format!("{}: {}", e.name, e.problems.join("; ")))
            .collect();
        assert!(problems.is_empty(), "trilha {id}:\n{}", problems.join("\n"));
    }
}
