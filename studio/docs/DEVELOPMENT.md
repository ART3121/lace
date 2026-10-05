# Desenvolvimento

Como preparar a máquina, rodar e testar o Studio, e onde mexer para cada
tipo de mudança. A arquitetura está em [ARCHITECTURE.md](ARCHITECTURE.md).

## 1. Preparar a máquina

| O quê | Versão | Para quê |
|---|---|---|
| Rust | 1.90 ou mais novo (`rust-version` do `src-tauri/Cargo.toml`) | o backend; o Tauri 2.12 pede 1.90 |
| Node.js | 22 | a interface (Vite, TypeScript) |
| Dependências de sistema do Tauri | ver abaixo | WebView e GTK |
| O repositório do Lace | o Studio está em `studio/` dele | o `lace-core` entra por caminho (`../crates/lace-core`, em `src-tauri/Cargo.toml`) e os tipos saem de `../docs/schema` |
| Uma instalação do Lace | qualquer | rodar o Studio e os testes de fluxo; ou um bundle em `LACE_TEST_BUNDLE` |

Dependências de sistema, conforme a página de pré-requisitos do Tauri 2
(<https://v2.tauri.app/start/prerequisites/>), confira lá a lista atual:

- **Fedora:** `sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel` e o grupo de desenvolvimento C (`sudo dnf group install c-development`).
- **Debian e Ubuntu:** `sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`.
- **macOS:** as Command Line Tools do Xcode.
- **Windows:** as Build Tools do Visual Studio com o C++ e o WebView2 (já vem no Windows 10 e 11 atualizados).

O Studio está no repositório do Lace, em `studio/`:

```
lace/
  crates/lace-core/   o Core, que o Studio usa por caminho
  docs/schema/        os tipos que o npm run gen:types lê
  studio/             este
```

O `src-tauri/` tem o próprio workspace do Cargo e o próprio `Cargo.lock`:
o `Cargo.toml` da raiz o deixa de fora (`exclude`), para os testes do Core
e da CLI não precisarem do WebView nem da interface compilada. O CI roda as
checagens do Studio no job `studio` do `ci.yml`, e os fluxos dele, com o
bundle instalado, no `installers.yml`.

## 2. Rodar

```sh
npm install
npm run tauri dev
```

O `tauri dev` sobe o Vite em `http://localhost:1420`, compila o backend e
abre a janela. Salvar um arquivo de `src/` recarrega a interface na hora;
salvar um de `src-tauri/` recompila e reabre o aplicativo. A primeira
compilação do backend leva vários minutos (o Tauri e o WebKitGTK).

Log do backend no terminal: `RUST_LOG=lace_studio_lib=debug,lace_core=debug npm run tauri dev`.
O inspetor da WebView abre com Ctrl+Shift+I, só na build de
desenvolvimento (o menu de contexto do navegador fica desligado fora dos
campos de texto, em `src/main.tsx`).

## 3. Conferir

| Comando | O que confere |
|---|---|
| `npm run typecheck` | os tipos da interface |
| `npm run build` | os tipos e o empacotamento da interface (Vite) |
| `cd src-tauri && cargo clippy` | o backend, com os avisos do clippy |
| `cd src-tauri && cargo test` | os testes do backend: unidade e os fluxos de verdade |
| `npm run tauri build` | o aplicativo inteiro e os instaladores da plataforma |

Os testes de `src-tauri/tests/flows.rs` rodam check, sim, synth e build
sobre cópias dos exemplos do Lace, com o bundle que o Studio acharia
sozinho ou com `LACE_TEST_BUNDLE=/caminho/do/toolchain`. Sem bundle, avisam e
passam; com `CI` definido, falham.

Antes de mandar uma mudança: `typecheck`, `clippy` sem aviso, `cargo test`
e, se mexeu na interface, abrir o Studio e passar pelo que mudou.

### Roteiro de fumaça

Na build de desenvolvimento, a variável `LACE_STUDIO_SMOKE` faz a interface
rodar uma sequência de ações sozinha, pelo mesmo caminho dos botões
(`src/dev/smoke.ts`). Serve para conferir a interface de ponta a ponta sem
mouse nem teclado, e para capturar a tela de cada passo:

```sh
LACE_STUDIO_SMOKE="wait:2000;open:rtl/contador.v;check;fastSim;synthesize;view:schematic" \
RUST_LOG=info npm run tauri dev
```

Os passos são ids de ação de `actions.ts` (`build`, `check`, `lint`,
`fastSim`, `synthesize`, `toggleTheme`...) ou:

| Passo | Faz |
|---|---|
| `wait:<ms>` | espera |
| `open:<caminho relativo>` | abre um arquivo do projeto |
| `aside:<caminho relativo>` | abre um arquivo no grupo do editor à direita |
| `project:<.spf>` | abre outro projeto |
| `add:<caminho>` | registra um Verilog como Projeto > Adicionar arquivos Verilog, sem o diálogo (caminho absoluto, ou relativo à pasta do projeto) |
| `target:<processador>` | escolhe o alvo (vazio: o projeto) |
| `view:<tipo>[:<nome>]` | abre uma vista |
| `panel:<aba>`, `sidebar:<vista>`, `explorer:<modo>` | mostra uma aba do painel, uma vista da barra lateral, um modo do explorador |
| `click:<seletor>` | clica no elemento |
| `focus:<seletor>` | põe o foco no elemento |
| `key:<tecla>` | manda a tecla ao elemento com foco |
| `type:<seletor>\|<texto>` | escreve num campo, como se fosse digitado |
| `drag:<de>\|<para>` | arrasta com eventos de ponteiro, de centro a centro |
| `osdrop:<seletor>\|<caminho>...` | emite o evento de arrastar do Tauri, como se os arquivos viessem do gerenciador de arquivos e caíssem no centro do elemento |
| `framekey:<tecla>` | uma tecla (`F7`, `Ctrl+B`) como o cliente web da aba de onda aberta a repassa ao Studio: uma mensagem vinda do iframe |
| `editor` | escreve no log os grupos do editor e as abas de cada um (`*` marca o grupo ativo, `>` a aba ativa) |
| `rects:<seletor>` | escreve no log a posição e o tamanho de cada elemento que casa |
| `count:<seletor>` | escreve no log quantos elementos casam |

Cada passo concluído sai no log como `smoke: <passo>`, com o diálogo aberto
entre colchetes (`newFolder [prompt]`), e o fim como `smoke: done`. Uma
ação que abre um diálogo não é esperada até o fim: o roteiro segue para os
passos que o preenchem. Na build de release a variável é ignorada.

Quando a janela abre em branco, o mais rápido é carregar a interface no
Chromium sem interface e ler o console: os erros de carregamento de módulo
(um import que o Vite não resolveu, por exemplo) aparecem lá, antes de
qualquer chamada ao Tauri.

```sh
chromium-browser --headless=new --enable-logging=stderr --virtual-time-budget=20000 \
  --dump-dom http://localhost:1420/ 2>&1 >/dev/null | grep CONSOLE
```

Os erros da interface (exceções, `console.error`, avisos de erro) também vão
para o log do backend, com o alvo `lace_studio::ui`, e também quanto a
abertura levou (`Startup: modules loaded in ... ms, first frame at ...
ms`, contado da navegação da WebView).

A janela só aparece quando a interface está desenhada (ARCHITECTURE.md,
4.1). No desenvolvimento isso leva de 1,5 s a 3 s com o Vite já aquecido, e
mais na primeira vez depois de subir o Vite, que pré-empacota as
dependências; se o Vite não estiver rodando, a janela aparece vazia depois
de 10 s. Um script que espera a janela (o `smoke.sh` da seção anterior, o
`wmctrl`) precisa esperar esse tempo.

## 4. Onde mudar o quê

### Uma ação nova (botão, menu, atalho)

1. Acrescente a ação em `src/actions.ts` (`ACTIONS`): `id`, chave de
   tradução, categoria, atalho, ícone, `enabled` e `run`.
2. Acrescente a chave em `src/i18n/pt.ts` e `src/i18n/en.ts`.
3. Para aparecer num menu, ponha o `id` na lista do menu em
   `src/components/layout/MenuBar.tsx`. A paleta de comandos e a tabela de
   atalhos a pegam sozinhas.

### Um comando novo no backend

1. Escreva a função em `src-tauri/src/commands/<assunto>.rs`, com
   `#[tauri::command]`, `async` e o trabalho dentro de `state::blocking`.
   Devolva `IpcResult<T>`; erro do Core vira `IpcError` com `?`.
2. Registre em `tauri::generate_handler!` (`src-tauri/src/lib.rs`).
3. Espelhe o tipo de retorno em `src/ipc/types.ts` e acrescente a função em
   `src/ipc/api.ts`.
4. Documente em [IPC.md](IPC.md).
5. Lógica de orquestração (que ferramenta rodar, em que ordem) não entra
   aqui: vai para o Lace (ADR 0001 do Lace). Se precisar mesmo, isole-a em
   `flows.rs` e anote em ARCHITECTURE.md, seção 8.

### Uma vista nova (aba central)

1. Crie o componente em `src/components/views/`.
2. Acrescente o tipo em `ViewKind` (`src/state/editor.ts`) e, se a vista
   tiver parâmetro, o identificador em `viewTabId`.
3. Ligue em `ViewContent`, `tabTitle` (`EditorArea.tsx`) e `tabIcon`
   (`tabIcons.ts`).

### Um tipo do Core mudou

```sh
npm run gen:types     # relê ../docs/schema/*.json
git diff src/ipc/lace-types.ts
```

`LACE_REPO=/outro/lace npm run gen:types` lê de outro lugar.

### Cores, temas e medidas

As cores ficam nos temas, em `src/themes/catalog.ts` (ADR 0010); cada tema
dá as cores da interface, da sintaxe, do editor e dos terminais, e o Monaco
e os consoles saem dele. Componente usa variável CSS (`var(--text-1)`),
nunca cor literal. Uma variável nova entra em `UiColors`
(`src/themes/model.ts`) e em todos os temas; o TypeScript aponta o tema que
ficou sem ela.

Para um tema novo, um objeto a mais no catálogo, com a fonte das cores na
tabela do começo do arquivo. O id não muda depois de lançado: ele fica
gravado no `settings.json` de quem escolheu o tema.

O padrão, Atlas, é neutro e sem enfeite (ADRs 0005 e 0007): cor só onde
informa algo, azul do CERN nos detalhes. As medidas (fontes, espaços, raios,
alturas) ficam em `src/styles/tokens.css`.

### Gramáticas de C± e do assembly

`src/editor/languages/cmm.ts` e `asm.ts`. A fonte da verdade é o léxico do
YANC (`CMMComp.l`, `ASMComp.l`, `isa.tsv` no repositório do YANC), não a
AURORA: onde as duas divergem há um comentário dizendo o que vale.

## 5. Convenções

- Comentários e documentação em português; identificadores em inglês, como
  no Lace.
- Texto de interface sempre por `t('chave')`, nunca literal no componente.
- Sem travessão nem emoji em texto de interface, comentário ou documento.
- Rust: `cargo fmt`, `clippy` sem aviso, `#![forbid(unsafe_code)]`.
- Um commit por assunto, com a mensagem em português, como no Lace.

## 6. Versão e distribuição

A versão está em três lugares, que andam juntos: `package.json`,
`src-tauri/Cargo.toml` e `src-tauri/tauri.conf.json`. Anote a mudança em
[CHANGELOG.md](../CHANGELOG.md).

Os ícones do app em `src-tauri/icons/` saem de `public/brand/lace-icon.svg`.
O `tauri icon` gera também os de Android e iOS, que o Studio não usa, então
gere numa pasta à parte e copie só os que já existem:

```sh
npx tauri icon public/brand/lace-icon.svg -o /tmp/lace-icons
for f in src-tauri/icons/*; do cp "/tmp/lace-icons/$(basename "$f")" "$f"; done
```

`npm run tauri build` gera os instaladores da plataforma em que roda
(`.deb`, `.rpm` e AppImage no Linux; `.dmg` no macOS; `.msi` e `.exe` no
Windows). Para os três sistemas, o caminho é um workflow de CI com uma
máquina de cada; ainda não existe (ver [AURORA_PARITY.md](AURORA_PARITY.md),
Fase 2, atualização do Studio).
