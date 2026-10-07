# Arquitetura do Lace Studio

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
┌──────────────────────────── processo do Lace Studio ────────────────────────────┐
│                                                                                 │
│  WebView (React)                          Rust (src-tauri)                      │
│  ┌────────────────────────┐   invoke    ┌─────────────────────────────┐         │
│  │ componentes            │ ──────────▶ │ commands/*  (casca fina)    │         │
│  │ stores (zustand)       │             │ flows.rs    (composição)    │──▶ lace-core
│  │ Monaco, xterm.js       │ ◀────────── │ jobs.rs     (thread, lotes) │   (biblioteca)
│  └────────────────────────┘   Channel   │ terminal.rs (PTY)           │         │
│              ▲                 + emit   │ watcher.rs  (notify)        │         │
│              └───────────────────────── │ toolchain.rs, settings.rs   │         │
│                                         └──────────────┬──────────────┘         │
└────────────────────────────────────────────────────────┼────────────────────────┘
                                                         │ só para instalar e atualizar
                                                         ▼
                                              lace install / lace update --json
                                                         │
                                  bundle do Lace: YANC, Icarus, Verilator, Yosys, dot, surfer-aurora
```

Três regras orientam tudo:

- **A regra mora no Lace.** O backend chama o `lace-core` como biblioteca
  Rust, do jeito que o Lace prevê para uma GUI: abre o projeto,
  chama a função, devolve o resultado serializado. O Studio não sabe montar
  a linha de comando do `iverilog` nem ler a saída do `cmmcomp`; quem sabe é
  o Core. A exceção, isolada e documentada, é a composição dos fluxos
  ([seção 8](#8-pendências-no-lace)).
- **O disco é a fonte da verdade.** O Studio guarda o caminho do `.spf` e
  reabre o projeto a cada comando, como a CLI faz a cada execução. O `.spf`
  pode mudar por fora (a CLI no terminal do próprio Studio, um editor de
  texto), e um `Project` aberto não percebe (API.md do Lace, 3.2).
- **A interface nunca bloqueia.** Todo comando que toca o disco roda numa
  thread de bloqueio (`state::blocking`); as operações longas rodam numa
  thread própria e mandam o progresso por um `Channel` (seção 5).

## 2. Pastas

```
lace-studio/
  src/                       a interface (React + TypeScript)
    main.tsx, App.tsx        entrada; a janela e as ligações globais
    actions.ts               todas as ações: menus, barra, paleta e atalhos leem daqui
    ipc/
      api.ts                 uma função por comando Tauri
      types.ts               os tipos do Studio (espelho dos structs Rust)
      lace-types.ts          os tipos do Core, GERADOS dos JSON Schemas do Lace
    state/                   stores: app, project, editor, jobs, layout, dialogs, toasts, hierarchy, reports, schematic;
                             layoutModel.ts e savedLayouts.ts (o modelo e os layouts com nome da janela)
    schematic/               o PRISM: netlist do Yosys, grafo, layout (ELK num worker), desenho, painéis, exportação
    editor/                  Monaco: workers, tema, modelos, marcadores, gramáticas C± e asm, modo Vim
    console/                 os consoles xterm.js e o terminal de shell
    components/              layout/, sidebar/, editor/, panel/, views/, dialogs/
    i18n/                    pt.ts (referência), en.ts, t() e useT()
    themes/                  os temas: model.ts (o formato), catalog.ts (os 15), index.ts (aplicar)
    styles/                  tokens.css (medidas), app.css
    util/paths.ts            caminhos com / e \
    dev/smoke.ts             roteiro de fumaça, só na build de desenvolvimento
  src-tauri/                 o backend (Rust)
    src/lib.rs               registro dos plugins, do estado e dos comandos
    src/commands/            app, project, files, history, toolchain
    src/flows.rs             a composição dos fluxos (espelho de lace-cli/src/commands.rs)
    src/jobs.rs              operações em thread, cancelamento, eventos em lote
    src/terminal.rs          o terminal de shell (portable-pty)
    src/watcher.rs           vigia da pasta do projeto (notify)
    src/wave_tab.rs          a onda numa aba: o servidor local do cliente web do Surfer
    src/toolchain.rs         onde está o bundle do Lace
    src/settings.rs          preferências e recentes (settings.json)
    src/state.rs, error.rs   estado do aplicativo e o erro dos comandos
    tests/flows.rs           os fluxos rodando de verdade contra o bundle
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
| `project` | o caminho do `.spf` aberto; o projeto em si é relido a cada comando (`AppState::project`) |
| `job` | a operação rodando (identificador e `CancelToken`); uma por vez |
| `terminals` | os pseudoterminais abertos |
| `watcher` | o vigia da pasta do projeto |

### 3.2 Comandos

Cada comando Tauri é uma função curta em `commands/`: valida a entrada,
chama o Core (ou o sistema de arquivos) e devolve `Result<T, IpcError>`. Os
que tocam o disco passam por `state::blocking`, que roda o trabalho numa
thread de bloqueio do runtime assíncrono do Tauri, fora da thread da
interface.

| Módulo | Comandos |
|---|---|
| `commands::app` | versão, preferências, recentes, abrir a onda (`lace wave`) |
| `commands::project` | abrir, criar, fechar, retrato do projeto (`lace status`), `add`, `remove`, `top`, testbench, `proc add`, `proc set`, valores de saída |
| `commands::files` | ler pasta, ler e gravar texto (com detecção de conflito), criar, renomear, lixeira, lista para o "abrir rápido", busca |
| `commands::history` | `lace report list`, `show`, `compare` |
| `commands::toolchain` | `lace tools [--verify]`, `lace update [--check]`, `lace install` |
| `jobs` | começar, cancelar e consultar a operação (build, check, sim, synth, esquemático) |
| `terminal` | abrir, escrever, redimensionar e encerrar o terminal de shell |
| `wave_tab` | a onda numa aba: registra a onda e o layout no servidor local que serve o cliente web do Surfer |

### 3.3 Erros

`IpcError` (`error.rs`) é `{ code, message }`. A separação da API do Lace se
mantém:

- `Err(IpcError)`: o Lace não conseguiu rodar (bundle, projeto, E/S). O
  `code` é o `LaceError::code()` do Core, estável, ou um dos códigos do
  Studio (`no_project`, `busy`, `conflict`, `outside_project`, `exists`,
  `cli`, `terminal`, `io`, `internal`). A interface traduz pelo código
  (`i18n`, chaves `error.<code>`) e mostra a mensagem original embaixo.
- Uma operação que rodou e falhou é `Ok`, com `succeeded: false` e os
  diagnósticos no resultado. Isso vai para os consoles, para o painel
  Problemas e para os marcadores do editor, não para um aviso de erro.

### 3.4 Onde está o bundle

A CLI acha o bundle ao lado do próprio executável. O Studio é outro
programa, então procura a instalação do Lace (`toolchain.rs`), nesta ordem:
a pasta das preferências, `LACE_TOOLCHAIN`, a pasta padrão do instalador, o
`lace` do `PATH` (resolvendo o atalho até a instalação) e a pasta do próprio
Studio. Uma pasta declarada e inválida é erro, sem cair para as outras, como
o `--toolchain` da CLI. O `PATH` só serve para achar a instalação; as
ferramentas continuam saindo do bundle.

O compilador do Verilator segue o `--compiler` da CLI: preferência ou
`LACE_COMPILER`. Uma pasta declarada sem `perl`, `make` e um compilador C++
não derruba o bundle: o Studio abre sem compilador, a simulação com
Verilator falha com `system_compiler_missing`, e a tela de ferramentas mostra
o motivo.

O bundle é reaberto a cada operação (ler meia dúzia de JSON pequenos), para
um `lace install` feito pelo terminal valer na hora.

### 3.5 Arquivos e vigia

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
destino no projeto. Mover e renomear na árvore (`project_move`) são o
`Project::move_path` do Core, que mantém o `.spf` em dia e recusa o que
fica no lugar (o `.spf`, `.lace/`, as pastas e o fonte dos processadores).
O Studio só acrescenta o substituir: com `overwrite`, se o Core responde
`path_exists`, o que está no destino vai para a lixeira e o movimento é
tentado de novo. O Core nunca sobrescreve.

### 3.6 Hierarquia e nome de projeto

A hierarquia é a operação `lace_core::hierarchy` (API.md do Lace, seção
5.4.1), chamada por `project_hierarchy` com o `Control` padrão: o Icarus
elabora o design e cada testbench, e o Core lê a árvore do `.vvp`. É o
Icarus qualquer que seja o simulador escolhido; sem ele, a vista mostra o
erro do componente.

A regra de nome de projeto também é do Core (`validate_project_name`): o
`Project::create` recusa, e o diálogo de projeto novo chama
`project_check_name` enquanto o usuário digita, para avisar antes. O de
processador novo faz o mesmo com `processor_check_name`
(`validate_processor_name`), e mantém o `#NUBITS` igual a `#NBMANT +
#NBEXPO + 1` enquanto o usuário muda a mantissa ou o expoente; o resto dos
parâmetros o Core confere ao criar (`invalid_parameter`).

### 3.7 Encerrar

Ao sair (evento `RunEvent::Exit`) e ao receber SIGTERM, SIGINT ou SIGHUP
(Unix), o Studio cancela a operação rodando, espera até 3 s e fecha os
terminais. Sem isso, no Linux e no macOS, as ferramentas, que o Lace roda
num grupo de processos próprio, continuariam rodando depois que o Studio
saísse (API.md do Lace, seção 9, "Limites"). No Windows, cada passo roda
num Job Object do Core e termina junto com o Studio mesmo se ele for
derrubado, e nenhuma ferramenta abre janela de console; a CLI chamada pelo
`run_cli` (`lace install`, `lace update`) usa o mesmo `ProcessJob` e o mesmo
`hide_console`.

## 4. Interface

### 4.1 Estrutura da janela

`App.tsx` monta, de cima para baixo: barra de menus, barra de ferramentas,
a área de trabalho (`components/layout/Workbench.tsx`) e a barra de status.
Diálogos, menu de contexto e avisos ficam por cima. O `.app` é um grid com
uma área por barra e linhas `auto`: uma barra que o layout esconde não é
montada, e a linha dela some sem deslocar as outras.

**Abertura.** A janela nasce escondida e com fundo escuro (`visible: false`
e `backgroundColor` em `tauri.conf.json`). A interface a mostra
(`reveal`, em `App.tsx`) depois de aplicar o tema e o zoom das preferências
e de reabrir o último projeto, com um limite de 1,5 s para um projeto
grande não segurar a abertura. Assim o primeiro quadro já é o de trabalho,
e não uma página branca enquanto o JavaScript carrega. Se a interface não
chegar a mostrar a janela (o Vite fora do ar, um módulo que não carregou),
o backend a mostra depois de 10 s (`REVEAL_FALLBACK`, em `lib.rs`). O log
registra quanto a abertura levou (`Startup: modules loaded in ... ms, first
frame at ... ms`).

**Layout** (`state/layoutModel.ts`, `state/layout.ts`,
`state/savedLayouts.ts`). Três regiões fixas, como no VS Code: a barra
lateral esquerda, a direita e o painel (embaixo ou à direita do editor). As
onze vistas (as quatro da barra lateral, os cinco consoles, Problemas e
Terminal) ficam cada uma em uma região e podem ir para qualquer outra. O
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

**Os terminais mudam de lugar.** Os consoles e o shell (`console/`) têm uma
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
| `useSchematic` | `state/schematic.ts` | o PRISM: o netlist carregado, o caminho na hierarquia, voltar e avançar, as opções da vista (no `localStorage`) |
| `useLayout` | `state/layout.ts` | a janela agora (`live`: regiões, vistas, barras, tamanhos), a maximização do painel, as marcas de não lido, o modo do explorador, o zoom e o zen; guardado no `localStorage` por conveniência (o zen não) |
| (sem store) | `state/savedLayouts.ts` | os layouts com nome: os prontos e os gravados no `settings.json`; aplicar, salvar, restaurar, renomear, excluir; `useLayoutStatus` (o em uso e se a janela está diferente dele) |
| `useDialogs` | `state/dialogs.ts` | o diálogo aberto; `prompt()` e `confirm()` devolvem promessas |
| `useToasts` | `state/toasts.ts` | os avisos rápidos; `showError` e `guarded` |
| `useHierarchy` | `state/hierarchy.ts` | a última hierarquia elaborada, se está desatualizada; atualiza depois de cada fluxo que passa |
| `useReports` | `state/reports.ts` | a versão da lista de relatórios; a limpeza (plano, confirmação, apagar) |
| `useDrag` | `components/sidebar/dnd.ts` | o arrasto em curso: o que, sobre qual destino, se pode soltar |

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

Um xterm.js somente leitura por canal, como os terminais da AURORA:

| Canal | Na AURORA | Passos do Lace |
|---|---|---|
| C± | TCMM | `preprocess`, `compile` |
| ASM | TASM | `pre_assemble`, `assemble` |
| Verilog | TVERI | `check_syntax`, `lint` |
| Wave | TWAVE | `elaborate`, `verilate`, `simulate` |
| PRISM | TPRISM | `synthesize` (`graph` e `render` só no fluxo `schematic`, que o Studio não usa mais) |

Cada linha vai para o console do passo que a escreveu (`STEP_CHANNEL` em
`state/jobs.ts`); o comando de cada operação e os avisos dela vão para o
console onde ela começa (`START_CHANNEL`). A saída do `lace install` e do
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

### 4.7 PRISM

O esquemático sai do netlist da síntese (`hierarchy.json`, o `write_json`
do Yosys), sem Graphviz. A síntese do Studio pede `schematic: false`; o
`show` + `dot` do Core fica para a CLI (`lace synth --svg`).

| Etapa | Arquivo | O que faz |
|---|---|---|
| Netlist | `schematic/yosys.ts` | tipos do JSON, nomes legíveis (`$paramod...` → `fir_tap`), parâmetros, constantes, o `src` |
| Grafo | `schematic/graph.ts` | nós, redes e arestas de um módulo: liga os bits em barramentos, com split e join onde o barramento se parte ou se junta; constantes e redes globais (entrada com 8 destinos ou mais) viram etiqueta na porta, sem nó |
| Símbolos | `schematic/cells.ts` | a família de cada célula (a cor), o símbolo e a geometria, com as portas em posição fixa |
| Layout | `schematic/layout.ts`, `engine.ts` | o ELK (layered, ortogonal, `BRANDES_KOEPF`) num Web Worker, com cache por netlist e módulo |
| Desenho | `schematic/Scene.tsx` | SVG no DOM; as cores são as `--sch-*` do tema (`schematicColors`, em `themes/index.ts`) |
| Vista | `components/views/SchematicView.tsx`, `schematic/Canvas.tsx`, `panels.tsx` | barra, trilha, busca, zoom e arraste, destaque, árvore e detalhes |
| Exportar | `schematic/export.ts` | o desenho sem destaque, com as cores calculadas escritas em cada elemento |

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
3. `jobs::flow_start` reserva a vaga (`busy` se já houver uma), cria o
   `CancelToken`, começa a thread de entrega e a thread de trabalho, e
   devolve o identificador.
4. Na thread de trabalho, `flows::run` faz a mesma composição do `lace sim`:
   compila os processadores que têm fonte (`build_processors`, parando no
   primeiro que falhar), simula o projeto (`simulate_project`), grava o
   relatório (`history::record`) e abre a onda (`open_waveform`).
5. O Core avisa cada linha que as ferramentas escrevem pelo receptor do
   `Control`. O receptor só manda o evento por um canal; a thread de entrega
   junta os eventos em lotes de até 500 ou 30 ms e os manda para a interface.
   Fases (`phase`) e resultados de build (`build`) vão na ordem, entre os
   lotes.
6. A interface escreve cada linha no console do passo e troca o painel para
   o console da fase.
7. No fim, a mensagem `finished` traz o `FlowOutcome` (com a saída de cada
   passo cortada em 256 KiB, guardando o fim: as linhas já chegaram pelos
   eventos). A interface escreve o resumo, preenche Problemas e os
   marcadores, relê o retrato do projeto e, numa síntese, abre o PRISM
   (seção 4.7).

Parar (Shift+F5) chama `flow_cancel`, que marca o `CancelToken`. O Core
encerra o processo do passo com tudo o que ele iniciou e devolve o resultado
com `status: cancelled`, que chega pelo mesmo caminho.

## 6. Contrato entre backend e interface

- **Tipos do Core:** `src/ipc/lace-types.ts` é gerado por
  `npm run gen:types` a partir de `docs/schema/*.json` da raiz, que o próprio
  Lace gera dos tipos Rust e confere por teste. Mudou um
  tipo público no Lace: rode de novo e confira o diff.
- **Tipos do Studio:** os structs de `src-tauri/src` e as interfaces de
  `src/ipc/types.ts` são mantidos à mão, lado a lado, com o arquivo Rust
  citado no comentário de cada interface. Mudou um, mude o outro e o
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
  no sistema, título, zoom e fechar a janela.
- Criar, renomear e apagar só dentro da pasta do projeto; apagar vai para a
  lixeira.
- O processo das ferramentas recebe o ambiente que o Core monta (vazio mais
  o mínimo); o Studio não passa o próprio ambiente para elas.

## 8. Pendências no Lace

Coisas que o Studio faz por conta própria e que deveriam ir para o Core,
onde mora a regra de negócio. Ficam isoladas para sair inteiras quando isso
acontecer.

| No Studio | Onde | Proposta para o Lace |
|---|---|---|
| A composição dos fluxos: compilar antes, parar no primeiro build que falhar, ler as portas de saída, gravar o relatório, abrir a onda | `src-tauri/src/flows.rs`, cópia de `lace-cli/src/commands.rs` | um módulo `lace_core::flows` com `check_flow`, `sim_flow`, `synth_flow`, que a CLI e o Studio chamem |
| Achar a instalação do Lace a partir de outro programa | `src-tauri/src/toolchain.rs` | `Toolchain::discover()` no Core, com a mesma ordem |
| Ler a porta de saída pelo nome do arquivo (`output_<n>.txt`) | `flows::port_values`, cópia da CLI | `Processor::output_ports()` no Core |
| Instalar e atualizar pela CLI (`lace install`, `lace update --json`) | `commands/toolchain.rs` | uma API em `lace-installer` que o Studio chame direto |

Já foram para o Core (2026-10-04): a hierarquia elaborada
(`lace_core::hierarchy`, e `lace hierarchy` na CLI), mover e renomear com
o `.spf` em dia (`Project::move_path`, e `lace move`) e a regra de nome de
projeto (`validate_project_name`). O Studio só os chama.
