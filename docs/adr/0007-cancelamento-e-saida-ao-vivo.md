# 0007. Cancelamento, prazo e saída ao vivo pelo `Control`

- **Status:** Aceita
- **Data:** 2026-10-01

## Contexto

Até a versão 0.1.0, `process::run` esperava a ferramenta terminar, sem
prazo, e devolvia stdout e stderr só no fim. Três problemas apareciam:

- **Testbench sem `$finish`.** É o erro mais comum de quem começa. A
  simulação roda para sempre, e o `vvp` grava uma onda que cresce sem
  limite.
- **Processo órfão.** Matar o `lace` não mata os filhos. Com um SIGTERM no
  `lace` de 0.1.0, o `vvp` de um testbench sem fim continuava rodando
  sozinho (conferido em 2026-10-01). Uma GUI ou uma extensão de editor que
  encerra o `lace` deixaria esse lixo para trás.
- **Nada aparece enquanto roda.** Os terminais da AURORA (TVERI, TCMM)
  mostram a saída das ferramentas. Uma GUI ou uma extensão precisa do
  mesmo.

Um detalhe pesa sobre o terceiro: num pipe, o `vvp` guarda o stdout em
buffer e só o solta quando o buffer enche ou no fim; se for morto, a saída
se perde (conferido com `timeout -s KILL`). A própria AURORA aceita isso: o
comentário em `js/wave/testbench_instrumenter.ts` diz que o `$display` sai
em bloco no fim da simulação, "trade deliberado por velocidade".

## Decisão

Toda operação que executa ferramentas recebe um `Control` como último
argumento (`crates/lace-core/src/control.rs`). Ele leva um `CancelToken` e,
opcionalmente, quem recebe os `Event`s: `StepStarted`, `Output` (uma por
linha, com `diagnostic` dizendo se a linha é mensagem da ferramenta) e
`StepFinished`. `Control::default()` não cancela nem avisa nada.

- `process::run` lê stdout e stderr linha a linha em duas threads, avisa
  cada linha e confere cancelamento e prazo a cada 20 ms, mesmo durante uma
  enxurrada de saída.
- **Encerrar é encerrar a árvore.** No Unix, cada passo roda num grupo de
  processos próprio (`process_group(0)`), e o Lace manda SIGTERM ao grupo e,
  depois de 1 s, SIGKILL (`rustix::process::kill_process_group`). No
  Windows, `taskkill /T /F` encerra a árvore. O `taskkill.exe` do
  `System32` é uma exceção à ADR 0002: só serve para encerrar, nunca para
  executar trabalho.
- Um passo encerrado termina com `Termination::Cancelled` ou `TimedOut`, e a
  operação, com `Status::Cancelled` ou `TimedOut`. É `Ok`, não `Err`: a
  operação rodou, e o resultado traz o que as ferramentas escreveram até
  ali. Cancelado, nenhum passo seguinte começa.
- O prazo é só da simulação (`SimulationOptions::timeout`), que é onde um
  código do usuário pode não terminar. Sem prazo por padrão.
- **Saída ao vivo tem custo, e só se paga quando alguém acompanha.** Com um
  receptor de eventos, o `vvp` roda com `-i` (stdout sem buffer). Sem
  receptor, fica o buffer, mais rápido, como a AURORA. O Verilator compila
  sempre com `--autoflush`, porque trocar a flag recompilaria o modelo.
- Na CLI, SIGINT, SIGTERM e, no Unix, SIGHUP pedem o cancelamento
  (`cancel_on_signals`, em `crates/lace-cli/src/main.rs`); o segundo sinal
  sai na hora. O código de saída é 130. Em texto, o `$display` do testbench
  sai enquanto roda; com `--events`, cada evento é uma linha JSON.

## Consequências

- Nenhum processo iniciado pelo Lace sobra depois de uma operação
  cancelada, por Ctrl+C ou por SIGTERM. Os testes conferem isso:
  `cancel_stops_the_process_and_what_it_started` (`process.rs`) e
  `interrupt_cancels_and_leaves_no_simulator_behind`
  (`crates/lace-cli/tests/cli.rs`).
- Um SIGKILL no `lace` ainda deixa os filhos vivos: não há como tratá-lo.
  Quem encerra o `lace` de fora (uma extensão, uma GUI) deve mandar SIGTERM,
  que é o padrão do `kill` e do `child.kill()` do Node.
- Fora do grupo do Lace, os filhos não recebem o Ctrl+C do terminal: quem
  os encerra é o Lace. Código que cria processo fora de `process::run`
  perde isso.
- O receptor de eventos roda na thread da operação, no meio dela: precisa
  ser rápido, e não pode chamar outra operação do Lace.
- Mudou a assinatura de `build`, `build_processors`, `check`, `simulate`,
  `simulate_project`, `synthesize` e `render_schematic`. Chamar com
  `&Control::default()` mantém o comportamento antigo.
- Duas linhas de stdout e stderr chegam na ordem em que o Lace as leu, que
  pode não ser a ordem exata em que a ferramenta as escreveu.
