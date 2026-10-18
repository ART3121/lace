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

/// A base de `LACE_RELEASE_URL` (um espelho do laboratório), se definida. Só
/// `https://`, ou `file://` para um espelho no disco: o `SHA256SUMS` que
/// confere os downloads vem do mesmo lugar, e por `http://` quem estivesse
/// na rede trocaria os dois juntos, e com eles o `lace` do próximo
/// `lace update`.
fn mirror() -> anyhow::Result<Option<String>> {
    mirror_from(std::env::var("LACE_RELEASE_URL").ok().as_deref())
}

/// [`mirror`] com o valor de `LACE_RELEASE_URL`.
fn mirror_from(base: Option<&str>) -> anyhow::Result<Option<String>> {
    let Some(base) = base.filter(|b| !b.is_empty()) else {
        return Ok(None);
    };
    let scheme = base.split_once("://").map(|(s, _)| s.to_ascii_lowercase());
    match scheme.as_deref() {
        Some("https" | "file") => Ok(Some(base.trim_end_matches('/').to_owned())),
        _ => bail!(
            "LACE_RELEASE_URL must start with https:// (or file:// for a mirror on disk), and it \
             is {base}: the SHA256SUMS that checks each download comes from there too"
        ),
    }
}

/// Uma versão como as do Lace: começa por dígito e tem só letras, dígitos,
/// `.`, `-` e `+` (`0.9.0`, `0.9.0-rc.1`). É o que vai no caminho das URLs e
/// no nome dos arquivos baixados, e o que o terminal mostra: uma resposta do
/// espelho com `/`, `..` ou uma sequência de escape não passa.
pub(crate) fn is_version(text: &str) -> bool {
    text.len() <= 64
        && text.starts_with(|c: char| c.is_ascii_digit())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

/// Um nome de arquivo de uma release (o índice de um espelho também os
/// lista): sem pasta e sem começar por `.`, com os caracteres de
/// [`is_version`] e `_`.
fn is_asset_name(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 255
        && !text.starts_with('.')
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'))
}

/// A URL de um arquivo da release `v<version>`. `LACE_RELEASE_URL` troca a
/// base (um espelho do laboratório, com uma pasta `v<versão>/` por release).
pub fn asset_url(version: &str, file: &str) -> anyhow::Result<String> {
    if !is_version(version) {
        bail!("Not a Lace version: {}", version.escape_debug());
    }
    if !is_asset_name(file) {
        bail!("Not a file name of a release: {}", file.escape_debug());
    }
    Ok(match mirror()? {
        Some(base) => format!("{base}/v{version}/{file}"),
        None => format!(
            "https://github.com/{}/releases/download/v{version}/{file}",
            repo()
        ),
    })
}

/// A tag da última release de um repositório do GitHub, sem passar pela API
/// (que limita as consultas): `/releases/latest` redireciona para
/// `/releases/tag/<tag>`. Uma tag fora dos caracteres de um nome de arquivo
/// de release ([`is_asset_name`]) é recusada.
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
        Some((_, tag)) if output.status.success() && is_asset_name(tag.trim()) => {
            Ok(tag.trim().to_owned())
        }
        _ => bail!("Could not look up the latest release at {url}"),
    }
}

/// A última versão do Lace publicada (a tag sem o `v`). Com um espelho
/// (`LACE_RELEASE_URL`), o arquivo `latest` dele, com a versão numa linha,
/// se houver; sem ele, a do GitHub.
pub fn latest_lace_version() -> anyhow::Result<String> {
    if let Some(base) = mirror()? {
        let url = format!("{base}/latest");
        match fetch_text(&url) {
            Ok(text) if !text.trim().is_empty() => {
                let version = text.trim().trim_start_matches('v');
                if !is_version(version) {
                    bail!("{url} does not hold a Lace version");
                }
                return Ok(version.to_owned());
            }
            _ => tracing::info!("No {url}: the latest version comes from GitHub"),
        }
    }
    let tag = latest_github_tag(&repo())?;
    let version = tag.trim_start_matches('v');
    if !is_version(version) {
        bail!(
            "The latest release of {} is not tagged with a Lace version",
            repo()
        );
    }
    Ok(version.to_owned())
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
        download_to(&asset_url(version, "SHA256SUMS")?, &sums, false)?;
    }
    let url = asset_url(version, file)?;
    let dest = dir.join(file);
    download_to(&url, &dest, progress)?;
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
        download_to(&asset_url(version, "SHA256SUMS")?, &sums, false)?;
    }
    let url = asset_url(version, file)?;
    let dest = dir.join(file);
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
/// grande numa rede lenta leva o tempo que leva. Só por `https` (e `file`,
/// o espelho no disco de [`mirror`]), também nos redirecionamentos: um
/// redirecionamento para `http` não troca o que o Lace baixa no caminho.
fn curl() -> std::process::Command {
    let mut command = std::process::Command::new(curl_program());
    command.args([
        "--connect-timeout",
        "20",
        "--proto",
        "=https,file",
        "--proto-redir",
        "=https",
    ]);
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
    fn versions_and_file_names_cannot_leave_the_release() {
        for ok in ["0.8.0", "0.9.0-rc.1", "1.0.0+build.5", "2026.10.13"] {
            assert!(is_version(ok), "{ok}");
        }
        for bad in [
            "",
            "v0.8.0",
            "0.8.0/../../x",
            "0.8\x1b[2J",
            "0.8.0 x",
            "..",
            "0.8.0\n",
        ] {
            assert!(!is_version(bad), "{bad:?}");
        }
        for ok in [
            "SHA256SUMS",
            "lace-0.8.0-windows-x64-index.json",
            "lace-0.8.0_x.tar.gz",
        ] {
            assert!(is_asset_name(ok), "{ok}");
        }
        for bad in ["", "../x", "a/b", "a\\b", ".hidden", "x\x1b]0;t\x07"] {
            assert!(!is_asset_name(bad), "{bad:?}");
        }
        assert!(asset_url("0.8.0/../..", "SHA256SUMS").is_err());
        assert!(asset_url("0.8.0", "../lace.exe").is_err());
    }

    #[test]
    fn a_mirror_is_https_or_on_disk() {
        assert_eq!(mirror_from(None).unwrap(), None);
        assert_eq!(mirror_from(Some("")).unwrap(), None);
        assert_eq!(
            mirror_from(Some("https://lab.example/lace/"))
                .unwrap()
                .as_deref(),
            Some("https://lab.example/lace")
        );
        assert!(mirror_from(Some("HTTPS://lab.example")).unwrap().is_some());
        assert!(mirror_from(Some("file:///srv/lace")).unwrap().is_some());
        for bad in [
            "http://lab.example/lace",
            "ftp://lab.example",
            "lab.example/lace",
        ] {
            assert!(mirror_from(Some(bad)).is_err(), "{bad}");
        }
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
