# Instala a última release do Solar no Windows 10 e 11 (x64).
#
#   irm https://raw.githubusercontent.com/ART3121/solar/main/install.ps1 | iex
#
# Baixa o assistente de instalação da release, confere o SHA-256 com o
# SHA256SUMS da release e o abre. Variáveis opcionais:
#   $env:SOLAR_VERSION = "0.1.0"          versão fixa, em vez da última
#   $env:SOLAR_SETUP_ARGS = "/VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=recomendada /TASKS=path"
#                                          instalação sem perguntas

& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'   # a barra de progresso deixa o download lento no PowerShell 5.1
    # O PowerShell 5.1 do Windows 10 pode não ligar o TLS 1.2, que o GitHub exige.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $repo = if ($env:SOLAR_REPO) { $env:SOLAR_REPO } else { 'ART3121/solar' }
    $version = $env:SOLAR_VERSION
    if (-not $version) {
        $release = Invoke-RestMethod -UseBasicParsing "https://api.github.com/repos/$repo/releases/latest"
        $version = $release.tag_name.TrimStart('v')
    }

    $name = "solar-$version-windows-x64-setup.exe"
    $base = "https://github.com/$repo/releases/download/v$version"
    $dir = Join-Path ([IO.Path]::GetTempPath()) "solar-$version"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $setup = Join-Path $dir $name
    $sums = Join-Path $dir 'SHA256SUMS'

    Write-Host "Baixando o Solar $version (windows-x64)"
    Invoke-WebRequest -UseBasicParsing -Uri "$base/$name" -OutFile $setup
    Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile $sums

    $line = Get-Content $sums | Where-Object { $_ -match ('\s' + [regex]::Escape($name) + '$') } | Select-Object -First 1
    if (-not $line) { throw "o SHA256SUMS da release não lista $name" }
    $expected = ($line -split '\s+')[0].ToLower()
    $actual = (Get-FileHash -Algorithm SHA256 -Path $setup).Hash.ToLower()
    if ($actual -ne $expected) { throw "o SHA-256 de $name não confere com o SHA256SUMS da release" }

    if ($env:SOLAR_SETUP_ARGS) {
        $p = Start-Process -FilePath $setup -ArgumentList $env:SOLAR_SETUP_ARGS -Wait -PassThru
    } else {
        $p = Start-Process -FilePath $setup -Wait -PassThru
    }
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
    if ($p.ExitCode -ne 0) { throw "o instalador saiu com o código $($p.ExitCode)" }
    Write-Host "Pronto. Abra um terminal novo e rode: solar tools --verify"
}
