# Inversor

A saída `y` é o contrário da entrada `a`: vale 1 quando `a` vale 0, e 0 quando `a` vale 1. É a porta NÃO, também chamada de inversor.

## Portas

| Porta | Direção | Bits | Descrição     |
|-------|---------|------|---------------|
| `a`   | entrada | 1    | o sinal       |
| `y`   | saída   | 1    | `a` invertido |

## Tabela verdade

| `a` | `y` |
|-----|-----|
| 0   | 1   |
| 1   | 0   |

## Em Verilog

O Verilog tem dois operadores de negação. O `~` inverte cada bit do sinal e é o operador da porta NÃO. O `!` é a negação lógica. Ele trata o sinal inteiro como um valor só, falso se todos os bits valem 0 e verdadeiro se algum vale 1, e responde com um único bit. Para um sinal de 1 bit, os dois dão o mesmo resultado; com vetores, que aparecem no capítulo 3, eles diferem. Nas portas lógicas, use o `~`.

## Dica

A expressão do `assign` é a entrada com um operador na frente.

## Dica 2

O operador é o `~`, escrito logo antes do nome do sinal.
