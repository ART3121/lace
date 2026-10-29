# Arquitetura da AURORA IDE

Este documento explica como o Studio é feito: as duas metades (backend em
Rust e interface em React), como elas conversam, como uma operação roda do
clique até o resultado, onde mora cada estado e o que o Studio faz por conta
própria em vez de deixar para o Lace. A referência de cada comando está em
[IPC.md](IPC.md).

Sumário:

1. [Visão geral](#1-visão-geral)
2. [Pastas](#2-pastas)
3. [Backend](#3-backend)
4. [Interface](#4-interface)
5. [Uma operação, do clique ao resultado](#5-uma-operação-do-clique-ao-resultado)
6. [Contrato entre backend e interface](#6-contrato-entre-backend-e-interface)
7. [Segurança](#7-segurança)
8. [Pendências no Lace](#8-pendências-no-lace)

---

## 1. Visão geral

```
┌─────────────────────── processo da AURORA IDE ────────────────────────┐
│                                                                       │
│  WebView (React)                       Rust (src-tauri)               │
│  ┌──────────────────────┐   invoke   ┌──────────────────────────┐     │
│  │ componentes          │ ─────────▶ │ server.rs  (lace_call)   │─────┼──▶ lace serve ──▶ bundle do Lace
│  │ stores (zustand)     │            │ jobs.rs, flows.rs        │     │    (filho, stdio,   (YANC, Icarus,
│  │ Monaco, xterm.js     │ ◀───────── │ commands/*  (Studio)     │     │     JSON-RPC)       Verilator, Yosys,
│  └──────────────────────┘  Channel   │ terminal.rs (PTY)        │     │                     dot, surfer-aurora)
│             ▲                        │ wave_tab.rs (HTTP local) │     │
│             └──────────── emit ───── │ watcher.rs  (notify)     │     │
│                                      │ assistant.rs             │─────┼──▶ lace-agent ──▶ claude ──▶ lace mcp
│                                      └──────────────────────────┘     │
└───────────────────────────────────────────────────────────────────────┘
```

Três regras orientam tudo:

- **A regra mora no Lace.** O backend é um cliente do servidor do Lace
  (`lace serve`), como a extensão do VS Code: abre o `lace` como processo
  filho e pede, pelo protocolo do Lace (JSON-RPC 2.0, uma mensagem por
  linha), o retrato do projeto, os fluxos, o histórico, instalar e
  atualizar. O Studio não sabe montar a linha de comando do `iverilog`, ler
  a saída do `cmmcomp` nem compor um fluxo; não linka o `lace-core`, o
  `lace-learn` nem o instalador. O que fica no backend é do Studio: as
  janelas, os arquivos, o terminal, a vigia da pasta, as preferências e a
  onda numa aba. O assistente (`lace-agent`) é a exceção até a 1.0.
- **O disco é a fonte da verdade.** O Studio guarda o caminho do `.spf`, e
  o servidor reabre o projeto a cada pedido, como a CLI faz a cada execução.
  O `.spf` pode mudar por fora (a CLI no terminal do próprio Studio, um
  editor de texto), e um `Project` aberto não percebe (API.md do Lace, 3.2).
- **A interface nunca bloqueia.** Todo comando que toca o disco ou fala com
  o servidor roda numa thread de bloqueio (`state::blocking`); as operações
  longas rodam numa thread própria e mandam o progresso por um `Channel`
  (seção 5).

## 2. Pastas

```
studio/
  src/                       a interface (React + TypeScript)
    main.tsx, App.tsx        entrada; a janela e as ligações globais
    actions.ts               todas as ações: menus, barra, paleta e atalhos leem daqui
    ipc/
      api.ts                 uma função por comando Tauri ou método do protocolo (lace())
      types.ts               os tipos do Studio (espelho dos structs Rust)
      lace-types.ts          os tipos do protocolo, GERADOS dos JSON Schemas do Lace
    state/                   stores: app, project, editor, jobs, layout, titleBar, dialogs, toasts, hierarchy, reports, schematic, assistant, experimental, learn, settingsPage, tour, updates, waves;
                             layoutModel.ts e savedLayouts.ts (o modelo e os layouts com nome da janela); learnSession.ts, splash.ts e wideTabs.ts, sem store
    editor/                  Monaco: workers, tema, modelos, marcadores, gramáticas C± e asm, modo Vim
    console/                 os consoles xterm.js e o terminal de shell
    components/              layout/, sidebar/, editor/, panel/, views/, dialogs/, assistant/, tour/
    i18n/                    pt.ts (referência), en.ts, t() e useT()
    themes/                  os temas: model.ts (o formato), catalog.ts (os 15), index.ts (aplicar)
    styles/                  tokens.css (medidas), app.css
    util/paths.ts            caminhos com / e \
    dev/smoke.ts             roteiro de fumaça, só na build de desenvolvimento
  src-tauri/                 o backend (Rust)
    src/lib.rs               registro dos plugins, do estado e dos comandos
    src/server.rs            o servidor do Lace (lace serve filho) e o lace_call
    src/commands/            app, project, files, toolchain
    src/flows.rs             as decisões da interface num fluxo (o comando, a onda numa aba)
    src/jobs.rs              operações longas: o pedido ao servidor e as notificações para a interface
    src/terminal.rs          o terminal de shell (portable-pty)
    src/assistant.rs         o assistente: sessões do Claude Code (lace-agent)
    src/watcher.rs           vigia da pasta do projeto (notify)
    src/wave_tab.rs          a onda numa aba: o servidor local do cliente web do Surfer
    src/toolchain.rs         onde está o lace do servidor e o bundle dele
    src/settings.rs          preferências e recentes (settings.json)
    src/state.rs, error.rs   estado do aplicativo e o erro dos comandos
    tests/flows.rs           os fluxos rodando de verdade, pelo servidor, contra o bundle
    capabilities/            permissões da janela (Tauri)
    tauri.conf.json          janela, CSP, empacotamento
  scripts/gen-lace-types.mjs gera src/ipc/lace-types.ts
  docs/                      esta documentação
```

## 3. Backend

### 3.1 Estado

`AppState` (`state.rs`) é gerenciado pelo Tauri e guarda pouco:

| Campo | O que é |
|---|---|
| `settings` | as preferências e os recentes (`SettingsStore`, gravado em `settings.json` na pasta de configuração do aplicativo) |
| `project` | o caminho do `.spf` aberto; o projeto em si é do servidor, que o relê a cada pedido |
| `job` | a operação rodando (identificador e o pedido ao servidor, para cancelar); uma por vez |
| `terminals` | os pseudoterminais abertos |
| `watcher` | o vigia da pasta do projeto |
| `assistants` | as sessões do assistente abertas, por identificador (`assistant::Assistants`) |
| `server` | o servidor do Lace (`server::LaceServer`): o processo `lace serve`, aberto no primeiro pedido |

### 3.2 O servidor do Lace

`server.rs` abre o `lace serve` como processo filho no primeiro pedido,
manda `initialize` e confere a versão do protocolo (um `lace` de outro
protocolo é `lace_incompatible`; de outra versão do Lace, com o mesmo
protocolo, só um aviso no log). Um pedido é uma linha JSON no stdin dele, e
uma thread lê o stdout: a resposta vai para quem a espera, pelo `id`, e uma
notificação (`phase`, `events`, `output_omitted`, `build`, `install`) vai
para quem ouve o pedido dela. Se o servidor cai, quem esperava recebe
`server_gone`, e o pedido seguinte o abre de novo.

O servidor recebe o bundle (`--toolchain`), o compilador do Verilator
(`--compiler`), o Quartus (`--quartus`) e as trilhas do `lace learn`
(`LACE_LEARN_DIR`); mudou uma dessas coisas nas preferências, o pedido
seguinte fecha o servidor e abre outro. No Windows ele roda com
`CREATE_NO_WINDOW` (o `lace` é um programa de console) e num Job Object que
o leva junto quando o Studio fecha ou morre; o job deixa um processo sair
dele, que é como o assistente de instalação, aberto pelo `update.apply`,
sobrevive ao Studio. Ao fechar, o Studio manda `shutdown`, espera até 3 s e
mata o que sobrou.

O `lace_call` é a porta da interface para o protocolo: o método e os
parâmetros, como o `lace serve` os recebe, e a resposta dele. O projeto
aberto entra sozinho no campo `project`; quais métodos têm esse campo vem do
schema dos pedidos (`docs/schema/protocol-request.json`, embutido no
binário e conferido por teste contra o do Lace). Os métodos longos (os
fluxos, a correção, instalar e atualizar) não passam por ele: são
operações (seção 5).

### 3.3 Comandos

Cada comando Tauri é uma função curta em `commands/`, `jobs.rs`,
`terminal.rs`, `wave_tab.rs` ou `assistant.rs`: valida a entrada, faz o
que é do Studio (ou pede ao servidor) e devolve `Result<T, IpcError>`. Os
que tocam o disco ou falam com o servidor passam por `state::blocking`, que
roda o trabalho numa thread de bloqueio do runtime assíncrono do Tauri,
fora da thread da interface.

| Módulo | Comandos |
|---|---|
| `server` | `lace_call`: os métodos do protocolo (o projeto, os processadores, o histórico, os sinais da onda, a placa FPGA, os exercícios, os recursos experimentais) |
| `commands::app` | versão, preferências, recentes, sair e reiniciar, fechar o splash, o log da interface e o roteiro de fumaça |
| `commands::project` | abrir, criar e fechar o projeto (o projeto aberto, os recentes e a vigia), a pasta padrão dos projetos, a hierarquia |
| `commands::files` | ler pasta, ler e gravar texto (com detecção de conflito), criar, renomear, copiar, lixeira, lista para o "abrir rápido", busca |
| `commands::toolchain` | a tela de ferramentas, `lace update [--check]` e `lace install`, pelo servidor |
| `jobs` | começar, cancelar e consultar a operação (build, check, sim, synth, esquemático, exercício, placa) |
| `terminal` | abrir, escrever, confirmar a saída, `cd`, redimensionar e encerrar o terminal de shell |
| `assistant` | o assistente, experimental: abrir, mandar mensagem, interromper e encerrar uma sessão do Claude Code |
| `wave_tab` | a onda numa aba: registra a onda e o layout no servidor local que serve o cliente web do Surfer |

### 3.4 Erros

`IpcError` (`error.rs`) é `{ code, message }`. A separação da API do Lace se
mantém:

- `Err(IpcError)`: o Lace não conseguiu rodar (bundle, projeto, E/S). O
  `code` é o do Lace (`LaceError::code()` ou um do servidor), estável, que
  chega do servidor em `error.data.code`, ou um dos códigos do Studio
  (`error::codes`: `no_project`, `spf_missing`, `busy`, `conflict`,
  `outside_project`, `exists`, `invalid_argument`, `terminal`,
  `unsafe_path`, `terminal_busy`, `io`, `surfer_web_missing`,
  `wave_too_large`, `internal`, `assistant`, `lace_missing`,
  `lace_incompatible`, `server_gone`, `cancelled`). A interface traduz pelo
  código (`i18n`, chaves `error.<code>`) e mostra a mensagem original
  embaixo.
- Uma operação que rodou e falhou é `Ok`, com `succeeded: false` e os
  diagnósticos no resultado. Isso vai para os consoles, para o painel
  Problemas e para os marcadores do editor, não para um aviso de erro.

### 3.5 Onde está o Lace

A CLI acha o bundle ao lado do próprio executável. O Studio é outro
programa: acha o `lace` que vai servir e o bundle que ele recebe, como a
extensão do VS Code acha o `lace` dela (`toolchain.rs`). O `lace` é o de
`LACE_STUDIO_LACE` (para desenvolvimento), o da preferência `lace_path`, o
da instalação do bundle das preferências (`<instalação>/bin/lace`), o da
instalação em que o Studio está (`<instalação>/toolchain/studio/`) e o do
`PATH`, nessa ordem. O bundle é o da preferência, o de `LACE_TOOLCHAIN` ou
o da instalação achada; sem nenhum, o servidor procura o dele. O resto (as
ferramentas, os componentes, o compilador, o Quartus, o cliente web do
Surfer) é o `toolchain.info` do servidor, que a tela de ferramentas mostra.

O compilador do Verilator segue o `--compiler` da CLI: preferência ou
`LACE_COMPILER`. Uma pasta declarada sem `perl`, `make` e um compilador C++
não derruba o bundle: o servidor segue sem compilador, a simulação com
Verilator falha com `system_compiler_missing`, e a tela de ferramentas
mostra o motivo. O Quartus, o mesmo.

O servidor reabre o bundle a cada pedido (ler meia dúzia de JSON pequenos),
para um `lace install` feito pelo terminal valer na hora.

### 3.6 Arquivos e vigia

Ler vale para qualquer caminho (o usuário pode abrir um arquivo de fora).
Criar, renomear e apagar só valem dentro da pasta do projeto, conferido
depois de resolver atalhos (`ensure_inside`). Apagar é mandar para a
lixeira do sistema (crate `trash`), como a AURORA.

Gravar recebe o horário de modificação lido ao abrir. Se o arquivo mudou
por fora, o backend recusa com `conflict` e a interface pergunta:
sobrescrever, recarregar ou cancelar.

`watcher.rs` vigia a pasta do projeto (crate `notify`), junta os caminhos
por 300 ms e emite `studio://fs-changed`. O que muda em `.lace/` e `.git/`
não é avisado: muda o tempo todo durante uma operação e não aparece na
árvore. Só mudança conta (criar, gravar, apagar, renomear, atributos): o
inotify avisa também quando um arquivo é aberto para leitura, e repassar
isso fazia a interface reler o projeto em ciclo, porque reler o projeto abre
o `.spf`.

Copiar (`fs_copy`, o arrastar do gerenciador de arquivos) só vale com
destino no projeto. Mover e renomear na árvore são o `project.move` do
servidor (o `Project::move_path` do Lace), que mantém o `.spf` em dia e
recusa o que fica no lugar (o `.spf`, `.lace/`, as pastas e o fonte dos
processadores). Com `overwrite`, o que está no destino vai para a lixeira e
o movimento é tentado de novo, menos quando o destino é o que o Lace mantém
no lugar, a pasta do projeto ou uma pasta que contém a origem: aí o erro é
`cannot_replace`, e nada vai para a lixeira. O Lace nunca sobrescreve sem
`overwrite`.

### 3.7 Hierarquia e nome de projeto

A hierarquia é o fluxo `hierarchy` do servidor (o `lace_core::hierarchy`,
API.md do Lace, seção 5.4.1), pedido por `project_hierarchy` sem saída ao
vivo: o Icarus elabora o design e cada testbench, e o Lace lê a árvore do
`.vvp`. É o Icarus qualquer que seja o simulador escolhido; sem ele, a
vista mostra o erro do componente.

A regra de nome de projeto também é do Lace (`project.check_name`): o
`project.create` recusa, e o diálogo de projeto novo confere enquanto o
usuário digita, para avisar antes. O de processador novo faz o mesmo com
`processor.check_name` (que recusa também os nomes das pastas do Lace), e
mantém o `#NUBITS` igual a `#NBMANT + #NBEXPO + 1` enquanto o usuário muda
a mantissa ou o expoente; o resto dos parâmetros o Lace confere ao criar
(`invalid_parameter`).

### 3.8 Encerrar

Ao sair (evento `RunEvent::Exit`) e ao receber SIGTERM, SIGINT ou SIGHUP
(Unix), o Studio cancela a operação rodando, espera até 3 s, encerra as
sessões do assistente, fecha os terminais e fecha o servidor do Lace
(`shutdown`, até 3 s, e depois mata). Sem isso, no Linux e no macOS, as
ferramentas, que o Lace roda num grupo de processos próprio, continuariam
rodando depois que o Studio saísse (API.md do Lace, seção 9, "Limites"). No
Windows, o servidor roda num Job Object do Studio, e cada passo num do
Lace, dentro dele: tudo termina junto com o Studio mesmo se ele for
derrubado, e nenhuma ferramenta abre janela de console.

## 4. Interface

### 4.1 Estrutura da janela

`App.tsx` monta, de cima para baixo: a barra de título integrada ou a barra
de menus, barra de ferramentas, a área de trabalho
(`components/layout/Workbench.tsx`) e a barra de status. Diálogos, menu de
contexto e avisos ficam por cima. O `.app` é um grid com uma área por barra
e linhas `auto`: uma barra que o layout esconde não é montada, e a linha
dela some sem deslocar as outras.

**Barra de título** (`state/titleBar.ts`, `components/layout/TitleBar.tsx`).
O Studio não usa a barra de título do sistema: uma faixa na linha da barra
de menus leva os menus (`Menus`, de `MenuBar.tsx`), o título
(`windowTitle`, o mesmo que `state/project.ts` põe na janela para a barra
de tarefas) e, no Windows e no Linux, os botões de minimizar, maximizar ou
restaurar e fechar; fechar passa pelo `onCloseRequested` de `App.tsx`. A
faixa arrasta a janela por `data-tauri-drag-region="deep"`: o script do
Tauri não arrasta por elementos clicáveis, e os menus abertos têm
`data-tauri-drag-region="false"`. Ela sai em tela cheia, e os menus, se à
vista, voltam para a barra de menus. As permissões são as de
`capabilities/default.json`: `set-decorations`, `minimize`,
`toggle-maximize`, `close` e `start-dragging`. Como cada sistema chega lá:

- **Windows e Linux.** A janela nasce com a moldura do sistema, e
  `applyTitleBar` a tira (`setDecorations(false)`) antes do `reveal`.
  Bordas e cantos continuam redimensionando (o tao trata a borda de cima no
  Windows e todas no Linux). Se a interface não carregar, a janela do
  `REVEAL_FALLBACK` tem a moldura; se a moldura não sair, os menus ficam na
  barra de menus. O menu de encaixe do Windows 11 sobre o maximizar não
  aparece: o Tauri não o oferece para botões desenhados pela página.
- **macOS.** O estilo `Overlay` (os botões coloridos por cima da página), o
  título do sistema escondido e a posição dos botões só se definem na
  criação da janela: são o `titleBarStyle`, o `hiddenTitle` e o
  `trafficLightPosition` do `tauri.conf.json`, que o Tauri só lê no macOS.
  A faixa deixa 78 px para os botões (`.titlebar--mac`).

**Abertura.** A janela nasce escondida e com fundo escuro (`visible: false`
e `backgroundColor` em `tauri.conf.json`). A interface a mostra
(`reveal`, em `App.tsx`) depois de aplicar o tema e o zoom das preferências,
de achar o Lace e de reabrir o último projeto, com um limite de 8 s
(`REVEAL_WAIT_MS`) para um projeto grande não segurar a abertura.
Enquanto isso, o splash (a janela `splash` do `tauri.conf.json`,
`public/splash.html`) fica na frente com o logo animado, pelo menos 3,2 s
(`SPLASH_MIN_MS`), e fecha quando a principal aparece. Assim o primeiro
quadro já é o de trabalho,
e não uma página branca enquanto o JavaScript carrega. Se a interface não
chegar a mostrar a janela (o Vite fora do ar, um módulo que não carregou),
o backend a mostra e fecha o splash depois de 10 s (`REVEAL_FALLBACK`, em `lib.rs`). O log
registra quanto a abertura levou (`Startup: modules loaded in ... ms, first
frame at ... ms`).

**Layout** (`state/layoutModel.ts`, `state/layout.ts`,
`state/savedLayouts.ts`). Três regiões fixas, como no VS Code: a barra
lateral esquerda, a direita e o painel (embaixo ou à direita do editor). As
quinze vistas (as cinco da barra lateral esquerda, o Assistente, experimental, o INFO, os seis terminais de
etapa, Problemas e Terminal) ficam cada uma em uma região e podem ir para
qualquer outra. O
arranjo é um `LayoutBody`: as vistas de cada região, a ativa e se ela
aparece; a posição do painel; os tamanhos em pixels CSS; as barras; e o que
está escondido (vistas e itens das barras de ferramentas e de status). O
layout guarda o escondido, não o que aparece: um item ou uma vista que uma
versão futura acrescentar aparece sozinho, e uma vista que não está em
região nenhuma vai para a região padrão dela (`normalizeBody`, o único
validador). `useLayout.live` é a janela agora, guardada no `localStorage`;
os layouts com nome são fotos dela no `settings.json` (`layouts`), que o
backend guarda sem ler. "Modificado" é `sameBody` entre a janela (sem o
zen, `effectiveLive`) e a foto do layout em uso; a vista ativa de cada
região não entra.

**Área de trabalho** (`components/layout/Workbench.tsx`). A barra de
atividades de um lado, um `Group` horizontal com a esquerda, o centro e a
direita e, no centro, um `Group` com o editor e o painel, vertical ou
horizontal conforme a posição dele. Os lugares na árvore não mudam e nada
tem `key` tirada do layout, para o `EditorArea` nunca remontar. Três
cuidados com o `react-resizable-panels`:

- o grupo lembra um tamanho por conjunto de ids de painéis, e essa memória
  vence o `defaultSize`. Por isso os tamanhos do layout entram com `resize`
  (`enforce`), um quadro depois de um layout aplicado, de uma região
  aparecer e de toda mudança de layout que não veio do usuário; e o painel
  tem um id por orientação (`panel`, `panel-right`);
- os tamanhos só são gravados quando o usuário arrasta uma divisão
  (`onLayoutChanged` com `isUserInteraction`), e as laterais e o painel
  mantêm os pixels quando a janela muda (`preserve-pixel-size`);
- maximizado, o painel é o único do grupo e volta a `preserve-relative-size`,
  porque a biblioteca exige um painel que acompanhe o grupo.

Numa janela estreita (`fitWorkbench`), primeiro os mínimos diminuem; se
ainda não couber, a barra lateral direita não é desenhada e depois o painel
vai para baixo. Isso só muda o desenho, não o layout.

**Regiões** (`components/layout/Region.tsx`). O cabeçalho é o título da
vista na barra lateral do lado da barra de atividades, que escolhe a vista;
nas outras regiões, são abas (com texto no painel, com ícone nas laterais).
Cada vista vem do catálogo (`components/layout/viewCatalog.tsx`: ícone,
componente e as marcas de Problemas e de saída nova) e leva o seu fundo
para a região onde estiver. Os botões de uma vista ficam no cabeçalho da
região: a vista os declara com `ViewActions`, e um portal os leva para lá
(`components/layout/ViewActions.tsx`). Os menus de contexto do layout
(`components/layout/layoutMenus.ts`) terminam todos com o submenu
Aparência, e a barra de abas do editor, que nunca some, tem o mesmo menu:
de qualquer parte à vista se volta às escondidas.

**Os terminais mudam de lugar.** O INFO, os terminais de etapa e o shell (`console/`) têm uma
pilha de donos: o último contêiner a montar fica com o elemento do xterm, e
ao sair o devolve ao anterior que ainda está na página. Um console que muda
de região, ou o shell disputado pela aba Terminal e pela gaveta do zen,
nunca fica órfão. O shell só pega o foco quando o usuário o pediu
(`revealView` com `explicit` e `takeFocus`); aparecer porque um layout foi
aplicado não tira o foco do editor. A Busca faz o mesmo.

**Modo zen** (`components/layout/Zen.tsx`). `useLayout.zen` faz o `App.tsx`
não montar a barra de menus, a de ferramentas, a de atividades e a de status;
ao entrar, `toggleZen` guarda em `zenSaved` a visibilidade das três regiões
e a maximização do painel, e ao sair as devolve (o `localStorage` guarda a
janela de antes do zen). No lugar da barra de status fica o `ZenHud`, que
também recebe a linha do Vim (`setVimStatusNode`); ele aparece também fora
do zen, quando o layout esconde a barra de status. A gaveta do shell
(`ZenShell`) é um `Panel` a mais no grupo da área central, que no zen é
sempre vertical, e usa o mesmo xterm da aba Terminal. `watchZen` põe e tira
a tela cheia (permissão `core:window:allow-set-fullscreen`) e mostra o
aviso de como sair. A coluna centralizada é a classe `app--zen-centered`
com a largura em `--zen-width`. Aplicar um layout sai do zen.

**Arrastar e soltar** (`components/sidebar/dnd.ts`). Dentro da janela, o
arrasto é feito com eventos de ponteiro, não com o drag-and-drop do HTML: no
Windows o Tauri desliga o do HTML quando recebe arquivos do sistema, e com
ponteiro o comportamento é o mesmo nas três plataformas. Os destinos se
marcam no DOM (`data-drop-dir` numa linha de pasta, `data-drop-section` em
Módulos e Testbenches) e são achados com `elementFromPoint`. Os arquivos do
gerenciador de arquivos do sistema chegam pelo evento de arrastar do Tauri
(`onDragDropEvent`, em `App.tsx`), com a posição em pixels físicos, e caem
nos mesmos destinos.

### 4.2 Estado

O estado da interface fica em stores do `zustand`, um por assunto:

| Store | Arquivo | Guarda |
|---|---|---|
| `useApp` | `state/app.ts` | versão, preferências, bundle, recentes, tema resolvido |
| `useProject` | `state/project.ts` | o retrato do projeto, o alvo (projeto ou processador), a versão da árvore |
| `useEditor` | `state/editor.ts` | as abas, os grupos do editor dividido e as abas de cada um, os documentos abertos, a aba e o grupo ativos, o cursor, a sessão |
| `useJobs` | `state/jobs.ts` | a operação rodando, o último resultado de cada fluxo, problemas, última síntese |
| `useSchematic` | `state/schematic.ts` | o RTLViewer: o estado do pacote `@lace/rtl-viewer` (o netlist carregado, o caminho na hierarquia, voltar e avançar, as opções da vista, no `localStorage`) |
| `useLayout` | `state/layout.ts` | a janela agora (`live`: regiões, vistas, barras, tamanhos), a maximização do painel, as marcas de não lido, o modo do explorador, o zoom e o zen; guardado no `localStorage` por conveniência (o zen não) |
| (sem store) | `state/savedLayouts.ts` | os layouts com nome: os prontos e os gravados no `settings.json`; aplicar, salvar, restaurar, renomear, excluir; `useLayoutStatus` (o em uso e se a janela está diferente dele) |
| `useSettingsPage` | `state/settingsPage.ts` | a página aberta das Preferências (no `localStorage`); "Personalizar layout..." a troca para a do layout antes de abrir a aba |
| `useTitleBar` | `state/titleBar.ts` | o sistema, se a janela já está sem a barra de título do sistema, se está maximizada, em tela cheia, com foco |
| `useDialogs` | `state/dialogs.ts` | o diálogo aberto; `prompt()` e `confirm()` devolvem promessas |
| `useToasts` | `state/toasts.ts` | os avisos rápidos; `showError` e `guarded` |
| `useHierarchy` | `state/hierarchy.ts` | a última hierarquia elaborada, se está desatualizada; atualiza depois de cada fluxo que passa |
| `useReports` | `state/reports.ts` | a versão da lista de relatórios; a limpeza (plano, confirmação, apagar) |
| `useDrag` | `components/sidebar/dnd.ts` | o arrasto em curso: o que, sobre qual destino, se pode soltar |
| `useAssistant` | `state/assistant.ts` | o assistente, experimental: a sessão aberta, a transcrição, o estado do turno, o rascunho e o custo da conversa |
| `useExperimental` | `state/experimental.ts` | os recursos experimentais ligados, lidos do `experimental.json` do Core e relidos quando a janela ganha o foco |
| `useLearn` | `state/learn.ts` | os exercícios do `lace learn`: a pasta aberta, a última correção de cada exercício e as dicas à mostra |
| `useUpdates` | `state/updates.ts` | o último `lace update --check`, a busca em andamento e a saída que ficou para depois de atualizar |
| `useTour` | `state/tour.ts` | o tour pela interface: se está aberto e em que passo |
| `useWaveReloads` | `state/waves.ts` | um contador por onda, que faz a aba aberta dela ler a onda de novo depois de uma simulação |

O que não é estado de React fica em módulos: o Monaco (`editor/monaco.ts`,
um modelo por arquivo), os consoles e o terminal (`console/`). Escrever uma
linha de simulação não redesenha componente nenhum.

### 4.3 Ações

`actions.ts` tem todas as ações do Studio, cada uma com rótulo (chave de
tradução), categoria, atalho, ícone, condição de habilitação e o que faz. A
barra de menus, a barra de ferramentas, o navegador de fluxo, a paleta de
comandos, a tabela de atalhos e o tratador de teclado leem dessa lista. Uma
ação que liga e desliga tem `checked` (a marca nos menus) e, quando o nome
no menu é outro, `menuLabel`: "Barra de status" com a marca no menu,
"Mostrar ou ocultar a barra de status" na paleta.

O tratador de teclado roda na fase de captura, antes do Monaco e do
xterm.js, para F8 (Wave) não virar "próximo problema" do Monaco. Dentro do
terminal de shell, só as teclas de função e as combinações com Ctrl+Shift
saem dele: Ctrl+C, Ctrl+W e companhia continuam sendo do shell. Com o modo
Vim ligado e o foco no editor, Ctrl+B e Ctrl+O também ficam com o editor.

Atalhos de duas etapas (`keys: 'Ctrl+K Z'`): a primeira tecla fica em
`useChord` (a barra de status mostra a espera) e a seguinte completa o
atalho ou só cancela. Com o foco no editor quem reconhece é o Monaco, que
tem os seus atalhos começando por Ctrl+K: `App.tsx` registra cada atalho de
duas etapas nele (`bindEditorChord`, em `editor/monaco.ts`), e o tratador
deixa o Ctrl+K passar. Com o Vim ligado é o contrário, porque o monaco-vim
fica com as teclas antes do Monaco. No terminal o Ctrl+K é do shell.

### 4.4 Editor

A área central tem de um a três grupos lado a lado, num `Group`
do `react-resizable-panels`. `useEditor` guarda cada aba aberta uma vez
(`tabs`) e, em `groups`, os ids das abas de cada grupo, na ordem da barra,
com a ativa de cada um; `activeId` é a ativa do grupo ativo, a que os menus,
os atalhos e a barra de status usam. Um arquivo pode estar em dois grupos
(é o mesmo modelo); uma vista, em um só. Toda mudança de grupos passa por
`settle`, que tira os grupos vazios (fica sempre um), escolhe o novo grupo
ativo e descarta os modelos dos arquivos que saíram do último grupo.

Cada grupo tem uma instância do Monaco (`MonacoHost`), que fica montada
enquanto o grupo tiver aba de arquivo e só se esconde quando a ativa dele é
uma vista. `editor/host.ts` guarda o editor de cada grupo, e as ações de
"Editar" chegam ao do grupo ativo. O foco num editor torna o grupo dele o
ativo; ativar um grupo por fora (clique numa aba, Ctrl+2) põe o foco no
editor dele. Cada arquivo aberto é um modelo, criado ao abrir e descartado
quando a última aba dele fecha; a posição do cursor e da rolagem de cada
arquivo é guardada ao trocar de aba. A aba aberta com um clique no
explorador é provisória (título em itálico) e é trocada pela próxima
provisória do mesmo grupo, como no VS Code; editar ou dar duplo clique a
fixa.

As abas se arrastam com eventos de ponteiro, como a árvore do explorador:
`EditorArea` acha o destino pelo `data-editor-group` debaixo do ponteiro (a
posição na barra de abas, o fim do grupo ou a faixa da direita, que cria um
grupo) e chama `dropTab` ou `moveTo`. A sessão gravada por projeto guarda
os grupos (`{ groups: [{ tabs, active }], activeGroup }`); a de antes dos
grupos (`{ tabs, active }`) ainda é lida, como um grupo só.

Linguagens: C± (`cmm`) e assembly do SAPHO (`sapho-asm`), portadas da
AURORA e conferidas contra os léxicos do YANC (os pontos em que as duas
divergiam estão comentados em `editor/languages/`); Verilog e SystemVerilog
do próprio Monaco; JSON para o `.spf`.

Diagnósticos do Lace com arquivo e linha viram marcadores no editor
(`setDiagnostics`); os de um arquivo que ainda não está aberto ficam
guardados e entram quando ele abrir.

O modo Vim (`editor/vim.ts`) é o `monaco-vim`, ligado e desligado em cada
editor pela preferência `editor.vim_mode`; cada editor tem o seu adaptador.
O modo e a linha de comando vão para um elemento da barra de status, onde
cada adaptador escreve no seu pedaço e só o do editor com o foco aparece. O pacote declara primeiro, para
navegador, um UMD que chama `require`, e importa `monaco-editor/esm/vs/...`,
caminho que o Monaco 0.57 não exporta; o `vite.config.ts` aponta o import
para a versão ESM e redireciona os caminhos do Monaco, para os dois usarem o
mesmo Monaco.

### 4.5 Consoles

Um xterm.js somente leitura para o INFO, que recebe tudo o que as operações
escrevem, e um por etapa, que correspondem aos terminais da AURORA. Os de etapa
começam fechados (`DEFAULT_BODY.hidden`) e abrem pelo seletor de terminal:

| Etapa | Na AURORA | Passos do Lace |
|---|---|---|
| C± | TCMM | `preprocess`, `compile` |
| ASM | TASM | `pre_assemble`, `assemble` |
| Verilog | TVERI | `check_syntax`, `lint` |
| Wave | TWAVE | `elaborate`, `verilate`, `simulate` |
| RTLViewer | TRTLViewer | `synthesize` (`graph` e `render` só no fluxo `schematic`, que o Studio não usa mais) |
| Placa | | os passos do Quartus (`lace fpga build` e `program`) |

Cada operação é uma rodada no INFO (`beginRun` e `endRun` em
`console/consoles.ts`): cabeçalho com a operação e a hora, as fases, e o fim
com o status, o tempo e a contagem de problemas. O formato de cada linha está
em `console/format.ts`: a cor só marca como algo terminou (os marcadores e a
palavra de gravidade das mensagens das ferramentas), o negrito marca rodada e
fase, e a saída das ferramentas sai recuada embaixo do nome de quem a
escreveu. Cada linha leva a etapa do passo que a escreveu (`STEP_CHANNEL` em
`state/jobs.ts`); o comando de cada operação e os avisos dela levam a etapa
onde ela começa (`START_CHANNEL`). O terminal da etapa, se aberto, recebe a
linha junto com o INFO; um aberto depois recebe as que a etapa já escreveu,
guardadas em `consoles.ts` (as últimas 20.000), com o cabeçalho de cada
rodada em que ela escreveu. O que está aberto e na frente fica no layout da
janela, e um layout gravado antes do INFO o ganha no lugar do primeiro
console, com os de etapa fechados (`withInfoConsole` em
`state/layoutModel.ts`). A saída do `lace install` e do
`lace update` fica na tela do bundle (`cliLog`, em `views/ToolchainView.tsx`). Links `arquivo:linha[:coluna]` abrem o arquivo no editor.
O terminal de shell (o TCMD) é outro xterm.js, ligado a um pseudoterminal
do backend: no Windows, o PowerShell ou o `cmd.exe`, pela preferência
`terminal_shell`; nos outros sistemas, o `$SHELL` do usuário.

### 4.6 Tradução e tema

Os textos ficam em `i18n/pt.ts` (a referência) e `i18n/en.ts`, com as
mesmas chaves (o TypeScript confere). `useT()` faz o componente se
redesenhar quando o idioma muda.

As cores vêm do tema. Cada tema de `themes/catalog.ts` é um
objeto com as cores da interface, os papéis da sintaxe, as cores do editor
e as dos terminais, e dele saem:

| O quê | Onde | Como |
|---|---|---|
| variáveis CSS (`--bg-0`, `--text-1`, `--brand`...) | `themes/index.ts`, `applyThemeCss` | um `<style id="lace-theme">` com `:root { ... }`, e `data-theme` (`dark` ou `light`) no `<html>` |
| tema do Monaco | `editor/monaco.ts`, `setMonacoTheme` | regras de token pelos papéis da sintaxe, mais as das gramáticas C± e asm (`cmmTokenRules(syntax)`) |
| tema do xterm.js | `console/consoles.ts`, `terminalTheme` | as 16 cores ANSI; o shell troca o cursor (`shell.ts`) |

`useApp.theme` é o tema resolvido (o "Do sistema" vira o Atlas ou o Atlas
Branco). `App.tsx` repassa a troca aos consoles e ao shell; o `MonacoHost`,
ao Monaco. As medidas (fontes, espaços, raios, alturas) ficam em
`styles/tokens.css`.

### 4.7 RTLViewer

O esquemático sai do netlist da síntese (`hierarchy.json`, o `write_json`
do Yosys), sem Graphviz. A síntese do Studio pede `schematic: false`; o
`show` + `dot` do Core fica para a CLI (`lace synth --svg`).

O viewer em si é o pacote `@lace/rtl-viewer`, em `packages/rtl-viewer/` do
repositório, que a extensão do VS Code e o `lace synth --open` também usam.
O Studio compila o fonte dele direto (o alias e o `dedupe` em
`vite.config.ts`, o `paths` em `tsconfig.json`), e o CSS dele vem em
`main.tsx`. Do lado do Studio ficam o estado (`state/schematic.ts`, o do
pacote com o leitor do netlist pelo backend) e a aba
(`components/views/SchematicView.tsx`): o netlist da síntese ou o que ficou
no disco, o "Ir ao código" pelo editor, a exportação pelo diálogo de salvar,
"Sintetizar de novo" e os botões e o menu do Studio, que a vista do pacote
recebe.

| Etapa | Arquivo, em `packages/rtl-viewer/src/` | O que faz |
|---|---|---|
| Netlist | `yosys.ts` | tipos do JSON, nomes legíveis (`$paramod...` → `fir_tap`), parâmetros, constantes, o `src` |
| Grafo | `graph.ts` | nós, redes e arestas de um módulo: liga os bits em barramentos, com split e join onde o barramento se parte ou se junta; constantes e redes globais (entrada com 8 destinos ou mais) viram etiqueta na porta, sem nó |
| Símbolos | `cells.ts` | a família de cada célula (a cor), o símbolo e a geometria, com as portas em posição fixa |
| Layout | `layout.ts`, `engine.ts` | o ELK (layered, ortogonal, `BRANDES_KOEPF`) num Web Worker, com cache por netlist e módulo; quem abriga cria o worker (`setLayoutWorker`) |
| Desenho | `Scene.tsx` | SVG no DOM; as cores são as `--sch-*` do tema (`schematicColors`, em `themes/index.ts` do Studio) |
| Vista | `RtlViewer.tsx`, `Canvas.tsx`, `panels.tsx` | barra, trilha, busca, zoom e arraste, destaque, árvore e detalhes |
| Exportar | `export.ts` | o desenho sem destaque, com as cores calculadas escritas em cada elemento |

As cores das famílias vêm da paleta dos terminais do tema (amarelo,
azul, magenta, ciano, verde), que todo tema define com matizes distintos;
o preenchimento é a mistura com o fundo do editor. Trocar o tema repinta
sem refazer o layout.

O `NETWORK_SIMPLEX` alinha melhor que o `BRANDES_KOEPF`, mas levou 199 s
no `ula_fdiv` do proc_fft; o `BRANDES_KOEPF`, menos de 2 s. Constante
e rede global como nó atrasavam o layout e, numa cadeia de instâncias (os
32 taps do fir), empurravam cada vizinho e o desenho descia em diagonal.

## 5. Uma operação, do clique ao resultado

Exemplo: F8 (Wave) com o alvo no projeto.

1. `actions.ts` monta o pedido `{ flow: 'simulate', simulator, timeout_s,
   open_wave }` e chama `useJobs.run`.
2. `useJobs.run` grava os arquivos abertos (compilar o que está na tela),
   marca a operação como rodando e chama `api.flow.start` com um `Channel`.
3. `jobs::flow_start` pede ao servidor o comando do fluxo (`flow.command`:
   `lace-studio sim --open`), decide se a onda abre em janela ou numa aba
   (preferência `wave_viewer` e o cliente web no bundle), reserva a vaga
   (`busy` se já houver uma), manda `started` e, numa thread de trabalho,
   pede o `flow.run` ao servidor. Devolve o identificador.
4. No servidor, o `flow.run` chama `lace_core::flows::run`, a mesma
   composição do `lace sim`: compila os processadores que têm fonte
   (`build_processors`, parando no primeiro que falhar), simula o projeto
   (`simulate_project`), grava o relatório (`history::record`) e, com
   `wave_viewer` em `window`, abre a onda em janela (`open_wave`); com
   `tab`, o padrão, e o cliente web no bundle, o servidor não abre, e o
   Studio acrescenta ao resultado a onda em `wave_tab` para a interface
   abrir a aba.
5. O Lace avisa cada linha que as ferramentas escrevem, e o servidor junta
   os eventos em lotes de até 500 ou 30 ms (`lace_core::delivery`) e os
   manda como notificações do pedido. Fases (`phase`) e resultados de build
   (`build`) vão na ordem, entre os lotes. O Studio repassa cada notificação
   à interface pelo `Channel`, com o `type` que ela lê.
6. A interface escreve cada linha no INFO, embaixo do nome da ferramenta, e
   no terminal da etapa, se ele estiver aberto. O INFO vem para a frente uma
   vez, no começo da rodada, e não troca de aba no meio dela.
7. No fim, a mensagem `finished` traz o `FlowOutcome` (com a saída de cada
   passo cortada em 256 KiB pelo servidor, guardando o fim: as linhas já
   chegaram pelos eventos). A interface escreve o resumo, preenche Problemas
   e os marcadores, relê o retrato do projeto e, numa síntese, abre o
   RTLViewer (seção 4.7).

Parar (Shift+F5) chama `flow_cancel`, que manda o `$/cancel` do pedido ao
servidor (ou assim que ele sair, se o cancelamento vier antes). O Lace
encerra o processo do passo com tudo o que ele iniciou e devolve o
resultado com `status: cancelled`, que chega pelo mesmo caminho.

## 6. Contrato entre backend e interface

- **Tipos do protocolo:** `src/ipc/lace-types.ts` é gerado por
  `npm run gen:types` a partir de `docs/schema/*.json` da raiz, que o
  próprio Lace gera dos tipos Rust e confere por teste. Mudou um tipo
  público ou um método no Lace: rode de novo e confira o diff. O schema dos
  pedidos também vai embutido no backend, que sabe por ele quais métodos
  têm `project`.
- **Versão do protocolo:** o backend fala a versão 1
  (`server::PROTOCOL_VERSION`) e recusa um `lace` de outra; um teste confere
  a constante contra a do Lace.
- **Tipos do Studio:** os structs de `src-tauri/src` e as interfaces de
  `src/ipc/types.ts` são mantidos à mão, lado a lado, com o arquivo Rust
  citado no comentário de cada interface. Vários tipos que a interface
  recebe são do protocolo e já vêm gerados em `lace-types.ts`
  (`ProjectStatus`, que é o `ProjectSnapshot`, `ToolchainInfo`,
  `FlowOutcome`, `Phase`, `WaveOpened`, `LayoutConversion`), mas
  `types.ts` ainda os copia à mão. Mudou um, mude o outro e o
  [IPC.md](IPC.md).
- **Nomes de argumento:** o Tauri converte os argumentos de primeiro nível
  de `camelCase` (JavaScript) para `snake_case` (Rust); os campos de structs
  passados inteiros (`request`, `settings`) vão como estão, em `snake_case`.
  `api.ts` cuida disso.

## 7. Segurança

- A interface só carrega o próprio código: CSP com `default-src 'self'`,
  sem script de fora (`tauri.conf.json`).
- O esquemático é desenhado pelo Studio (React), não um SVG de fora posto
  no DOM.
- Os plugins têm só as permissões da janela principal
  (`capabilities/default.json`): diálogos de arquivo, abrir caminhos e URLs
  no sistema, título, zoom, mostrar a janela, tela cheia, os botões e o arraste da barra de título integrada (minimizar, maximizar, tirar a moldura, arrastar) e fechar a janela.
- Criar, renomear e apagar só dentro da pasta do projeto; apagar vai para a
  lixeira.
- O processo das ferramentas recebe o ambiente que o Lace monta (vazio
  mais o mínimo), no servidor; o Studio não passa o próprio ambiente para
  elas.
- O servidor do Lace fala só pelo stdin e pelo stdout do processo filho:
  não abre porta nenhuma.

## 8. Pendências no Lace

Coisas que o Studio faz por conta própria e que deveriam ir para o Lace,
onde mora a regra de negócio. Hoje não há nenhuma. Em 2026-10-26 o Studio
virou cliente do servidor do Lace e deixou de linkar o `lace-core`, o
`lace-learn` e o instalador; o que ele fazia com eles virou método do
protocolo:

| Era do Studio | Foi para |
|---|---|
| Os comandos que repetiam um método (retrato, ordem, conversão, nomes, configurar e remover processador, saídas, histórico, recursos experimentais, placa FPGA, conferência do bundle, sinais e layout salvo da onda) | os métodos do protocolo, pelo `lace_call` |
| Arquivos, topo, testbench, processador novo, mover com substituir, escolher sinais, abrir a onda | `project.add`, `project.remove`, `project.top`, `project.testbench`, `processor.add`, `project.move` com `overwrite`, `wave.choose` com `set`, `wave.open` (com o layout do exercício) |
| Os fluxos, a correção de um exercício, a entrega em lotes e o corte da saída | `flow.run` (com o fluxo `learn` e `step_output_limit`) e `flow.command` |
| Instalar, atualizar, as linhas do console do instalador | `install.add`, `update.apply` (com `reopen_studio`), `update.check` e o `line` da notificação `install` |
| Os padrões de processador, os módulos de um texto, os nomes de testbench, o topo da placa, o layout da aba de onda, os exercícios do `lace learn` | `processor.defaults`, `verilog.modules`, `verilog.testbench_names`, `fpga.top`, `wave.layout`, `learn.tracks`, `learn.status`, `learn.init`, `learn.set_current`, `learn.reset` |

O que ficou no Studio é dele: achar o `lace` que serve (como a extensão do
VS Code), esconder o console e o Job Object do servidor no Windows, os
arquivos, o terminal, a vigia, as preferências e a onda numa aba. O
assistente ainda linka o `lace-agent`, que traz o core; ele vai para o
protocolo na 1.0.

Antes disso, em 2026-10-19, a composição dos fluxos, a busca da instalação,
a leitura das portas de saída e instalar e atualizar tinham saído do Studio
para o Lace, que o Studio chamava em processo; e em 2026-10-04, a
hierarquia elaborada, mover e renomear com o `.spf` em dia e a regra de
nome de projeto.
