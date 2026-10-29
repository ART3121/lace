//! `install`: o instalador da AURORA para Linux e macOS.
//!
//! Sem opções, abre a instalação guiada no terminal. Com `--yes`, instala sem
//! perguntar: o perfil padrão, o de `--profile`, ou os componentes de
//! `--components` (instalação Custom).
//!
//! Para acrescentar aplicativos do bundle a uma instalação que já existe, sem
//! reinstalar a AURORA, o caminho é `aurora install`.

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, bail};
use aurora_installer::desktop;
use aurora_installer::install::{self, Event, Target};
use aurora_installer::payload::{Index, PAYLOAD_DIR};
use aurora_installer::plan::{self, Selection};
use aurora_installer::tui::{self, App};
use clap::Parser;

/// Installer for AURORA and its tool bundle.
#[derive(Parser, Debug)]
#[command(
    name = "install",
    version = aurora_installer::AURORA_VERSION,
    after_help = "Without --yes, opens the guided installation in the terminal: a profile, or Custom \
                  to choose the components.\n\nExamples:\n  ./install\n  ./install --yes\n  \
                  ./install --yes --profile verilog\n  \
                  ./install --yes --components yanc,icarus,verilator\n  sudo ./install --yes --prefix /opt/aurora"
)]
struct Args {
    /// Install without asking: the default profile, the one in --profile, or the components in --components
    #[arg(short, long)]
    yes: bool,

    /// Installation profile; see --list [default: the first one]
    #[arg(long, value_name = "NAME", conflicts_with = "components")]
    profile: Option<String>,

    /// Comma-separated components (Custom installation); see --list
    #[arg(long, value_delimiter = ',', value_name = "LIST")]
    components: Option<Vec<String>>,

    /// Installation folder [default: ~/.local/share/aurora; as root, /opt/aurora]
    #[arg(long, value_name = "DIR")]
    prefix: Option<PathBuf>,

    /// Where to create the `aurora` link [default: ~/.local/bin/aurora; as root, /usr/local/bin/aurora]
    #[arg(long, value_name = "FILE", conflicts_with = "no_link")]
    link: Option<PathBuf>,

    /// Do not create the link
    #[arg(long)]
    no_link: bool,

    /// List the profiles, the components and the size of each
    #[arg(long)]
    list: bool,

    /// Payload directory [default: payload/ next to this program]
    #[arg(long, value_name = "DIR", hide = true)]
    payload: Option<PathBuf>,
}

fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

fn payload_dir(args: &Args) -> anyhow::Result<PathBuf> {
    if let Some(dir) = &args.payload {
        return Ok(dir.clone());
    }
    let exe = std::env::current_exe().context("Could not find where the installer is")?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    Ok(exe
        .parent()
        .context("The installer has no parent folder")?
        .join(PAYLOAD_DIR))
}

fn run(args: Args) -> anyhow::Result<()> {
    if cfg!(windows) {
        bail!("On Windows, use the aurora-<version>-windows-x64-setup.exe installer");
    }
    let payload = payload_dir(&args)?;
    let index = Index::load(&payload)?;

    if args.list {
        list(&index);
        return Ok(());
    }

    let mut target = Target::default_for_user();
    if let Some(prefix) = &args.prefix {
        target.prefix = std::path::absolute(prefix)?;
    }
    if let Some(link) = &args.link {
        target.link = Some(std::path::absolute(link)?);
    }
    if args.no_link {
        target.link = None;
    }

    if args.yes || args.components.is_some() || args.profile.is_some() {
        if !args.yes {
            bail!("--profile and --components install without asking: add --yes");
        }
        let profile = match &args.profile {
            Some(name) => name.clone(),
            None => plan::default_profile(&index),
        };
        let selection = match &args.components {
            Some(names) => {
                let (selection, added) = plan::from_names(&index, names)?;
                if !added.is_empty() {
                    println!("Included as dependencies: {}", added.join(", "));
                }
                selection
            }
            // Numa pasta com uma instalação, o que ela tem fica.
            None => match install::check_prefix(&target.prefix) {
                Ok(Some(existing)) => {
                    let selection = plan::profile_keeping(&index, &profile, &existing.components)?;
                    let kept: Vec<&str> = existing
                        .components
                        .iter()
                        .filter(|c| selection.contains(*c))
                        .map(String::as_str)
                        .collect();
                    if !kept.is_empty() {
                        println!("Keeping the apps already installed: {}", kept.join(", "));
                    }
                    selection
                }
                _ => plan::profile(&index, &profile)?,
            },
        };
        return unattended(&payload, &index, &selection, &target);
    }

    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!(
            "No terminal for the guided installation: use --yes (with --profile or --components to choose)"
        );
    }
    let app = App::new(
        index.clone(),
        payload,
        target,
        aurora_core::SystemCompiler::detect(),
    );
    let app = tui::run(app)?;
    match &app.outcome {
        Some(Ok(tui::Finished::Installed(report))) => {
            println!("{}", tui::plain(&tui::report_lines(report, &index)));
            desktop::offer_runtime_install(has_studio(&report.components));
        }
        Some(Ok(tui::Finished::Added(report))) => {
            println!("{}", tui::plain(&tui::added_lines(report, &index)));
            desktop::offer_runtime_install(has_studio(&report.added));
        }
        Some(Err(e)) => bail!("{e}"),
        None => println!("Installation cancelled; nothing was changed."),
    }
    Ok(())
}

/// O Studio está entre os componentes instalados?
fn has_studio(components: &[String]) -> bool {
    components
        .iter()
        .any(|c| c == aurora_core::component::STUDIO)
}

fn list(index: &Index) {
    println!(
        "AURORA {} ({}), bundle {}\n",
        index.aurora_version, index.platform, index.bundle
    );
    println!("Profiles (--profile):\n");
    let default = plan::default_profile(index);
    for p in index.profiles() {
        let selection = plan::profile(index, &p.name).unwrap_or_default();
        let labels: Vec<&str> = index
            .components
            .iter()
            .filter(|c| selection.contains(&c.name))
            .map(|c| c.label.as_str())
            .collect();
        let label = if p.name == default {
            format!("{} (Recommended)", p.label)
        } else {
            p.label.clone()
        };
        println!(
            "  {:<14} {:>9}  {label}: {}",
            p.name,
            aurora_installer::mib(index.size_of(&selection)),
            p.description
        );
        if !p.audience.is_empty() {
            println!("  {:<25}  For: {}", "", p.audience);
        }
        println!(
            "  {:<25}  Installs: AURORA (CLI & Core), {}\n",
            "",
            labels.join(", ")
        );
    }
    println!(
        "Custom installation (--components), for advanced use: for users who know which \
         components they need.\n"
    );
    for c in &index.components {
        println!(
            "  {:<14} {:>9}  {}{}",
            c.name,
            aurora_installer::mib(index.component_size(&c.name)),
            c.description,
            if c.requires.is_empty() {
                String::new()
            } else {
                format!(" (requires {})", c.requires.join(", "))
            }
        );
    }
}

fn unattended(
    payload: &std::path::Path,
    index: &Index,
    selection: &Selection,
    target: &Target,
) -> anyhow::Result<()> {
    println!(
        "Installing AURORA {} in {} with: {}",
        index.aurora_version,
        target.prefix.display(),
        if selection.is_empty() {
            "no components".to_owned()
        } else {
            selection.iter().cloned().collect::<Vec<_>>().join(", ")
        }
    );
    let report = install::install(payload, index, selection, target, |e| match e {
        Event::Chunk {
            index,
            count,
            components,
            ..
        } => {
            let what = if components.is_empty() {
                "aurora".to_owned()
            } else {
                components.join(", ")
            };
            println!("  [{index}/{count}] {what}");
        }
        Event::Verifying => println!("  Verifying the executables"),
        _ => {}
    })?;
    println!("{}", tui::plain(&tui::report_lines(&report, index)));
    Ok(())
}
