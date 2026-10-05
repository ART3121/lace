//! O atalho do Lace Studio no menu de aplicativos, quando o componente
//! `studio` está instalado:
//!
//! - no Linux, `lace-studio.desktop` na pasta de aplicativos do usuário
//!   (`$XDG_DATA_HOME/applications`, ou `~/.local/share/applications`; como
//!   root, `/usr/local/share/applications`);
//! - no macOS, o symlink `Lace Studio.app` em `~/Applications` (como root,
//!   `/Applications`), para o `.app` que fica no bundle;
//! - no Windows, quem cria o atalho no menu Iniciar é o assistente (Inno
//!   Setup); aqui não há nada a fazer.
//!
//! O atalho só é tocado quando é desta instalação: o de outra instalação, ou
//! um aplicativo de verdade com o mesmo nome, fica como está. O
//! `uninstall.sh` tira o atalho pela mesma regra ([`uninstall_lines`]).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::install::{home, is_root};

/// O arquivo do atalho no Linux.
pub const DESKTOP_FILE: &str = "lace-studio.desktop";
/// O `.app` do Studio no macOS (o `productName` do `tauri.conf.json`).
pub const APP_NAME: &str = "Lace Studio.app";
/// A linha do `.desktop` que diz de que instalação ele é.
const PREFIX_KEY: &str = "X-Lace-Prefix=";

/// Onde fica o atalho nesta plataforma; `None` no Windows.
pub fn shortcut_path() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let dir = if is_root() {
            PathBuf::from("/Applications")
        } else {
            home().join("Applications")
        };
        Some(dir.join(APP_NAME))
    } else if cfg!(unix) {
        let dir = if is_root() {
            PathBuf::from("/usr/local/share/applications")
        } else {
            data_home().join("applications")
        };
        Some(dir.join(DESKTOP_FILE))
    } else {
        None
    }
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".local/share"))
}

/// O que o atalho abre na instalação em `prefix`.
fn studio_in(prefix: &Path) -> PathBuf {
    let studio = prefix.join("toolchain").join(lace_core::component::STUDIO);
    if cfg!(target_os = "macos") {
        studio.join(APP_NAME)
    } else {
        studio.join("lace-studio")
    }
}

/// O atalho em `shortcut` é desta instalação?
fn is_ours(shortcut: &Path, prefix: &Path) -> bool {
    if cfg!(target_os = "macos") {
        fs::read_link(shortcut).is_ok_and(|t| t == studio_in(prefix))
    } else {
        let mark = format!("{PREFIX_KEY}{}", prefix.display());
        fs::read_to_string(shortcut).is_ok_and(|text| text.lines().any(|l| l == mark))
    }
}

/// Põe o atalho em dia com a instalação em `prefix`: com o Studio instalado,
/// cria (ou refaz) o atalho; sem ele, tira o atalho desta instalação. Devolve
/// o atalho que ficou, se ficou. Um arquivo de outra instalação no lugar não
/// é mexido, e o Studio fica sem atalho.
pub fn sync(prefix: &Path) -> anyhow::Result<Option<PathBuf>> {
    let Some(shortcut) = shortcut_path() else {
        return Ok(None);
    };
    let studio = studio_in(prefix);
    let installed = fs::symlink_metadata(&studio).is_ok();
    let exists = fs::symlink_metadata(&shortcut).is_ok();
    if !installed {
        if exists && is_ours(&shortcut, prefix) {
            fs::remove_file(&shortcut)
                .with_context(|| format!("Removing {}", shortcut.display()))?;
        }
        return Ok(None);
    }
    if exists && !is_ours(&shortcut, prefix) {
        return Ok(None);
    }
    if let Some(dir) = shortcut.parent() {
        fs::create_dir_all(dir).with_context(|| format!("Creating {}", dir.display()))?;
    }
    if exists {
        fs::remove_file(&shortcut).with_context(|| format!("Replacing {}", shortcut.display()))?;
    }
    write_shortcut(&shortcut, &studio, prefix)?;
    Ok(Some(shortcut))
}

#[cfg(target_os = "macos")]
fn write_shortcut(shortcut: &Path, studio: &Path, _prefix: &Path) -> anyhow::Result<()> {
    std::os::unix::fs::symlink(studio, shortcut)
        .with_context(|| format!("Creating the link {}", shortcut.display()))
}

#[cfg(not(target_os = "macos"))]
fn write_shortcut(shortcut: &Path, studio: &Path, prefix: &Path) -> anyhow::Result<()> {
    let icon = studio.with_file_name("lace-studio.png");
    let text = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Lace Studio\n\
         GenericName=Hardware development environment\n\
         Comment=Verilog projects and SAPHO processors, with the Lace tools\n\
         Exec={exec}\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Development;IDE;Electronics;\n\
         {PREFIX_KEY}{prefix}\n",
        exec = desktop_exec(studio),
        icon = icon.display(),
        prefix = prefix.display(),
    );
    fs::write(shortcut, text).with_context(|| format!("Writing {}", shortcut.display()))
}

/// O programa no `Exec=` de um `.desktop`, entre aspas e com os caracteres
/// que a especificação reserva escapados (`"`, `` ` ``, `$` e `\`).
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn desktop_exec(program: &Path) -> String {
    let mut out = String::from("\"");
    for c in program.to_string_lossy().chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            // A barra do escape é ela mesma escapada pela regra geral das
            // strings do `.desktop`.
            out.push_str("\\\\");
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// O aviso de que falta ao sistema o WebView que o Studio usa, ou `None`.
/// Só o Linux tem o que conferir: o Studio liga ao webkit2gtk 4.1 do
/// sistema, que nem toda instalação traz. No macOS o WebView é do sistema;
/// no Windows, o WebView2 vem com o Windows 10 e o 11.
pub fn missing_runtime() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    const LIB: &str = "libwebkit2gtk-4.1.so.0";
    let listed = std::process::Command::new("ldconfig")
        .arg("-p")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(LIB));
    let found = listed.unwrap_or_else(|| {
        [
            "/usr/lib",
            "/usr/lib64",
            "/usr/lib/x86_64-linux-gnu",
            "/lib/x86_64-linux-gnu",
        ]
        .iter()
        .any(|d| Path::new(d).join(LIB).exists())
    });
    (!found).then(|| {
        "Lace Studio needs webkit2gtk 4.1, which this system does not have: \
         sudo apt install libwebkit2gtk-4.1-0 (Debian, Ubuntu), \
         sudo dnf install webkit2gtk4.1 (Fedora)"
            .to_owned()
    })
}

/// As linhas do `uninstall.sh` que tiram o atalho desta instalação (vazio
/// no Windows). Usam a variável `$prefix` do script e valem mesmo se o
/// Studio entrou depois, pelo `lace install`.
pub fn uninstall_lines(quote: impl Fn(&Path) -> String) -> String {
    let Some(shortcut) = shortcut_path() else {
        return String::new();
    };
    if cfg!(target_os = "macos") {
        format!(
            "app={}\nif [ -L \"$app\" ] && [ \"$(readlink \"$app\")\" = \"$prefix/toolchain/{}/{APP_NAME}\" ]; then\n  rm -f \"$app\"\nfi\n",
            quote(&shortcut),
            lace_core::component::STUDIO,
        )
    } else {
        format!(
            "desktop={}\nif [ -f \"$desktop\" ] && grep -qxF \"{PREFIX_KEY}$prefix\" \"$desktop\"; then\n  rm -f \"$desktop\"\nfi\n",
            quote(&shortcut),
        )
    }
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn exec_quotes_and_escapes_the_program() {
        assert_eq!(
            desktop_exec(Path::new("/opt/my lace/toolchain/studio/lace-studio")),
            "\"/opt/my lace/toolchain/studio/lace-studio\""
        );
        assert_eq!(desktop_exec(Path::new("/a$b")), "\"/a\\\\$b\"");
    }
}
