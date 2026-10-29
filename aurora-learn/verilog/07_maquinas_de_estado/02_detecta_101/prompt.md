# Detector de 1, 0, 1

A lógica de trigger de um detector de partículas recebe, a cada ciclo do clock, um bit por canal que diz se o sinal passou do limiar. Alguns triggers procuram um padrão no tempo em vez de um bit isolado. Este procura dois ciclos acima do limiar com um ciclo abaixo no meio: os bits 1, 0, 1, em três bordas seguidas.

O módulo `detecta_101` é uma máquina de Moore. A cada borda de subida sem reset, ele amostra `x`. Depois de uma borda, `achou` vale 1 quando os bits amostrados nessa borda e nas duas anteriores foram 1, 0 e 1, nessa ordem, sem reset em nenhuma das três. A saída fica assim até a borda seguinte. As ocorrências podem se sobrepor: o último 1 de um padrão serve de primeiro 1 do seguinte, e então 1, 0, 1, 0, 1 dá dois acertos.

## Portas

| Porta   | Direção | Bits | Descrição                                   |
|---------|---------|------|---------------------------------------------|
| `clk`   | entrada | 1    | o clock                                     |
| `reset` | entrada | 1    | reset síncrono, ativo em 1                  |
| `x`     | entrada | 1    | o bit do canal, amostrado na borda          |
| `achou` | saída   | 1    | 1 no ciclo depois de completar 1, 0, 1      |

## Estados

Cada estado diz quanto do começo do padrão os últimos bits amostrados já formam.

| Estado   | Os últimos bits amostrados terminam em | `achou` |
|----------|----------------------------------------|---------|
| `NADA`   | nenhum começo do padrão                | 0       |
| `VIU_1`  | 1                                      | 0       |
| `VIU_10` | 1, 0                                   | 0       |
| `ACHOU`  | 1, 0, 1                                | 1       |

O reset leva a máquina para `NADA`, e o bit amostrado numa borda com reset não conta.

## Exemplo

Cada linha mostra os valores na borda de subida e o estado e a saída logo depois dela.

| Borda | `reset` | `x` | estado depois | `achou` depois |
|-------|---------|-----|---------------|----------------|
| 1     | 1       | 1   | `NADA`        | 0              |
| 2     | 1       | 0   | `NADA`        | 0              |
| 3     | 0       | 1   | `VIU_1`       | 0              |
| 4     | 0       | 0   | `VIU_10`      | 0              |
| 5     | 0       | 1   | `ACHOU`       | 1              |
| 6     | 0       | 0   | `VIU_10`      | 0              |
| 7     | 0       | 1   | `ACHOU`       | 1              |
| 8     | 0       | 1   | `VIU_1`       | 0              |
| 9     | 0       | 1   | `VIU_1`       | 0              |
| 10    | 0       | 0   | `VIU_10`      | 0              |
| 11    | 0       | 0   | `NADA`        | 0              |

Nas bordas 5 e 7, o 1 da borda 5 fecha um padrão e abre o outro.

## Dica

São quatro estados, então o registrador de estado tem 2 bits: `localparam NADA = 2'd0;` e assim por diante. Para cada estado e cada valor de `x`, pergunte quais são os últimos bits amostrados depois da borda e qual estado da tabela corresponde a eles. Um 0 depois de `NADA` continua sem começo nenhum; um 1 depois de `VIU_1` continua terminando em 1.

## Dica 2

O caminho do acerto é `NADA` com 1 para `VIU_1`, `VIU_1` com 0 para `VIU_10` e `VIU_10` com 1 para `ACHOU`. De `ACHOU`, os últimos bits já terminam em 1, então ele se comporta como `VIU_1`. Escreva isso num `case (estado)` dentro de `always @(*)`, com `proximo = estado;` antes do `case` e um `default` que vai para `NADA`. A saída é `estado == ACHOU`.
