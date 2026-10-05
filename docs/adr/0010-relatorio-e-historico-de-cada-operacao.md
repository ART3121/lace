# 0010. Relatório e histórico de cada operação

- **Status:** Aceita
- **Data:** 2026-10-03

## Contexto

O Alpha-Solar (ART3121/Alpha-Solar, `solar report`) grava um relatório de
cada build em `.solar/reports/build-NNNNNN/`, com o texto para quem lê e
arquivos `.dat` para comparar: as estatísticas genéricas de síntese do
`stat` do Yosys e os tempos da simulação. `solar report compare` compara
dois desses, escolhe a referência sozinho quando o usuário não diz qual, e
avisa quando o contexto mudou (fontes, versões, máquina). O laboratório
quer essa função no Lace.

No Lace, as operações são separadas (`build` compila processadores,
`check`, `sim`, `synth`), devolvem resultados com os passos e as durações, e
a ADR 0001 põe a regra no Core e deixa as interfaces finas.

## Decisão

O relatório e o histórico ficam no Core, no módulo público
`lace_core::history` (`crates/lace-core/src/history.rs`):

- A interface monta um `Operation` com o que cada fase devolveu e chama
  `history::record`. Nenhuma operação grava sozinha: a CLI grava depois de
  `build`, `check`, `sim` e `synth` (`record`, em
  `crates/lace-cli/src/commands.rs`); uma GUI decide por ela. A operação que
  falha grava; a que nem começa, não.
- `record` grava `report.txt` e `record.json` numa pasta montada com outro
  nome e renomeada no fim, em `.lace/reports/run-NNNNNN/`. O número só
  cresce (`sequence`). O JSON é o `RunRecord`, com o schema gerado do tipo,
  como o resto do contrato (ADR 0008), no lugar dos `.dat` do Alpha-Solar.
- Os identificadores são `run-`, e não `build-`, porque no Lace "build" é
  compilar processador.
- A síntese passa a rodar `stat -json` no fim do script do Yosys
  (`yosys_script`, em `synth.rs`), e `stats.rs` lê o JSON, e não o texto do
  `stat`, que muda de formato.
- Todo resultado de operação ganha `duration_ms`, medido do começo ao fim no
  Core; é o tempo total da simulação no relatório.
- A comparação (`compare`, `compare_reports`, `previous_comparable`) segue
  as regras do Alpha-Solar: síntese só com o mesmo topo, simulação só com o
  mesmo simulador e testbench, o resto vira aviso, a mudança é só descrita.
- A máquina vem do que o sistema informa sem rodar outro programa (o
  `uname`, e `/proc` no Linux), para não abrir uma exceção à ADR 0002.

## Consequências

- `lace report`, `report show`, `report list` e `report compare` só leem o
  histórico. O JSON de `build`, `check`, `sim` e `synth` traz o relatório
  gravado em `report`.
- O tempo é guardado em milissegundos, a resolução dos passos; o Alpha-Solar
  guarda nanossegundos. O tempo simulado vem do `$finish called at` do
  `vvp`, em femtossegundos; o Verilator não o informa.
- Fora do Linux, o modelo do processador e a memória ficam `null` no
  relatório.
- O histórico não tem limite de tamanho: quem apaga é o usuário, com
  `lace report clean` (todos, os mais antigos com `--keep N`, ou pelo
  identificador), que usa `history::plan_cleanup` e `history::remove`.
  Apagar uma pasta `run-*` à mão também é seguro, e o número dela não
  volta.
- As estatísticas contam as células genéricas do netlist do Lace (depois de
  `proc` e `opt_clean`), não um mapeamento para FPGA.
