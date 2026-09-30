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

## Instalar

Linux x64 e macOS Apple Silicon, em qualquer shell (bash, zsh, fish):

```sh
curl -fsSL https://raw.githubusercontent.com/ART3121/solar/main/install.sh | sh
```

Windows 10 e 11, no PowerShell:

```powershell
irm https://raw.githubusercontent.com/ART3121/solar/main/install.ps1 | iex
```

O script baixa o instalador da última release, confere o SHA-256 com o
`SHA256SUMS` da release e o abre: a instalação guiada no terminal no Linux e
no macOS, o assistente no Windows. Nos dois, o tipo **Recomendada** é o
padrão e o **Avançada** deixa escolher os componentes. No Linux e no macOS,
instala em `~/.local/share/solar`, com o atalho `~/.local/bin/solar`.

Sem perguntas (scripts, laboratórios, CI):

```sh
curl -fsSL https://raw.githubusercontent.com/ART3121/solar/main/install.sh | sh -s -- --yes
curl -fsSL https://raw.githubusercontent.com/ART3121/solar/main/install.sh | sh -s -- --yes --components yanc,icarus,verilator
```

```powershell
$env:SOLAR_SETUP_ARGS = "/VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=recomendada /TASKS=path"
irm https://raw.githubusercontent.com/ART3121/solar/main/install.ps1 | iex
```

Sem o script, direto da [página da release](https://github.com/ART3121/solar/releases/latest):

```sh
curl -fLO https://github.com/ART3121/solar/releases/download/v0.1.0/solar-0.1.0-linux-x64.tar.gz
tar xzf solar-0.1.0-linux-x64.tar.gz && ./solar-0.1.0-linux-x64/install
```

No macOS, troque `linux-x64` por `darwin-arm64`. Baixe pelo `curl`, não
pelo navegador: o instalador não é assinado, e o macOS recusa abrir um
arquivo baixado pelo navegador (ver [docs/INSTALL.md](docs/INSTALL.md)).

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
