# Contador de 0 a 9

Um contador que vai de 0 a 9 e volta a 0 conta em decimal: cada dígito de um relógio num mostrador de sete segmentos tem um. Com `en` sempre em 1, ele passa pelo 0 uma vez a cada dez ciclos, o que também divide a frequência do clock por dez.

A cada borda de subida com `en` em 1, `q` soma 1, e depois do 9 vem o 0. Com `en` em 0, `q` fica como está. O reset é síncrono e leva `q` a 0.

## Portas

| Porta   | Direção | Bits | Descrição               |
|---------|---------|------|-------------------------|
| `clk`   | entrada | 1    | o clock                 |
| `reset` | entrada | 1    | em 1 na borda, zera `q` |
| `en`    | entrada | 1    | em 1 na borda, conta    |
| `q`     | saída   | 4    | a contagem, de 0 a 9    |

## Exemplo

Cada linha mostra os valores logo depois de uma borda de subida do clock.

| Borda  | `reset` | `en` | `q`   |
|--------|---------|------|-------|
| 1      | 1       | 1    | 0     |
| 2      | 0       | 1    | 1     |
| 3      | 0       | 1    | 2     |
| 4      | 0       | 0    | 2     |
| 5      | 0       | 1    | 3     |
| 6 a 10 | 0       | 1    | 4 a 8 |
| 11     | 0       | 1    | 9     |
| 12     | 0       | 1    | 0     |

## Dica

Com `en` em 1, há dois casos: `q` já vale 9, ou ainda não.

## Dica 2

Compare com `q == 4'd9`. Num caso `q` recebe `4'd0`; no outro, `q + 4'd1`. Só somar 1 levaria `q` a 10, 11 e assim até 15, porque 4 bits contam até 15.
