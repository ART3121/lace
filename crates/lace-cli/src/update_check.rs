//! A verificação automática de atualização: um `lace` instalado pelo
//! instalador, rodando num terminal, procura uma vez por dia se há Lace mais
//! novo publicado e, se há, avisa no fim do comando, no stderr.
//!
//! A consulta é a mesma do `lace update` (`Sources::latest_lace` da biblioteca do instalador: a
//! última release no GitHub, ou o `latest` do espelho de `LACE_RELEASE_URL`)
//! e roda numa thread enquanto o comando roda. No fim, o comando espera por
//! ela no máximo [`WAIT`]; se ela não voltou a tempo, o aviso fica para a
//! próxima vez. O resultado fica em `update-check.json`, na pasta de cache do
//! usuário, com a hora da consulta e a do último aviso: o aviso de uma versão
//! sai uma vez por dia até a atualização.
//!
//! Não consulta nem avisa com `--json` ou `--events` (quem lê é um programa,
//! como o Studio e a extensão do VS Code), fora de um terminal, em
//! `lace update` (que já compara as versões), `lace uninstall` e nos comandos
//! internos, num `lace` que não veio do instalador, com `CI` definida ou com
//! `LACE_NO_UPDATE_CHECK` definida.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use lace_core::CancelToken;
use lace_installer::release::{Remote, Sources, is_newer, is_version};

use crate::installation;
use crate::output::{BOLD, paint};

/// De quanto em quanto tempo procurar, e repetir o aviso de uma versão.
const INTERVAL: u64 = 24 * 60 * 60;
/// Depois de uma consulta que não voltou (sem rede, ou o comando acabou
/// antes), quanto esperar para tentar de novo.
const RETRY: u64 = 60 * 60;
/// Quanto o fim do comando espera pela consulta que ainda roda.
const WAIT: Duration = Duration::from_millis(1500);
/// A variável que desliga a verificação.
pub const DISABLE_VAR: &str = "LACE_NO_UPDATE_CHECK";
/// O arquivo do estado, na pasta de cache do Lace.
const STATE_FILE: &str = "update-check.json";

/// O que a verificação guarda entre um comando e outro. As horas são
/// segundos desde 1970; zero é nunca.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct State {
    /// Quando a última consulta respondeu.
    checked: u64,
    /// Quando a última consulta começou, tenha respondido ou não.
    attempted: u64,
    /// A última versão publicada, na última consulta que respondeu.
    latest: Option<String>,
    /// A versão do último aviso.
    notified: Option<String>,
    /// Quando saiu o último aviso.
    notified_at: u64,
}

/// A verificação deste comando: o estado lido e, se uma consulta começou, de
/// onde vem a resposta.
pub struct Pending {
    path: PathBuf,
    state: State,
    answer: Option<mpsc::Receiver<Option<String>>>,
}

/// Começa a verificação, se ela vale para este comando (`wanted`: o comando
/// não é dos que ficam de fora, e a saída é texto). Sem consulta a fazer,
/// devolve só o estado, para o aviso de uma versão já conhecida.
pub fn start(wanted: bool) -> Option<Pending> {
    if !wanted
        || !std::io::stderr().is_terminal()
        || std::env::var_os(DISABLE_VAR).is_some_and(|v| !v.is_empty())
        || std::env::var_os("CI").is_some_and(|v| !v.is_empty())
        || installation::prefix().is_err()
    {
        return None;
    }
    let path = state_path()?;
    let mut state = load(&path);
    let now = now();
    let mut answer = None;
    if should_check(&state, now) {
        state.attempted = now;
        // Gravado antes de consultar: outro `lace` aberto ao mesmo tempo, ou
        // a próxima vez se esta não chegar ao fim, não repete a consulta.
        save(&path, &state);
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let latest = crate::update::network()
                .and_then(|network| {
                    Remote(network)
                        .latest_lace(&CancelToken::new())
                        .map_err(anyhow::Error::new)
                })
                .inspect_err(|error| tracing::info!("Update check: {error:#}"))
                .ok();
            let _ = send.send(latest);
        });
        answer = Some(receive);
    }
    Some(Pending {
        path,
        state,
        answer,
    })
}

/// No fim do comando: espera a consulta, se houver uma, por [`WAIT`] no
/// máximo, e avisa se há Lace mais novo.
pub fn finish(pending: Option<Pending>) {
    let Some(Pending {
        path,
        mut state,
        answer,
    }) = pending
    else {
        return;
    };
    let mut changed = false;
    if let Some(answer) = answer
        && let Ok(Some(latest)) = answer.recv_timeout(WAIT)
    {
        state.checked = now();
        state.latest = Some(latest);
        changed = true;
    }
    let installed = env!("CARGO_PKG_VERSION");
    if let Some(latest) = to_notify(&state, installed, now()).map(str::to_owned) {
        anstream::eprintln!(
            "\nLace {latest} is available (this is {installed}): update with {}",
            paint(BOLD, "lace update")
        );
        state.notified = Some(latest);
        state.notified_at = now();
        changed = true;
    }
    if changed {
        save(&path, &state);
    }
}

/// Guarda a última versão publicada que o `lace update` consultou: a
/// verificação automática não repete a consulta no mesmo dia.
pub fn record(latest: &str) {
    let Some(path) = state_path() else {
        return;
    };
    let mut state = load(&path);
    state.checked = now();
    state.attempted = state.checked;
    state.latest = Some(latest.to_owned());
    save(&path, &state);
}

/// Está na hora de consultar: a última resposta tem mais de um dia, e a
/// última tentativa, mais de uma hora. Uma hora no futuro (o relógio
/// voltou) não segura a consulta.
fn should_check(state: &State, now: u64) -> bool {
    let older = |time: u64, limit: u64| now.checked_sub(time).is_none_or(|age| age >= limit);
    older(state.checked, INTERVAL) && older(state.attempted, RETRY)
}

/// A versão a avisar, se há: a publicada é mais nova que a instalada, e o
/// aviso dela ainda não saiu, ou saiu há mais de um dia.
fn to_notify<'a>(state: &'a State, installed: &str, now: u64) -> Option<&'a str> {
    // O estado é um arquivo que outro `lace` gravou: só uma versão vai para o
    // terminal ([`is_version`]).
    let latest = state.latest.as_deref().filter(|v| is_version(v))?;
    if !is_newer(latest, installed) {
        return None;
    }
    let again = state.notified.as_deref() != Some(latest)
        || now
            .checked_sub(state.notified_at)
            .is_none_or(|age| age >= INTERVAL);
    again.then_some(latest)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `update-check.json` na pasta de cache do Lace: `%LOCALAPPDATA%\lace` no
/// Windows; nos outros, `$XDG_CACHE_HOME/lace` ou `~/.cache/lace`, como a
/// pasta dos downloads do `lace update` (`Network::work_dir`).
fn state_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)?
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?
    };
    Some(base.join("lace").join(STATE_FILE))
}

/// O estado gravado; um arquivo que falta ou que não se lê vale como nunca.
fn load(path: &std::path::Path) -> State {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Grava o estado por um arquivo provisório e renomeia: um `lace` que sai no
/// meio, ou dois gravando juntos, não deixam o arquivo pela metade. Uma
/// falha só vai para o log: a verificação não atrapalha o comando.
fn save(path: &std::path::Path, state: &State) {
    let write = || -> anyhow::Result<()> {
        let dir = path.parent().context("no parent folder")?;
        std::fs::create_dir_all(dir)?;
        let temp = dir.join(format!("{STATE_FILE}.{}", std::process::id()));
        std::fs::write(&temp, serde_json::to_string_pretty(state)?)?;
        std::fs::rename(&temp, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&temp);
        })?;
        Ok(())
    };
    if let Err(error) = write() {
        tracing::info!("Could not save {}: {error:#}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = INTERVAL;
    const NOW: u64 = 1_800_000_000;

    fn state(checked: u64, attempted: u64) -> State {
        State {
            checked,
            attempted,
            ..State::default()
        }
    }

    #[test]
    fn checks_once_a_day_and_retries_after_an_hour() {
        assert!(should_check(&State::default(), NOW));
        assert!(should_check(&state(NOW - DAY, NOW - DAY), NOW));
        assert!(!should_check(&state(NOW - DAY + 60, NOW - DAY + 60), NOW));
        // Uma tentativa sem resposta há pouco segura a próxima por uma hora.
        assert!(!should_check(&state(NOW - 2 * DAY, NOW - 60), NOW));
        assert!(should_check(&state(NOW - 2 * DAY, NOW - RETRY), NOW));
        // O relógio voltou: as horas gravadas ficaram no futuro.
        assert!(should_check(&state(NOW + DAY, NOW + DAY), NOW));
    }

    #[test]
    fn a_newer_version_is_announced_once_a_day() {
        let mut state = State {
            latest: Some("0.8.0".into()),
            ..State::default()
        };
        assert_eq!(to_notify(&state, "0.7.0", NOW), Some("0.8.0"));
        assert_eq!(to_notify(&state, "0.8.0", NOW), None);
        assert_eq!(to_notify(&state, "0.9.0", NOW), None);

        state.notified = Some("0.8.0".into());
        state.notified_at = NOW - 60;
        assert_eq!(to_notify(&state, "0.7.0", NOW), None);
        state.notified_at = NOW - DAY;
        assert_eq!(to_notify(&state, "0.7.0", NOW), Some("0.8.0"));

        // Uma versão mais nova que a do último aviso sai na hora.
        state.notified_at = NOW - 60;
        state.latest = Some("0.8.1".into());
        assert_eq!(to_notify(&state, "0.7.0", NOW), Some("0.8.1"));

        assert_eq!(to_notify(&State::default(), "0.7.0", NOW), None);

        // Um estado gravado com algo que não é versão (um espelho que mandou
        // uma sequência de escape) não vai para o terminal.
        state.latest = Some("9.9.9\x1b]0;x\x07".into());
        assert_eq!(to_notify(&state, "0.7.0", NOW), None);
    }

    #[test]
    fn the_state_survives_a_round_trip_and_a_bad_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lace").join(STATE_FILE);
        assert_eq!(load(&path), State::default());
        let state = State {
            checked: NOW,
            attempted: NOW,
            latest: Some("0.8.0".into()),
            notified: Some("0.8.0".into()),
            notified_at: NOW,
        };
        save(&path, &state);
        assert_eq!(load(&path), state);
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path), State::default());
    }
}
