# 0011. A onda numa aba, com o cliente web do Surfer

- **Status:** Aceita
- **Data:** 2026-10-04

## Contexto

A AURORA mostrava a onda numa aba do editor, com o layout dos processadores
SAPHO (variáveis, assembly e linha do C±). O Studio abria o surfer-aurora em
janela, sem layout. O layout passou a sair do Core (`wave_layout`, ADR 0011
do Lace), e o cliente web do surfer-aurora (o mesmo Surfer em WebAssembly,
da mesma tag do executável) passou a vir no bundle, em
`surfer-aurora/web/`.

A AURORA ligava esse cliente a um `surfer-aurora server` por aba, por um
servidor HTTP local que juntava tudo numa origem só (`main/ipc/surfer_tab.js`).
Testado no Studio, esse caminho perde o layout de vez em quando: o
`load_state` do fork zera o arquivo escolhido do servidor
(`selected_server_file_index`, `#[serde(skip)]` em `libsurfer/src/state.rs`),
e a próxima consulta de status, que o cliente faz a cada 250 ms enquanto a
onda carrega, recarrega a onda com `Clear`.

## Decisão

- A preferência `wave_viewer` escolhe `tab` (o padrão) ou `window`. Sem o
  cliente web no bundle (`ToolchainInfo.surfer_web` nulo), a onda abre em
  janela.
- `src-tauri/src/wave_tab.rs` sobe, na primeira aba, um servidor HTTP só em
  `127.0.0.1` (crate `tiny_http`) que serve numa origem: o cliente web
  (`/web/`), a própria onda (`/wave/<aba>/<nome>`), o `.surf.ron`
  (`/layout/<aba>`), os tradutores e os comandos de partida (`/doc/<aba>/`).
  Cada aba é um segredo aleatório; o servidor só entrega o que foi registrado
  para uma aba aberta.
- O cliente lê a onda direto do arquivo, sem `surfer server`: carrega a onda
  inteira e só então roda `load_mapping_translator_from_url` e
  `load_state_from_url`, e nada mais recarrega a onda. Acima de 256 MB
  (`MAX_TAB_WAVE`), a aba recusa e oferece a janela.
- A vista `WaveView` mostra o cliente num iframe. Depois que a página
  carrega, ela manda `InvalidateDrawCommands` por `postMessage` por 15 s: o
  cliente só trata as mensagens que chegam por HTTP quando desenha um
  quadro, e só desenha com entrada do usuário ou mensagem injetada.
- Uma simulação que regrava a onda recarrega a aba aberta dela
  (`FlowOutcome.wave_tab`, `openWaveTab(path, true)`). Em janela, o Studio
  abre o Surfer com o layout preparado (`prepare_wave_layout`).

## Consequências

- A onda é lida inteira no WebAssembly, mais devagar que no Surfer nativo e
  com memória limitada; para ondas grandes, a janela continua.
- As abas de onda ficam montadas enquanto existem (escondidas quando não são
  a ativa), para o Surfer guardar o zoom, o cursor e os sinais entre trocas
  de aba. Cada uma mantém o cliente WebAssembly carregado; a memória só sai
  quando a aba fecha.
- Com o foco dentro do iframe, os atalhos do Studio não chegariam: o iframe
  é de outra origem. O `index.html` do cliente sai do servidor com um script
  no começo que repassa ao Studio, por `postMessage`, as teclas de função
  (menos a F11) e as combinações com Ctrl, Alt ou Cmd (menos as de edição e
  o Ctrl+K), e a `WaveView` as transforma em teclas da janela. Letras,
  setas e espaço continuam do Surfer.
- O build de produção libera o iframe com `frame-src http://127.0.0.1:*` na
  CSP; as páginas do cliente vão com a CSP própria (`wasm-unsafe-eval`).
- Quando o fork preservar o arquivo escolhido no `load_state`, o
  `surfer server` volta a ser opção para ondas grandes, sem mudar a vista.
