//! `lace-pack`: monta os instaladores a partir de um bundle do
//! `scripts/bundle.py`.
//!
//! ```text
//! lace-pack tui  --toolchain dist/toolchain --lace target/release/lace \
//!                --installer target/release/lace-installer --out dist/lace-0.1.0-linux-x64 --tar-gz
//! lace-pack inno --toolchain dist/toolchain --lace target/release/lace.exe --out dist/inno
//! ```

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use lace_installer::pack::{self, Contents};

#[derive(Parser, Debug)]
#[command(name = "lace-pack", version = lace_installer::LACE_VERSION)]
/// Build the Lace installers
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Args, Debug)]
struct Common {
    /// The bundle built by scripts/bundle.py
    #[arg(long, value_name = "DIR")]
    toolchain: PathBuf,
    #[arg(
        long,
        value_name = "FILE",
        help = "The contents index [default: <toolchain>.contents.json]"
    )]
    contents: Option<PathBuf>,
    /// The Lace executable for the platform
    #[arg(long, value_name = "FILE")]
    lace: PathBuf,
    /// Where to build (must be empty)
    #[arg(long, value_name = "DIR")]
    out: PathBuf,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Linux and macOS installer: `install` + `payload/`
    Tui {
        #[command(flatten)]
        common: Common,
        /// The installer executable (lace-installer) for the platform
        #[arg(long, value_name = "FILE")]
        installer: PathBuf,
        /// zstd compression level
        #[arg(long, default_value_t = 19)]
        level: i32,
        #[arg(long, help = "Also write <out>.tar.gz")]
        tar_gz: bool,
        /// Also write to DIR the index and the chunks under the names the
        /// release publishes, so that `lace install` downloads only the apps
        #[arg(long, value_name = "DIR")]
        assets: Option<PathBuf>,
    },
    /// Inno Setup stage for the Windows installer
    Inno {
        #[command(flatten)]
        common: Common,
        /// Also write web.iss for the installer that downloads the apps (iscc /DWeb), from the
        /// `lace-pack tui` installer folder with the chunks the release publishes
        #[arg(long, value_name = "DIR")]
        web: Option<PathBuf>,
    },
}

fn contents(common: &Common) -> anyhow::Result<Contents> {
    let path = match &common.contents {
        Some(p) => p.clone(),
        None => {
            let name = common
                .toolchain
                .file_name()
                .context("--toolchain has no file name")?
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
            assets,
        } => {
            let contents = contents(&common)?;
            let index = pack::tui(
                &common.toolchain,
                &contents,
                &common.lace,
                &installer,
                &common.out,
                level,
            )?;
            println!(
                "Installer {} in {}: {} chunks",
                index.platform,
                common.out.display(),
                index.chunks.len()
            );
            if tar_gz {
                let name = common
                    .out
                    .file_name()
                    .context("--out has no file name")?
                    .to_string_lossy();
                let archive = common.out.with_file_name(format!("{name}.tar.gz"));
                pack::tar_gz(&common.out, &archive)?;
                println!("Archive {}", archive.display());
            }
            if let Some(dir) = assets {
                let written = pack::release_assets(&common.out, &dir)?;
                println!(
                    "{} payload files for the release in {}",
                    written.len(),
                    dir.display()
                );
            }
        }
        Command::Inno { common, web } => {
            let contents = contents(&common)?;
            pack::inno(
                &common.toolchain,
                &contents,
                &common.lace,
                &common.out,
                web.as_deref(),
            )?;
            println!("Inno Setup stage in {}", common.out.display());
        }
    }
    Ok(())
}
