# Bits em ordem inversa

Alguns periféricos mandam primeiro o bit menos significativo de cada byte, e outros, o mais significativo. Para ligar um ao outro, a ordem dos bits precisa ser invertida: o bit 0 da entrada vai para o bit 7 da saída, o bit 1 para o bit 6, e assim por diante.

## Portas

| Porta     | Direção | Bits | Descrição                             |
|-----------|---------|------|---------------------------------------|
| `entrada` | entrada | 8    | o byte                                |
| `saida`   | saída   | 8    | o byte com a ordem dos bits invertida |

## Exemplos

| `entrada`     | `saida`       |
|---------------|---------------|
| `8'b00000001` | `8'b10000000` |
| `8'b11010010` | `8'b01001011` |
| `8'b11110000` | `8'b00001111` |

## Dica

Escrever `entrada[0:7]` não inverte a ordem: a faixa precisa seguir a ordem da declaração, e o compilador recusa a faixa ao contrário com um erro.

## Dica 2

A concatenação aceita bits isolados: `{v[0], v[1]}` é um vetor de 2 bits com `v[0]` no bit mais significativo. Monte a saída com os oito bits da entrada, começando por `entrada[0]`.
