//! `install`: o instalador do Lace para Linux e macOS.
//!
//! Sem opções, abre a instalação guiada no terminal. Com `--yes`, instala sem
//! perguntar: o perfil recomendado, ou os componentes de `--components`
//! (instalação avançada).
//!
//! Para acrescentar aplicativos do bundle a uma instalação que já existe, sem
//! reinstalar o Lace, o caminho é `lace install`.

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::Parser;
use lace_installer::install::{self, Event, Target};
use lace_installer::payload::{Index, PAYLOAD_DIR};
use lace_installer::plan::{self, Selection};
use lace_installer::tui::{self, App};

/// Installer for Lace and its tool bundle.
#[derive(Parser, Debug)]
#[command(
    name = "install",
    version = lace_installer::LACE_VERSION,
    after_help = "Without --yes, opens the guided installation in the terminal: Recommended, or Advanced \
                  to choose the components.\n\nExamples:\n  ./install\n  ./install --yes\n  \
                  ./install --yes --components yanc,icarus,verilator\n  sudo ./install --yes --prefix /opt/lace"
)]
struct Args {
    /// Install without asking: the recommended profile, or the components in --components
    #[arg(short, long)]
    yes: bool,

    /// Comma-separated components (advanced installation); see --list
    #[arg(long, value_delimiter = ',', value_name = "LIST")]
    components: Option<Vec<String>>,

    /// Installation folder [default: ~/.local/share/lace; as root, /opt/lace]
    #[arg(long, value_name = "DIR")]
    prefix: Option<PathBuf>,

    /// Where to create the `lace` link [default: ~/.local/bin/lace; as root, /usr/local/bin/lace]
    #[arg(long, value_name = "FILE", conflicts_with = "no_link")]
    link: Option<PathBuf>,

    /// Do not create the link
    #[arg(long)]
    no_link: bool,

    /// List the components, the size of each and the recommended profile
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
        bail!("On Windows, use the lace-<version>-windows-x64-setup.exe installer");
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

    if args.yes || args.components.is_some() {
        if !args.yes {
            bail!("--components installs without asking: add --yes");
        }
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
                    let selection = plan::recommended_keeping(&index, &existing.components);
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
                _ => plan::recommended(&index),
            },
        };
        return unattended(&payload, &index, &selection, &target);
    }

    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("No terminal for the guided installation: use --yes (and --components to choose)");
    }
    let app = App::new(
        index.clone(),
        payload,
        target,
        lace_core::SystemCompiler::detect(),
    );
    let app = tui::run(app)?;
    match &app.outcome {
        Some(Ok(tui::Finished::Installed(report))) => {
            println!("{}", tui::plain(&tui::report_lines(report, &index)));
        }
        Some(Ok(tui::Finished::Added(report))) => {
            println!("{}", tui::plain(&tui::added_lines(report, &index)));
        }
        Some(Err(e)) => bail!("{e}"),
        None => println!("Installation cancelled; nothing was changed."),
    }
    Ok(())
}

fn list(index: &Index) {
    println!(
        "Lace {} ({}), bundle {}\n",
        index.lace_version, index.platform, index.bundle
    );
    for c in &index.components {
        println!(
            "  {:<14} {:<3} {:>9}  {}{}",
            c.name,
            if c.recommended { "rec" } else { "" },
            lace_installer::mib(index.component_size(&c.name)),
            c.description,
            if c.requires.is_empty() {
                String::new()
            } else {
                format!(" (requires {})", c.requires.join(", "))
            }
        );
    }
    let rec = plan::recommended(index);
    let names: Vec<&str> = index
        .components
        .iter()
        .filter(|c| rec.contains(&c.name))
        .map(|c| c.name.as_str())
        .collect();
    println!(
        "\nRecommended ({}): {}",
        lace_installer::mib(index.size_of(&rec)),
        names.join(", ")
    );
}

fn unattended(
    payload: &std::path::Path,
    index: &Index,
    selection: &Selection,
    target: &Target,
) -> anyhow::Result<()> {
    println!(
        "Installing Lace {} in {} with: {}",
        index.lace_version,
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
                "lace".to_owned()
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
