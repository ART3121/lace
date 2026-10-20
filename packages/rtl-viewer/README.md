# RTL viewer do Lace

O esquemático interativo do netlist que o Yosys grava na síntese
(`hierarchy.json`, o `SynthesisResult.netlist` do `lace synth --json`):
entrar nos submódulos, voltar com Esc, buscar célula, rede ou porta,
destacar uma ligação, ver os detalhes de cada célula, exportar o SVG.

Três lugares o usam:

| Onde | Como |
|---|---|
| Lace Studio | compila o fonte deste pacote direto (`studio/vite.config.ts`) e o mostra numa aba, com os botões e o menu do Studio |
| Extensão do VS Code | carrega `dist/rtl-viewer.js` num webview, com `dist/theme-vscode.css`; o netlist vem da extensão |
| `lace synth --open` | o modelo `dist/rtl-viewer.html`, que vira o componente `rtl-viewer` do bundle; o Core o preenche com o netlist (`lace_core::rtl_viewer`) e o navegador o abre |

O viewer não conhece quem o abriga. Quem o monta passa o estado
(`createRtlViewerStore`), cria o worker do layout (`setLayoutWorker`) e dá o
que só ele sabe fazer: abrir o fonte de uma célula, gravar o SVG,
sintetizar de novo. As cores são variáveis CSS (`--sch-*`, `--text-*` e as
outras listadas em `src/rtl-viewer.css`) que quem abriga define.

## Compilar

Com Node 22:

```sh
cd packages/rtl-viewer
npm ci
npm run typecheck
npm run build        # dist/: rtl-viewer.js, os CSS e o modelo rtl-viewer.html
```

`npm run gen:theme` gera `src/theme-standalone.css`, o tema do HTML do
`lace synth --open`, a partir dos temas padrão do Studio (atlas e
atlas-light): rode de novo quando eles mudarem e confira o diff.

Para testar o `lace synth --open` com este build, sem o componente
instalado, desta pasta:
`LACE_RTL_VIEWER=$PWD/dist/rtl-viewer.html lace -C <projeto> synth --open`.
