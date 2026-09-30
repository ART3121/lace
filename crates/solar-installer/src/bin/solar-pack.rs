//! `solar-pack`: monta os instaladores a partir de um bundle do
//! `scripts/bundle.py`.
//!
//! ```text
//! solar-pack tui  --toolchain dist/toolchain --solar target/release/solar \
//!                 --installer target/release/solar-installer --out dist/solar-0.1.0-linux-x64 --tar-gz
//! solar-pack inno --toolchain dist/toolchain --solar target/release/solar.exe --out dist/inno
//! ```

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use solar_installer::pack::{self, Contents};

#[derive(Parser, Debug)]
#[command(name = "solar-pack", version = solar_installer::SOLAR_VERSION)]
/// Monta os instaladores do Solar
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Args, Debug)]
struct Common {
    /// O bundle montado pelo scripts/bundle.py
    #[arg(long, value_name = "DIR")]
    toolchain: PathBuf,
    #[arg(
        long,
        value_name = "ARQUIVO",
        help = "O índice de conteúdo [padrão: <toolchain>.contents.json]"
    )]
    contents: Option<PathBuf>,
    /// O executável do Solar para a plataforma
    #[arg(long, value_name = "ARQUIVO")]
    solar: PathBuf,
    /// Onde montar (precisa estar vazio)
    #[arg(long, value_name = "DIR")]
    out: PathBuf,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Instalador de Linux e macOS: `install` + `payload/`
    Tui {
        #[command(flatten)]
        common: Common,
        /// O executável do instalador (solar-installer) para a plataforma
        #[arg(long, value_name = "ARQUIVO")]
        installer: PathBuf,
        /// Nível do zstd
        #[arg(long, default_value_t = 19)]
        level: i32,
        #[arg(long, help = "Também grava <out>.tar.gz")]
        tar_gz: bool,
    },
    /// Estágio do Inno Setup para o instalador de Windows
    Inno {
        #[command(flatten)]
        common: Common,
    },
}

fn contents(common: &Common) -> anyhow::Result<Contents> {
    let path = match &common.contents {
        Some(p) => p.clone(),
        None => {
            let name = common
                .toolchain
                .file_name()
                .context("--toolchain sem nome")?
                .to_string_lossy()
                .into_owned();
            common
                .toolchain
                .with_file_name(format!("{name}.contents.json"))
        }
    };
    Contents::load(&path)
}

fn main() -> anyhow::Result<()> {
    match Args::parse().command {
        Command::Tui {
            common,
            installer,
            level,
            tar_gz,
        } => {
            let contents = contents(&common)?;
            let index = pack::tui(
                &common.toolchain,
                &contents,
                &common.solar,
                &installer,
                &common.out,
                level,
            )?;
            println!(
                "instalador {} em {}: {} pedaços",
                index.platform,
                common.out.display(),
                index.chunks.len()
            );
            if tar_gz {
                let name = common
                    .out
                    .file_name()
                    .context("--out sem nome")?
                    .to_string_lossy();
                let archive = common.out.with_file_name(format!("{name}.tar.gz"));
                pack::tar_gz(&common.out, &archive)?;
                println!("arquivo {}", archive.display());
            }
        }
        Command::Inno { common } => {
            let contents = contents(&common)?;
            pack::inno(&common.toolchain, &contents, &common.solar, &common.out)?;
            println!("estágio do Inno Setup em {}", common.out.display());
        }
    }
    Ok(())
}
