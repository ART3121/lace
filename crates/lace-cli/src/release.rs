//! As releases do Lace no GitHub: a última versão, e baixar um arquivo de
//! uma release conferindo o SHA-256 pelo `SHA256SUMS` dela. Também baixa da
//! internet o que mais o `lace install` e o `lace update` precisam ler.
//!
//! Baixa com o `curl` do sistema, o mesmo que o `install.sh` usa (no Windows
//! 10 e 11 ele vem no `System32`): o Lace não traz cliente HTTP próprio.

use std::io::Read;
use std::process::Stdio;

use anyhow::{Context, bail};
use camino::Utf8Path;
use sha2::{Digest, Sha256};

/// De onde vêm as releases, como no `install.sh`: `LACE_REPO` troca.
const DEFAULT_REPO: &str = "ART3121/lace";

/// O repositório das releases do Lace (`dono/nome`).
pub fn repo() -> String {
    std::env::var("LACE_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_owned())
}

/// A URL de um arquivo da release `v<version>`. `LACE_RELEASE_URL` troca a
/// base (um espelho do laboratório, com uma pasta `v<versão>/` por release).
pub fn asset_url(version: &str, file: &str) -> String {
    match std::env::var("LACE_RELEASE_URL") {
        Ok(base) if !base.is_empty() => {
            format!("{}/v{version}/{file}", base.trim_end_matches('/'))
        }
        _ => format!(
            "https://github.com/{}/releases/download/v{version}/{file}",
            repo()
        ),
    }
}

/// A tag da última release de um repositório do GitHub, sem passar pela API
/// (que limita as consultas): `/releases/latest` redireciona para
/// `/releases/tag/<tag>`.
pub fn latest_github_tag(repo: &str) -> anyhow::Result<String> {
    let url = format!("https://github.com/{repo}/releases/latest");
    let output = curl()
        .args(["-fsSIL", "-o", null_device(), "-w", "%{url_effective}"])
        .arg(&url)
        .stderr(Stdio::null())
        .output()
        .map_err(curl_missing)?;
    let effective = String::from_utf8_lossy(&output.stdout);
    match effective.rsplit_once("/releases/tag/") {
        Some((_, tag)) if output.status.success() && !tag.is_empty() => Ok(tag.trim().to_owned()),
        _ => bail!("Could not look up the latest release at {url}"),
    }
}

/// A última versão do Lace publicada (a tag sem o `v`).
pub fn latest_lace_version() -> anyhow::Result<String> {
    let tag = latest_github_tag(&repo())?;
    Ok(tag.trim_start_matches('v').to_owned())
}

/// O conteúdo de `url`, como texto.
pub fn fetch_text(url: &str) -> anyhow::Result<String> {
    let output = curl()
        .args(["-fsSL", url])
        .stderr(Stdio::null())
        .output()
        .map_err(curl_missing)?;
    if !output.status.success() {
        bail!("Could not download {url}");
    }
    String::from_utf8(output.stdout).with_context(|| format!("{url} is not UTF-8 text"))
}

/// Baixa `url` para `dest`. Com `progress`, a barra do `curl` vai para o
/// stderr.
pub fn download_to(url: &str, dest: &Utf8Path, progress: bool) -> anyhow::Result<()> {
    let status = curl()
        .arg("-fL")
        .arg(if progress { "--progress-bar" } else { "-sS" })
        .arg("-o")
        .arg(dest.as_std_path())
        .arg(url)
        .stdout(Stdio::null())
        .status()
        .map_err(curl_missing)?;
    if !status.success() {
        bail!("Could not download {url}");
    }
    Ok(())
}

/// Baixa `file` da release `v<version>` para `dir` e o confere com o
/// `SHA256SUMS` dela (que fica em `dir`, e é baixado uma vez só).
pub fn download_checked(
    version: &str,
    file: &str,
    dir: &Utf8Path,
    progress: bool,
) -> anyhow::Result<camino::Utf8PathBuf> {
    let sums = dir.join("SHA256SUMS");
    if !sums.is_file() {
        download_to(&asset_url(version, "SHA256SUMS"), &sums, false)?;
    }
    let dest = dir.join(file);
    download_to(&asset_url(version, file), &dest, progress)?;
    let sums = std::fs::read_to_string(&sums).with_context(|| format!("Reading {sums}"))?;
    let expected = expected_sha256(&sums, file)
        .with_context(|| format!("The SHA256SUMS of release v{version} does not list {file}"))?;
    if sha256_of(&dest)? != expected {
        bail!("The SHA-256 of {file} does not match the SHA256SUMS of release v{version}");
    }
    Ok(dest)
}

/// Como [`download_checked`], avisando a `downloaded` os bytes já baixados
/// enquanto o `curl` roda (para uma barra de progresso que não é a dele).
pub fn download_checked_quiet(
    version: &str,
    file: &str,
    dir: &Utf8Path,
    downloaded: &mut dyn FnMut(u64),
) -> anyhow::Result<camino::Utf8PathBuf> {
    let sums = dir.join("SHA256SUMS");
    if !sums.is_file() {
        download_to(&asset_url(version, "SHA256SUMS"), &sums, false)?;
    }
    let dest = dir.join(file);
    let url = asset_url(version, file);
    let mut child = curl()
        .args(["-fsSL", "-o"])
        .arg(dest.as_std_path())
        .arg(&url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(curl_missing)?;
    let status = loop {
        if let Some(status) = child.try_wait().context("Waiting for curl")? {
            break status;
        }
        if let Ok(meta) = std::fs::metadata(&dest) {
            downloaded(meta.len());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    if !status.success() {
        bail!("Could not download {url}");
    }
    let sums = std::fs::read_to_string(&sums).with_context(|| format!("Reading {sums}"))?;
    let expected = expected_sha256(&sums, file)
        .with_context(|| format!("The SHA256SUMS of release v{version} does not list {file}"))?;
    if sha256_of(&dest)? != expected {
        bail!("The SHA-256 of {file} does not match the SHA256SUMS of release v{version}");
    }
    Ok(dest)
}

/// O hash de `file` num `SHA256SUMS` (`<hash>  <arquivo>`, com `*` antes do
/// nome no modo binário).
pub fn expected_sha256(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        (name == file).then(|| hash.to_ascii_lowercase())
    })
}

/// O SHA-256 de um arquivo, em hexadecimal.
pub fn sha256_of(path: &Utf8Path) -> anyhow::Result<String> {
    let mut file = std::fs::File::open(path).with_context(|| format!("Opening {path}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 16];
    loop {
        let n = file
            .read(&mut buffer)
            .with_context(|| format!("Reading {path}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Uma pasta temporária fora do `/tmp`, como no `install.sh`: há sistemas
/// que o montam sem permissão de execução, e um instalador baixado roda dela.
pub fn work_dir() -> anyhow::Result<tempfile::TempDir> {
    let base = if cfg!(windows) {
        std::env::temp_dir()
    } else if std::env::var("USER").is_ok_and(|u| u == "root") {
        "/var/tmp".into()
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".cache")))
            .unwrap_or_else(std::env::temp_dir)
    };
    std::fs::create_dir_all(&base).with_context(|| format!("Creating {}", base.display()))?;
    tempfile::Builder::new()
        .prefix("lace-install.")
        .tempdir_in(&base)
        .with_context(|| format!("Creating a temporary folder in {}", base.display()))
}

/// O `curl`, com prazo para conectar: uma rede que pendura em vez de recusar
/// não trava o comando. O download em si não tem prazo, porque um pedaço
/// grande numa rede lenta leva o tempo que leva.
fn curl() -> std::process::Command {
    let mut command = std::process::Command::new(curl_program());
    command.args(["--connect-timeout", "20"]);
    command
}

fn curl_program() -> String {
    if cfg!(windows) {
        std::env::var("SystemRoot")
            .map(|root| format!("{root}\\System32\\curl.exe"))
            .unwrap_or_else(|_| "curl.exe".to_owned())
    } else {
        "curl".to_owned()
    }
}

fn null_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

fn curl_missing(error: std::io::Error) -> anyhow::Error {
    if error.kind() == std::io::ErrorKind::NotFound {
        anyhow::anyhow!("Lace downloads with curl, which was not found; install curl")
    } else {
        anyhow::Error::new(error).context("Running curl")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256sums_lines_are_read_in_both_modes() {
        let sums = "\
AAAA  lace-0.2.0-linux-x64.tar.gz
bbbb *lace-0.2.0-windows-x64-setup.exe
";
        assert_eq!(
            expected_sha256(sums, "lace-0.2.0-linux-x64.tar.gz").as_deref(),
            Some("aaaa")
        );
        assert_eq!(
            expected_sha256(sums, "lace-0.2.0-windows-x64-setup.exe").as_deref(),
            Some("bbbb")
        );
        assert_eq!(
            expected_sha256(sums, "lace-0.2.0-darwin-arm64.tar.gz"),
            None
        );
    }

    #[test]
    fn sha256_of_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("abc")).unwrap();
        std::fs::write(&path, "abc").unwrap();
        assert_eq!(
            sha256_of(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
