//! De onde vem o bundle do Lace.
//!
//! A busca é a de `lace_core::locate`, na ordem descrita lá: a pasta das
//! preferências, a variável `LACE_TOOLCHAIN`, o bundle em que o Studio está
//! instalado, as pastas padrão do instalador, o `lace` do `PATH` e, por fim,
//! ao lado do executável. O Studio só monta o [`Lookup`] com o que lê do
//! ambiente e das preferências; a regra mora no core, e é a mesma da CLI.
//!
//! O compilador do Verilator e o Quartus seguem a regra da CLI: o declarado
//! (`compiler_dir`/`LACE_COMPILER`, `quartus_dir`/`LACE_QUARTUS`) vale; um
//! declarado que não serve não derruba o bundle: o Studio abre sem ele, a
//! operação que o exige falha com o erro dela, e a tela de ferramentas
//! mostra o motivo (`compiler_error`, `quartus_error`).

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::Toolchain;
use lace_core::locate::{self, Lookup};

pub use lace_core::locate::{Found as Located, Origin, ToolStatus, ToolchainInfo};

use crate::error::IpcResult;
use crate::settings::Settings;

/// Uma variável de ambiente como caminho, ou `None` quando ausente ou vazia.
fn env_path(name: &str) -> Option<Utf8PathBuf> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .map(Utf8PathBuf::from)
}

/// Uma preferência como caminho, ou `None` quando ausente ou vazia.
fn setting_path(value: &Option<String>) -> Option<Utf8PathBuf> {
    value
        .as_deref()
        .filter(|v| !v.is_empty())
        .map(Utf8PathBuf::from)
}

/// Onde procurar, com o que o Studio sabe: as preferências e o ambiente.
fn lookup(settings: &Settings) -> Lookup {
    Lookup {
        settings: setting_path(&settings.toolchain_dir),
        environment: env_path("LACE_TOOLCHAIN"),
        exe: std::env::current_exe()
            .ok()
            .and_then(|p| Utf8PathBuf::from_path_buf(p).ok()),
        installation_dirs: Lookup::installation_dirs(env_path),
        path_dirs: std::env::var_os("PATH")
            .map(|path| {
                std::env::split_paths(&path)
                    .filter_map(|dir| Utf8PathBuf::from_path_buf(dir).ok())
                    .collect()
            })
            .unwrap_or_default(),
        compiler: setting_path(&settings.compiler_dir).or_else(|| env_path("LACE_COMPILER")),
        quartus: setting_path(&settings.quartus_dir).or_else(|| env_path("LACE_QUARTUS")),
        quartus_rootdir: env_path("QUARTUS_ROOTDIR"),
        home: env_path("HOME"),
    }
}

/// Abre o bundle pela ordem do core.
pub fn resolve(settings: &Settings) -> IpcResult<Located> {
    Ok(locate::resolve(&lookup(settings))?)
}

/// O `lace` da instalação do bundle: `<instalação>/bin/lace`. É ele que o
/// Studio chama para o que só a CLI faz (instalar componentes, procurar
/// atualização).
pub fn lace_cli(toolchain: &Toolchain) -> Option<Utf8PathBuf> {
    locate::lace_cli(toolchain)
}

/// O que a tela de ferramentas mostra: o equivalente do `lace tools`. Nunca
/// falha: um bundle ausente vira `found: false` com o erro.
pub fn info(settings: &Settings) -> ToolchainInfo {
    match locate::resolve(&lookup(settings)) {
        Ok(found) => locate::describe(&found),
        Err(error) => locate::describe_error(&error),
    }
}

/// O bundle, para as operações: o erro de não achar vira erro do comando.
pub fn require(settings: &Settings) -> IpcResult<Toolchain> {
    Ok(resolve(settings)?.toolchain)
}

/// A pasta `bin/` da instalação, para pôr no `PATH` do terminal de shell e
/// o usuário poder digitar `lace` nele.
pub fn install_bin_dir(settings: &Settings) -> Option<Utf8PathBuf> {
    let located = resolve(settings).ok()?;
    lace_cli(&located.toolchain).and_then(|cli| cli.parent().map(Utf8Path::to_owned))
}
