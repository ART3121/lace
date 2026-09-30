# Fazer uma release

## Pelo GitHub

1. Subir a versão em `Cargo.toml` (`[workspace.package] version`).
2. Acrescentar a seção `## <versão> (<data>)` no `CHANGELOG.md`.
   Trocar a versão nos comandos manuais do `README.md` e do
   `docs/INSTALL.md` (os que baixam `solar-<versão>-...` direto). O
   `install.sh` e o `install.ps1` não mudam: eles pegam a última release.
3. Commit, tag e push da tag:

   ```
   git tag v0.1.0
   git push origin v0.1.0
   ```

4. O `release.yml` confere que a tag é a versão do `Cargo.toml` e que o
   CHANGELOG tem a seção, e chama o `installers.yml` nas três plataformas:
   monta o bundle, monta o instalador, instala por ele, roda todos os testes
   contra a instalação, instala a Recomendada, confere e desinstala.
5. Com tudo verde, ele cria um **rascunho** de release com os três
   instaladores, o `SHA256SUMS` e as notas do CHANGELOG. Revisar e publicar o
   rascunho é manual.

O `installers.yml` também roda a cada push (pelo `ci.yml`), e os instaladores
ficam como artefatos do workflow por 14 dias.

O `install.sh` e o `install.ps1` são servidos da `main` pelo
`raw.githubusercontent.com`: uma mudança neles vale para todo mundo assim que
chega na `main`, sem release.

## Localmente

Linux ou macOS:

```
python3 scripts/bundle.py --out dist/toolchain
cargo build --release -p solar-cli -p solar-installer
target/release/solar-pack tui --toolchain dist/toolchain \
  --solar target/release/solar --installer target/release/solar-installer \
  --out dist/solar-0.1.0-linux-x64 --tar-gz
```

Windows (bundle montado no shell MINGW64 do MSYS2, o resto em PowerShell):

```
python scripts/bundle.py --out dist/toolchain
cargo build --release -p solar-cli -p solar-installer
target\release\solar-pack.exe inno --toolchain dist\toolchain --solar target\release\solar.exe --out dist\inno
& "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" /DStage=$PWD\dist\inno /DSolarVersion=0.1.0 /DBundle=2026.09.29 /O$PWD\dist installer\windows\solar.iss
```

O `solar-pack inno` roda em qualquer sistema; só o `ISCC` precisa do Windows
(ou do Wine).

## Como os instaladores são feitos

- `scripts/bundle.py` monta o bundle e grava `<bundle>.contents.json`: que
  arquivo é de que componente (`bundle/components.json` diz o que cada
  componente leva).
- `solar-pack` (crate `solar-installer`) agrupa os arquivos pelo conjunto
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
do Graphviz 16.1.0 trava quando o ambiente não tem `LANG`, e o Solar roda as
ferramentas com ambiente limpo: o esquemático não sai no Wine. Num Windows
de verdade o `LANG` normalmente nem existe; quem confirma que o esquemático
funciona lá é o teste do `installers.yml` no `windows-2022`.

## O que falta

- Assinar o instalador de Windows (Authenticode) e assinar e notarizar o de
  macOS. Sem isso, o SmartScreen e o Gatekeeper avisam na primeira execução
  (ver [INSTALL.md](INSTALL.md)).
