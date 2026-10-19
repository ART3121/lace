//! Lace Studio: o backend do ambiente gráfico do Lace.
//!
//! O backend é uma casca fina sobre o `lace-core`, como a CLI: cada comando
//! Tauri abre o projeto, chama uma função do Core e devolve o resultado
//! serializado. A interface (React, em `src/`) só mostra
//! e pede.
//!
//! | Módulo | O que tem |
//! |---|---|
//! | [`commands::project`] | abrir, criar, retrato do projeto, arquivos Verilog, topo, testbench, processadores, mover arquivos, hierarquia |
//! | [`commands::files`] | árvore, ler e gravar texto, criar, renomear, copiar, lixeira, busca |
//! | [`jobs`] e [`flows`] | build, check, sim, synth, esquemático, compilação para a placa: operações canceláveis com saída ao vivo |
//! | [`commands::fpga`] | as placas, o `fpga.json` e a conferência das ligações; só com a placa FPGA ligada |
//! | [`commands::experimental`] | os recursos experimentais do Lace (a placa FPGA): quais estão ligados, ligar e desligar |
//! | [`commands::history`] | relatórios do `.lace/reports`: listar, mostrar, comparar, apagar |
//! | [`commands::learn`] | os exercícios do `lace learn`: abrir e criar a pasta, o exercício atual, restaurar |
//! | [`commands::toolchain`] e [`toolchain`] | o bundle, `lace install`, `lace update --check` |
//! | [`terminal`] | o terminal de shell |
//! | [`commands::app`] | versão, preferências, recentes, abrir a onda, sair e reiniciar |
//! | [`wave_tab`] | a onda numa aba: o cliente web do Surfer servido pelo Studio, e o layout que ela salva no projeto |
//! | [`commands::wave`] | a escolha dos sinais da onda e o layout salvo |
//!
//! A referência de cada comando, com o formato do que recebe e devolve,
//! está em `docs/IPC.md`.

#![forbid(unsafe_code)]

pub mod commands {
    //! Os comandos que a interface chama com `invoke`.
    pub mod app;
    pub mod experimental;
    pub mod files;
    pub mod fpga;
    pub mod history;
    pub mod learn;
    pub mod project;
    pub mod toolchain;
    pub mod wave;
}
pub mod error;
pub mod flows;
pub mod jobs;
pub mod settings;
pub mod state;
pub mod terminal;
pub mod toolchain;
pub mod watcher;
pub mod wave_tab;

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use camino::Utf8PathBuf;
use tauri::{AppHandle, Manager, RunEvent};

use crate::settings::SettingsStore;
use crate::state::AppState;

/// Quanto esperar a operação rodando terminar depois de cancelada, ao
/// fechar o Studio.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);

/// A janela nasce escondida, com o splash na frente, e a interface a mostra
/// quando está pronta (`reveal`, em `src/App.tsx`). Se isso não acontecer
/// (o servidor do Vite fora do ar, um módulo que não carregou), ela aparece
/// mesmo assim depois deste tempo, com o fundo escuro, e o splash fecha, em
/// vez de o Studio ficar só no splash.
const REVEAL_FALLBACK: Duration = Duration::from_secs(10);

/// O executável com que o Studio abriu. Fica guardado antes de tudo porque
/// a atualização o troca: no Linux, o `current_exe` de depois seria o
/// arquivo apagado.
static STARTING_EXE: OnceLock<PathBuf> = OnceLock::new();

/// O Studio abre de novo depois de sair ([`exit`]).
static RESTART: AtomicBool = AtomicBool::new(false);

/// Fecha o splash, se ele ainda está aberto.
pub fn close_splash(app: &AppHandle) {
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.close();
    }
}

/// Sai do Studio do jeito de sempre (a operação rodando é cancelada e os
/// terminais fecham, em `shutdown`) e, com `restart`, abre de novo o
/// executável com que ele começou, que depois de uma atualização é o da
/// versão nova.
pub fn exit(app: &AppHandle, restart: bool) {
    RESTART.store(restart, Ordering::SeqCst);
    app.exit(0);
}

/// Abre o Studio.
pub fn run() {
    init_tracing();
    match std::env::current_exe() {
        Ok(exe) => {
            let _ = STARTING_EXE.set(exe);
        }
        Err(error) => tracing::warn!("Could not find the Studio executable: {error}"),
    }
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            let dir = Utf8PathBuf::from_path_buf(dir)
                .map_err(|p| format!("Config directory is not UTF-8: {}", p.display()))?;
            app.manage(AppState::new(SettingsStore::load(&dir)));
            app.manage(wave_tab::WaveTabs::default());
            #[cfg(unix)]
            signals::install(app.handle().clone());
            tracing::info!("Backend ready");
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(REVEAL_FALLBACK);
                if let Some(window) = handle.get_webview_window("main")
                    && !window.is_visible().unwrap_or(true)
                {
                    tracing::warn!("The interface did not show the window in time; showing it");
                    let _ = window.show();
                }
                close_splash(&handle);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_info,
            commands::app::settings_get,
            commands::app::settings_set,
            commands::app::recent_projects,
            commands::app::recent_forget,
            commands::app::wave_open,
            commands::app::log_frontend,
            commands::app::dev_smoke_script,
            commands::app::app_exit,
            commands::app::splash_close,
            commands::project::project_default_parent,
            commands::project::project_open,
            commands::project::project_create,
            commands::project::project_close,
            commands::project::project_snapshot,
            commands::project::project_add_verilog,
            commands::project::project_remove_verilog,
            commands::project::project_set_top,
            commands::project::project_set_testbench,
            commands::project::project_processor_defaults,
            commands::project::project_add_processor,
            commands::project::project_remove_processor,
            commands::project::project_configure_processor,
            commands::project::project_output_values,
            commands::project::verilog_modules,
            commands::project::project_check_name,
            commands::project::processor_check_name,
            commands::project::project_reorder,
            commands::project::project_move,
            commands::project::project_convert_layout,
            commands::project::project_hierarchy,
            commands::files::fs_read_dir,
            commands::files::fs_read_text,
            commands::files::fs_write_text,
            commands::files::fs_stat,
            commands::files::fs_create_file,
            commands::files::fs_create_dir,
            commands::files::fs_rename,
            commands::files::fs_copy,
            commands::files::fs_trash,
            commands::files::fs_list_files,
            commands::files::fs_search,
            commands::history::history_list,
            commands::history::history_show,
            commands::history::history_compare,
            commands::history::history_plan_cleanup,
            commands::history::history_clean,
            commands::learn::learn_tracks,
            commands::learn::learn_open,
            commands::learn::learn_init,
            commands::learn::learn_set_current,
            commands::learn::learn_reset,
            commands::experimental::experimental_list,
            commands::experimental::experimental_set,
            commands::fpga::fpga_boards,
            commands::fpga::fpga_config,
            commands::fpga::fpga_config_set,
            commands::fpga::fpga_top,
            commands::fpga::fpga_check,
            commands::fpga::fpga_modules,
            commands::fpga::fpga_status,
            commands::fpga::fpga_cables,
            commands::toolchain::toolchain_info,
            commands::toolchain::toolchain_verify,
            commands::toolchain::lace_update_check,
            commands::toolchain::lace_install,
            commands::toolchain::lace_update,
            jobs::flow_start,
            jobs::flow_cancel,
            jobs::flow_running,
            terminal::terminal_spawn,
            terminal::terminal_write,
            terminal::terminal_ack,
            terminal::terminal_cd,
            terminal::terminal_resize,
            terminal::terminal_kill,
            wave_tab::wave_tab_open,
            wave_tab::wave_tab_close,
            commands::wave::wave_signals,
            commands::wave::wave_selection_set,
            commands::wave::wave_layout_reset,
        ])
        .build(tauri::generate_context!())
        .expect("Lace Studio could not start");
    app.run(|app, event| {
        if let RunEvent::Exit = event {
            shutdown(app);
            if RESTART.load(Ordering::SeqCst) {
                relaunch();
            }
        }
    });
}

/// Abre um Studio novo, com os mesmos argumentos, enquanto este termina de
/// sair. Não é o `AppHandle::restart` do Tauri porque ele recusa, no macOS,
/// um caminho com symlink, e o Studio pode ter sido aberto pelo atalho de
/// `~/Applications`, que é um.
fn relaunch() {
    let Some(exe) = STARTING_EXE.get() else {
        tracing::error!("Not restarting: the Studio executable is unknown");
        return;
    };
    match std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .spawn()
    {
        Ok(_) => tracing::info!("Restarting {}", exe.display()),
        Err(error) => tracing::error!("Could not restart {}: {error}", exe.display()),
    }
}

/// Ao sair: cancela a operação rodando (que encerra as ferramentas com tudo
/// o que elas iniciaram) e fecha os terminais.
fn shutdown(app: &AppHandle) {
    jobs::cancel_and_wait(app, SHUTDOWN_WAIT);
    terminal::kill_all(app);
}

/// Log no stderr. `RUST_LOG` escolhe o nível (padrão: avisos, e `info` do
/// Studio).
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,lace_studio_lib=info,lace_studio=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

#[cfg(unix)]
mod signals {
    //! SIGTERM, SIGINT e SIGHUP fecham o Studio do jeito certo: cancelam a
    //! operação rodando antes de sair. Sem isso, um sinal mataria só o
    //! Studio, e a ferramenta, num grupo de processos próprio, continuaria
    //! rodando (API.md do Lace, 9, "Limites").

    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;
    use tauri::AppHandle;

    pub fn install(app: AppHandle) {
        let mut signals = match Signals::new([SIGTERM, SIGINT, SIGHUP]) {
            Ok(signals) => signals,
            Err(error) => {
                tracing::warn!("Could not install the signal handlers: {error}");
                return;
            }
        };
        std::thread::spawn(move || {
            if signals.forever().next().is_some() {
                super::shutdown(&app);
                app.exit(130);
            }
        });
    }
}
