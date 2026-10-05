# 0005. Módulo ou testbench pelo conteúdo, com a regra da AURORA

- **Status:** Aceita
- **Data:** 2026-10-01

## Contexto

Cada arquivo Verilog do projeto é sintetizável (entra na síntese e na
simulação) ou testbench (só na simulação). Perguntar a cada `lace add` é
lento, e o nome do arquivo nem sempre diz o papel. A AURORA já decide pelo
conteúdo, com uma pontuação (`js/project/verilog_classifier.ts`), e refaz a
classificação a cada carga do projeto (`docs/API.md`, seção 10). Como Lace
e AURORA abrem o mesmo `.spf` (ADR 0003), uma regra diferente faria o mesmo
arquivo ser módulo num programa e testbench no outro.

## Decisão

`verilog::classify(texto, nome_do_arquivo)`, em
`crates/lace-core/src/verilog.rs`, copia a regra da AURORA. Tira
comentários e strings do texto, soma os pontos dos indícios (cada um conta
uma vez) e, com 3 ou mais (`TESTBENCH_THRESHOLD`), o arquivo é testbench.

| Indício | Pontos |
|---|---|
| `$dumpfile` ou `$dumpvars` (também `$dumpon`, `$dumpoff`, `$dumpall`, `$dumplimit`, `$dumpflush`) | 3 |
| `$finish` ou `$stop` | 3 |
| módulo sem portas: `module tb;`, `module tb();`, também com `#(...)` de parâmetros | 3 |
| `initial` | 2 |
| `$display`, `$write`, `$monitor`, `$strobe`, `$time`, `$realtime`, `$random`, `$sformat` ou `$sformatf` | 1 |
| atraso `#<dígito>`, como `#10` (o `#(` de parâmetro não conta) | 1 |
| nome do arquivo com `tb`, `test` ou `testbench` como palavra (`alu_tb.v`, `test_alu.v`) | 2 |

Texto vazio é sintetizável.

Onde a regra vale:

- `Project::add_verilog` (`files.rs`) classifica um arquivo que já existe,
  a menos que o chamador peça testbench (`lace add --tb`). O papel fica
  gravado no `.spf`; o Lace não reclassifica ao abrir.
- Um arquivo novo não tem conteúdo para classificar: o papel vem da opção
  `testbench` ou do nome (`name_suggests_testbench`), e o modelo criado já
  nasce com o papel certo.
- `check` com um arquivo que não está registrado usa `classify` para saber
  se o elabora como testbench.

Copiamos a regra em vez de inventar outra porque a compatibilidade com a
AURORA vale mais que uma regra melhor: qualquer melhoria seria uma
divergência entre os dois programas.

## Consequências

- Na dúvida, sintetizável. `initial` sozinho (+2), comum em módulo que
  inicializa memória, não basta; o nome sozinho (+2) também não. O teste
  `classification_follows_the_aurora_scores`, em `verilog.rs`, fixa esses
  casos.
- Um arquivo classificado errado se corrige com `lace add --tb`; um arquivo
  registrado de novo com outro papel muda de lista.
- Os modelos de `module_template` e `testbench_template` precisam continuar
  classificados como o que são (teste `templates_are_classified_as_intended`):
  o módulo-modelo tem portas, o testbench-modelo tem `$dumpfile` e
  `$finish`.
- Mudar pesos, indícios ou limiar é mudar esta decisão: escreve-se uma ADR
  que a substitui, e mudam juntos os testes de `verilog.rs` e os de
  classificação em `crates/lace-core/tests/verilog_flow.rs`.
