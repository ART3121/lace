# Fazer uma release

## Pelo GitHub

1. Subir a versão nos seis lugares que a guardam, todos iguais: o
   `Cargo.toml` (`[workspace.package] version`), o `studio/package.json` e o
   `studio/package-lock.json` (o `npm version <versão> --no-git-tag-version`
   em `studio/` troca os dois), o `studio/src-tauri/Cargo.toml`, o
   `studio/src-tauri/tauri.conf.json` e os pacotes `studio` e `lace-learn`
   de `bundle/versions.json`. O `bundle.py` recusa o Studio com versões
   diferentes, e o `release.yml` recusa a tag se o Studio não for a versão
   do Lace.
2. Acrescentar a seção `## <versão> (<data>)` no `CHANGELOG.md`.
   Trocar a versão nos comandos manuais do `README.md` e do
   `docs/INSTALL.md` (os que baixam `lace-<versão>-...` direto). O
   `install.sh` e o `install.ps1` não mudam: eles pegam a última release.
3. O bloco de Windows tem que estar publicado: o pacote `msys` de
   `bundle/versions.json` aponta para uma release do lace-toolchain, com os
   dois SHA-256 preenchidos (do `SHA256SUMS` dela). O surfer-aurora também:
   se o commit dele mudou, rode antes o `gh workflow run surfer-aurora.yml` e
   ponha as URLs e os SHA-256 do resumo no `prebuilt` dele (sem isso, a
   release compila o surfer-aurora, uns 14 minutos a mais).
4. Mandar numa branch, não na `main`: o `install.sh` e o `install.ps1` saem
   da `main` e procuram na última release os arquivos com os nomes da
   versão deles; na `main` antes da release publicada, eles procuram a
   versão nova numa release que ainda é a antiga. Não há CI a cada push:
   quem monta a release é o `release.yml`, na tag. A tag vai no commit da
   branch:

   ```
   git tag v0.2.0
   git push origin v0.2.0
   ```

5. O `release.yml` confere que a tag é a versão do `Cargo.toml` e do Studio
   e que o CHANGELOG tem a seção, e chama o `installers.yml` nas três
   plataformas: monta o bundle e o instalador, sem testes.
6. Ele publica a release com os três instaladores, os pedaços do bundle, o
   `SHA256SUMS` e as notas do CHANGELOG. Os instaladores publicados se
   testam à mão.
7. A branch vai para a `main`.

Os instaladores de cada execução do `release.yml` também ficam como
artefatos do workflow por 14 dias.

O `install.sh` e o `install.ps1` são servidos da `main` pelo
`raw.githubusercontent.com`: uma mudança neles vale para todo mundo assim que
chega na `main`, sem release.

Além dos instaladores, a release publica o bundle em pedaços, para o
`lace install` baixar só os aplicativos que faltam: para cada plataforma, o
índice (`lace-<versão>-<plataforma>-index.json`) e cada pedaço
(`lace-<versão>-<plataforma>-c03.tar.zst`), que o `lace-pack tui --assets`
grava e o `installers.yml` sobe com os instaladores.

Uma release publicada não se apaga nem se renomeia: o `lace install` de
cada versão baixa desses arquivos da própria versão, conferidos pelo
`SHA256SUMS` dela, e o `lace update` baixa o instalador da versão nova pelos
nomes `lace-<versão>-<plataforma>.tar.gz` e
`lace-<versão>-windows-x64-setup.exe`. Sem esses arquivos, quem tem aquela
versão instalada não consegue mais instalar aplicativos nem atualizar.

## Localmente

Linux ou macOS (o Studio pede o Node.js e, no Linux, o
`libwebkit2gtk-4.1-dev`; `--only` sem `studio` monta sem ele):

```
python3 scripts/bundle.py --out dist/toolchain
cargo build --release -p lace-cli -p lace-installer
target/release/lace-pack tui --toolchain dist/toolchain \
  --lace target/release/lace --installer target/release/lace-installer \
  --out dist/lace-0.2.0-linux-x64 --tar-gz
```

Windows (bundle montado no shell MINGW64 do MSYS2, que compila o YANC; o
resto em PowerShell). O pacote msys de `bundle/versions.json` precisa
apontar para uma release publicada do lace-toolchain, com os dois hashes
([BUNDLE.md](BUNDLE.md), seção 6):

```
python scripts/bundle.py --out dist/toolchain
cargo build --release -p lace-cli -p lace-installer
target\release\lace-pack.exe inno --toolchain dist\toolchain --lace target\release\lace.exe --out dist\inno
& "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" /DStage=$PWD\dist\inno /DLaceVersion=0.2.0 /DBundle=2026.09.29 /O$PWD\dist installer\windows\lace.iss
```

O `lace-pack inno` roda em qualquer sistema; só o `ISCC` precisa do Windows
(ou do Wine).

## Como os instaladores são feitos

- `scripts/bundle.py` monta o bundle e grava `<bundle>.contents.json`: que
  arquivo é de que componente (`bundle/components.json` diz o que cada
  componente leva).
- `lace-pack` (crate `lace-installer`) agrupa os arquivos pelo conjunto
  exato de componentes que os usa. Cada grupo vira um pedaço `.tar.zst` do
  payload da TUI, ou uma entrada `[Files]` do Inno Setup com
  `Components: a or b`.
- A TUI (`install`) e o assistente só extraem os grupos que a seleção usa.

| Ferramenta | Versão fixada | Onde |
|---|---|---|
| Inno Setup | 6.7.3 | `INNO_VERSION` no `installers.yml`; instalador em `github.com/jrsoftware/issrc/releases`, SHA-256 `9c73c3bae7ed48d44112a0f48e66742c00090bdb5bef71d9d3c056c66e97b732` |
| zstd (pedaços da TUI) | crate `zstd` 0.14 | `Cargo.toml` |

## Testar sem Windows

O instalador de Windows compila e instala no Wine. No Wine 11, o `dot.exe`
do Graphviz 16.1.0 trava quando o ambiente não tem `LANG`, e o Lace roda as
ferramentas com ambiente limpo: o esquemático não sai no Wine. Num Windows
de verdade o `LANG` normalmente nem existe; quem confirma que o esquemático
funciona lá é o teste do `installers.yml` no `windows-2022`.

## O que falta

- Assinar o instalador de Windows (Authenticode) e assinar e notarizar o de
  macOS. Sem isso, o SmartScreen e o Gatekeeper avisam na primeira execução
  (ver [INSTALL.md](INSTALL.md)).
