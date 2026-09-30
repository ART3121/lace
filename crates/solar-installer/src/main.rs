//! `install`: o instalador do Solar para Linux e macOS.
//!
//! Sem opções, abre a instalação guiada no terminal. Com `--yes`, instala sem
//! perguntar: o perfil recomendado, ou os componentes de `--components`
//! (instalação avançada).

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::Parser;
use solar_installer::install::{self, Event, Target};
use solar_installer::payload::{Index, PAYLOAD_DIR};
use solar_installer::plan::{self, Selection};
use solar_installer::tui::{self, App};

/// Instalador do Solar e do bundle de ferramentas.
#[derive(Parser, Debug)]
#[command(
    name = "install",
    version = solar_installer::SOLAR_VERSION,
    after_help = "Sem --yes, abre a instalação guiada no terminal: tipo Recomendada, ou Avançada \
                  para escolher os componentes.\n\nExemplos:\n  ./install\n  ./install --yes\n  \
                  ./install --yes --components yanc,icarus,verilator\n  sudo ./install --yes --prefix /opt/solar"
)]
struct Args {
    /// Instala sem perguntar: o perfil recomendado, ou os componentes de --components
    #[arg(short, long)]
    yes: bool,

    /// Componentes separados por vírgula (instalação avançada); veja --list
    #[arg(long, value_delimiter = ',', value_name = "LISTA")]
    components: Option<Vec<String>>,

    /// Pasta da instalação [padrão: ~/.local/share/solar; como root, /opt/solar]
    #[arg(long, value_name = "DIR")]
    prefix: Option<PathBuf>,

    /// Onde criar o atalho `solar` [padrão: ~/.local/bin/solar; como root, /usr/local/bin/solar]
    #[arg(long, value_name = "ARQUIVO", conflicts_with = "no_link")]
    link: Option<PathBuf>,

    /// Não cria o atalho
    #[arg(long)]
    no_link: bool,

    /// Lista os componentes, o tamanho de cada um e o perfil recomendado
    #[arg(long)]
    list: bool,

    /// Diretório do payload [padrão: payload/ ao lado deste programa]
    #[arg(long, value_name = "DIR", hide = true)]
    payload: Option<PathBuf>,
}

fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("erro: {e:#}");
        std::process::exit(1);
    }
}

fn payload_dir(args: &Args) -> anyhow::Result<PathBuf> {
    if let Some(dir) = &args.payload {
        return Ok(dir.clone());
    }
    let exe = std::env::current_exe().context("não sei onde está o instalador")?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    Ok(exe
        .parent()
        .context("instalador sem pasta")?
        .join(PAYLOAD_DIR))
}

fn run(args: Args) -> anyhow::Result<()> {
    if cfg!(windows) {
        bail!("no Windows, use o instalador solar-<versão>-windows-x64-setup.exe");
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
            bail!("--components instala sem perguntar: acrescente --yes");
        }
        let selection = match &args.components {
            Some(names) => {
                let (selection, added) = plan::from_names(&index, names)?;
                if !added.is_empty() {
                    println!("incluídos por dependência: {}", added.join(", "));
                }
                selection
            }
            None => plan::recommended(&index),
        };
        return unattended(&payload, &index, &selection, &target);
    }

    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("sem terminal para a instalação guiada: use --yes (e --components para escolher)");
    }
    let app = App::new(
        index.clone(),
        payload,
        target,
        solar_core::SystemCompiler::detect(),
    );
    let app = tui::run(app)?;
    match &app.outcome {
        Some(Ok(report)) => println!("{}", tui::plain(&tui::report_lines(report, &index))),
        Some(Err(e)) => bail!("{e}"),
        None => println!("Instalação cancelada; nada foi alterado."),
    }
    Ok(())
}

fn list(index: &Index) {
    println!(
        "Solar {} ({}), bundle {}\n",
        index.solar_version, index.platform, index.bundle
    );
    for c in &index.components {
        println!(
            "  {:<14} {:<3} {:>9}  {}{}",
            c.name,
            if c.recommended { "rec" } else { "" },
            solar_installer::mib(index.component_size(&c.name)),
            c.description,
            if c.requires.is_empty() {
                String::new()
            } else {
                format!(" (precisa de {})", c.requires.join(", "))
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
        "\nRecomendada ({}): {}",
        solar_installer::mib(index.size_of(&rec)),
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
        "Instalando o Solar {} em {} com: {}",
        index.solar_version,
        target.prefix.display(),
        if selection.is_empty() {
            "nenhum componente".to_owned()
        } else {
            selection.iter().cloned().collect::<Vec<_>>().join(", ")
        }
    );
    let report = install::install(payload, index, selection, target, |e| match e {
        Event::Chunk {
            index,
            count,
            components,
        } => {
            let what = if components.is_empty() {
                "solar".to_owned()
            } else {
                components.join(", ")
            };
            println!("  [{index}/{count}] {what}");
        }
        Event::Verifying => println!("  conferindo os executáveis"),
        _ => {}
    })?;
    println!("{}", tui::plain(&tui::report_lines(&report, index)));
    Ok(())
}
