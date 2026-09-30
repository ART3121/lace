# Solar

Orquestrador das ferramentas de desenvolvimento do SAPHO, o processador
soft-core do NIPS-CERN (UFJF). Cria projetos, compila C± e C com o YANC,
simula com Icarus Verilog e Verilator, sintetiza com Yosys, desenha o
esquemático com Graphviz e abre as ondas no surfer-aurora. Substitui a
orquestração da AURORA.

O Solar só executa as ferramentas do bundle instalado com ele, nas versões
exatas do bundle (ver [docs/BUNDLE.md](docs/BUNDLE.md)). A exceção é o
Verilator, que compila com o compilador C++, o `make` e o Perl do sistema.

Plataformas: Linux x64, macOS Apple Silicon, Windows 10 e 11 x64.

## Instalar com curl

Os comandos abaixo baixam o instalador da release `v0.1.0`. Para outra
versão, troque `VERSAO`.

### Linux

```sh
VERSAO=0.1.0
URL=https://github.com/ART3121/solar/releases/download/v$VERSAO

curl -fLO "$URL/solar-$VERSAO-linux-x64.tar.gz"
curl -fLO "$URL/SHA256SUMS"
grep "solar-$VERSAO-linux-x64.tar.gz" SHA256SUMS | sha256sum -c
tar xzf "solar-$VERSAO-linux-x64.tar.gz"
./solar-$VERSAO-linux-x64/install
```

O `install` abre a instalação guiada no terminal: tipo **Recomendada** (o
padrão) ou **Avançada**, para escolher os componentes. Instala em
`~/.local/share/solar`, com o atalho `~/.local/bin/solar`; com `sudo`, em
`/opt/solar`, com o atalho `/usr/local/bin/solar`.

### macOS (Apple Silicon)

```sh
VERSAO=0.1.0
URL=https://github.com/ART3121/solar/releases/download/v$VERSAO

curl -fLO "$URL/solar-$VERSAO-darwin-arm64.tar.gz"
curl -fLO "$URL/SHA256SUMS"
grep "solar-$VERSAO-darwin-arm64.tar.gz" SHA256SUMS | shasum -a 256 -c
tar xzf "solar-$VERSAO-darwin-arm64.tar.gz"
./solar-$VERSAO-darwin-arm64/install
```

Baixe pelo `curl`, não pelo navegador: o instalador não é assinado, e o
arquivo baixado pelo navegador recebe a marca de quarentena, que faz o macOS
recusar o `install` (ver [docs/INSTALL.md](docs/INSTALL.md)). O
`~/.local/bin` não está no `PATH` do macOS por padrão; o instalador mostra a
linha para acrescentar ao `~/.zshrc`.

### Windows

No PowerShell (o `curl.exe` vem com o Windows 10 e 11):

```powershell
$VERSAO = "0.1.0"
$URL = "https://github.com/ART3121/solar/releases/download/v$VERSAO"

curl.exe -fLO "$URL/solar-$VERSAO-windows-x64-setup.exe"
curl.exe -fLO "$URL/SHA256SUMS"
(Get-FileHash "solar-$VERSAO-windows-x64-setup.exe" -Algorithm SHA256).Hash.ToLower()
Select-String "windows-x64" SHA256SUMS
.\solar-$VERSAO-windows-x64-setup.exe
```

Os dois hashes impressos têm que ser iguais. O assistente oferece os mesmos
tipos, Recomendada e Avançada, e a opção de pôr o Solar no `PATH`.

### Sem perguntas (scripts, laboratórios, CI)

Linux e macOS, depois de extrair:

```sh
./solar-$VERSAO-linux-x64/install --list                                   # componentes e tamanhos
./solar-$VERSAO-linux-x64/install --yes                                    # Recomendada
./solar-$VERSAO-linux-x64/install --yes --components yanc,icarus,verilator # Avançada
./solar-$VERSAO-linux-x64/install --yes --prefix /opt/solar --no-link
```

Windows:

```powershell
.\solar-$VERSAO-windows-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=recomendada /TASKS=path
.\solar-$VERSAO-windows-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=avancada /COMPONENTS="solar,yanc,icarus,verilator"
```

### Componentes

| Componente | O que é | Recomendada |
|---|---|---|
| YANC | compiladores C± e C do SAPHO e a biblioteca SAPHO | sim |
| Icarus Verilog | simulador Verilog, o padrão da AURORA | sim |
| Verilator | simulador compilado; precisa de `g++` ou `clang++`, `make` e Perl do sistema | não |
| Yosys | síntese e verificação de sintaxe | sim |
| Graphviz (dot) | desenho do esquemático; precisa do Yosys | sim |
| surfer-aurora | visualizador de formas de onda | sim |

Detalhes, remoção e problemas conhecidos: [docs/INSTALL.md](docs/INSTALL.md).

## Depois de instalar

```sh
solar tools --verify                   # o bundle, as versões, e a conferência dos hashes
solar new meu_projeto
solar -C meu_projeto proc add filtro   # cria filtro/Software/filtro.cmm
solar -C meu_projeto build             # C± -> Verilog, memórias e testbench, pelo YANC
solar -C meu_projeto sim -p filtro     # simula o processador com o Icarus
```

A saída da simulação fica em `meu_projeto/filtro/Simulation/output_0.txt`.
`solar sim` sem `-p` simula o testbench do projeto, que é preciso registrar
antes (`solar file testbench`).

A referência da linha de comando está em [docs/CLI.md](docs/CLI.md).

## Documentação

| Documento | Para quê |
|---|---|
| [docs/INSTALL.md](docs/INSTALL.md) | instalar, mudar componentes, remover |
| [docs/CLI.md](docs/CLI.md) | a linha de comando `solar` |
| [docs/API.md](docs/API.md) | a API do `solar-core`, para a CLI e para interfaces futuras |
| [docs/BUNDLE.md](docs/BUNDLE.md) | o que vem no bundle, como montá-lo e o que vem do sistema |
| [docs/RELEASE.md](docs/RELEASE.md) | como fazer uma release |
| [CHANGELOG.md](CHANGELOG.md) | mudanças por versão |

## Compilar do fonte

```sh
cargo build --release
python3 scripts/bundle.py --out dist/toolchain     # o bundle de ferramentas
target/release/solar tools --toolchain dist/toolchain --verify
```

O que montar o bundle exige está em [docs/BUNDLE.md](docs/BUNDLE.md).

NIPS-CERN, Núcleo de Instrumentação e Processamento de Sinais, Faculdade de
Engenharia da UFJF. <https://nipscern.com>
