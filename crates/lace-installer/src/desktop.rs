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
/// O lançador que o atalho do Linux abre, ao lado do `lace-studio`
/// ([`write_launcher`]).
const LAUNCHER: &str = "lace-studio-launch";

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
        // O lançador do Linux fica ao lado do Studio, e o manifesto não o
        // conhece: sai com ele.
        let _ = fs::remove_file(studio.with_file_name(LAUNCHER));
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
    let launcher = write_launcher(studio)?;
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
        exec = desktop_exec(&launcher),
        icon = icon.display(),
        prefix = prefix.display(),
    );
    fs::write(shortcut, text).with_context(|| format!("Writing {}", shortcut.display()))
}

/// Grava o lançador que o atalho do Linux abre, ao lado do Studio, e devolve
/// o caminho dele. O atalho roda sem terminal: sem uma biblioteca do sistema
/// (em geral o webkit2gtk 4.1, que o KDE e o Arch não trazem), o Studio
/// morria antes de abrir e o erro do carregador ia para lugar nenhum. O
/// lançador confere as bibliotecas com o `ldd` e, faltando alguma, mostra o
/// que falta e o comando que a instala numa janela (`kdialog`, `zenity`,
/// `notify-send` ou `xmessage`, o que houver).
#[cfg(not(target_os = "macos"))]
fn write_launcher(studio: &Path) -> anyhow::Result<PathBuf> {
    let launcher = studio.with_file_name(LAUNCHER);
    let fix = MissingRuntime {
        install: install_command(),
    }
    .fix();
    let script = format!(
        "#!/bin/sh\n\
         # Abre o Lace Studio pelo atalho do menu (gravado pelo instalador do Lace).\n\
         # Sem uma biblioteca do sistema, o Studio não abre, e o erro iria para\n\
         # um terminal que não existe: aqui ele aparece numa janela.\n\
         studio={studio}\n\
         missing=$(ldd \"$studio\" 2>/dev/null | sed -n 's/^[[:space:]]*\\([^ ]*\\) => not found.*/\\1/p' | tr '\\n' ' ')\n\
         if [ -n \"$missing\" ]; then\n\
         \x20   text=\"Lace Studio cannot start: this system does not have $missing\n\
         Lace Studio needs webkit2gtk 4.1. {fix}\"\n\
         \x20   if command -v kdialog >/dev/null 2>&1; then\n\
         \x20       kdialog --title 'Lace Studio' --error \"$text\"\n\
         \x20   elif command -v zenity >/dev/null 2>&1; then\n\
         \x20       zenity --error --no-markup --title='Lace Studio' --text=\"$text\"\n\
         \x20   elif command -v notify-send >/dev/null 2>&1; then\n\
         \x20       notify-send -u critical 'Lace Studio' \"$text\"\n\
         \x20   elif command -v xmessage >/dev/null 2>&1; then\n\
         \x20       xmessage \"$text\"\n\
         \x20   fi\n\
         \x20   echo \"$text\" >&2\n\
         \x20   exit 1\n\
         fi\n\
         exec \"$studio\" \"$@\"\n",
        studio = sh_quote(studio),
    );
    fs::write(&launcher, script).with_context(|| format!("Writing {}", launcher.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("Making {} executable", launcher.display()))?;
    }
    Ok(launcher)
}

/// Aspas simples de shell.
#[cfg(not(target_os = "macos"))]
fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
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

/// O webkit2gtk 4.1 que falta ao sistema, e como instalá-lo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingRuntime {
    /// O comando que instala o pacote pelo gerenciador do sistema (`apt`,
    /// `dnf` ou `pacman`, com `sudo` fora do root); `None` sem nenhum deles,
    /// ou sem `sudo`.
    pub install: Option<Vec<String>>,
}

/// Os comandos de cada família, para quando o gerenciador do sistema não é
/// um dos conhecidos.
const ALL_COMMANDS: &str = "sudo apt install libwebkit2gtk-4.1-0 (Debian, Ubuntu), \
     sudo dnf install webkit2gtk4.1 (Fedora), sudo pacman -S webkit2gtk-4.1 (Arch)";

impl MissingRuntime {
    /// O comando, numa linha.
    pub fn command_line(&self) -> Option<String> {
        self.install.as_ref().map(|c| c.join(" "))
    }

    /// Como instalar: o comando deste sistema, ou os de cada família.
    fn fix(&self) -> String {
        match self.command_line() {
            Some(command) => format!("Install it with: {command}"),
            None => format!("Install it with the system package manager: {ALL_COMMANDS}"),
        }
    }

    /// O aviso inteiro, em uma linha.
    pub fn message(&self) -> String {
        format!(
            "Lace Studio needs webkit2gtk 4.1, which this system does not have. {}",
            self.fix()
        )
    }
}

/// O comando que instala o webkit2gtk 4.1 pelo gerenciador de pacotes deste
/// sistema, se ele é um dos conhecidos. O gerenciador é achado pelo programa
/// em `/usr/bin` (ou `/bin`), não pelo nome da distribuição: as derivadas
/// (Mint, Pop!_OS, Nobara, Manjaro, EndeavourOS) usam o da família.
fn install_command() -> Option<Vec<String>> {
    const MANAGERS: [(&str, &[&str]); 3] = [
        ("apt", &["install", "libwebkit2gtk-4.1-0"]),
        ("dnf", &["install", "webkit2gtk4.1"]),
        ("pacman", &["-S", "--needed", "webkit2gtk-4.1"]),
    ];
    let in_system = |name: &str| {
        ["/usr/bin", "/bin"]
            .iter()
            .any(|d| Path::new(d).join(name).is_file())
    };
    let (manager, args) = MANAGERS.iter().find(|(name, _)| in_system(name))?;
    let mut command = Vec::new();
    if !is_root() {
        if !in_system("sudo") {
            return None;
        }
        command.push("sudo".to_owned());
    }
    command.push((*manager).to_owned());
    command.extend(args.iter().map(|a| (*a).to_owned()));
    Some(command)
}

/// Depois de instalar o Studio: faltando o webkit2gtk 4.1, pergunta no
/// terminal se o instala agora e, com o sim, roda o comando do gerenciador
/// de pacotes com o terminal dele (a senha do `sudo`, a confirmação do
/// gerenciador). Sem terminal ou sem gerenciador conhecido, não pergunta: o
/// aviso já saiu no resumo da instalação. Não falha: o Lace já está
/// instalado, e o que der errado aqui sai como aviso.
pub fn offer_runtime_install(studio_installed: bool) {
    use std::io::{BufRead, IsTerminal, Write};
    if !studio_installed {
        return;
    }
    let Some(missing) = missing_runtime() else {
        return;
    };
    let (Some(command), Some(line)) = (&missing.install, missing.command_line()) else {
        return;
    };
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return;
    }
    print!("\nInstall webkit2gtk 4.1 now with `{line}`? [Y/n] ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    if std::io::stdin().lock().read_line(&mut answer).is_err() {
        return;
    }
    if answer.trim().to_lowercase().starts_with('n') {
        println!("Lace Studio will not open until webkit2gtk 4.1 is installed: {line}");
        return;
    }
    let status = std::process::Command::new(&command[0])
        .args(&command[1..])
        .status();
    match status {
        Ok(status) if status.success() && missing_runtime().is_none() => {
            println!("webkit2gtk 4.1 is installed: Lace Studio is ready to open");
        }
        Ok(_) => eprintln!("webkit2gtk 4.1 was not installed. {}", missing.fix()),
        Err(e) => eprintln!("Could not run {line} ({e}). {}", missing.fix()),
    }
}

/// O que falta ao sistema do WebView que o Studio usa, ou `None`.
/// Só o Linux tem o que conferir: o Studio liga ao webkit2gtk 4.1 do
/// sistema, que nem toda instalação traz (o GNOME traz; o KDE e o Arch, em
/// geral, não). No macOS o WebView é do sistema; no Windows, o WebView2 vem
/// com o Windows 10 e o 11.
pub fn missing_runtime() -> Option<MissingRuntime> {
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
    (!found).then(|| MissingRuntime {
        install: install_command(),
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

    #[test]
    fn the_shortcut_opens_the_launcher_and_it_runs_the_studio() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        // Aspa no caminho, para as aspas do lançador.
        let prefix = dir.path().join("it's");
        let studio = studio_in(&prefix);
        fs::create_dir_all(studio.parent().unwrap()).unwrap();
        fs::write(&studio, "#!/bin/sh\necho studio \"$@\"\n").unwrap();
        fs::set_permissions(&studio, fs::Permissions::from_mode(0o755)).unwrap();
        let shortcut = dir.path().join(DESKTOP_FILE);
        write_shortcut(&shortcut, &studio, &prefix).unwrap();

        let launcher = studio.with_file_name(LAUNCHER);
        let text = fs::read_to_string(&shortcut).unwrap();
        assert!(text.contains(&format!("Exec={}\n", desktop_exec(&launcher))));
        // Um script não tem bibliotecas para o ldd: o lançador segue para o
        // Studio, com os argumentos.
        let out = std::process::Command::new(&launcher)
            .arg("x.spf")
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "studio x.spf\n");

        // Sem o Studio, o lançador sai com ele (o atalho do menu, que não é
        // desta instalação, fica como está).
        fs::remove_file(&studio).unwrap();
        sync(&prefix).unwrap();
        assert!(!launcher.exists());
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;

    #[test]
    fn the_warning_names_this_system_command_or_every_family() {
        let dnf = MissingRuntime {
            install: Some(vec![
                "sudo".into(),
                "dnf".into(),
                "install".into(),
                "webkit2gtk4.1".into(),
            ]),
        };
        assert_eq!(
            dnf.command_line().as_deref(),
            Some("sudo dnf install webkit2gtk4.1")
        );
        assert!(
            dnf.message()
                .ends_with("Install it with: sudo dnf install webkit2gtk4.1")
        );
        let unknown = MissingRuntime { install: None };
        let message = unknown.message();
        for family in ["apt install", "dnf install", "pacman -S webkit2gtk-4.1"] {
            assert!(message.contains(family), "{message}");
        }
    }
}
