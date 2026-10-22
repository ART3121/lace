# Instalar o Lace

Cada release traz um instalador para o Linux, um para o macOS e dois para o
Windows. O instalador põe o Lace, o
Lace Studio e o bundle de ferramentas (as versões exatas, ver
[BUNDLE.md](BUNDLE.md)) numa pasta só, e deixa escolher o que instalar: o
Studio e cada ferramenta do bundle são componentes, e só a linha de comando
é sempre instalada.

| Plataforma | Arquivo | Instalador |
|---|---|---|
| Windows 10 e 11, x64 | `lace-<versão>-windows-x64-web-setup.exe`, que baixa só os aplicativos marcados, ou `lace-<versão>-windows-x64-setup.exe`, com o bundle inteiro | assistente (Inno Setup) |
| Linux x64 | `lace-<versão>-linux-x64.tar.gz` | no terminal (TUI) |
| macOS, Apple Silicon | `lace-<versão>-darwin-arm64.tar.gz` | no terminal (TUI) |

O `SHA256SUMS` da release tem o hash de cada arquivo que o `release.yml`
publica. O `.vsix` da extensão do VS Code, anexado à mão depois, não está
nele.

## Requisitos

| Sistema | Mínimo | O Lace Studio pede ainda |
|---|---|---|
| Linux x64 | glibc 2.35: Ubuntu 22.04, Debian 12, Fedora 36, Arch ou mais novos | o webkit2gtk 4.1 e o GTK do sistema |
| macOS, Apple Silicon | macOS 13 | nada |
| Windows x64 | Windows 10 ou 11 | o WebView2, que vem com o Windows 11 e com o Windows 10 atualizado |

O webkit2gtk 4.1 não vem com o Lace. O GNOME costuma já tê-lo; o KDE
Plasma, o Arch e os ambientes leves, em geral, não:

```sh
sudo apt install libwebkit2gtk-4.1-0     # Debian, Ubuntu, Mint
sudo dnf install webkit2gtk4.1           # Fedora
sudo pacman -S webkit2gtk-4.1            # Arch, Manjaro
```

O instalador confere a biblioteca quando o Studio entra (na instalação ou
no `lace install studio`). Faltando, ele diz o comando do gerenciador de
pacotes do sistema e, na instalação guiada, pergunta se o roda na hora. Com
`--yes`, só avisa. Aberto pelo menu sem a biblioteca, o Studio mostra numa
janela o que falta e o comando que a instala.

Para compilar o Studio do código-fonte no Linux, instale as dependências do Tauri:

```sh
# Ubuntu / Debian
sudo apt update
sudo apt install build-essential curl wget file libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev

# Fedora
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel
sudo dnf group install c-development
```

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
instalador. Os dois mostram o progresso do download: o `install.sh`, a
barra do `curl`; o `install.ps1`, uma linha com a barra, a porcentagem, os
MiB, a velocidade e o tempo que falta, que encolhe para caber na janela. O
`install.sh` roda em qualquer shell, porque é executado pelo `sh`; a
instalação guiada lê o teclado pelo terminal mesmo com o `| sh`. Ele baixa
para `~/.cache`, ou para `/var/tmp` como root (há distribuições que montam o `/tmp` sem permissão de
execução) e apaga o que baixou no fim.

| O quê | Linux, macOS | Windows |
|---|---|---|
| versão fixa | `... \| LACE_VERSION=0.9.0 sh` | `$env:LACE_VERSION = "0.9.0"` antes do `irm` |
| sem perguntas | `... \| sh -s -- --yes` | `$env:LACE_SETUP_ARGS = "/VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=sapho /TASKS=path"` antes do `irm` |
| outras opções do instalador | `... \| sh -s -- --yes --profile verilog --prefix /opt/lace` | os parâmetros do Inno Setup em `LACE_SETUP_ARGS` |
| para todos os usuários | `... \| sudo sh` (vai para `/opt/lace`) | o assistente pergunta |

## Idioma, perfis e componentes

O assistente do Windows começa perguntando o idioma, English ou Português
(Brasil), com o do Windows já escolhido. Depois vem o perfil: um conjunto de
aplicativos para um tipo de trabalho. A página do perfil mostra a lista
inteira dos aplicativos, com os que ele instala marcados, e diz para quem
ele serve.

| Perfil | Para quem | Aplicativos |
|---|---|---|
| **SAPHO** (recomendado, o padrão) | quem constrói processadores SAPHO e os programa em C±, com as ferramentas de Verilog em volta | todos |
| **Verilog** | quem projeta em Verilog sem processadores SAPHO: módulos, testbenches em Verilog ou cocotb, simulação no Icarus ou no Verilator, síntese e placas FPGA | todos menos o YANC |
| **Custom** (avançada; no assistente em português, Personalizada) | uso avançado, para quem sabe de quais componentes precisa | os marcados |

Com um perfil, a lista fica travada: ela só mostra o que vai ser instalado.
Só o Custom deixa marcar e desmarcar. O Lace (CLI & Core), a linha de
comando e o Core, vem sempre marcado e travado, em qualquer perfil. Na
instalação guiada do Linux e do macOS, o perfil sob o cursor mostra cada
aplicativo com `[x]` ou `[ ]`, e o Custom abre a lista para marcar.

| Componente | O que é | SAPHO | Verilog |
|---|---|---|---|
| Lace (CLI & Core) | a linha de comando e o Core; sempre instalado, porque é ele que instala, atualiza e desinstala (`lace install`, `lace update`, `lace uninstall`) | sim | sim |
| Lace Studio | o ambiente gráfico: editor, compilação, simulação, ondas; atalho no menu de aplicativos | sim | sim |
| YANC | compiladores C± e C do SAPHO e a biblioteca SAPHO | sim | não |
| Icarus Verilog | simulador Verilog, o padrão da AURORA | sim | sim |
| Verilator | simulador compilado; no Linux e no macOS precisa de compilador C++, `make` e Perl do sistema, no Windows vem com eles | sim | sim |
| cocotb | testbenches em Python, com o Python que os roda; precisa do Icarus. O Lace roda os testes no Icarus (`lace sim test_x.py`, ou Wave e Rápida no Studio) e, com o componente Verilator, no Verilator (`lace sim --verilator`) | sim | sim |
| Yosys | síntese e esquemático; lê as portas para o testbench-modelo | sim | sim |
| Graphviz (dot) | desenho do esquemático; precisa do Yosys | sim | sim |
| RTL viewer | o esquemático interativo do netlist da síntese no navegador (`lace synth --open`); precisa do Yosys | sim | sim |
| surfer-aurora | visualizador de formas de onda (o fork do Surfer da AURORA) | sim | sim |
| openFPGALoader | grava as placas FPGA pelo cabo USB delas ([FPGA.md](FPGA.md)); no Linux, o acesso ao cabo pede uma regra do udev | sim | sim |
| Lace Learn | os exercícios de Verilog do `lace learn` e da vista Exercícios do Studio ([LEARN.md](LEARN.md)); precisa do Icarus | sim | sim |

O Verilator, no Linux e no macOS, só funciona com o compilador do sistema
instalado (ver [BUNDLE.md](BUNDLE.md), seção 4); no Windows ele traz o g++,
o `make` e o Perl, uns 630 MiB. Quem não quer o Verilator usa o Custom.
Marcar o Graphviz ou o RTL viewer marca o Yosys; desmarcar o Yosys desmarca
os dois. Do mesmo jeito, marcar o cocotb ou o Lace Learn marca o Icarus.

Os perfis ficam em `bundle/components.json` (`profiles`), com o nome, o
texto de cada um e os componentes; o primeiro é o padrão e o recomendado. Os
textos em português ficam em `translations`, ao lado. Um perfil traz o
que cada componente dele exige, e o `bundle.py` recusa um que não traga.

O Lace Studio fica no bundle, em `toolchain/studio/`, e usa o bundle em que
está. O atalho
dele vai para o menu de aplicativos: o `lace-studio.desktop` em
`~/.local/share/applications` no Linux (que abre o `lace-studio-launch`, ao
lado do Studio: ele confere as bibliotecas do sistema antes de abrir), o
`Lace Studio.app` em
`~/Applications` no macOS (um symlink para o do bundle), o menu Iniciar no
Windows.

Uma ferramenta de componente não instalado dá erro claro quando usada:

```
Error: Component verilator is not installed
  Install it with: lace install verilator
```

`lace tools` mostra o que está instalado e o que não está.

Tamanho instalado no Linux, de referência (0.2.0, com menos componentes que
hoje): 378 MiB com tudo, dos quais 16 MiB são do Lace Studio; o instalador
(`.tar.gz`) tem 106 MB. Icarus, Verilator, Yosys e Graphviz dividem bibliotecas
do OSS CAD Suite, então a soma dos tamanhos de cada um é maior que o total.
`./install --list` mostra o tamanho de cada perfil e de cada componente na
plataforma, e a página de componentes do assistente, o da seleção.

## Windows

1. Rode o `irm ... | iex` acima, ou baixe e rode um dos dois assistentes da
   página da release:
   - `lace-<versão>-windows-x64-web-setup.exe` (desde a 0.7.0, alguns MB):
     ao clicar em Instalar, baixa da release só os aplicativos marcados, com
     a barra de cada download, confere cada um pelo SHA-256 e os extrai.
     É o que o `irm ... | iex` usa;
   - `lace-<versão>-windows-x64-setup.exe`: o bundle inteiro dentro dele,
     para instalar sem rede (o `irm ... | iex` o usa com
     `$env:LACE_FULL_SETUP = "1"`).
2. Escolha instalar só para você (padrão, sem administrador, em
   `%LOCALAPPDATA%\Programs\Lace`) ou para todos os usuários (pede
   administrador, em `C:\Lace`). Para todos os usuários, a pasta fica com
   permissões próprias: os administradores e o sistema escrevem nela, os
   outros usuários só leem e executam. Sem isso, ela herdaria de `C:\` a
   escrita de qualquer usuário, e o `lace.exe`, no PATH do sistema, poderia
   ser trocado por qualquer um.
3. Escolha o idioma do assistente.
4. Na página **Choose a profile** (Escolha um perfil), escolha o perfil na
   caixa: a lista embaixo mostra, travada, os aplicativos que ele instala.
   Para escolher à mão, use o **Custom** (Personalizada), para uso avançado.
5. Deixe marcada a tarefa **Add Lace to PATH** para usar `lace` em
   qualquer terminal novo. Com o Studio, o assistente põe o atalho no menu
   Iniciar e oferece um na área de trabalho.

A pasta padrão não tem espaço no caminho de propósito: o `make` usado pelo
Verilator não aceita espaços, e `C:\Program Files` tem um. O assistente avisa
se a pasta escolhida tiver. Com o Studio marcado e sem o WebView2 no
computador, ele avisa antes de instalar.

O assistente recusa uma pasta que já tem uma `toolchain` que não é de uma
instalação do Lace (sem `toolchain\bundle.json` nem `bin\lace.exe`):
instalar trocaria essa pasta inteira, sem passar pela lixeira.

Numa pasta que já tem o Lace, a primeira página diz de que versão para qual
ele atualiza, e os aplicativos que a pasta tem vêm marcados, inclusive os
que o `lace install` acrescentou: uma seleção que não é a de nenhum perfil
fica no Custom. O mesmo vale numa instalação silenciosa sem `/COMPONENTS`.

O assistente web baixa do GitHub; `/MIRROR=<url>` o faz baixar de um
espelho do laboratório, com uma pasta `v<versão>/` por release, como o
`LACE_RELEASE_URL`. Uma falha no download para o assistente na página de
instalação, sem ter mudado nada; uma falha ao extrair, antes de copiar
qualquer arquivo.

O Verilator no Windows vem com o g++, o `make` e o Perl que ele usa (o bloco
MSYS2 do lace-toolchain): não é preciso instalar o MSYS2.

**Instalar aplicativos do bundle:** `lace install` abre no terminal a lista
dos aplicativos do bundle (YANC, Icarus, Verilator, cocotb, Yosys, Graphviz,
surfer-aurora, openFPGALoader, Lace Studio, Lace Learn, RTL viewer), com os instalados marcados e travados; marque os novos com
`Space` e confirme. `lace install verilator` instala direto, sem a lista,
com o que o aplicativo exige (`graphviz` traz o Yosys). O Lace não é
reinstalado: só os aplicativos marcados são baixados e extraídos em
`toolchain/`, e uma falha no meio desfaz o que entrou.

Os aplicativos vêm da release desta versão do Lace no GitHub, que publica
o bundle em pedaços (`lace-<versão>-<plataforma>-<pedaço>`), conferidos
pelo `SHA256SUMS` da release. Sem rede, `lace install --from` usa um
instalador do Lace no disco (a pasta, o `.tar.gz` da release ou o
`payload/` dele, ou uma pasta com os arquivos que a release publica);
`LACE_RELEASE_URL` aponta para um espelho do laboratório, com uma pasta
`v<versão>/` por release e, se quiser, um arquivo `latest` com a versão
mais nova, que o `lace update` lê no lugar do GitHub. O espelho é
`https://`, ou `file://` numa pasta do disco: o `SHA256SUMS` que confere
os downloads vem dele também, e o Lace recusa `http://`. O bundle de lá
precisa ser o instalado; outro bundle é atualizar (`lace update`).

No Windows, o Studio instalado pelo `lace install studio` não ganha atalho
no menu Iniciar (o atalho é do assistente): abra
`<pasta>\toolchain\studio\lace-studio.exe`, ou rode o assistente de novo
com o Studio marcado.

O desinstalador do Windows apaga o `toolchain/` inteiro, com o que o
`lace install` pôs. Rodar o assistente de novo à mão também refaz o
`toolchain/` com a seleção dele; os aplicativos que a pasta tem, inclusive
os que o `lace install` acrescentou, já vêm marcados, e o que for
desmarcado sai.

**Atualizar:** o Lace Studio procura versão nova ao abrir, oferece a
atualização e reinicia na versão nova; o `lace`, num terminal, avisa no fim
de um comando, uma vez por dia ([CLI.md](CLI.md), "Bundle").
`lace update --check` compara as versões; `lace update`
atualiza por componentes (abaixo, em [Atualizar por componentes](#atualizar-por-componentes)),
sem abrir o assistente e sem tirar o que o `lace install` acrescentou, e a
lista de programas do Windows passa a mostrar a versão nova. Numa
instalação anterior à 0.7.0, ou com `lace update --full`, ele baixa o
assistente da release nova, confere o SHA-256 e o abre: ele lembra a pasta,
e os componentes instalados vêm marcados, os do assistente e os que o
`lace install` acrescentou.

**Mudar os componentes:** rode o instalador de novo. Ele lembra a pasta e a
seleção anteriores; o que for desmarcado é removido.

**Remover:** `lace uninstall`, Configurações > Aplicativos > Lace, ou
`<pasta>\unins000.exe`; os três abrem o mesmo desinstalador. Sai o Lace,
o bundle inteiro e a entrada do PATH. `lace uninstall --yes` desinstala sem
perguntar.

**Sem interação** (scripts, laboratórios):

```
lace-0.9.0-windows-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=sapho /TASKS=path
lace-0.9.0-windows-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /LANG=pt /TYPE=verilog /TASKS=path
lace-0.9.0-windows-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER /TYPE=avancada /COMPONENTS="lace,yanc,icarus,verilator,studio" /DIR=D:\Lace
```

O idioma (`/LANG=`) é `en` ou `pt`; sem ele, o do Windows. Os tipos
(`/TYPE=`) são os perfis, `sapho` e `verilog`, e
`avancada`, o Custom, que guarda o nome da instalação Advanced de antes dos
perfis. O `/TYPE=recomendada` de antes cai no perfil padrão.
Nos parâmetros do Inno Setup, os nomes dos componentes são `lace`, `yanc`, `icarus`,
`icarus\cocotb`, `icarus\lace_learn`, `verilator`, `yosys`,
`yosys\graphviz`, `yosys\rtl_viewer`, `surfer_aurora`, `openfpgaloader` e `studio`; as
tarefas, `path` e
`studioicon` (o atalho do Studio na área de trabalho).

O instalador não é assinado: o SmartScreen pode avisar "Editor
desconhecido" na primeira vez (Mais informações > Executar assim mesmo).

## Linux e macOS

Pelo `install.sh` (acima), ou à mão, com o arquivo da release:

```
curl -fLO https://github.com/ART3121/lace/releases/download/v0.9.0/lace-0.9.0-linux-x64.tar.gz
tar xzf lace-0.9.0-linux-x64.tar.gz && ./lace-0.9.0-linux-x64/install
```

A instalação guiada no terminal tem estas telas:

1. boas-vindas (avisa se já há uma instalação na pasta padrão);
2. perfil: **SAPHO** (recomendado), **Verilog** ou **Custom** (avançada); o perfil
   sob o cursor mostra para quem é e todos os aplicativos, com `[x]` nos
   que ele instala;
3. componentes, só no Custom: `Space` marca e desmarca; o Verilator mostra
   se o compilador do sistema foi encontrado;
4. pasta da instalação e atalho;
5. resumo;
6. progresso e resultado, com a conferência dos hashes dos executáveis.

`Esc` volta uma tela, `Ctrl+C` sai sem mudar nada; durante a extração, ele
não interrompe.

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
./install --list                                   # perfis, componentes e tamanhos
./install --yes                                    # o perfil padrão (SAPHO)
./install --yes --profile verilog                  # outro perfil
./install --yes --components yanc,icarus,verilator # Custom
./install --yes --prefix /opt/lace --no-link
```

Um componente que exige outro traz o outro junto (`--components graphviz`
instala também o Yosys, e avisa).

**Instalar aplicativos do bundle:** `lace install` abre no terminal a lista
dos aplicativos do bundle (YANC, Icarus, Verilator, cocotb, Yosys, Graphviz,
surfer-aurora, openFPGALoader, Lace Studio, Lace Learn, RTL viewer), com os instalados marcados e travados; marque os novos com
`Space` e confirme. `lace install verilator` instala direto, sem a lista,
com o que o aplicativo exige (`graphviz` traz o Yosys). O Lace não é
reinstalado: só os aplicativos marcados são baixados e extraídos em
`toolchain/`, e uma falha no meio desfaz o que entrou. O
`lace install studio` também cria o atalho do Studio no menu.

Os aplicativos vêm da release desta versão do Lace no GitHub, que publica
o bundle em pedaços (`lace-<versão>-<plataforma>-<pedaço>`), conferidos
pelo `SHA256SUMS` da release. Sem rede, `lace install --from` usa um
instalador do Lace no disco (a pasta, o `.tar.gz` da release ou o
`payload/` dele, ou uma pasta com os arquivos que a release publica);
`LACE_RELEASE_URL` aponta para um espelho do laboratório, com uma pasta
`v<versão>/` por release e, se quiser, um arquivo `latest` com a versão
mais nova, que o `lace update` lê no lugar do GitHub. O espelho é
`https://`, ou `file://` numa pasta do disco: o `SHA256SUMS` que confere
os downloads vem dele também, e o Lace recusa `http://`. O bundle de lá
precisa ser o instalado; outro bundle é atualizar (`lace update`).

**Atualizar:** o Lace Studio procura versão nova ao abrir, oferece a
atualização e reinicia na versão nova; o `lace`, num terminal, avisa no fim
de um comando, uma vez por dia ([CLI.md](CLI.md), "Bundle").
`lace update --check` mostra se há Lace mais novo e,
para cada aplicativo, a versão instalada, a do bundle da última release e a
última upstream. `lace update` atualiza por componentes (abaixo). Numa
instalação anterior à 0.7.0, ou com `lace update --full`, ele reinstala
pela release nova, com os mesmos aplicativos, a mesma pasta e o mesmo
atalho. Uma ferramenta mais nova
upstream não é instalada sozinha: o bundle compila o YANC e o surfer-aurora e
divide o OSS CAD Suite por ferramenta, então cada versão nova passa pelo
`bundle.py` e pelo CI e chega numa release nova do Lace.

**Mudar os componentes:** rode o instalador de novo na mesma pasta; a
instalação é trocada pela nova seleção. Com um perfil (e com
`./install --yes` sem `--components`), os aplicativos que a instalação já
tem ficam, junto dos do perfil; para tirar algum, use o Custom.

**Remover:** `lace uninstall` (pergunta antes; `--yes` não pergunta) ou
`<pasta>/uninstall.sh`. Sai o Lace, o bundle inteiro (`toolchain/`), o
atalho, o do Studio no menu e a pasta; uma instalação do sistema (`/opt/lace`) pede
`sudo lace uninstall`.

O que não sai, porque não é da instalação: os projetos, com o `.lace/` de
cada um (intermediários e ondas, que se apagam à mão), e o histórico de
arquivos do surfer-aurora (`~/.local/share/surfer` no Linux), que o Surfer e
a AURORA também usam, as preferências do Studio, o `experimental.json` do
`lace experimental` e o `update-check.json` do aviso de versão nova. Um
`~/.config/lace/config.json` de um build de desenvolvimento anterior à
0.2.0 (o `lace config`, que saiu), que nenhuma versão lê, sai junto com
`lace uninstall`.

**Verilator:** precisa de `g++` (ou `clang++`), `make` e `perl` do sistema:
`sudo apt install build-essential perl` (Debian, Ubuntu),
`sudo dnf install gcc-c++ make perl` (Fedora), `sudo pacman -S --needed
base-devel` (Arch), `xcode-select --install` (macOS).

**macOS e o Gatekeeper:** o instalador não é assinado nem notarizado. Um
`.tar.gz` baixado pelo navegador recebe a marca de quarentena, e o macOS
recusa abrir o `install` ("desenvolvedor não pode ser verificado"). O
`install.sh` e o `curl -fLO` baixam pelo `curl`, que não põe a marca; se já
foi baixado pelo navegador:

```
xattr -dr com.apple.quarantine lace-0.9.0-darwin-arm64
```

Isto não foi verificado num Mac: é o comportamento documentado do
Gatekeeper para binários sem assinatura. Os arquivos que o instalador extrai
não recebem a marca.

## Atualizar por componentes

Desde a 0.7.0, cada instalação guarda o manifesto de arquivos da versão
dela (`toolchain/files.json`): o SHA-256 de cada arquivo que o instalador
pôs, agrupado pelos pedaços em que a release publica o bundle. A release
publica também o seu (`lace-<versão>-<plataforma>-files.json`). O
`lace update` compara os dois, para os aplicativos instalados:

```
Lace 0.7.0 to 0.8.0 by components: Lace, Lace Studio, Lace Learn
3 of 15 chunks to download (14 MiB), 412 files in, 3 out
Update Lace 0.7.0 to 0.8.0 in /home/ana/.local/share/lace? [y/N] y
  [1/3] Lace  [##############################] 100%  2.3/2.3 MiB
  [2/3] Lace Studio  [#############.................]  45%  5.1/11.3 MiB
```

Só os pedaços com algum arquivo novo ou mudado são baixados, conferidos
pelo `SHA256SUMS` da release, e dentro deles só esses arquivos são
extraídos, numa pasta provisória (`.instalando-<pid>`), e conferidos com o
manifesto novo. Depois os arquivos são trocados no lugar, os antigos vão
para `.antigo-<pid>`, os que a versão nova não tem saem, e o Lace confere
os executáveis do bundle. Uma falha em qualquer passo devolve os arquivos
antigos, e a instalação fica como estava; aí o `lace update` passa para o
instalador inteiro.

Antes de trocar, o Lace grava o que vai fazer num diário
(`.atualizando.json`, na pasta da instalação). Se o processo morre no meio
da troca (o terminal fechado, falta de energia), o próximo `lace update` ou
`lace install` desfaz o que ficou feito com o diário: os arquivos antigos
voltam, os novos saem, e ele avisa e pede para rodar o `lace update` de
novo. Uma trava (`.instalacao.lock`) impede duas instalações ou
atualizações ao mesmo tempo na mesma pasta. O Lace Studio não fecha
enquanto uma atualização ou instalação dele está rodando.

O próprio `lace` é um dos arquivos trocados. No Windows, um executável em
uso pode ser renomeado, mas não apagado: o `lace.exe` antigo fica na pasta
`.antigo-<pid>` até o próximo `lace install` ou `lace update`. Se o Lace
Studio estiver aberto durante a atualização, feche e abra de novo para usar
a versão nova.

Um aplicativo que a versão nova passa a exigir entra junto; um que ela não
tem mais sai. Um arquivo apagado à mão da instalação volta no próximo
`lace update`; com o instalador da mesma versão no disco,
`lace update --from` repara a instalação sem rede.

Sem o manifesto (uma instalação anterior à 0.7.0), com `--full`, ou se a
release nova não o publica, o `lace update` usa o instalador inteiro, como
antes. A primeira atualização de uma instalação 0.6 para a 0.7 é assim; a
partir da 0.7, vai por componentes.

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
    files.json          o SHA-256 de cada arquivo instalado (desde a 0.7.0)
    yanc/ oss-cad-suite/ surfer-aurora/ graphviz/ msys/ studio/ lace-learn/ rtl-viewer/
  install.json          (Linux, macOS) o que foi instalado
  uninstall.sh          (Linux, macOS)
  unins000.exe          (Windows)
```
