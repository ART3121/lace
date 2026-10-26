//! Abrir uma onda com o layout dos processadores SAPHO, e a escolha de
//! sinais do testbench do projeto: o `lace wave` e o `lace wave select` da
//! CLI e o `wave.open` e o `wave.choose` do protocolo, numa regra só.

use std::time::Duration;

use camino::Utf8PathBuf;

use super::{
    LayoutPolicy, SavedLayoutInfo, WaveChoice, WaveChoiceOutcome, WaveOpened, WaveRequest,
    WaveSource,
};
use crate::{Control, LaceError, Project, Result, Severity, SignalScope, Toolchain, ViewerOptions};

/// Quanto esperar para saber se o surfer-aurora abriu: sem display ou com
/// onda inválida, ele fecha em menos de um segundo.
pub const SURFER_GRACE: Duration = Duration::from_millis(1500);

/// Abre uma onda no surfer-aurora e confere que ele subiu.
///
/// Com [`WaveSource::Project`], a onda é a da simulação do processador ou do
/// projeto ([`waveform_path`](crate::waveform_path)). Uma onda que não existe,
/// do projeto ou um arquivo, é [`LaceError::NoWaveform`]. Com
/// [`LayoutPolicy::Reset`], o layout
/// salvo no projeto da onda sai antes, e o testbench dele vai em
/// [`WaveOpened::reset_deleted`]. Com `Generated` ou `Reset`, o layout dos
/// processadores é gerado ([`prepare_wave_layout`](crate::prepare_wave_layout));
/// se isso falhar, a onda abre sem layout e o motivo vai em
/// [`WaveOpened::layout_error`]. O Surfer continua aberto depois; uma thread
/// espera ele fechar, para o processo não ficar zumbi.
pub fn open_wave(
    toolchain: &Toolchain,
    project: Option<&Project>,
    request: &WaveRequest,
) -> Result<WaveOpened> {
    let waveform = wave_file(project, &request.source)?;

    // O layout salvo no projeto sai antes: a onda abre com o gerado, que
    // passa a ser o salvo. O projeto é o da onda, não o de quem chamou.
    let mut reset_deleted = None;
    if matches!(request.layout, LayoutPolicy::Reset)
        && let Ok(owner) = Project::discover(&waveform)
        && let Some(testbench) = crate::wave_testbench_of(&waveform)?
        && crate::reset_saved_layout(&owner, &testbench)?
    {
        reset_deleted = Some(testbench);
    }

    let mut layout_error = None;
    let (options, prepared) = match &request.layout {
        LayoutPolicy::None => (ViewerOptions::default(), None),
        LayoutPolicy::File { path } => (
            ViewerOptions {
                layout: Some(path.clone()),
                working_dir: None,
            },
            None,
        ),
        LayoutPolicy::Generated | LayoutPolicy::Reset => {
            match crate::prepare_wave_layout(&waveform) {
                Ok(Some(prepared)) => (ViewerOptions::with_layout(&prepared), Some(prepared)),
                Ok(None) => (ViewerOptions::default(), None),
                Err(error) => {
                    tracing::warn!("No processor layout for {waveform}: {error}");
                    layout_error = Some(error.to_string());
                    (ViewerOptions::default(), None)
                }
            }
        }
    };

    let mut surfer = crate::open_waveform(toolchain, &waveform, &options)?;
    surfer.ensure_started(SURFER_GRACE)?;
    let processors = prepared
        .as_ref()
        .map(|p| p.layout.processors.clone())
        .unwrap_or_default();
    let opened = WaveOpened {
        waveform,
        pid: surfer.id(),
        log: surfer.log_file().to_owned(),
        layout: options.layout.clone(),
        saved_layout: prepared
            .as_ref()
            .and_then(|p| p.saved.as_ref())
            .map(|saved| SavedLayoutInfo {
                path: saved.path.clone(),
                customized: saved.customized,
            }),
        outdated: processors
            .iter()
            .filter(|p| p.outdated)
            .map(|p| p.processor.clone())
            .collect(),
        selection: prepared
            .as_ref()
            .map(|p| p.layout.selection.clone())
            .unwrap_or_default(),
        processors,
        layout_error,
        reset_deleted,
    };
    // Colher o processo quando ele terminar, para não ficar zumbi; se a
    // thread não puder nascer, o Surfer fica como a CLI sempre o deixou.
    let _ = std::thread::Builder::new()
        .name("lace-surfer-wait".into())
        .spawn(move || {
            let _ = surfer.wait();
        });
    Ok(opened)
}

/// [`open_wave`] pedindo o bundle só depois de conferir que a onda existe
/// ([`wave_file`]): a onda que a simulação ainda não gravou, ou o
/// processador ainda não compilado, é o erro, e não um bundle que falta.
/// `toolchain` é como quem chama acha o bundle, e o erro dele sai com o tipo
/// de quem chama.
pub fn open_wave_with<E: From<LaceError>>(
    project: Option<&Project>,
    request: &WaveRequest,
    toolchain: impl FnOnce() -> std::result::Result<Toolchain, E>,
) -> std::result::Result<WaveOpened, E> {
    wave_file(project, &request.source)?;
    let toolchain = toolchain()?;
    Ok(open_wave(&toolchain, project, request)?)
}

/// O arquivo da onda de `source`, que precisa existir: um arquivo, ou a
/// onda da simulação do processador ou do projeto
/// ([`waveform_path`](crate::waveform_path)). Uma onda que não existe é
/// [`LaceError::NoWaveform`], com o processador quando é a dele. Não precisa
/// do bundle: quem abre a onda pode conferir antes de procurar o Surfer.
pub fn wave_file(project: Option<&Project>, source: &WaveSource) -> Result<Utf8PathBuf> {
    match source {
        WaveSource::File { path } => {
            if !path.is_file() {
                return Err(LaceError::NoWaveform {
                    path: path.clone(),
                    processor: None,
                });
            }
            Ok(path.clone())
        }
        WaveSource::Project { processor } => {
            let Some(project) = project else {
                return Err(LaceError::InvalidProject {
                    path: Utf8PathBuf::new(),
                    reason: "a project is needed to find its waveform".to_owned(),
                });
            };
            let processor = processor
                .as_deref()
                .map(|name| project.require_processor(name))
                .transpose()?;
            let path = crate::waveform_path(project, processor)?;
            if !path.is_file() {
                return Err(LaceError::NoWaveform {
                    path,
                    processor: processor.map(|p| p.name.clone()),
                });
            }
            Ok(path)
        }
    }
}

/// Muda a escolha de sinais da onda do testbench do projeto
/// (`wave/<testbench>.json`) e devolve como ficou.
///
/// `All`, e `Set` com a lista vazia, esvaziam a escolha sem elaborar nada.
/// `Add`, `Remove` e `Set` elaboram o testbench pelo Icarus
/// ([`wave_signals`](crate::wave_signals)) para conferir os caminhos: um que
/// a árvore não tem é
/// [`LaceError::SignalNotFound`] (com até cinco caminhos de mesmo nome
/// final, para sugerir); em `Remove`, um que não está na escolha nem no
/// design é [`LaceError::SignalNotChosen`], e um que está na escolha mas o
/// design não tem mais sai do mesmo jeito. Uma elaboração que falhou é
/// [`LaceError::TestbenchNotElaborated`].
pub fn choose_signals(
    toolchain: &Toolchain,
    project: &Project,
    choice: &WaveChoice,
    control: &Control,
) -> Result<WaveChoiceOutcome> {
    let module = crate::wave_testbench(project)?;
    let selection = match choice {
        WaveChoice::All => Vec::new(),
        WaveChoice::Set(paths) if paths.is_empty() => Vec::new(),
        WaveChoice::Add(paths) | WaveChoice::Set(paths) => {
            let tree = elaborated(toolchain, project, control)?;
            let root = tree.root.as_ref().expect("conferido em elaborated");
            for path in paths {
                if !root.contains(path) {
                    return Err(not_found(root, path));
                }
            }
            let mut selection = match choice {
                WaveChoice::Add(_) => tree.selection.clone(),
                _ => Vec::new(),
            };
            selection.extend(paths.iter().cloned());
            selection
        }
        WaveChoice::Remove(paths) => {
            let tree = elaborated(toolchain, project, control)?;
            let root = tree.root.as_ref().expect("conferido em elaborated");
            let mut selection = tree.selection.clone();
            for path in paths {
                selection = match root.without(&selection, path) {
                    Some(rest) => rest,
                    // Um item que o design não tem mais também sai.
                    None if selection.contains(path) => {
                        selection.into_iter().filter(|s| s != path).collect()
                    }
                    None => {
                        return Err(LaceError::SignalNotChosen {
                            path: path.clone(),
                            testbench: module,
                        });
                    }
                };
            }
            selection
        }
    };
    let file = crate::write_selection(project, &module, &selection)?;
    let selection = crate::read_selection(project, &module)?;
    Ok(WaveChoiceOutcome {
        testbench: module,
        file,
        selection,
    })
}

/// A árvore de sinais, exigindo que a elaboração tenha dado certo.
fn elaborated(
    toolchain: &Toolchain,
    project: &Project,
    control: &Control,
) -> Result<crate::WaveSignals> {
    let tree = crate::wave_signals(toolchain, project, control)?;
    if tree.root.is_none() {
        let reason = tree
            .diagnostics
            .iter()
            .find(|d| d.severity == Severity::Error)
            .map(|d| d.message.clone())
            .unwrap_or_default();
        return Err(LaceError::TestbenchNotElaborated { reason });
    }
    Ok(tree)
}

/// O erro de um caminho que a árvore não tem, com até cinco caminhos de
/// mesmo nome final.
fn not_found(root: &SignalScope, path: &str) -> LaceError {
    let name = path.rsplit('.').next().unwrap_or(path);
    let mut similar = Vec::new();
    collect_named(root, name, &mut similar);
    similar.truncate(5);
    LaceError::SignalNotFound {
        path: path.to_owned(),
        root: root.path.clone(),
        similar,
    }
}

fn collect_named(scope: &SignalScope, name: &str, out: &mut Vec<String>) {
    if scope.name == name {
        out.push(scope.path.clone());
    }
    out.extend(
        scope
            .signals
            .iter()
            .filter(|s| s.name == name)
            .map(|s| s.path.clone()),
    );
    for child in &scope.scopes {
        collect_named(child, name, out);
    }
}
