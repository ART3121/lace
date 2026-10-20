# Código Gray

No código Gray, dois números vizinhos diferem em um único bit. No binário comum, a passagem de 7 (`0111`) para 8 (`1000`) muda os quatro bits, e eles nunca mudam exatamente no mesmo instante. Quem lê no meio da passagem pode ver qualquer valor, como `1111`. No Gray, a mesma passagem muda um bit só, e a leitura dá o valor antigo ou o novo, nunca um terceiro.

Por isso o código Gray aparece em encoders de posição, os sensores que medem o ângulo de um eixo, e nos contadores que passam de um domínio de clock para outro, como os ponteiros de uma FIFO assíncrona: o outro lado amostra o contador com o seu próprio clock, a qualquer momento, e precisa de um valor válido.

O módulo `gray` converte um número de 4 bits de binário para código Gray.

## Portas

| Porta    | Direção | Bits | Descrição                     |
|----------|---------|------|-------------------------------|
| `bin`    | entrada | 4    | o número em binário           |
| `codigo` | saída   | 4    | o mesmo número em código Gray |

## Tabela

| Número | `bin`  | `codigo` |
|--------|--------|----------|
| 0      | `0000` | `0000`   |
| 1      | `0001` | `0001`   |
| 2      | `0010` | `0011`   |
| 3      | `0011` | `0010`   |
| 4      | `0100` | `0110`   |
| 5      | `0101` | `0111`   |
| 6      | `0110` | `0101`   |
| 7      | `0111` | `0100`   |
| 8      | `1000` | `1100`   |
| 9      | `1001` | `1101`   |
| 10     | `1010` | `1111`   |
| 11     | `1011` | `1110`   |
| 12     | `1100` | `1010`   |
| 13     | `1101` | `1011`   |
| 14     | `1110` | `1001`   |
| 15     | `1111` | `1000`   |

Desça a coluna `codigo`: de uma linha para a seguinte, e também da última para a primeira, muda só um bit.

## Em Verilog

O operador `>>` desloca os bits de um vetor para a direita: `v >> 1` move cada bit uma posição, perde o bit 0 e põe um 0 no bit mais significativo. Com `v` valendo `4'b1011`, `v >> 1` vale `4'b0101`. O `<<` desloca para a esquerda, do mesmo jeito.

## Dica

Compare cada bit de `codigo` com os bits de `bin` na tabela. O bit 3 é igual a `bin[3]`, e cada um dos outros depende só de dois bits vizinhos de `bin`.

## Dica 2

O bit `n` de `codigo` é `bin[n] ^ bin[n+1]`. Deslocar `bin` uma posição para a direita põe `bin[n+1]` na posição `n`, e o 0 que entra no bit 3 deixa `bin[3]` igual no OU exclusivo.
