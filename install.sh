#!/bin/sh
# Instala a última release do Solar (Linux x64, macOS Apple Silicon).
#
#   curl -fsSL https://raw.githubusercontent.com/ART3121/solar/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/ART3121/solar/main/install.sh | sh -s -- --yes
#
# Baixa o instalador da release, confere o SHA-256 com o SHA256SUMS da
# release, extrai numa pasta provisória e roda o `install`. Os argumentos
# depois de `sh -s --` vão para o `install` (--yes, --components, --prefix,
# --no-link, --list). SOLAR_VERSION=0.1.0 fixa a versão.

set -eu

repo="${SOLAR_REPO:-ART3121/solar}"

die() {
    echo "erro: $*" >&2
    exit 1
}

case "$(uname -s) $(uname -m)" in
    "Linux x86_64") platform=linux-x64 ;;
    "Darwin arm64") platform=darwin-arm64 ;;
    *) die "não há instalador do Solar para $(uname -s) $(uname -m) (só Linux x64 e macOS Apple Silicon; no Windows, use o install.ps1)" ;;
esac

command -v curl >/dev/null 2>&1 || die "precisa do curl"
command -v tar >/dev/null 2>&1 || die "precisa do tar"
if command -v sha256sum >/dev/null 2>&1; then
    sha256="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
    sha256="shasum -a 256"
else
    die "precisa do sha256sum ou do shasum para conferir o download"
fi

# A última versão: /releases/latest redireciona para /releases/tag/v<versão>.
version="${SOLAR_VERSION:-}"
if [ -z "$version" ]; then
    latest=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest") ||
        die "não consegui consultar https://github.com/$repo/releases"
    version=${latest##*/v}
fi
case "$version" in
    "" | */*) die "não achei a última versão em https://github.com/$repo/releases" ;;
esac

name="solar-$version-$platform"
base="https://github.com/$repo/releases/download/v$version"

# Fora do /tmp: há distribuições que o montam sem permissão de execução.
# Como root (`| sudo sh`), /var/tmp: o sudo pode manter o HOME do usuário, e
# um ~/.cache criado pelo root ficaria no caminho dele.
if [ "$(id -u)" -eq 0 ]; then
    cache=/var/tmp
else
    cache="${XDG_CACHE_HOME:-$HOME/.cache}"
fi
mkdir -p "$cache"
tmp=$(mktemp -d "$cache/solar-install.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
trap 'exit 130' INT TERM

echo "Baixando o Solar $version ($platform)"
curl -fL --progress-bar -o "$tmp/$name.tar.gz" "$base/$name.tar.gz" ||
    die "não consegui baixar $base/$name.tar.gz"
curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS" ||
    die "não consegui baixar $base/SHA256SUMS"
(
    cd "$tmp"
    grep " $name.tar.gz\$" SHA256SUMS >"$name.sha256" || die "o SHA256SUMS da release não lista $name.tar.gz"
    $sha256 -c "$name.sha256" >/dev/null 2>&1 || die "o SHA-256 de $name.tar.gz não confere com o SHA256SUMS da release"
)
tar xzf "$tmp/$name.tar.gz" -C "$tmp"

# Com `curl | sh`, a entrada padrão é o pipe: a instalação guiada lê o
# teclado pelo terminal. Sem terminal, o install pede --yes.
if [ -t 0 ]; then
    "$tmp/$name/install" "$@"
elif (: </dev/tty) 2>/dev/null; then
    "$tmp/$name/install" "$@" </dev/tty
else
    "$tmp/$name/install" "$@"
fi
