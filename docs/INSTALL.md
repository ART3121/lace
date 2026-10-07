# Instalar o Lace

Cada release traz um instalador por plataforma. O instalador põe o Lace, o
Lace Studio e o bundle de ferramentas (as versões exatas, ver
[BUNDLE.md](BUNDLE.md)) numa pasta só, e deixa escolher o que instalar: o
Studio e cada ferramenta do bundle são componentes, e só a linha de comando
é sempre instalada.

| Plataforma | Arquivo | Instalador |
|---|---|---|
| Windows 10 e 11, x64 | `lace-<versão>-windows-x64-setup.exe` | assistente (Inno Setup) |
| Linux x64 | `lace-<versão>-linux-x64.tar.gz` | no terminal (TUI) |
| macOS, Apple Silicon | `lace-<versão>-darwin-arm64.tar.gz` | no terminal (TUI) |

O `SHA256SUMS` da release tem o hash de cada arquivo.

## Requisitos

| Sistema | Mínimo | O Lace Studio pede ainda |
|---|---|---|
| Linux x64 | glibc 2.35: Ubuntu 22.04, Debian 12, Fedora 36 ou mais novos | o webkit2gtk 4.1 (`sudo apt install libwebkit2gtk-4.1-0`, `sudo dnf install webkit2gtk4.1`) |
| macOS, Apple Silicon | macOS 13 | nada |
| Windows x64 | Windows 10 ou 11 | o WebView2, que vem com o Windows 11 e com o Windows 10 atualizado |

O mínimo de Linux é o da distribuição em que o CI compila (`ubuntu-22.04`);
o de macOS, o dos binários do OSS CAD Suite. O instalador avisa quando o
WebView do Studio falta, sem deixar de instalar.

## Quem tem o Solar 0.1.0

A 0.1.0 saiu com o nome Solar. O Lace 0.2.0 é a versão seguinte, com outro
nome, outra pasta e outro executável, e não troca o Solar sozinho: os dois
ficam instalados lado a lado até o Solar sair.

- **Linux e macOS:** `~/.local/share/solar/uninstall.sh` (ou
  `sudo /opt/solar/uninstall.sh`), e instalar o Lace.
- **Windows:** Configurações > Aplicativos > Solar > Desinstalar, que tira
  também a entrada do Solar no PATH, e instalar o Lace.

A configuração do Solar (`~/.config/solar/config.json`) nenhuma versão do
Lace lê; ela pode ser apagada à mão. Os projetos (`.spf`) não mudam.

## Em um comando

```sh
curl -fsSL https://raw.githubusercontent.com/ART3121/lace/main/install.sh | sh     # Linux, macOS
```

```powershell
irm https://raw.githubusercontent.com/ART3121/lace/main/install.ps1 | iex          # Windows, PowerShell
```

Os dois scripts, `install.sh` e `install.ps1` na raiz do repositório, fazem
a mesma coisa: descobrem a última release, baixam o instalador da
plataforma, conferem o SHA-256 com o `SHA256SUMS` da release e abrem o
instalador. O `install.sh` roda em qualquer shell, porque é executado pelo
`sh`; a instalação guiada lê o teclado pelo terminal mesmo com o `| sh`.
Ele baixa para `~/.cache` (há distribuições que montam o `/tmp` sem
permissão de execução) e apaga o que baixou no fim.

| O quê | Linux, macOS | Windows |
|---|---|---|
| versão fixa | `... \| LACE_VERSION=0.3.0 sh` | `$env:LACE_VERSION = "0.3.0"` antes do `irm` |
| sem perguntas | `... \| sh -s -- --yes` | `$env:LACE_SETUP_ARGS = "/VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=recomendada /TASKS=path"` antes do `irm` |
| outras opções do instalador | `... \| sh -s -- --components yanc,icarus --prefix /opt/lace` | os parâmetros do Inno Setup em `LACE_SETUP_ARGS` |
| para todos os usuários | `... \| sudo sh` (vai para `/opt/lace`) | o assistente pergunta |

## Tipos de instalação e componentes

Os dois instaladores oferecem os mesmos tipos:

- **Recommended** (o padrão): o Lace Studio, YANC, Icarus Verilog, Yosys,
  Graphviz e surfer-aurora. É o que a AURORA usa no dia a dia, com o
  ambiente gráfico.
- **Advanced**: a lista de componentes, para marcar exatamente o que
  instalar.

| Componente | O que é | Na instalação Recommended |
|---|---|---|
| Lace | a linha de comando; sempre instalada, porque é ela que instala, atualiza e desinstala (`lace install`, `lace update`, `lace uninstall`) | sim |
| Lace Studio | o ambiente gráfico: editor, compilação, simulação, ondas; atalho no menu de aplicativos | sim |
| YANC | compiladores C± e C do SAPHO e a biblioteca SAPHO | sim |
| Icarus Verilog | simulador Verilog, o padrão da AURORA | sim |
| Verilator | simulador compilado; no Linux e no macOS precisa de compilador C++, `make` e Perl do sistema, no Windows vem com eles | não |
| cocotb | testbenches em Python, com o Python que os roda; precisa do Icarus. O Lace roda os testes no Icarus (`lace sim test_x.py`, ou Wave e Rápida no Studio) e, com o componente Verilator, no Verilator (`lace sim --verilator`) | não |
| Yosys | síntese e esquemático; lê as portas para o testbench-modelo | sim |
| Graphviz (dot) | desenho do esquemático; precisa do Yosys | sim |
| surfer-aurora | visualizador de formas de onda (o fork do Surfer da AURORA) | sim |

O Verilator fica fora da instalação Recommended porque, no Linux e no macOS, só
funciona com o compilador do sistema instalado (ver [BUNDLE.md](BUNDLE.md),
seção 4). No Windows ele já traz o g++, o `make` e o Perl, e fica fora da
Recommended do mesmo jeito. O cocotb fica fora porque só serve a quem
escreve testbench em Python e acrescenta 115 MiB no Linux (42 MiB com o
Verilator, que traz o mesmo Python).
Marcar o Graphviz marca o Yosys; desmarcar o Yosys desmarca o Graphviz.

O Lace Studio fica no bundle, em `toolchain/studio/`, e usa o bundle em que
está. O atalho
dele vai para o menu de aplicativos: o `lace-studio.desktop` em
`~/.local/share/applications` no Linux, o `Lace Studio.app` em
`~/Applications` no macOS (um symlink para o do bundle), o menu Iniciar no
Windows.

Uma ferramenta de componente não instalado dá erro claro quando usada:

```
Error: Component verilator is not installed (run the Lace installer again and select it)
```

`lace tools` mostra o que está instalado e o que não está.

Tamanho instalado no Linux, de referência (0.2.0): 378 MiB com tudo, 242 MiB
na Recommended, dos quais 16 MiB são do Lace Studio; o instalador
(`.tar.gz`) tem 106 MB. Icarus, Verilator, Yosys e Graphviz dividem bibliotecas
do OSS CAD Suite, então a soma dos tamanhos de cada um é maior que o total.
`./install --list` e a página de componentes do assistente mostram os
números da plataforma.

## Windows

1. Rode o `irm ... | iex` acima, ou baixe e rode
   `lace-<versão>-windows-x64-setup.exe` da página da release.
2. Escolha instalar só para você (padrão, sem administrador, em
   `%LOCALAPPDATA%\Programs\Lace`) ou para todos os usuários (pede
   administrador, em `C:\Lace`).
3. Na página de componentes, fique na **Recommended** ou troque para
   **Advanced** e marque os componentes.
4. Deixe marcada a tarefa **Add Lace to PATH** para usar `lace` em
   qualquer terminal novo. Com o Studio, o assistente põe o atalho no menu
   Iniciar e oferece um na área de trabalho.

A pasta padrão não tem espaço no caminho de propósito: o `make` usado pelo
Verilator não aceita espaços, e `C:\Program Files` tem um. O assistente avisa
se a pasta escolhida tiver.

O Verilator no Windows vem com o g++, o `make` e o Perl que ele usa (o bloco
MSYS2 do lace-toolchain): não é preciso instalar o MSYS2.

**Instalar aplicativos do bundle:** `lace install` abre no terminal a lista
dos aplicativos do bundle (YANC, Icarus, Verilator, cocotb, Yosys, Graphviz,
surfer-aurora, Lace Studio), com os instalados marcados e travados; marque os novos com
`Space` e confirme. `lace install verilator` instala direto, sem a lista,
com o que o aplicativo exige (`graphviz` traz o Yosys). O Lace não é
reinstalado: só os aplicativos marcados são baixados e extraídos em
`toolchain/`, e uma falha no meio desfaz o que entrou.

Os aplicativos vêm da release desta versão do Lace no GitHub, que publica
o bundle em pedaços (`lace-<versão>-<plataforma>-<pedaço>`), conferidos
pelo `SHA256SUMS` da release. Sem rede, `lace install --from` usa um
instalador do Lace no disco (a pasta, o `.tar.gz` da release ou o
`payload/` dele); `LACE_RELEASE_URL` aponta para um espelho do laboratório,
com uma pasta `v<versão>/` por release. O bundle de lá precisa ser o
instalado; outro bundle é atualizar (`lace update`).

No Windows, o Studio instalado pelo `lace install studio` não ganha atalho
no menu Iniciar (o atalho é do assistente): abra
`<pasta>\toolchain\studio\lace-studio.exe`, ou rode o assistente de novo
com o Studio marcado.

O desinstalador do Windows apaga o `toolchain/` inteiro, com o que o
`lace install` pôs. Rodar o assistente de novo à mão também refaz o
`toolchain/` com a seleção dele, que não conhece os aplicativos
acrescentados depois: marque-os lá também. O `lace update` já abre o
assistente com eles marcados.

**Atualizar:** `lace update --check` compara as versões; `lace update`
baixa o assistente da release nova, confere o SHA-256 e o abre. Ele lembra
a pasta, e os componentes instalados vêm marcados: os do assistente e os que
o `lace install` acrescentou.

**Mudar os componentes:** rode o instalador de novo. Ele lembra a pasta e a
seleção anteriores; o que for desmarcado é removido.

**Remover:** `lace uninstall`, Configurações > Aplicativos > Lace, ou
`<pasta>\unins000.exe`; os três abrem o mesmo desinstalador. Sai o Lace,
o bundle inteiro e a entrada do PATH. `lace uninstall --yes` desinstala sem
perguntar.

**Sem interação** (scripts, laboratórios):

```
lace-0.3.0-windows-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=recomendada /TASKS=path
lace-0.3.0-windows-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=avancada /COMPONENTS="lace,yanc,icarus,verilator,studio" /DIR=D:\Lace
```

Nos parâmetros do Inno Setup, os nomes são `lace`, `yanc`, `icarus`,
`icarus\cocotb`, `verilator`, `yosys`, `yosys\graphviz`, `surfer_aurora` e
`studio`; as tarefas, `path` e `studioicon` (o atalho do Studio na área de
trabalho).

O instalador não é assinado: o SmartScreen pode avisar "Editor
desconhecido" na primeira vez (Mais informações > Executar assim mesmo).

## Linux e macOS

Pelo `install.sh` (acima), ou à mão, com o arquivo da release:

```
curl -fLO https://github.com/ART3121/lace/releases/download/v0.3.0/lace-0.3.0-linux-x64.tar.gz
tar xzf lace-0.3.0-linux-x64.tar.gz && ./lace-0.3.0-linux-x64/install
```

A instalação guiada no terminal tem estas telas:

1. boas-vindas (avisa se já há uma instalação na pasta padrão);
2. tipo de instalação: **Recommended** ou **Advanced**;
3. componentes, só na instalação Advanced: `Space` marca e desmarca; o Verilator mostra
   se o compilador do sistema foi encontrado;
4. pasta da instalação e atalho;
5. resumo;
6. progresso e resultado, com a conferência dos hashes dos executáveis.

`Esc` volta uma tela, `Ctrl+C` sai sem mudar nada.

| | Usuário comum | root (`sudo ./install`, ou `curl ... \| sudo sh`) |
|---|---|---|
| Pasta | `~/.local/share/lace` | `/opt/lace` |
| Atalho | `~/.local/bin/lace` | `/usr/local/bin/lace` |
| Studio no menu (Linux) | `~/.local/share/applications/lace-studio.desktop` | `/usr/local/share/applications/lace-studio.desktop` |
| Studio no menu (macOS) | `~/Applications/Lace Studio.app` | `/Applications/Lace Studio.app` |

Se a pasta do atalho não estiver no `PATH`, o instalador diz a linha para
pôr no `~/.bashrc` ou `~/.zshrc`, e o comando do fish
(`fish_add_path ~/.local/bin`). No macOS, `~/.local/bin` não está no `PATH`
por padrão.

O instalador só escreve numa pasta nova, vazia ou com uma instalação do
Lace. Ele extrai tudo numa pasta provisória, confere o hash de cada
executável com o próprio Lace e só então troca a instalação anterior: uma
falha no meio não estraga o que já estava instalado.

**Sem interação:**

```
./install --list                                   # componentes e tamanhos
./install --yes                                    # Recommended
./install --yes --components yanc,icarus,verilator # Advanced
./install --yes --prefix /opt/lace --no-link
```

Um componente que exige outro traz o outro junto (`--components graphviz`
instala também o Yosys, e avisa).

**Instalar aplicativos do bundle:** `lace install` abre no terminal a lista
dos aplicativos do bundle (YANC, Icarus, Verilator, cocotb, Yosys, Graphviz,
surfer-aurora, Lace Studio), com os instalados marcados e travados; marque os novos com
`Space` e confirme. `lace install verilator` instala direto, sem a lista,
com o que o aplicativo exige (`graphviz` traz o Yosys). O Lace não é
reinstalado: só os aplicativos marcados são baixados e extraídos em
`toolchain/`, e uma falha no meio desfaz o que entrou. O
`lace install studio` também cria o atalho do Studio no menu.

Os aplicativos vêm da release desta versão do Lace no GitHub, que publica
o bundle em pedaços (`lace-<versão>-<plataforma>-<pedaço>`), conferidos
pelo `SHA256SUMS` da release. Sem rede, `lace install --from` usa um
instalador do Lace no disco (a pasta, o `.tar.gz` da release ou o
`payload/` dele); `LACE_RELEASE_URL` aponta para um espelho do laboratório,
com uma pasta `v<versão>/` por release. O bundle de lá precisa ser o
instalado; outro bundle é atualizar (`lace update`).

**Atualizar:** `lace update --check` mostra se há Lace mais novo e,
para cada aplicativo, a versão instalada, a do bundle da última release e a
última upstream. `lace update` reinstala pela release nova, com os mesmos
aplicativos, a mesma pasta e o mesmo atalho. Uma ferramenta mais nova
upstream não é instalada sozinha: o bundle compila o YANC e o surfer-aurora e
divide o OSS CAD Suite por ferramenta, então cada versão nova passa pelo
`bundle.py` e pelo CI e chega numa release nova do Lace.

**Mudar os componentes:** rode o instalador de novo na mesma pasta; a
instalação é trocada pela nova seleção.

**Remover:** `lace uninstall` (pergunta antes; `--yes` não pergunta) ou
`<pasta>/uninstall.sh`. Sai o Lace, o bundle inteiro (`toolchain/`), o
atalho, o do Studio no menu e a pasta; uma instalação do sistema (`/opt/lace`) pede
`sudo lace uninstall`.

O que não sai, porque não é da instalação: os projetos, com o `.lace/` de
cada um (intermediários e ondas, que se apagam à mão), e o histórico de
arquivos do surfer-aurora (`~/.local/share/surfer` no Linux), que o Surfer e
a AURORA também usam, e as preferências do Studio. Um
`~/.config/lace/config.json` de um build de desenvolvimento anterior à
0.2.0 (o `lace config`, que saiu), que nenhuma versão lê, sai junto com
`lace uninstall`.

**Verilator:** precisa de `g++` (ou `clang++`), `make` e `perl` do sistema:
`sudo apt install build-essential perl` (Debian, Ubuntu),
`sudo dnf install gcc-c++ make perl` (Fedora), `xcode-select --install`
(macOS).

**macOS e o Gatekeeper:** o instalador não é assinado nem notarizado. Um
`.tar.gz` baixado pelo navegador recebe a marca de quarentena, e o macOS
recusa abrir o `install` ("desenvolvedor não pode ser verificado"). O
`install.sh` e o `curl -fLO` baixam pelo `curl`, que não põe a marca; se já
foi baixado pelo navegador:

```
xattr -dr com.apple.quarantine lace-0.3.0-darwin-arm64
```

Isto não foi verificado num Mac: é o comportamento documentado do
Gatekeeper para binários sem assinatura. Os arquivos que o instalador extrai
não recebem a marca.

## Depois de instalar

```
lace tools --verify        # o bundle, as versões e a conferência dos hashes
lace new meu_projeto
cd meu_projeto
lace add top.v             # Verilog: cria o módulo e o registra como topo
lace proc add filtro       # SAPHO: cria o processador filtro
```

Os dois fluxos, Verilog e SAPHO, completos estão em
[CLI.md, Primeiro uso](CLI.md#primeiro-uso).

## O que fica na pasta

```
<pasta>/
  bin/lace[.exe]
  toolchain/            o bundle, só com os componentes escolhidos
    bundle.json
    components/<nome>.json
    yanc/ oss-cad-suite/ surfer-aurora/ graphviz/ msys/ studio/
  install.json          (Linux, macOS) o que foi instalado
  uninstall.sh          (Linux, macOS)
  unins000.exe          (Windows)
```
