# 0013. O Lace Studio no repositório do Lace e no bundle, como componente

- **Status:** Aceita
- **Data:** 2026-10-05

## Contexto

O Lace Studio, o ambiente gráfico do Lace (Tauri 2 e React), nasceu num
repositório ao lado (`../lace-studio`), com o `lace-core` por caminho
(`../lace/crates/lace-core`) e os tipos lidos de `../lace/docs/schema`.
Cada mudança no Core pedia os dois repositórios na mesma versão, e o Studio
não tinha instalador: quem o usava compilava.

A decisão do usuário (2026-10-05) foi juntar os dois e dar ao instalador a
opção de instalar o Studio ou não, com todos os componentes do bundle
selecionáveis. O Studio entra marcado na instalação Recommended.

## Decisão

- **Repositório.** O Studio fica em `studio/`. O `src-tauri/` tem o próprio
  workspace do Cargo e o próprio `Cargo.lock`, e o `Cargo.toml` da raiz o
  deixa de fora (`exclude`): o Tauri pede o WebView do sistema e a
  interface já compilada, que os testes do Core e da CLI não precisam. O
  `lace-core` continua por caminho, agora `../crates/lace-core`.
- **Versão.** O Studio tem a versão do Lace (`studio/package.json`,
  `studio/src-tauri/Cargo.toml`, `tauri.conf.json` e o pacote `studio` de
  `bundle/versions.json`). O `bundle.py` recusa o build se o
  `versions.json` e o `studio/` discordam, e o `release.yml` recusa a tag
  se o Studio não for a versão do `Cargo.toml`.
- **No bundle.** O Studio é o componente `studio`, do pacote `studio` (o
  próprio repositório), que o `bundle.py` compila com `npm ci` e
  `tauri build`: no Linux e no Windows o executável `lace-studio`; no macOS
  o `Lace Studio.app`. Ele vai para `toolchain/studio/` como os outros, com
  o hash do executável no manifesto. Por ser componente, vem de graça o que
  os outros têm: a escolha na TUI e no Inno Setup, o `lace install studio`,
  o `lace tools --verify`, o `lace update` com os mesmos componentes e o
  desinstalador. `closure: false`: ele liga só às bibliotecas do sistema,
  como o surfer-aurora.
- **Achar o bundle.** Instalado, o Studio usa o bundle em que está (a
  primeira pasta acima do executável com `bundle.json`) antes da pasta
  padrão da instalação. Uma instalação em outra pasta abre o próprio bundle.
- **Atalho.** O componente `studio` ganha um atalho no menu de aplicativos
  (`lace_installer::desktop`): o `lace-studio.desktop` no Linux, o symlink
  `Lace Studio.app` em `~/Applications` no macOS, a entrada do menu Iniciar
  no Windows (o `[Icons]` do `lace.iss`, com ícone na área de trabalho
  opcional). O instalador, o `lace install studio` e o `uninstall.sh`
  mexem só no atalho desta instalação.
- **WebView.** O Studio usa o WebView do sistema: o webkit2gtk 4.1 no Linux,
  o WebView2 no Windows. Os instaladores avisam quando ele falta, sem
  recusar: a TUI confere o `libwebkit2gtk-4.1.so.0`, o Inno Setup a versão
  do WebView2 no registro.
- **Todos os componentes selecionáveis.** O cocotb do Windows, que o
  lace-toolchain já compila, passa a ser componente também lá (do pacote
  `msys`, com a libstdc++ do UCRT64). A CLI continua sempre instalada: é ela
  que faz `lace install`, `lace update` e `lace uninstall`.

## Consequências

- Uma mudança no Core e no Studio vai num commit só, e o CI confere os dois
  (`ci.yml`, job `studio`; os fluxos do Studio rodam no `installers.yml`
  contra a instalação).
- O build dos instaladores leva o Node.js e, no Linux, as bibliotecas de
  desenvolvimento do webkit2gtk. O runner de Linux passa a ser o
  `ubuntu-22.04`: o que sai dele exige a glibc 2.35 (Ubuntu 22.04, Debian 12,
  Fedora 36 em diante) e o webkit2gtk 4.1 que o 22.04 tem.
- O Studio fica dentro de `toolchain/`, que o instalador troca inteiro a
  cada reinstalação; as preferências dele ficam fora, na pasta de
  configuração do sistema, e sobrevivem.
- No Windows, o `lace install studio` (sem o assistente) instala o Studio
  sem atalho no menu Iniciar: o atalho é do assistente, que o desinstalador
  dele tira.
- O macOS só ganha o symlink em `~/Applications`: o Spotlight e o Launchpad
  podem não listar o Studio por ele; o `open` e o Finder abrem. Não foi
  verificado num Mac.
