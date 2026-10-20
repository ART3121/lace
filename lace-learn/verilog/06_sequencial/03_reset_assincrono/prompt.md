# Reset assíncrono

No reset assíncrono, `q` vai a 0 assim que `reset` sobe, sem esperar a borda do clock, e fica em 0 enquanto `reset` vale 1. Ele serve quando o circuito precisa ir para um estado conhecido mesmo com o clock parado ou ainda instável, como logo que a placa liga.

Fora do reset, o flip-flop funciona como antes: na borda de subida, `q` passa a valer `d`.

## Portas

| Porta   | Direção | Bits | Descrição              |
|---------|---------|------|------------------------|
| `clk`   | entrada | 1    | o clock                |
| `reset` | entrada | 1    | em 1, zera `q` na hora |
| `d`     | entrada | 1    | o bit a guardar        |
| `q`     | saída   | 1    | o bit guardado         |

## Exemplo

| Momento                              | `reset` | `d` | `q` |
|--------------------------------------|---------|-----|-----|
| logo depois da borda 1               | 0       | 1   | 1   |
| entre as bordas 1 e 2, `reset` sobe  | 1       | 1   | 0   |
| logo depois da borda 2               | 1       | 1   | 0   |
| entre as bordas 2 e 3, `reset` desce | 0       | 1   | 0   |
| logo depois da borda 3               | 0       | 1   | 1   |

Com o reset síncrono do exercício anterior, `q` ficaria em 1 até a borda 2. A correção confere `q` também no meio do ciclo, depois de as entradas mudarem, e vê a diferença.

## Dica

O bloco `always` precisa executar também quando `reset` sobe. Os dois eventos entram na lista de sensibilidade, separados por `or`.

## Dica 2

A lista fica `@(posedge clk or posedge reset)`. Dentro do bloco, o `if (reset)` vem primeiro: se `reset` vale 1, seja qual for o evento que acordou o bloco, `q` vai a 0.
