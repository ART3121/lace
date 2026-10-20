# Registrador de deslocamento

Muitos chips trocam dados por uma linha serial, um bit por ciclo de clock. É assim no SPI, que as placas de FPGA usam para falar com conversores e memórias. Do lado de quem recebe, um registrador de deslocamento junta os bits: a cada borda, o conteúdo anda uma posição e o bit novo entra na ponta. Depois de quatro bordas, os quatro bits estão lado a lado, prontos para ler de uma vez.

Na borda de subida, `q[3]` recebe `q[2]`, `q[2]` recebe `q[1]`, `q[1]` recebe `q[0]` e `q[0]` recebe `serial`. O bit que estava em `q[3]` sai. O reset é síncrono e zera `q`.

## Portas

| Porta    | Direção | Bits | Descrição                                    |
|----------|---------|------|----------------------------------------------|
| `clk`    | entrada | 1    | o clock                                      |
| `reset`  | entrada | 1    | em 1 na borda, zera `q`                      |
| `serial` | entrada | 1    | o bit que entra                              |
| `q`      | saída   | 4    | os quatro últimos bits; `q[0]` é o mais novo |

## Exemplo

Cada linha mostra os valores logo depois de uma borda de subida do clock.

| Borda | `reset` | `serial` | `q`       |
|-------|---------|----------|-----------|
| 1     | 1       | 1        | `4'b0000` |
| 2     | 0       | 1        | `4'b0001` |
| 3     | 0       | 0        | `4'b0010` |
| 4     | 0       | 1        | `4'b0101` |
| 5     | 0       | 1        | `4'b1011` |
| 6     | 0       | 0        | `4'b0110` |

## Dica

Os quatro bits mudam juntos, na mesma borda. Com `<=`, cada lado direito usa os valores de antes da borda, então a ordem das atribuições não importa.

## Dica 2

O valor novo de `q` cabe numa concatenação: os três bits de baixo de `q` à esquerda e `serial` à direita.
