# Porta E

A saída `y` vale 1 só quando as duas entradas, `a` e `b`, valem 1. É a porta E (AND, em inglês).

## Portas

| Porta | Direção | Bits | Descrição        |
|-------|---------|------|------------------|
| `a`   | entrada | 1    | primeira entrada |
| `b`   | entrada | 1    | segunda entrada  |
| `y`   | saída   | 1    | `a` E `b`        |

## Tabela verdade

| `a` | `b` | `y` |
|-----|-----|-----|
| 0   | 0   | 0   |
| 0   | 1   | 0   |
| 1   | 0   | 0   |
| 1   | 1   | 1   |

## Em Verilog

O operador da porta E é o `&`, escrito entre os dois operandos. Existe também o `&&`, o E lógico, que dá o mesmo resultado com sinais de 1 bit. A diferença entre os dois aparece com vetores, no capítulo 3. Nas portas lógicas, use o `&`.

## Dica

É um `assign` só, com as duas entradas na expressão.

## Dica 2

O `&` vai entre `a` e `b`.
