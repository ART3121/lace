<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="studio/public/brand/lace-lockup-reverse.svg">
    <img src="studio/public/brand/lace-lockup.svg" alt="Lace" width="260">
  </picture>
</h1>

<p align="center">
  <a href="https://github.com/ART3121/lace/releases"><img src="https://img.shields.io/github/v/release/ART3121/lace?label=vers%C3%A3o&color=2e4374" alt="Versão"></a>
  <a href="https://nipscern.com"><img src="https://img.shields.io/badge/NIPS--CERN-UFJF-2e4374" alt="NIPS-CERN, UFJF"></a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Linux-x64-2e4374?logo=linux&logoColor=white" alt="Linux x64">
  <img src="https://img.shields.io/badge/macOS-Apple%20Silicon-2e4374?logo=apple&logoColor=white" alt="macOS Apple Silicon">
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011%20x64-2e4374" alt="Windows 10 e 11 x64">
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.89%2B-2e4374?logo=rust&logoColor=white" alt="Rust 1.89 ou mais novo">
  <img src="https://img.shields.io/badge/TypeScript-Studio-2e4374?logo=typescript&logoColor=white" alt="TypeScript, no Studio">
  <img src="https://img.shields.io/badge/Tauri-2-2e4374?logo=tauri&logoColor=white" alt="Tauri 2">
  <img src="https://img.shields.io/badge/HDL-Verilog-2e4374" alt="Verilog">
  <img src="https://img.shields.io/badge/SAPHO-C%C2%B1-2e4374" alt="SAPHO e C±">
  <img src="https://img.shields.io/badge/testbench-cocotb-2e4374?logo=python&logoColor=white" alt="Testbench em Python com o cocotb">
</p>

Orquestrador do desenvolvimento em Verilog e de processadores SAPHO, o
processador soft-core do NIPS-CERN (UFJF). Cria projetos, compila C± e C com
o YANC, verifica e simula com Icarus Verilog e Verilator, sintetiza com
Yosys, desenha o esquemático com Graphviz e abre as ondas no surfer-aurora.
Substitui a orquestração da AURORA.

Duas interfaces sobre o mesmo núcleo (`lace-core`): a linha de comando
`lace` e o [Lace Studio](studio/README.md), o ambiente gráfico (editor,
compilação, simulação e ondas numa janela), em `studio/`. O instalador
traz os dois.

O Lace só executa as ferramentas do bundle instalado com ele, nas versões
exatas do bundle (ver [docs/BUNDLE.md](docs/BUNDLE.md)). No Linux e no macOS
o bundle acompanha o OSS CAD Suite, e a exceção é o Verilator, que compila
com o compilador C++, o `make` e o Perl do sistema. No Windows o Icarus e o
Verilator, com o compilador, vêm do bloco MSYS2 do
[lace-toolchain](https://github.com/ART3121/lace-toolchain).

Plataformas: Linux x64 (glibc 2.35: Ubuntu 22.04, Debian 12, Fedora 36 ou
mais novos), macOS 13 ou mais novo em Apple Silicon, Windows 10 e 11 x64.

## Instalar

Linux x64 e macOS Apple Silicon, em qualquer shell (bash, zsh, fish):

```sh
curl -fsSL https://raw.githubusercontent.com/ART3121/lace/main/install.sh | sh
```

Windows 10 e 11, no PowerShell:

```powershell
irm https://raw.githubusercontent.com/ART3121/lace/main/install.ps1 | iex
```

O script baixa o instalador da última release, confere o SHA-256 com o
`SHA256SUMS` da release e o abre: a instalação guiada no terminal no Linux e
no macOS, o assistente no Windows. Nos dois, o tipo **Recommended** é o
padrão e o **Advanced** deixa escolher os componentes. No Linux e no macOS,
instala em `~/.local/share/lace`, com o atalho `~/.local/bin/lace`. O Lace
Studio entra no menu de aplicativos.

Sem perguntas (scripts, laboratórios, CI):

```sh
curl -fsSL https://raw.githubusercontent.com/ART3121/lace/main/install.sh | sh -s -- --yes
curl -fsSL https://raw.githubusercontent.com/ART3121/lace/main/install.sh | sh -s -- --yes --components yanc,icarus,verilator
```

```powershell
$env:LACE_SETUP_ARGS = "/VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=recomendada /TASKS=path"
irm https://raw.githubusercontent.com/ART3121/lace/main/install.ps1 | iex
```

Sem o script, direto da [página da release](https://github.com/ART3121/lace/releases/latest):

```sh
curl -fLO https://github.com/ART3121/lace/releases/download/v0.3.0/lace-0.3.0-linux-x64.tar.gz
tar xzf lace-0.3.0-linux-x64.tar.gz && ./lace-0.3.0-linux-x64/install
```

No macOS, troque `linux-x64` por `darwin-arm64`. Baixe pelo `curl`, não
pelo navegador: o instalador não é assinado, e o macOS recusa abrir um
arquivo baixado pelo navegador (ver [docs/INSTALL.md](docs/INSTALL.md)).

Para remover o Lace com o bundle inteiro: `lace uninstall`.

### Componentes

| Componente | O que é | Na instalação Recommended |
|---|---|---|
| Lace Studio | o ambiente gráfico; no Linux precisa do webkit2gtk 4.1 do sistema | sim |
| YANC | compiladores C± e C do SAPHO e a biblioteca SAPHO | sim |
| Icarus Verilog | simulador Verilog, o padrão da AURORA, e a verificação do `lace check` | sim |
| Verilator | simulador compilado e o `lace check --lint`; no Linux e no macOS precisa de `g++` ou `clang++`, `make` e Perl do sistema, no Windows vem com eles | não |
| cocotb | testbenches em Python, com o Python que os roda; precisa do Icarus. O Lace roda os testes no Icarus (`lace sim test_x.py`, ou Wave e Rápida no Studio) e, com o componente Verilator, no Verilator (`lace sim --verilator`) | não |
| Yosys | síntese; lê as portas para o testbench-modelo do `lace add` | sim |
| Graphviz (dot) | desenho do esquemático; precisa do Yosys | sim |
| surfer-aurora | visualizador de formas de onda | sim |

Para instalar um aplicativo do bundle depois, sem reinstalar o Lace:
`lace install`, que abre a lista no terminal, ou `lace install verilator`.
Para ver se há versão nova do Lace e das ferramentas: `lace update --check`.

Detalhes, remoção e problemas conhecidos: [docs/INSTALL.md](docs/INSTALL.md).
Quem tem o Solar 0.1.0 (o nome anterior do Lace) desinstala o Solar antes:
[docs/INSTALL.md, Quem tem o Solar 0.1.0](docs/INSTALL.md#quem-tem-o-solar-010).

## Depois de instalar

```sh
lace tools --verify        # o bundle, as versões e a conferência dos hashes
```

Um projeto do Lace é Verilog; processadores SAPHO são uma etapa a mais, que
o YANC compila antes de verificar, simular ou sintetizar. Verilog:

```sh
lace new contador
cd contador
lace add contador.v        # cria o módulo e o registra como topo
lace add contador_tb.v     # cria o testbench, que já instancia contador
lace check                 # Icarus: o design e cada testbench
lace hierarchy             # a árvore de instâncias do design e do testbench
lace sim                   # simula e mostra o que o testbench escreve, enquanto roda
lace wave                  # abre a onda no surfer-aurora
```

Um testbench sem `$finish` não termina sozinho: o Ctrl+C encerra a
simulação, e `lace sim --timeout 30` a encerra depois de 30 s.

Processador SAPHO:

```sh
lace new demo
cd demo
lace proc add filtro       # cria filtro/Software/filtro.cmm
lace build                 # C± -> Verilog, memórias e testbench, pelo YANC
lace sim -p filtro         # simula e mostra as saídas
lace wave -p filtro
```

Cada `build`, `check`, `sim` e `synth` grava um relatório no projeto
(`.lace/reports/`): a máquina, as ferramentas e versões, o tempo de cada
passo e, na síntese, as estatísticas do Yosys:

```sh
lace synth                 # sintetiza e grava o relatório
lace report                # o relatório mais novo
lace report list           # todos, do mais novo para o mais antigo
lace report compare        # o mais novo contra o anterior comparável
```

Os valores de entrada do processador ficam em
`filtro/Simulation/input_0.txt`, um por linha. Para mover ou renomear um
arquivo do projeto sem tirá-lo do `.spf`, use `lace move` (`lace move
contador.v rtl/`). Caminhos na linha de comando são relativos ao diretório
atual do shell. A referência da linha de comando,
com as regras de cada fluxo, está em [docs/CLI.md](docs/CLI.md).

Para outro programa (uma extensão de editor, um script), todo comando aceita
`--json`, um objeto no fim, e `--events`, um objeto JSON por linha enquanto
as ferramentas rodam. O formato de cada um está em
[docs/schema/](docs/schema/), gerado do código.

## Documentação

| Documento | Para quê |
|---|---|
| [docs/INSTALL.md](docs/INSTALL.md) | instalar, mudar componentes, remover |
| [studio/README.md](studio/README.md) | o Lace Studio: usar, compilar, a documentação dele (`studio/docs/`) |
| [docs/CLI.md](docs/CLI.md) | a linha de comando `lace` |
| [docs/API.md](docs/API.md) | a API do `lace-core`, para a CLI e para interfaces futuras |
| [docs/BUNDLE.md](docs/BUNDLE.md) | o que vem no bundle, como montá-lo e o que vem do sistema |
| [docs/schema/](docs/schema/) | o JSON Schema do `--json` e do `--events` de cada comando |
| [docs/RELEASE.md](docs/RELEASE.md) | como fazer uma release |
| [CONTRIBUTING.md](CONTRIBUTING.md) | como contribuir: preparar a máquina, onde mudar o quê, o que conferir |
| [CHANGELOG.md](CHANGELOG.md) | mudanças por versão |

## Compilar do fonte

```sh
cargo build --release
python3 scripts/bundle.py --out dist/toolchain     # o bundle de ferramentas, com o Studio
target/release/lace tools --toolchain dist/toolchain --verify
```

O Studio sozinho, para desenvolver: `cd studio && npm install && npm run tauri dev`
([studio/docs/DEVELOPMENT.md](studio/docs/DEVELOPMENT.md)).

O que montar o bundle exige está em [docs/BUNDLE.md](docs/BUNDLE.md).

NIPS-CERN, Núcleo de Instrumentação e Processamento de Sinais, Faculdade de
Engenharia da UFJF. <https://nipscern.com>
