# Instalar o Solar

Cada release traz um instalador por plataforma. O instalador põe o Solar e o
bundle de ferramentas (as versões exatas, ver [BUNDLE.md](BUNDLE.md)) numa
pasta só, e deixa escolher quais ferramentas do bundle instalar.

| Plataforma | Arquivo | Instalador |
|---|---|---|
| Windows 10 e 11, x64 | `solar-<versão>-windows-x64-setup.exe` | assistente (Inno Setup) |
| Linux x64 | `solar-<versão>-linux-x64.tar.gz` | no terminal (TUI) |
| macOS, Apple Silicon | `solar-<versão>-darwin-arm64.tar.gz` | no terminal (TUI) |

O `SHA256SUMS` da release tem o hash de cada arquivo.

## Tipos de instalação e componentes

Os dois instaladores oferecem os mesmos tipos:

- **Recomendada** (o padrão): YANC, Icarus Verilog, Yosys, Graphviz e
  surfer-aurora. É o que a AURORA usa no dia a dia.
- **Avançada**: a lista de componentes, para marcar exatamente o que
  instalar.

| Componente | O que é | Recomendada |
|---|---|---|
| Solar | a linha de comando; sempre instalado | sim |
| YANC | compiladores C± e C do SAPHO e a biblioteca SAPHO | sim |
| Icarus Verilog | simulador Verilog, o padrão da AURORA | sim |
| Verilator | simulador compilado; precisa de compilador C++, `make` e Perl do sistema | não |
| Yosys | síntese e verificação de sintaxe | sim |
| Graphviz (dot) | desenho do esquemático; precisa do Yosys | sim |
| surfer-aurora | visualizador de formas de onda (o fork do Surfer da AURORA) | sim |

O Verilator fica fora da Recomendada porque só funciona com o compilador do
sistema instalado (ver [BUNDLE.md](BUNDLE.md), seção 4). Marcar o Graphviz
marca o Yosys; desmarcar o Yosys desmarca o Graphviz.

Uma ferramenta de componente não instalado dá erro claro quando usada:

```
erro: o componente verilator não está instalado (rode o instalador do Solar de novo e escolha-o)
```

`solar tools` mostra o que está instalado e o que não está.

Tamanho instalado no Linux, de referência: 300 MiB com tudo, 209 MiB na
Recomendada; o instalador (`.tar.gz`) tem 81 MB. Icarus, Verilator, Yosys e Graphviz dividem bibliotecas
do OSS CAD Suite, então a soma dos tamanhos de cada um é maior que o total.
`./install --list` e a página de componentes do assistente mostram os
números da plataforma.

## Windows

1. Rode `solar-<versão>-windows-x64-setup.exe`.
2. Escolha instalar só para você (padrão, sem administrador, em
   `%LOCALAPPDATA%\Programs\Solar`) ou para todos os usuários (pede
   administrador, em `C:\Solar`).
3. Na página de componentes, fique na **Recomendada** ou troque para
   **Avançada** e marque os componentes.
4. Deixe marcada a tarefa **Adicionar o Solar ao PATH** para usar `solar` em
   qualquer terminal novo.

A pasta padrão não tem espaço no caminho de propósito: o `make` usado pelo
Verilator não aceita espaços, e `C:\Program Files` tem um. O assistente avisa
se a pasta escolhida tiver.

O Verilator no Windows usa o MSYS2 em `C:\msys64` (ou `C:\tools\msys64`).
Se o MSYS2 não estiver lá, o assistente avisa, e o Verilator só simula
depois que ele for instalado:

```
pacman -S make perl mingw-w64-ucrt-x86_64-gcc
```

**Mudar os componentes:** rode o instalador de novo. Ele lembra a pasta e a
seleção anteriores; o que for desmarcado é removido.

**Remover:** Configurações > Aplicativos > Solar, ou
`<pasta>\unins000.exe`. Remove também a entrada do PATH.

**Sem interação** (scripts, laboratórios):

```
solar-0.1.0-windows-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=recomendada /TASKS=path
solar-0.1.0-windows-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=avancada /COMPONENTS="solar,yanc,icarus,verilator" /DIR=D:\Solar
```

Nos parâmetros do Inno Setup, os nomes são `solar`, `yanc`, `icarus`,
`verilator`, `yosys`, `yosys\graphviz` e `surfer_aurora`.

O instalador não é assinado: o SmartScreen pode avisar "Editor
desconhecido" na primeira vez (Mais informações > Executar assim mesmo).

## Linux e macOS

```
tar xzf solar-0.1.0-linux-x64.tar.gz
./solar-0.1.0-linux-x64/install
```

A instalação guiada no terminal tem estas telas:

1. boas-vindas (avisa se já há uma instalação na pasta padrão);
2. tipo de instalação: **Recomendada** ou **Avançada**;
3. componentes, só na Avançada: `Espaço` marca e desmarca; o Verilator mostra
   se o compilador do sistema foi encontrado;
4. pasta da instalação e atalho;
5. resumo;
6. progresso e resultado, com a conferência dos hashes dos executáveis.

`Esc` volta uma tela, `Ctrl+C` sai sem mudar nada.

| | Usuário comum | root (`sudo ./install`) |
|---|---|---|
| Pasta | `~/.local/share/solar` | `/opt/solar` |
| Atalho | `~/.local/bin/solar` | `/usr/local/bin/solar` |

Se a pasta do atalho não estiver no `PATH`, o instalador diz a linha para
pôr no `~/.bashrc` ou `~/.zshrc`. No macOS, `~/.local/bin` não está no `PATH`
por padrão.

O instalador só escreve numa pasta nova, vazia ou com uma instalação do
Solar. Ele extrai tudo numa pasta provisória, confere o hash de cada
executável com o próprio Solar e só então troca a instalação anterior: uma
falha no meio não estraga o que já estava instalado.

**Sem interação:**

```
./install --list                                   # componentes e tamanhos
./install --yes                                    # Recomendada
./install --yes --components yanc,icarus,verilator # Avançada
./install --yes --prefix /opt/solar --no-link
```

Um componente que exige outro traz o outro junto (`--components graphviz`
instala também o Yosys, e avisa).

**Mudar os componentes:** rode o instalador de novo na mesma pasta; a
instalação é trocada pela nova seleção.

**Remover:** `<pasta>/uninstall.sh`. Remove a pasta e o atalho.

**Verilator:** precisa de `g++` (ou `clang++`), `make` e `perl` do sistema:
`sudo apt install build-essential perl` (Debian, Ubuntu),
`sudo dnf install gcc-c++ make perl` (Fedora), `xcode-select --install`
(macOS).

**macOS e o Gatekeeper:** o instalador não é assinado nem notarizado. Um
`.tar.gz` baixado pelo navegador recebe a marca de quarentena, e o macOS
recusa abrir o `install` ("desenvolvedor não pode ser verificado"). Baixar
com `curl -LO` não põe a marca; se já foi baixado pelo navegador:

```
xattr -dr com.apple.quarantine solar-0.1.0-darwin-arm64
```

Isto não foi verificado num Mac: é o comportamento documentado do
Gatekeeper para binários sem assinatura. Os arquivos que o instalador extrai
não recebem a marca.

## Depois de instalar

```
solar tools --verify
solar new meu_projeto
```

## O que fica na pasta

```
<pasta>/
  bin/solar[.exe]
  toolchain/            o bundle, só com os componentes escolhidos
    bundle.json
    components/<nome>.json
    yanc/ oss-cad-suite/ surfer-aurora/ graphviz/
  install.json          (Linux, macOS) o que foi instalado
  uninstall.sh          (Linux, macOS)
  unins000.exe          (Windows)
```
