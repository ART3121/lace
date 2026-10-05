# 0001. Tauri 2 e React, com o lace-core como biblioteca no backend

- **Status:** Aceita
- **Data:** 2026-10-03

## Contexto

A AURORA é um aplicativo Electron com a orquestração das ferramentas dentro
dela. O Lace tirou essa orquestração para uma biblioteca Rust, o
`lace-core`, e a ADR 0001 do Lace prevê que uma GUI em Rust use a biblioteca
direto, e que interfaces em outra linguagem chamem `lace ... --json`. A
nova GUI precisa rodar em Linux, macOS e Windows, como o Lace, e substituir
a AURORA, uma IDE com editor de código, terminais, árvore de arquivos e
visualizações.

## Decisão

- **Tauri 2.** O backend do aplicativo é Rust e linka o `lace-core` como
  dependência, sem processo intermediário: os tipos do Core (resultados,
  eventos, erros) atravessam para a interface pela serialização que eles já
  têm. A WebView é a do sistema; o instalador fica muito menor que um
  Electron.
- **React com TypeScript e Vite** na interface, com `zustand` para o
  estado. É o ecossistema mais conhecido pelos alunos que vão manter o
  Studio, e tem os componentes que uma IDE precisa: Monaco, xterm.js,
  painéis redimensionáveis.
- O `lace-core` entra por caminho enquanto o Lace não publica o crate. Era
  `../lace/crates/lace-core`, com o Studio num repositório ao lado; desde a
  0.2.0 o Studio está no repositório do Lace, em `studio/`, e o caminho é
  `../crates/lace-core` (ADR 0013 do Lace).
- Os tipos do Core na interface são gerados dos JSON Schemas que o Lace
  versiona (`npm run gen:types`), em vez de escritos à mão.

## Consequências

- Nenhuma regra de negócio na interface nem no backend do Studio, com a
  exceção registrada na ADR 0002.
- Quem compila o Studio precisa do repositório do Lace ao lado.
- O Studio acha a instalação do Lace por conta própria (não está ao lado
  dela, como a CLI): `src-tauri/src/toolchain.rs`.
- As operações do Core bloqueiam; o backend as roda em threads e fala com a
  interface por `Channel` (ARCHITECTURE.md, seção 5).
