# Paridade serial

Num enlace serial, como o de uma UART, os bits chegam um por ciclo, e o transmissor costuma mandar um bit de paridade no fim de cada palavra para o receptor descobrir se algum bit trocou no caminho. O receptor calcula a paridade enquanto os bits chegam, sem guardar a palavra: basta lembrar se já recebeu um número par ou ímpar de 1s.

O módulo `paridade_serial` é essa conta, como uma máquina de Moore de dois estados. A cada borda de subida do clock sem reset, ele amostra `bit_in`. A saída `impar` vale 1 quando o número de 1s amostrados desde o último reset é ímpar.

## Portas

| Porta    | Direção | Bits | Descrição                                    |
|----------|---------|------|----------------------------------------------|
| `clk`    | entrada | 1    | o clock                                      |
| `reset`  | entrada | 1    | reset síncrono, ativo em 1                   |
| `bit_in` | entrada | 1    | o bit recebido, amostrado na borda de subida |
| `impar`  | saída   | 1    | 1 com um número ímpar de 1s desde o reset    |

## Estados

| Estado  | O que significa                             | `impar` |
|---------|---------------------------------------------|---------|
| `PAR`   | chegou um número par de 1s (zero inclusive) | 0       |
| `IMPAR` | chegou um número ímpar de 1s                | 1       |

O reset leva a máquina para `PAR`. Numa borda com o reset em 1, o bit em `bit_in` não conta. A saída depende só do estado: ela muda logo depois da borda e fica igual até a borda seguinte.

## Exemplo

Cada linha mostra os valores na borda de subida e a saída logo depois dela.

| Borda | `reset` | `bit_in` | `impar` depois |
|-------|---------|----------|----------------|
| 1     | 1       | 0        | 0              |
| 2     | 1       | 1        | 0              |
| 3     | 0       | 1        | 1              |
| 4     | 0       | 1        | 0              |
| 5     | 0       | 0        | 0              |
| 6     | 0       | 1        | 1              |
| 7     | 0       | 0        | 1              |
| 8     | 1       | 1        | 0              |
| 9     | 0       | 1        | 1              |

## Dica

Dê nome aos dois estados com `localparam`, por exemplo `localparam PAR = 1'b0;`. Guarde o estado num `reg` atualizado em `always @(posedge clk)`, e calcule o próximo estado num `always @(*)` à parte. A saída sai de uma comparação do estado com `IMPAR`.

## Dica 2

No `always @(*)`, comece com `proximo = estado;` e depois, num `case (estado)`, troque de estado quando `bit_in` vale 1: de `PAR` para `IMPAR` e de `IMPAR` para `PAR`. Feche o `case` com um `default` que vai para `PAR`. No `always @(posedge clk)`, o reset tem prioridade: com `reset` em 1, `estado <= PAR`; senão, `estado <= proximo`.
