# Porta NÃO OU

A saída `y` vale 1 só quando nenhuma entrada está em 1, isto é, quando `a` e `b` valem 0. É a porta NÃO OU (NOR, em inglês): um OU com a saída invertida.

## Portas

| Porta | Direção | Bits | Descrição                    |
|-------|---------|------|------------------------------|
| `a`   | entrada | 1    | primeira entrada             |
| `b`   | entrada | 1    | segunda entrada              |
| `y`   | saída   | 1    | o OU de `a` e `b`, invertido |

## Tabela verdade

| `a` | `b` | `y` |
|-----|-----|-----|
| 0   | 0   | 1   |
| 0   | 1   | 0   |
| 1   | 0   | 0   |
| 1   | 1   | 0   |

## Em Verilog

A porta NÃO OU se escreve com dois operadores: o OU, `|`, e o NÃO, `~`. A ordem em que eles se aplicam importa. O `~` vale só para o que vem logo depois dele: `~a | b` inverte `a` e depois faz o OU com `b`, que é outra função. Para inverter o resultado de uma operação inteira, ponha a operação entre parênteses.

## Dica

Calcule o OU de `a` e `b` e inverta o resultado.

## Dica 2

Os parênteses ficam em volta do OU, e o `~` fica do lado de fora, antes deles.
