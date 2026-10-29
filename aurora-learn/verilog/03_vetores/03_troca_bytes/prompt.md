# Troca da ordem dos bytes

Processadores diferentes guardam os bytes de uma palavra em ordens diferentes. Para passar uma palavra de 32 bits de um para outro, é preciso inverter a ordem dos bytes: o byte menos significativo passa a ser o mais significativo, e assim por diante. A ordem dos bits dentro de cada byte não muda.

## Portas

| Porta     | Direção | Bits | Descrição                               |
|-----------|---------|------|-----------------------------------------|
| `entrada` | entrada | 32   | a palavra                               |
| `saida`   | saída   | 32   | a palavra com os bytes em ordem inversa |

## Exemplo

| Sinal     | bits 31:24 | bits 23:16 | bits 15:8 | bits 7:0 |
|-----------|------------|------------|-----------|----------|
| `entrada` | `8'h11`    | `8'h22`    | `8'h33`   | `8'h44`  |
| `saida`   | `8'h44`    | `8'h33`    | `8'h22`   | `8'h11`  |

Em hexadecimal, `32'h11223344` vira `32'h44332211`.

## Em Verilog

Uma faixa também pode ficar do lado esquerdo do `assign`: `assign v[3:0] = ...;` liga só aqueles bits de `v`. Assim um vetor de saída pode ser montado em pedaços, com um `assign` para cada faixa, desde que cada bit seja ligado uma única vez.

## Dica

São quatro `assign`, um para cada byte da saída, e em cada um os dois lados são faixas de 8 bits.

## Dica 2

O byte mais significativo da saída, `saida[31:24]`, recebe o byte menos significativo da entrada, `entrada[7:0]`. Os outros três seguem o mesmo padrão.
