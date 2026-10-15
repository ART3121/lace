//! Instalador do Lace para Linux e macOS, e o empacotador dos instaladores.
//!
//! O instalador é um diretório com o executável `install` e o `payload/`:
//!
//! ```text
//! lace-<versão>-<plataforma>/
//!   install                    este programa (TUI, ou --yes para scripts)
//!   LEIA-ME.txt
//!   payload/
//!     index.json               versão, componentes, pedaços
//!     files.json               o SHA-256 de cada arquivo, por pedaço
//!     lace.tar.zst             bin/lace, o cabeçalho do bundle e o files.json: sempre instalados
//!     c01.tar.zst ...          um pedaço por conjunto de componentes que divide arquivos
//! ```
//!
//! Os componentes do OSS CAD Suite dividem bibliotecas: cada arquivo vai para
//! o pedaço do conjunto exato de componentes que o usa, e o instalador extrai
//! os pedaços de que a seleção precisa. O mesmo agrupamento vira as entradas
//! `[Files]` do instalador de Windows (Inno Setup), gerado por
//! [`pack::inno`].
//!
//! Módulos:
//! - [`payload`]: o formato do `index.json`;
//! - [`files`]: o manifesto de arquivos (`files.json`), que cada instalação
//!   guarda e o `lace update` compara;
//! - [`plan`]: perfis (os do índice e o Custom) e seleção com dependências;
//! - [`install`]: extrair, conferir, trocar a instalação antiga, atalho,
//!   desinstalador;
//! - [`add`]: acrescentar aplicativos a uma instalação (`lace install`);
//! - [`update`]: atualizar trocando só os arquivos que mudaram (`lace update`);
//! - [`desktop`]: o atalho do Lace Studio no menu de aplicativos;
//! - [`pack`]: montar o payload e o estágio do Inno Setup a partir do bundle;
//! - [`tui`]: a interface de terminal;
//! - [`brand`]: as cores e a marca do Lace na interface de terminal.

#![forbid(unsafe_code)]

pub mod add;
pub mod brand;
pub mod desktop;
pub mod files;
pub mod install;
pub mod pack;
pub mod payload;
pub mod plan;
pub mod tui;
pub mod update;

/// Versão do Lace que este instalador instala (a do workspace).
pub const LACE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A primeira letra em minúscula, para a frase vir depois de dois-pontos ou
/// de "For": "Every app" vira "every app". Uma sigla no começo ("YANC
/// only") fica como está.
pub(crate) fn lowercase_first(text: &str) -> String {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(second)) if second.is_lowercase() => {
            first.to_lowercase().chain([second]).chain(chars).collect()
        }
        _ => text.to_owned(),
    }
}

/// Quebra `text` em linhas de até `width` caracteres, nos espaços.
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Tamanho legível, em MiB.
pub fn mib(bytes: u64) -> String {
    let m = bytes as f64 / (1024.0 * 1024.0);
    if m < 10.0 {
        format!("{m:.1} MiB")
    } else {
        format!("{m:.0} MiB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_helpers() {
        assert_eq!(lowercase_first("Every app"), "every app");
        assert_eq!(lowercase_first("YANC only"), "YANC only");
        assert_eq!(lowercase_first(""), "");
        assert_eq!(
            wrap("um dois três quatro", 9),
            ["um dois", "três", "quatro"]
        );
        assert_eq!(wrap("palavragrande", 4), ["palavragrande"]);
    }
}
