# 0002. A composição dos fluxos fica isolada em flows.rs até ir para o Core

- **Status:** Aceita
- **Data:** 2026-10-03

## Contexto

O Core oferece operações separadas (`build_processors`, `check`,
`simulate`, `simulate_project`, `synthesize`, `render_schematic`,
`history::record`). Quem as compõe num fluxo, como "compilar os
processadores que têm fonte, parar no primeiro que falhar, simular, ler as
portas de saída, gravar o relatório, abrir a onda", é a CLI, em
`lace-cli/src/commands.rs`. A ADR 0001 do Lace diz que regra posta na CLI é
regra que a GUI vai ter de copiar. O botão Wave do Studio precisa fazer
exatamente o que `lace sim --open` faz.

## Decisão

O Studio copia a composição da CLI para `src-tauri/src/flows.rs`, num
módulo só, sem dependência da interface: recebe o pedido, as preferências,
o `.spf` e o `Control`, e devolve um `FlowOutcome`. O comando gravado no
relatório é o equivalente da CLI com o prefixo `lace-studio`, para ficar
claro quem rodou.

A proposta para o Lace é um módulo `lace_core::flows` com essas
composições, que a CLI e o Studio chamem (ARCHITECTURE.md, seção 8).

## Consequências

- Uma mudança no fluxo da CLI precisa ser repetida em `flows.rs`, até a
  proposta entrar no Core. Os testes de `src-tauri/tests/flows.rs` conferem
  o comportamento contra o bundle de verdade.
- Quando o Core ganhar os fluxos, `flows.rs` vira uma chamada por fluxo, e a
  interface não muda: o `FlowOutcome` continua o mesmo.
