# Instala a última release do Lace no Windows 10 e 11 (x64).
#
#   irm https://raw.githubusercontent.com/ART3121/lace/main/install.ps1 | iex
#
# Baixa o assistente de instalação da release, confere o SHA-256 com o
# SHA256SUMS da release e o abre. Variáveis opcionais:
#   $env:LACE_VERSION = "0.2.0"            versão fixa, em vez da última
#   $env:LACE_SETUP_ARGS = "/VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=recomendada /TASKS=path"
#                                          instalação sem perguntas

& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'   # a barra de progresso deixa o download lento no PowerShell 5.1
    # O PowerShell 5.1 do Windows 10 pode não ligar o TLS 1.2, que o GitHub exige.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $repo = if ($env:LACE_REPO) { $env:LACE_REPO } else { 'ART3121/lace' }
    $version = $env:LACE_VERSION
    if (-not $version) {
        $release = Invoke-RestMethod -UseBasicParsing "https://api.github.com/repos/$repo/releases/latest"
        $version = $release.tag_name.TrimStart('v')
    }

    $name = "lace-$version-windows-x64-setup.exe"
    $base = "https://github.com/$repo/releases/download/v$version"
    $dir = Join-Path ([IO.Path]::GetTempPath()) "lace-$version"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $setup = Join-Path $dir $name
    $sums = Join-Path $dir 'SHA256SUMS'

    Write-Host "Downloading Lace $version (windows-x64)"
    Invoke-WebRequest -UseBasicParsing -Uri "$base/$name" -OutFile $setup
    Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile $sums

    $line = Get-Content $sums | Where-Object { $_ -match ('\s' + [regex]::Escape($name) + '$') } | Select-Object -First 1
    if (-not $line) { throw "The release SHA256SUMS does not list $name" }
    $expected = ($line -split '\s+')[0].ToLower()
    $actual = (Get-FileHash -Algorithm SHA256 -Path $setup).Hash.ToLower()
    if ($actual -ne $expected) { throw "The SHA-256 of $name does not match the release SHA256SUMS" }

    if ($env:LACE_SETUP_ARGS) {
        $p = Start-Process -FilePath $setup -ArgumentList $env:LACE_SETUP_ARGS -Wait -PassThru
    } else {
        $p = Start-Process -FilePath $setup -Wait -PassThru
    }
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
    if ($p.ExitCode -ne 0) { throw "The installer exited with code $($p.ExitCode)" }
    Write-Host "Done. Open a new terminal and run: lace tools --verify"
}
