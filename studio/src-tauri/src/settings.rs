//! As preferências do Studio e a lista de projetos recentes.
//!
//! Ficam num arquivo só, `settings.json`, na pasta de configuração do
//! aplicativo (`~/.config/com.nipscern.aurora-ide/` no Linux,
//! `%APPDATA%\com.nipscern.aurora-ide\` no Windows,
//! `~/Library/Application Support/com.nipscern.aurora-ide/` no macOS).
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
    /// A paleta do tema: o id do tema escuro dela em
    /// `src/themes/catalog.ts` (`boreal`, `dark-modern`, `dracula`...). Um id
    /// que a interface não conhece vale o padrão, `boreal`.
    pub theme: String,
    /// O modo do tema: `dark`, `light` ou `system` (segue o sistema). Vazio
    /// num `settings.json` de antes do campo: `migrate` o tira do `theme`, que
    /// então era o tema inteiro (`boreal-claro`) ou `system`.
    #[serde(default)]
    pub theme_mode: String,
    /// A aparência da interface: `classic`, com as regiões encostadas e
    /// separadas por linhas, ou `islands`, com as regiões em ilhas de cantos
    /// redondos e vãos entre elas. Um valor que a interface não conhece vale
    /// `classic`.
    pub look: String,
    /// Outro bundle no lugar do instalado, como o `--toolchain` da CLI.
    /// `None`: procura a instalação da AURORA (ver `toolchain.rs`).
    pub toolchain_dir: Option<String>,
    /// Onde estão `perl`, `make` e o compilador C++ do Verilator, como o
    /// `--compiler` da CLI. No Windows, a raiz de um MSYS2.
    pub compiler_dir: Option<String>,
    /// Onde está o Quartus Prime, que compila para as placas Intel, como o
    /// `--quartus` da CLI. `None`: o `AURORA_QUARTUS`, o `QUARTUS_ROOTDIR` e as
    /// pastas padrão do instalador.
    pub quartus_dir: Option<String>,
    /// As trilhas do `aurora learn` de outra pasta, como o `AURORA_LEARN_DIR` da
    /// CLI: é como quem escreve exercícios os testa no Studio. `None`: as do
    /// componente `aurora-learn`.
    pub learn_dir: Option<String>,
    /// O programa `aurora` que o Studio abre como servidor (`aurora serve`).
    /// `None`: o da instalação do bundle. Para desenvolvimento, um build em
    /// `target/`; a variável `AURORA_IDE_CLI` vale antes desta preferência.
    /// `lace_path` no `settings.json` do Lace Studio.
    #[serde(alias = "lace_path")]
    pub aurora_path: Option<String>,
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
    /// Procurar AURORA mais nova ao abrir o Studio e, com ele aberto, a cada
    /// 12 horas (`aurora update --check`), e oferecer a atualização.
    pub check_updates: bool,
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
    /// O tour pela interface já apareceu (`src/state/tour.ts`). Sem
    /// `settings.json` (o Studio recém-instalado), começa com `false`, e o
    /// tour abre sozinho; um arquivo de uma versão anterior, sem o campo, é de
    /// quem já usa o Studio, e vale `true`. O tour volta por Ajuda > Tour
    /// pela interface.
    #[serde(default = "already_toured")]
    pub tour_done: bool,
    /// O executável do Claude Code para o assistente (ou a pasta dele);
    /// `None` procura no `PATH` e nas pastas dos instaladores.
    pub assistant_claude: Option<String>,
    /// O `aurora` que serve as ferramentas ao assistente; `None` é o da
    /// instalação do bundle. Para desenvolvimento, um build em `target/`.
    /// `assistant_lace` no `settings.json` do Lace Studio.
    #[serde(alias = "assistant_lace")]
    pub assistant_aurora: Option<String>,
    /// O modelo do assistente (`--model` do Claude Code); `None` é o padrão.
    pub assistant_model: Option<String>,
    /// O assistente pode editar e criar arquivos na pasta do projeto, além
    /// de usar as ferramentas da AURORA e ler arquivos.
    pub assistant_allow_edits: bool,
    /// Projetos abertos recentemente, do mais novo para o mais antigo.
    pub recent_projects: Vec<RecentProject>,
}

/// O `tour_done` de um `settings.json` gravado antes de o campo existir.
fn already_toured() -> bool {
    true
}

impl Settings {
    /// Traz valores gravados por versões anteriores para os de agora.
    fn migrate(&mut self) {
        // O tema era um id só (o claro de cada paleta tinha o seu) ou
        // `system`; agora é a paleta e o modo. Os temas que saíram vão para o
        // Boreal: o `dark` e o `light`, os únicos até 2026-10-04; o Atlas e o
        // Atlas Branco, os padrões do Lace Studio; e os de fora que não são o
        // Dark e o Light Modern nem o Dracula, que saíram em 2026-10-30.
        let (palette, mode) = match self.theme.as_str() {
            "system" => ("boreal", "system"),
            "dark" | "atlas" | "gruvbox-dark" | "monokai" | "nord" | "one-dark"
            | "solarized-dark" | "catppuccin-mocha" | "tokyo-night" => ("boreal", "dark"),
            "light" | "atlas-light" | "gruvbox-light" | "solarized-light" => ("boreal", "light"),
            "light-modern" => ("dark-modern", "light"),
            id => match id
                .strip_suffix("-claro")
                .or_else(|| id.strip_suffix("-clara"))
            {
                Some(dark) => (dark, "light"),
                None => (id, "dark"),
            },
        };
        self.theme = palette.to_owned();
        if self.theme_mode.is_empty() {
            self.theme_mode = mode.to_owned();
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "system".into(),
            theme: "boreal".into(),
            theme_mode: "dark".into(),
            look: "classic".into(),
            toolchain_dir: None,
            aurora_path: None,
            compiler_dir: None,
            quartus_dir: None,
            learn_dir: None,
            simulator: "icarus".into(),
            open_wave_after_sim: true,
            wave_viewer: "tab".into(),
            sim_timeout_s: None,
            verbose: false,
            restore_last_project: true,
            check_updates: true,
            terminal_shell: "powershell".into(),
            editor: EditorSettings::default(),
            zen: ZenSettings::default(),
            layouts: LayoutSettings::default(),
            tour_done: false,
            assistant_claude: None,
            assistant_aurora: None,
            assistant_model: None,
            assistant_allow_edits: false,
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

/// A pasta de configuração do Lace Studio, o nome da AURORA IDE até a
/// renomeação (o `identifier` do Tauri de antes), vizinha da de agora.
const LACE_STUDIO_DIR: &str = "com.nipscern.lace-studio";

/// Copia o `settings.json` do Lace Studio para `dir` quando `dir` ainda não
/// tem um: quem vem do Lace Studio fica com as preferências, os layouts e os
/// recentes. A pasta antiga fica como estava.
fn adopt_lace_studio_settings(dir: &Utf8Path) {
    let new = dir.join("settings.json");
    if new.exists() {
        return;
    }
    let Some(old) = dir
        .parent()
        .map(|parent| parent.join(LACE_STUDIO_DIR).join("settings.json"))
    else {
        return;
    };
    if old.is_file()
        && std::fs::create_dir_all(dir).is_ok()
        && let Err(error) = std::fs::copy(&old, &new)
    {
        tracing::warn!("Could not copy {old} to {new}: {error}");
    }
}

/// As preferências na memória e o arquivo onde ficam.
pub struct SettingsStore {
    path: Utf8PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// Lê `<dir>/settings.json`, ou começa com os padrões. Sem ele, traz
    /// antes o do Lace Studio ([`adopt_lace_studio_settings`]).
    pub fn load(dir: &Utf8Path) -> Self {
        adopt_lace_studio_settings(dir);
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

    /// O tour só abre sozinho num Studio sem `settings.json`: um arquivo de
    /// uma versão anterior, sem o campo, é de quem já usa o Studio.
    #[test]
    fn tour_opens_only_on_a_fresh_install() {
        assert!(!Settings::default().tour_done);
        let older: Settings = serde_json::from_str(r#"{ "theme": "dracula" }"#).unwrap();
        assert!(older.tour_done);
        let pending: Settings = serde_json::from_str(r#"{ "tour_done": false }"#).unwrap();
        assert!(!pending.tour_done);
    }

    /// Um campo que esta versão não conhece (que saiu, ou de uma versão mais
    /// nova) não leva o arquivo para o `.bad`: é ignorado e some na próxima
    /// gravação.
    #[test]
    fn unknown_fields_are_ignored() {
        let settings: Settings =
            serde_json::from_str(r#"{ "theme": "dracula", "retired": "native" }"#).unwrap();
        assert_eq!(settings.theme, "dracula");
        let saved = serde_json::to_value(&settings).unwrap();
        assert!(saved.get("retired").is_none());
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
                theme: "lunar".into(),
                ..settings
            })
            .unwrap();
        let reloaded = SettingsStore::load(dir).get();
        assert_eq!(reloaded.theme, "lunar");
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
        assert_eq!(store.get().theme, "boreal");
        assert!(dir.join("settings.json.bad").is_file());
    }

    /// O tema de antes do modo vira paleta e modo: o claro de uma paleta dá
    /// o modo claro, o `system` dá o "Do sistema", e os temas que saíram (o
    /// `dark` e o `light`, o Atlas e os de fora que não ficaram) vão para o
    /// Boreal no esquema que tinham.
    #[test]
    fn the_old_theme_becomes_palette_and_mode() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(dir.path()).unwrap();
        for (old, palette, mode) in [
            ("boreal", "boreal", "dark"),
            ("boreal-claro", "boreal", "light"),
            ("alvorada-clara", "alvorada", "light"),
            ("nebulosa", "nebulosa", "dark"),
            ("system", "boreal", "system"),
            ("dark-modern", "dark-modern", "dark"),
            ("light-modern", "dark-modern", "light"),
            ("dracula", "dracula", "dark"),
            ("aurora-legacy", "aurora-legacy", "dark"),
            ("dark", "boreal", "dark"),
            ("light", "boreal", "light"),
            ("atlas", "boreal", "dark"),
            ("atlas-light", "boreal", "light"),
            ("monokai", "boreal", "dark"),
            ("tokyo-night", "boreal", "dark"),
            ("solarized-light", "boreal", "light"),
        ] {
            std::fs::write(
                dir.join("settings.json"),
                format!(r#"{{ "theme": "{old}" }}"#),
            )
            .unwrap();
            let settings = SettingsStore::load(dir).get();
            assert_eq!(
                (settings.theme.as_str(), settings.theme_mode.as_str()),
                (palette, mode),
                "{old}"
            );
        }
    }

    /// Com o modo gravado, a paleta e o modo ficam como estão.
    #[test]
    fn palette_and_mode_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            dir.join("settings.json"),
            r#"{ "theme": "austral", "theme_mode": "system" }"#,
        )
        .unwrap();
        let settings = SettingsStore::load(dir).get();
        assert_eq!(settings.theme, "austral");
        assert_eq!(settings.theme_mode, "system");
        assert_eq!(Settings::default().theme_mode, "dark");
    }

    /// Um `settings.json` de antes da aparência abre na clássica; a escolhida
    /// fica.
    #[test]
    fn the_look_is_classic_unless_chosen() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(dir.join("settings.json"), r#"{ "theme": "boreal" }"#).unwrap();
        assert_eq!(SettingsStore::load(dir).get().look, "classic");
        std::fs::write(dir.join("settings.json"), r#"{ "look": "islands" }"#).unwrap();
        assert_eq!(SettingsStore::load(dir).get().look, "islands");
    }

    /// Sem preferências, a IDE traz as do Lace Studio da pasta vizinha; com
    /// elas, a antiga não conta.
    #[test]
    fn the_lace_studio_settings_come_along_once() {
        let root = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(root.path()).unwrap();
        let old = root.join(LACE_STUDIO_DIR);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(
            old.join("settings.json"),
            r#"{ "language": "en", "theme": "atlas", "lace_path": "C:/x/lace.exe" }"#,
        )
        .unwrap();
        let dir = root.join("com.nipscern.aurora-ide");

        let settings = SettingsStore::load(&dir).get();
        assert_eq!(settings.language, "en");
        assert_eq!(settings.theme, "boreal");
        assert_eq!(settings.aurora_path.as_deref(), Some("C:/x/lace.exe"));
        assert!(dir.join("settings.json").is_file());
        assert!(old.join("settings.json").is_file());

        std::fs::write(old.join("settings.json"), r#"{ "language": "pt" }"#).unwrap();
        assert_eq!(SettingsStore::load(&dir).get().language, "en");
    }

    /// Os campos com o nome do Lace Studio valem com o nome novo.
    #[test]
    fn aurora_ide_fields_are_read_under_the_new_names() {
        let settings: Settings = serde_json::from_str(
            r#"{ "lace_path": "C:/aurora/target/debug/aurora.exe", "assistant_lace": "/opt/aurora/bin/aurora" }"#,
        )
        .unwrap();
        assert_eq!(
            settings.aurora_path.as_deref(),
            Some("C:/aurora/target/debug/aurora.exe")
        );
        assert_eq!(
            settings.assistant_aurora.as_deref(),
            Some("/opt/aurora/bin/aurora")
        );
        let saved = serde_json::to_value(&settings).unwrap();
        assert!(saved.get("lace_path").is_none());
        assert_eq!(saved["aurora_path"], "C:/aurora/target/debug/aurora.exe");
    }

    /// O Atlas escolhido numa AURORA IDE de desenvolvimento, que gravava o
    /// `aurora_themes`, também vai para o Boreal: o campo antigo não conta.
    #[test]
    fn atlas_chosen_in_a_development_build_also_becomes_boreal() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(
            dir.join("settings.json"),
            r#"{ "theme": "atlas", "aurora_themes": true }"#,
        )
        .unwrap();
        let settings = SettingsStore::load(dir).get();
        assert_eq!(settings.theme, "boreal");
        assert_eq!(settings.theme_mode, "dark");
        assert_eq!(Settings::default().theme, "boreal");
    }
}
