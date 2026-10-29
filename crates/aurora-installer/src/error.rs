//! Os erros da biblioteca que atravessam a fronteira: os casos em que a
//! CLI, o Studio e a extensão decidem algo diferente ganham variante e
//! código próprios; o resto vira [`Error::Other`], com a mensagem.

use camino::Utf8PathBuf;

/// Um erro de instalar, atualizar ou desinstalar.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// O `aurora` não veio do instalador: não há instalação para mexer.
    #[error(
        "This aurora ({exe}) was not installed by the installer, so there is no installation here"
    )]
    NoInstallation {
        /// O executável (ou a pasta) examinado.
        exe: Utf8PathBuf,
    },
    /// Outra instalação ou atualização está mexendo na mesma pasta.
    #[error("Another installation or update of AURORA is running in {prefix}; wait for it to finish")]
    InstallInProgress {
        /// A pasta da instalação.
        prefix: Utf8PathBuf,
    },
    /// Uma atualização interrompida foi desfeita agora: quem roda pode ser o
    /// `aurora` novo, que a troca já tinha posto no lugar, e a operação para.
    #[error(
        "An update to AURORA {to} stopped in the middle, and its changes were undone: the \
         installation in {prefix} is back to the version it had. Run aurora update again"
    )]
    InterruptedUpdateUndone {
        /// A versão que estava entrando.
        to: String,
        /// A pasta da instalação.
        prefix: Utf8PathBuf,
    },
    /// A rede falhou (um download, uma consulta de versão).
    #[error("{reason}")]
    Network {
        /// A URL, quando há uma.
        url: Option<String>,
        /// O que aconteceu.
        reason: String,
    },
    /// O `curl` não está no sistema.
    #[error("AURORA downloads with curl, which was not found; install curl")]
    CurlMissing,
    /// A última versão da AURORA não pôde ser consultada.
    #[error("Could not get the latest AURORA version (no network, or GitHub is down)")]
    NoLatestVersion,
    /// Um arquivo baixado não confere com o `SHA256SUMS` da release.
    #[error("The SHA-256 of {file} does not match the SHA256SUMS of release v{version}")]
    ChecksumMismatch {
        /// O arquivo.
        file: String,
        /// A release.
        version: String,
    },
    /// O instalador inteiro (Linux, macOS) saiu com erro.
    #[error(
        "The installer exited with code {code}; if the installation is system-wide (/opt/aurora), run: sudo aurora update"
    )]
    InstallerFailed {
        /// O código de saída, ou `?`.
        code: String,
    },
    /// O assistente do Windows (instalador ou desinstalador) não abriu.
    #[error("{reason}")]
    WizardFailed {
        /// O que aconteceu.
        reason: String,
    },
    /// `AURORA_RELEASE_URL` não é `https://` nem `file://`.
    #[error(
        "AURORA_RELEASE_URL must start with https:// (or file:// for a mirror on disk), and it is \
         {base}: the SHA256SUMS that checks each download comes from there too"
    )]
    InvalidMirror {
        /// O valor recebido.
        base: String,
    },
    /// A operação parou no pedido de cancelamento, antes de mudar a
    /// instalação (ou com o que entrou já desfeito).
    #[error("The operation was cancelled before it finished")]
    Cancelled,
    /// Qualquer outro erro, com a mensagem.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Error {
    /// O código estável, em `snake_case`, para o JSON.
    pub fn code(&self) -> &'static str {
        match self {
            Error::NoInstallation { .. } => "no_installation",
            Error::InstallInProgress { .. } => "install_in_progress",
            Error::InterruptedUpdateUndone { .. } => "interrupted_update_undone",
            Error::Network { .. } => "network",
            Error::CurlMissing => "curl_missing",
            Error::NoLatestVersion => "no_latest_version",
            Error::ChecksumMismatch { .. } => "checksum_mismatch",
            Error::InstallerFailed { .. } => "installer_failed",
            Error::WizardFailed { .. } => "wizard_failed",
            Error::InvalidMirror { .. } => "invalid_mirror",
            Error::Cancelled => "cancelled",
            Error::Other(_) => "install",
        }
    }

    /// O erro é o cancelamento.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Error::Cancelled)
    }

    /// Um erro de rede com a URL.
    pub(crate) fn network(url: impl Into<String>, reason: impl Into<String>) -> Error {
        Error::Network {
            url: Some(url.into()),
            reason: reason.into(),
        }
    }
}

/// O resultado da biblioteca.
pub type Result<T> = std::result::Result<T, Error>;

/// Um `anyhow` que embrulhou um [`Error`] (um `with_context` no meio) volta a
/// ser ele; os outros viram [`Error::Other`].
pub(crate) fn from_anyhow(error: anyhow::Error) -> Error {
    match error.downcast::<Error>() {
        Ok(error) => error,
        Err(error) => Error::Other(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_snake_case_and_stable() {
        let errors = [
            Error::NoInstallation { exe: "x".into() },
            Error::InstallInProgress { prefix: "p".into() },
            Error::InterruptedUpdateUndone {
                to: "1".into(),
                prefix: "p".into(),
            },
            Error::network("u", "r"),
            Error::CurlMissing,
            Error::NoLatestVersion,
            Error::ChecksumMismatch {
                file: "f".into(),
                version: "1".into(),
            },
            Error::InstallerFailed { code: "1".into() },
            Error::WizardFailed { reason: "r".into() },
            Error::InvalidMirror { base: "b".into() },
            Error::Cancelled,
            Error::Other(anyhow::anyhow!("x")),
        ];
        let mut codes: Vec<&str> = errors.iter().map(Error::code).collect();
        for code in &codes {
            assert!(
                code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{code}"
            );
        }
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), errors.len(), "cada variante tem o seu");
    }

    #[test]
    fn a_wrapped_error_comes_back_as_itself() {
        let wrapped = anyhow::Error::new(Error::Cancelled).context("Downloading");
        assert!(from_anyhow(wrapped).is_cancelled());
        assert_eq!(from_anyhow(anyhow::anyhow!("x")).code(), "install");
    }
}
