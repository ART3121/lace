# 0009. O editor dividido em grupos

- **Status:** Aceita
- **Data:** 2026-10-04
- **Emenda:** [0003](0003-um-editor-monaco-um-modelo-por-arquivo.md), no ponto de "uma instância só"

## Contexto

A 0003 decidiu por uma instância só do Monaco e já anotava que dividir o
editor (a AURORA tinha até três painéis) ia exigir mais de uma. O pedido
chegou: pôr dois arquivos abertos um ao lado do outro.

## Decisão

- A área central tem de um a três grupos lado a lado, o mesmo limite da
  AURORA. Cada grupo tem a sua barra de abas e a sua instância do Monaco.
- Os modelos continuam um por arquivo, compartilhados: o mesmo arquivo em
  dois grupos é o mesmo modelo em dois editores, e o que se digita num
  aparece no outro. O modelo só é descartado quando a última aba do arquivo
  fecha, e só então o Studio pergunta pelo que não foi salvo.
- Uma vista (preferências, esquemático, relatórios, processador) fica num
  grupo só: dividir uma vista a leva para o lado em vez de abrir outra
  cópia, que buscaria os mesmos dados duas vezes.
- O estado fica em `useEditor`: `tabs` com cada aba uma vez, `groups` com os
  ids de cada grupo, `activeGroup`, e `activeId` (a aba ativa do grupo
  ativo) mantido para quem só precisa da aba atual: menus, atalhos, barra
  de status, explorador.
- Grupo que fica sem abas some. O foco num editor decide o grupo ativo.
- O modo Vim passa a ter um adaptador por editor; a barra de status mostra
  o do editor com o foco.

## Consequências

- Até três instâncias do Monaco na memória; só existem enquanto o grupo
  tiver aba de arquivo.
- A sessão gravada por projeto ganhou o formato com grupos; a antiga,
  sem grupos, ainda é lida.
- Dividir na vertical (um grupo em cima do outro) não foi feito: a AURORA
  não tinha, e o espaço vertical já é do painel inferior.
