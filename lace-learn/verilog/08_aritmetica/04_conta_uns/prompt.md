# Contagem de uns

Num sistema de disparo, cada bit de um vetor diz se um canal do detector passou de um limiar naquele instante, e a quantidade de canais acesos ajuda a decidir se vale a pena guardar o evento. Contar os bits em 1 de um vetor é uma conta comum em hardware, e o resultado precisa de bits suficientes para o caso de todos estarem em 1.

O módulo `conta_uns` dá quantos bits de `x` valem 1. Como a contagem vai de 0 a 8, a saída tem 4 bits.

## Portas

| Porta | Direção | Bits | Descrição                 |
|-------|---------|------|---------------------------|
| `x`   | entrada | 8    | o vetor                   |
| `n`   | saída   | 4    | quantos bits de `x` são 1 |

## Exemplo

| `x`           | `n` |
|---------------|-----|
| `8'b00000000` | 0   |
| `8'b00010000` | 1   |
| `8'b10110001` | 4   |
| `8'b11111111` | 8   |

## Dica

Cada bit de `x`, lido como número, vale 0 ou 1. A contagem é a soma dos 8 bits.

## Dica 2

Some os oito bits num só `assign`: `x[0] + x[1] + ...`. Cada bit sozinho é um número de 1 bit, mas o Verilog faz a conta com a largura do destino, os 4 bits de `n`, e os vai-uns não se perdem.
