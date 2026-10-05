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
//!     lace.tar.zst             bin/lace e o cabeçalho do bundle: sempre instalados
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
//! - [`plan`]: perfis (recomendado, avançado) e seleção com dependências;
//! - [`install`]: extrair, conferir, trocar a instalação antiga, atalho,
//!   desinstalador;
//! - [`desktop`]: o atalho do Lace Studio no menu de aplicativos;
//! - [`pack`]: montar o payload e o estágio do Inno Setup a partir do bundle;
//! - [`tui`]: a interface de terminal.

#![forbid(unsafe_code)]

pub mod add;
pub mod desktop;
pub mod install;
pub mod pack;
pub mod payload;
pub mod plan;
pub mod tui;

/// Versão do Lace que este instalador instala (a do workspace).
pub const LACE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Tamanho legível, em MiB.
pub fn mib(bytes: u64) -> String {
    let m = bytes as f64 / (1024.0 * 1024.0);
    if m < 10.0 {
        format!("{m:.1} MiB")
    } else {
        format!("{m:.0} MiB")
    }
}
