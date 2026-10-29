# Um fio

A saída `saida` repete a entrada `entrada`: quando a entrada muda, a saída muda junto. É o circuito mais simples com uma entrada, um fio entre as duas portas.

## Portas

| Porta     | Direção | Bits | Descrição         |
|-----------|---------|------|-------------------|
| `entrada` | entrada | 1    | o sinal que chega |
| `saida`   | saída   | 1    | o mesmo sinal     |

## Tabela verdade

| `entrada` | `saida` |
|-----------|---------|
| 0         | 0       |
| 1         | 1       |

## Em Verilog

O lado direito de um `assign` pode ser o nome de outro sinal. Como a atribuição é contínua, os dois sinais ficam ligados como por um fio soldado: cada mudança da entrada aparece na saída, durante toda a simulação.

A direção do `assign` importa. Quem fica à esquerda do `=` recebe o valor; quem fica à direita fornece.

## Dica

A saída fica à esquerda do `=`, e a entrada à direita.

## Dica 2

Não é preciso operador nenhum: o nome da entrada, sozinho, já é a expressão.
