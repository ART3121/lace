//! O RTL viewer fora do Studio: um HTML autocontido com o esquemático
//! interativo do netlist que a síntese grava ([`SynthesisResult::netlist`]).
//!
//! O viewer é o pacote `packages/rtl-viewer/` do repositório, o mesmo que o
//! Studio mostra numa aba e a extensão do VS Code num webview. O build dele
//! gera um modelo, `rtl-viewer.html`, com o código e o CSS embutidos e dois
//! lugares para preencher; o componente `rtl-viewer` do bundle o traz. Este
//! módulo preenche o modelo com o netlist e grava o resultado ao lado dele,
//! num arquivo que abre em qualquer navegador, sem servidor e sem rede.
//!
//! [`SynthesisResult::netlist`]: crate::SynthesisResult::netlist

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;

use crate::error::{LaceError, Result};
use crate::toolchain::{Toolchain, component};

/// A variável que aponta outro modelo no lugar do do bundle: numa build de
/// desenvolvimento, `packages/rtl-viewer/dist/rtl-viewer.html`.
pub const TEMPLATE_ENV: &str = "LACE_RTL_VIEWER";

/// O modelo, na pasta do componente `rtl-viewer`.
pub const TEMPLATE_FILE: &str = "rtl-viewer.html";

/// O HTML gravado, ao lado do netlist.
pub const OUTPUT_FILE: &str = "rtl-viewer.html";

/// Onde o modelo recebe o netlist (o `hierarchy.json` inteiro).
const NETLIST_SLOT: &str = "@@LACE_NETLIST@@";
/// Onde o modelo recebe [`ViewerConfig`].
const CONFIG_SLOT: &str = "@@LACE_CONFIG@@";

/// O que o HTML sabe do netlist além do conteúdo.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewerConfig<'a> {
    /// O caminho do netlist, que o viewer mostra e usa para guardar o lugar.
    path: &'a str,
    /// A data de modificação, em milissegundos desde 1970.
    modified_ms: u64,
    /// O título da aba do navegador.
    title: &'a str,
}

/// O modelo do HTML: o de [`TEMPLATE_ENV`], se definida, ou o do componente
/// `rtl-viewer` do bundle.
///
/// # Erros
///
/// [`LaceError::ComponentMissing`] se o componente não foi instalado;
/// [`LaceError::ToolchainIncomplete`] se o arquivo não está onde devia.
pub fn template(toolchain: &Toolchain) -> Result<Utf8PathBuf> {
    let path = match std::env::var(TEMPLATE_ENV) {
        Ok(path) if !path.is_empty() => Utf8PathBuf::from(path),
        _ => {
            let installed = toolchain
                .component(component::RTL_VIEWER)
                .ok_or_else(|| LaceError::ComponentMissing(component::RTL_VIEWER.to_owned()))?;
            toolchain.root().join(&installed.dir).join(TEMPLATE_FILE)
        }
    };
    if !path.is_file() {
        return Err(LaceError::ToolchainIncomplete {
            what: "RTL viewer template".to_owned(),
            path,
        });
    }
    Ok(path)
}

/// Preenche o modelo com o netlist e grava `rtl-viewer.html` na pasta dele.
/// Devolve o caminho do HTML. Um HTML de antes é sobrescrito: é um artefato,
/// refeito a cada síntese.
///
/// # Erros
///
/// [`LaceError::Io`] se o modelo ou o netlist não puderam ser lidos ou o HTML
/// não pôde ser gravado; [`LaceError::ToolchainIncomplete`] se o modelo não
/// tem os lugares do netlist e da configuração (um modelo de outra versão,
/// ou estragado).
pub fn write(template: &Utf8Path, netlist: &Utf8Path, title: &str) -> Result<Utf8PathBuf> {
    let model = std::fs::read_to_string(template).map_err(LaceError::io("Reading", template))?;
    if !model.contains(NETLIST_SLOT) || !model.contains(CONFIG_SLOT) {
        return Err(LaceError::ToolchainIncomplete {
            what: format!("{NETLIST_SLOT} and {CONFIG_SLOT} in the RTL viewer template"),
            path: template.to_owned(),
        });
    }
    let content = std::fs::read_to_string(netlist).map_err(LaceError::io("Reading", netlist))?;
    let modified_ms = std::fs::metadata(netlist)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    let config = ViewerConfig {
        path: netlist.as_str(),
        modified_ms,
        title,
    };
    let config = serde_json::to_string(&config).expect("a configuração sempre vira JSON");
    // Os dois vão dentro de <script type="application/json">: um "</script"
    // no meio fecharia o elemento. Em JSON, "<" só aparece dentro de strings,
    // onde < é o mesmo caractere.
    let html = model
        .replacen(NETLIST_SLOT, &script_safe(&content), 1)
        .replacen(CONFIG_SLOT, &script_safe(&config), 1);
    let out = netlist.parent().map_or_else(
        || Utf8PathBuf::from(OUTPUT_FILE),
        |dir| dir.join(OUTPUT_FILE),
    );
    std::fs::write(&out, html).map_err(LaceError::io("Writing", &out))?;
    Ok(out)
}

fn script_safe(json: &str) -> String {
    json.replace('<', "\\u003c")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &str = "<html><script id=\"lace-netlist\" type=\"application/json\">@@LACE_NETLIST@@</script>\
<script id=\"lace-config\" type=\"application/json\">@@LACE_CONFIG@@</script></html>";

    fn slot<'a>(html: &'a str, id: &str) -> &'a str {
        let start = html
            .find(&format!("id=\"{id}\" type=\"application/json\">"))
            .expect("o script");
        let rest = &html[start..];
        let open = rest.find('>').expect("o fim da tag") + 1;
        let close = rest.find("</script>").expect("o fechamento");
        &rest[open..close]
    }

    #[test]
    fn fills_the_template_beside_the_netlist() {
        let dir = tempfile::tempdir().expect("pasta temporária");
        let dir = Utf8Path::from_path(dir.path())
            .expect("caminho UTF-8")
            .to_owned();
        let template = dir.join("modelo.html");
        std::fs::write(&template, MODEL).expect("o modelo");
        let netlist = dir.join("hierarchy.json");
        // Um "</script>" dentro de uma string do netlist não pode fechar o elemento.
        let text = r#"{"modules":{"top":{"attributes":{"src":"a.v:1.1-2.3","x":"</script><b>"}}}}"#;
        std::fs::write(&netlist, text).expect("o netlist");

        let out = write(&template, &netlist, "top (RTL viewer)").expect("o HTML");
        assert_eq!(out, dir.join(OUTPUT_FILE));
        let html = std::fs::read_to_string(&out).expect("lê o HTML");
        assert!(
            !html.contains("@@LACE_"),
            "sobrou um lugar sem preencher: {html}"
        );
        assert_eq!(
            html.matches("</script>").count(),
            2,
            "o netlist fechou o script: {html}"
        );

        let parsed: serde_json::Value =
            serde_json::from_str(slot(&html, "lace-netlist")).expect("o netlist é JSON");
        assert_eq!(parsed["modules"]["top"]["attributes"]["x"], "</script><b>");
        let config: serde_json::Value =
            serde_json::from_str(slot(&html, "lace-config")).expect("a configuração é JSON");
        assert_eq!(config["path"], netlist.as_str());
        assert_eq!(config["title"], "top (RTL viewer)");
        assert!(config["modifiedMs"].as_u64().is_some_and(|ms| ms > 0));
    }

    #[test]
    fn refuses_a_template_without_the_slots() {
        let dir = tempfile::tempdir().expect("pasta temporária");
        let dir = Utf8Path::from_path(dir.path())
            .expect("caminho UTF-8")
            .to_owned();
        let template = dir.join("modelo.html");
        std::fs::write(&template, "<html></html>").expect("o modelo");
        let netlist = dir.join("hierarchy.json");
        std::fs::write(&netlist, "{}").expect("o netlist");
        let error = write(&template, &netlist, "x").expect_err("um modelo sem os lugares");
        assert_eq!(error.code(), "toolchain_incomplete");
    }
}
