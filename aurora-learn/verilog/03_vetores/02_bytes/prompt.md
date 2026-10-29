# Os dois bytes de uma palavra

Uma memória entrega palavras de 16 bits, mas o barramento seguinte tem só 8 bits, e cada palavra precisa seguir em duas partes. O módulo `bytes` separa a palavra nos seus dois bytes: `alto`, com os 8 bits mais significativos, e `baixo`, com os 8 menos significativos.

## Portas

| Porta     | Direção | Bits | Descrição                |
|-----------|---------|------|--------------------------|
| `palavra` | entrada | 16   | a palavra                |
| `alto`    | saída   | 8    | bits 15 a 8 de `palavra` |
| `baixo`   | saída   | 8    | bits 7 a 0 de `palavra`  |

## Exemplo

Com `palavra` igual a `16'h12AB`, `alto` vale `8'h12` e `baixo` vale `8'hAB`.

## Em Verilog

Uma faixa de bits se seleciona com dois índices separados por dois-pontos: `v[5:2]` são os bits 5, 4, 3 e 2 de `v`, um vetor de 4 bits. Os índices seguem a ordem da declaração: num vetor declarado `[15:0]`, o índice maior vem primeiro.

## Dica

Cada saída é uma faixa de 8 bits de `palavra`.

## Dica 2

O byte alto vai do bit 15 ao 8; o baixo, do bit 7 ao 0.
