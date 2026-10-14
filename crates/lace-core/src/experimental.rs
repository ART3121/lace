//! Os recursos experimentais: partes do Lace que ainda mudam muito e que
//! ficam desligadas até o usuário ligar. Hoje há um, a placa FPGA
//! ([`Feature::Fpga`]): as ligações do `fpga.json`, a compilação pelo Quartus
//! Prime e a gravação na placa ([`crate::fpga`]).
//!
//! O que está ligado fica em `experimental.json`, na pasta de configuração
//! do Lace do usuário: `%APPDATA%\lace` no Windows, `$XDG_CONFIG_HOME/lace`
//! (ou `~/.config/lace`) no Linux e `~/Library/Application Support/lace` no
//! macOS. A CLI liga e desliga com `lace experimental enable|disable
//! <recurso>`, e o Studio, em Preferências > Experimental; os dois leem o
//! mesmo arquivo.
//!
//! ```json
//! { "enabled": ["fpga"] }
//! ```
//!
//! O Core não confere sozinho: [`crate::fpga`] funciona como sempre, para
//! quem o usa como biblioteca e para os testes. Quem oferece o recurso ao
//! usuário (a CLI, o Studio) chama [`require`] antes e, com ele desligado,
//! recebe [`LaceError::ExperimentalDisabled`].
//!
//! A leitura falha fechada: sem a pasta de configuração, sem o arquivo ou
//! com ele ilegível, nada está ligado. Ligar ou desligar um recurso mantém
//! no arquivo os que esta versão não conhece (de uma versão mais nova).

use camino::{Utf8Path, Utf8PathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{LaceError, Result};

/// O nome do arquivo, na pasta de configuração do Lace.
pub const SETTINGS_FILE: &str = "experimental.json";

/// A lista do arquivo com os recursos ligados.
const ENABLED: &str = "enabled";

/// Um recurso experimental. Em JSON e na CLI, pelo identificador (`fpga`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Feature {
    /// A placa FPGA: as ligações do `fpga.json`, a compilação pelo Quartus
    /// Prime e a gravação na placa (`lace fpga`, a aba Placa FPGA do
    /// Studio).
    Fpga,
}

impl Feature {
    /// Todos, na ordem da lista.
    pub const ALL: &'static [Feature] = &[Feature::Fpga];

    /// O identificador, o do arquivo e da CLI (`fpga`).
    pub fn id(self) -> &'static str {
        match self {
            Feature::Fpga => "fpga",
        }
    }

    /// O recurso com esse identificador.
    pub fn from_id(id: &str) -> Option<Feature> {
        Feature::ALL.iter().copied().find(|f| f.id() == id)
    }

    /// O nome curto, em inglês (`FPGA boards`).
    pub fn name(self) -> &'static str {
        match self {
            Feature::Fpga => "FPGA boards",
        }
    }

    /// O que ele faz, em inglês, para a lista da CLI.
    pub fn description(self) -> &'static str {
        match self {
            Feature::Fpga => {
                "FPGA boards: the fpga.json connections, the Quartus Prime build and board programming"
            }
        }
    }
}

impl std::fmt::Display for Feature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

/// Um recurso e se está ligado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct FeatureState {
    /// O recurso.
    pub feature: Feature,
    /// Ligado.
    pub enabled: bool,
    /// O que ele faz, em inglês ([`Feature::description`]).
    pub description: String,
}

/// O arquivo do que está ligado: `experimental.json` na pasta de
/// configuração do Lace. `None` se o sistema não diz onde ela fica (sem
/// `APPDATA` no Windows, sem `HOME` nos outros).
pub fn settings_path() -> Option<Utf8PathBuf> {
    crate::paths::user_config_dir().map(|dir| dir.join(SETTINGS_FILE))
}

/// O recurso está ligado.
pub fn is_enabled(feature: Feature) -> bool {
    settings_path().is_some_and(|path| enabled_at(&path, feature))
}

/// Confere que o recurso está ligado: o que a CLI e o Studio chamam antes de
/// oferecê-lo.
///
/// # Erros
///
/// [`LaceError::ExperimentalDisabled`] se ele está desligado.
pub fn require(feature: Feature) -> Result<()> {
    if is_enabled(feature) {
        Ok(())
    } else {
        Err(LaceError::ExperimentalDisabled { feature })
    }
}

/// Cada recurso e se está ligado, na ordem de [`Feature::ALL`].
pub fn states() -> Vec<FeatureState> {
    let enabled = settings_path().map(|path| read(&path)).unwrap_or_default();
    states_from(&enabled)
}

/// Liga ou desliga um recurso e grava o arquivo, criando a pasta de
/// configuração se preciso.
///
/// # Erros
///
/// [`LaceError::Io`] sem a pasta de configuração (sem `APPDATA` ou `HOME`),
/// ou se ela ou o arquivo não podem ser gravados.
pub fn set_enabled(feature: Feature, enabled: bool) -> Result<()> {
    let path = settings_path().ok_or_else(|| LaceError::Io {
        context: "Finding the Lace settings folder",
        path: SETTINGS_FILE.into(),
        source: std::io::Error::new(
            std::io::ErrorKind::NotFound,
            if cfg!(windows) {
                "APPDATA is not set"
            } else {
                "HOME is not set"
            },
        ),
    })?;
    set_at(&path, feature, enabled)
}

fn states_from(enabled: &[String]) -> Vec<FeatureState> {
    Feature::ALL
        .iter()
        .map(|&feature| FeatureState {
            feature,
            enabled: enabled.iter().any(|id| id == feature.id()),
            description: feature.description().to_owned(),
        })
        .collect()
}

fn enabled_at(path: &Utf8Path, feature: Feature) -> bool {
    read(path).iter().any(|id| id == feature.id())
}

/// Os identificadores ligados no arquivo; nenhum sem ele ou com ele
/// ilegível.
fn read(path: &Utf8Path) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(doc) => ids(&doc),
            Err(error) => {
                tracing::warn!(
                    "{path} is not valid JSON ({error}); no experimental component is enabled"
                );
                Vec::new()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            tracing::warn!("Cannot read {path} ({error}); no experimental component is enabled");
            Vec::new()
        }
    }
}

fn ids(doc: &Value) -> Vec<String> {
    doc.get(ENABLED)
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn set_at(path: &Utf8Path, feature: Feature, enabled: bool) -> Result<()> {
    // O arquivo como está, com os campos e os recursos que esta versão não
    // conhece; ilegível, ele não guardava nada que valesse, e começa de novo.
    let mut doc = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    let mut list = ids(&doc);
    list.retain(|id| id != feature.id());
    if enabled {
        list.push(feature.id().to_owned());
    }
    doc[ENABLED] = serde_json::json!(list);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(LaceError::io("Creating directory", dir))?;
    }
    crate::spf::write_json(path, &doc, "Writing settings")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8Path::from_path(dir.path())
            .unwrap()
            .join("lace")
            .join(SETTINGS_FILE);
        (dir, path)
    }

    #[test]
    fn everything_starts_disabled() {
        let (_dir, path) = file();
        assert!(!enabled_at(&path, Feature::Fpga));
        assert_eq!(
            states_from(&read(&path)),
            [FeatureState {
                feature: Feature::Fpga,
                enabled: false,
                description: Feature::Fpga.description().to_owned(),
            }]
        );
    }

    #[test]
    fn enabling_and_disabling_write_the_file() {
        let (_dir, path) = file();
        set_at(&path, Feature::Fpga, true).unwrap();
        assert!(enabled_at(&path, Feature::Fpga));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, "{\n  \"enabled\": [\n    \"fpga\"\n  ]\n}\n");
        // Ligar de novo não repete.
        set_at(&path, Feature::Fpga, true).unwrap();
        assert_eq!(read(&path), ["fpga"]);
        set_at(&path, Feature::Fpga, false).unwrap();
        assert!(!enabled_at(&path, Feature::Fpga));
        assert!(read(&path).is_empty());
    }

    #[test]
    fn what_this_version_does_not_know_stays_in_the_file() {
        let (_dir, path) = file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"enabled": ["future"], "note": 1}"#).unwrap();
        set_at(&path, Feature::Fpga, true).unwrap();
        assert_eq!(read(&path), ["future", "fpga"]);
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["note"], 1);
    }

    #[test]
    fn an_unreadable_file_enables_nothing_and_is_rewritten() {
        let (_dir, path) = file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for text in ["{ not json", r#"["fpga"]"#, r#"{"enabled": "fpga"}"#] {
            std::fs::write(&path, text).unwrap();
            assert!(!enabled_at(&path, Feature::Fpga), "{text}");
        }
        set_at(&path, Feature::Fpga, true).unwrap();
        assert!(enabled_at(&path, Feature::Fpga));
    }

    #[test]
    fn features_go_by_their_id() {
        assert_eq!(Feature::from_id("fpga"), Some(Feature::Fpga));
        assert_eq!(Feature::from_id("FPGA"), None);
        assert_eq!(serde_json::to_string(&Feature::Fpga).unwrap(), "\"fpga\"");
        let error = LaceError::ExperimentalDisabled {
            feature: Feature::Fpga,
        };
        assert_eq!(error.code(), "experimental_disabled");
        assert_eq!(
            error.to_string(),
            "The experimental component fpga (FPGA boards) is disabled"
        );
    }
}
