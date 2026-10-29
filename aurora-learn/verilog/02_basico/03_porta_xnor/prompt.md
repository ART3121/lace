# Porta XNOR

A saída `iguais` vale 1 quando as entradas `a` e `b` têm o mesmo valor, e 0 quando são diferentes. É a porta XNOR, o OU exclusivo com a saída invertida. Por comparar dois bits, ela é a peça básica dos comparadores de igualdade.

## Portas

| Porta    | Direção | Bits | Descrição                     |
|----------|---------|------|-------------------------------|
| `a`      | entrada | 1    | primeiro bit                  |
| `b`      | entrada | 1    | segundo bit                   |
| `iguais` | saída   | 1    | 1 quando `a` e `b` são iguais |

## Tabela verdade

| `a` | `b` | `iguais` |
|-----|-----|----------|
| 0   | 0   | 1        |
| 0   | 1   | 0        |
| 1   | 0   | 0        |
| 1   | 1   | 1        |

## Em Verilog

O OU exclusivo é o `^`: vale 1 quando os dois bits são diferentes. A porta XNOR dá o resultado oposto em todas as linhas da tabela.

## Dica

Compare esta tabela com a do OU exclusivo: em cada linha, a saída é a oposta.

## Dica 2

Há duas formas. Uma é inverter o OU exclusivo com `~` e parênteses, como na porta NÃO OU. A outra é o operador de XNOR do Verilog, `~^`, escrito entre os dois sinais.
