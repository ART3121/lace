//! As preferências do Studio e a lista de projetos recentes.
//!
//! Ficam num arquivo só, `settings.json`, na pasta de configuração do
//! aplicativo (`~/.config/com.nipscern.lace-studio/` no Linux,
//! `%APPDATA%\com.nipscern.lace-studio\` no Windows,
//! `~/Library/Application Support/com.nipscern.lace-studio/` no macOS).
//!
//! A leitura é tolerante: campo ausente fica com o padrão, e um arquivo
//! ilegível é renomeado para `settings.json.bad` e o Studio abre com os
//! padrões, em vez de não abrir.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::error::{IpcError, IpcResult};

/// Quantos projetos recentes guardar.
const MAX_RECENT: usize = 15;

/// As preferências. Todo campo tem padrão (`#[serde(default)]`), para que um
/// arquivo de uma versão anterior continue valendo.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Idioma da interface: `system`, `pt` ou `en`.
    pub language: String,
    /// Tema: o id de um tema de `src/themes/catalog.ts` (`atlas`,
    /// `atlas-light`, `dracula`...) ou `system`, que fica com o Atlas ou o
    /// Atlas Branco conforme o sistema. Um id que a interface não conhece
    /// vale o padrão, `atlas`.
    pub theme: String,
    /// Outro bundle no lugar do instalado, como o `--toolchain` da CLI.
    /// `None`: procura a instalação do Lace (ver `toolchain.rs`).
    pub toolchain_dir: Option<String>,
    /// Onde estão `perl`, `make` e o compilador C++ do Verilator, como o
    /// `--compiler` da CLI. No Windows, a raiz de um MSYS2.
    pub compiler_dir: Option<String>,
    /// Simulador padrão: `icarus` ou `verilator`.
    pub simulator: String,
    /// Abrir a onda no surfer-aurora quando a simulação terminar bem, como o
    /// botão Wave da AURORA.
    pub open_wave_after_sim: bool,
    /// Onde a onda abre: `tab` (numa aba do Studio, com o cliente web do
    /// Surfer) ou `window` (o surfer-aurora em janela própria). Sem o
    /// cliente web no bundle, `tab` abre em janela.
    pub wave_viewer: String,
    /// Prazo da simulação, em segundos (o `--timeout` da CLI). `None`: sem
    /// prazo.
    pub sim_timeout_s: Option<u64>,
    /// Mostrar nos consoles o comando de cada passo e as mensagens `info`
    /// (o `-v` da CLI).
    pub verbose: bool,
    /// Reabrir o último projeto ao iniciar.
    pub restore_last_project: bool,
    /// O shell do terminal no Windows: `powershell` (o Windows PowerShell)
    /// ou `cmd` (o Prompt de Comando). Nos outros sistemas não vale: o
    /// terminal abre o shell do usuário (`$SHELL`).
    pub terminal_shell: String,
    /// Preferências do editor.
    pub editor: EditorSettings,
    /// Preferências do modo zen.
    pub zen: ZenSettings,
    /// Os layouts da janela.
    pub layouts: LayoutSettings,
    /// Projetos abertos recentemente, do mais novo para o mais antigo.
    pub recent_projects: Vec<RecentProject>,
}

impl Settings {
    /// Traz valores gravados por versões anteriores para os de agora.
    fn migrate(&mut self) {
        // Até 2026-10-04 havia só os temas `dark` e `light`, que viraram o
        // Atlas e o Atlas Branco.
        match self.theme.as_str() {
            "dark" => self.theme = "atlas".into(),
            "light" => self.theme = "atlas-light".into(),
            _ => {}
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "system".into(),
            theme: "atlas".into(),
            toolchain_dir: None,
            compiler_dir: None,
            simulator: "icarus".into(),
            open_wave_after_sim: true,
            wave_viewer: "tab".into(),
            sim_timeout_s: None,
            verbose: false,
            restore_last_project: true,
            terminal_shell: "powershell".into(),
            editor: EditorSettings::default(),
            zen: ZenSettings::default(),
            layouts: LayoutSettings::default(),
            recent_projects: Vec::new(),
        }
    }
}

/// Preferências do editor de texto.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorSettings {
    /// Tamanho da fonte, em pixels.
    pub font_size: u32,
    /// Largura da tabulação, em espaços.
    pub tab_size: u32,
    /// Quebrar linhas longas.
    pub word_wrap: bool,
    /// Mostrar o minimapa.
    pub minimap: bool,
    /// Gravar sozinho ao trocar de aba ou de janela.
    pub auto_save: bool,
    /// Teclas do Vim no editor (`monaco-vim`).
    pub vim_mode: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        EditorSettings {
            font_size: 13,
            tab_size: 4,
            word_wrap: false,
            minimap: true,
            auto_save: false,
            vim_mode: false,
        }
    }
}

/// Preferências do modo zen, em que só o editor fica na tela.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ZenSettings {
    /// Pôr a janela em tela cheia ao entrar.
    pub fullscreen: bool,
    /// Centralizar o editor, com largura de umas 110 colunas, quando há um
    /// grupo só.
    pub center_layout: bool,
    /// Mostrar a barra de abas.
    pub show_tabs: bool,
    /// Esconder os números de linha.
    pub hide_line_numbers: bool,
}

impl Default for ZenSettings {
    fn default() -> Self {
        ZenSettings {
            fullscreen: true,
            center_layout: true,
            show_tabs: false,
            hide_line_numbers: false,
        }
    }
}

/// Os layouts da janela: o que está em uso e os que o usuário gravou
/// (Exibir > Layout, Preferências > Layout).
///
/// O formato de um layout é da interface (`src/state/layoutModel.ts`, com a
/// versão em `v`), e o backend não o lê: guarda cada um como veio. Assim um
/// layout malformado, ou gravado por uma versão mais nova do Studio, não leva
/// o `settings.json` inteiro para o `.bad`, e um Studio mais velho não corta
/// os campos que não conhece ao gravar outra preferência.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutSettings {
    /// O layout em uso: o id de um dos gravados ou de um pronto da
    /// interface (`default`, o Padrão).
    pub active: String,
    /// Os layouts gravados, na ordem em que aparecem.
    pub saved: Vec<serde_json::Value>,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        LayoutSettings {
            active: "default".into(),
            saved: Vec::new(),
        }
    }
}

/// Um projeto da lista de recentes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentProject {
    /// O `.spf`.
    pub spf: String,
    /// O nome do projeto.
    pub name: String,
    /// Quando foi aberto pela última vez, em milissegundos desde 1970.
    pub opened_at_ms: u64,
}

/// As preferências na memória e o arquivo onde ficam.
pub struct SettingsStore {
    path: Utf8PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// Lê `<dir>/settings.json`, ou começa com os padrões.
    pub fn load(dir: &Utf8Path) -> Self {
        let path = dir.join("settings.json");
        let current = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(mut settings) => {
                    settings.migrate();
                    settings
                }
                Err(error) => {
                    tracing::warn!("Unreadable {path} ({error}); starting with defaults");
                    let _ = std::fs::rename(&path, path.with_extension("json.bad"));
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        };
        SettingsStore {
            path,
            current: Mutex::new(current),
        }
    }

    /// Uma cópia das preferências atuais.
    pub fn get(&self) -> Settings {
        self.current.lock().expect("settings lock").clone()
    }

    /// Troca as preferências e grava. A lista de recentes não vem da
    /// interface: é mantida aqui, por [`SettingsStore::touch_recent`].
    pub fn set(&self, mut settings: Settings) -> IpcResult<Settings> {
        let mut current = self.current.lock().expect("settings lock");
        settings.recent_projects = current.recent_projects.clone();
        *current = settings;
        self.save(&current)?;
        Ok(current.clone())
    }

    /// Põe um projeto no topo da lista de recentes.
    pub fn touch_recent(&self, spf: &Utf8Path, name: &str) -> IpcResult<()> {
        let mut current = self.current.lock().expect("settings lock");
        current.recent_projects.retain(|r| r.spf != spf.as_str());
        current.recent_projects.insert(
            0,
            RecentProject {
                spf: spf.to_string(),
                name: name.to_owned(),
                opened_at_ms: now_ms(),
            },
        );
        current.recent_projects.truncate(MAX_RECENT);
        self.save(&current)
    }

    /// Tira um projeto da lista de recentes.
    pub fn forget_recent(&self, spf: &str) -> IpcResult<()> {
        let mut current = self.current.lock().expect("settings lock");
        current.recent_projects.retain(|r| r.spf != spf);
        self.save(&current)
    }

    /// Gravação atômica: arquivo temporário e `rename`, como o Core faz com
    /// o `.spf`.
    fn save(&self, settings: &Settings) -> IpcResult<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| IpcError::io(dir, e))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(settings)?;
        std::fs::write(&tmp, text).map_err(|e| IpcError::io(&tmp, e))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| IpcError::io(&self.path, e))?;
        Ok(())
    }
}

/// Agora, em milissegundos desde 1970.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_defaults() {
        let settings: Settings = serde_json::from_str(r#"{ "theme": "dracula" }"#).unwrap();
        assert_eq!(settings.theme, "dracula");
        assert_eq!(settings.simulator, "icarus");
        assert_eq!(settings.editor.tab_size, 4);
        assert!(settings.zen.fullscreen);
        assert!(!settings.zen.show_tabs);
        assert_eq!(settings.layouts.active, "default");
        assert!(settings.layouts.saved.is_empty());
    }

    #[test]
    fn saved_layouts_pass_through_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(dir.path()).unwrap();
        // Um layout de uma versão futura, com um campo que esta não conhece,
        // e duas entradas que nem são layouts.
        let layouts = serde_json::json!({
            "active": "u-1",
            "saved": [
                { "v": 9, "id": "u-1", "name": "Simulação", "future": [1, 2] },
                42,
                "nem é objeto",
            ],
        });
        let text = serde_json::json!({ "theme": "dracula", "layouts": layouts }).to_string();
        std::fs::write(dir.join("settings.json"), text).unwrap();

        let store = SettingsStore::load(dir);
        assert!(!dir.join("settings.json.bad").exists());
        let settings = store.get();
        assert_eq!(settings.layouts.active, "u-1");
        assert_eq!(serde_json::json!(settings.layouts.saved), layouts["saved"]);

        // Gravar outra preferência devolve os layouts como vieram.
        store
            .set(Settings {
                theme: "atlas".into(),
                ..settings
            })
            .unwrap();
        let reloaded = SettingsStore::load(dir).get();
        assert_eq!(reloaded.theme, "atlas");
        assert_eq!(serde_json::json!(reloaded.layouts.saved), layouts["saved"]);
    }

    #[test]
    fn layouts_come_from_the_interface() {
        // Ao contrário dos recentes, que `set` mantém, os layouts gravados
        // são os que a interface manda.
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(dir.path()).unwrap();
        let store = SettingsStore::load(dir);
        let mut settings = store.get();
        settings.layouts.active = "u-2".into();
        settings
            .layouts
            .saved
            .push(serde_json::json!({ "v": 1, "id": "u-2", "name": "Painel à direita" }));
        store.set(settings).unwrap();

        let reloaded = SettingsStore::load(dir).get();
        assert_eq!(reloaded.layouts.active, "u-2");
        assert_eq!(reloaded.layouts.saved.len(), 1);
        assert_eq!(reloaded.layouts.saved[0]["name"], "Painel à direita");
    }

    #[test]
    fn recents_are_unique_newest_first_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(dir.path()).unwrap();
        let store = SettingsStore::load(dir);
        for i in 0..20 {
            store
                .touch_recent(Utf8Path::new(&format!("/p/{i}.spf")), &format!("p{i}"))
                .unwrap();
        }
        store.touch_recent(Utf8Path::new("/p/3.spf"), "p3").unwrap();
        let recent = SettingsStore::load(dir).get().recent_projects;
        assert_eq!(recent.len(), MAX_RECENT);
        assert_eq!(recent[0].spf, "/p/3.spf");
        assert_eq!(recent.iter().filter(|r| r.spf == "/p/3.spf").count(), 1);
    }

    #[test]
    fn unreadable_file_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(dir.join("settings.json"), "{ não é json").unwrap();
        let store = SettingsStore::load(dir);
        assert_eq!(store.get().theme, "atlas");
        assert!(dir.join("settings.json.bad").is_file());
    }

    #[test]
    fn old_dark_and_light_themes_become_atlas() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(dir.path()).unwrap();
        for (old, new) in [
            ("dark", "atlas"),
            ("light", "atlas-light"),
            ("system", "system"),
        ] {
            std::fs::write(
                dir.join("settings.json"),
                format!(r#"{{ "theme": "{old}" }}"#),
            )
            .unwrap();
            assert_eq!(SettingsStore::load(dir).get().theme, new);
        }
    }
}
