# Maioria de três

Em circuitos expostos à radiação, como os de satélites e os de detectores de partículas, um bit guardado pode trocar de valor sozinho. Uma defesa comum é guardar o mesmo bit em três lugares e usar o valor da maioria: se uma das cópias errar, as outras duas ainda dão o valor certo.

O módulo `maioria` faz essa votação: a saída `y` vale 1 quando pelo menos duas das três entradas valem 1.

## Portas

| Porta | Direção | Bits | Descrição             |
|-------|---------|------|-----------------------|
| `a`   | entrada | 1    | primeira cópia do bit |
| `b`   | entrada | 1    | segunda cópia         |
| `c`   | entrada | 1    | terceira cópia        |
| `y`   | saída   | 1    | o valor da maioria    |

## Tabela verdade

| `a` | `b` | `c` | `y` |
|-----|-----|-----|-----|
| 0   | 0   | 0   | 0   |
| 0   | 0   | 1   | 0   |
| 0   | 1   | 0   | 0   |
| 0   | 1   | 1   | 1   |
| 1   | 0   | 0   | 0   |
| 1   | 0   | 1   | 1   |
| 1   | 1   | 0   | 1   |
| 1   | 1   | 1   | 1   |

## Dica

Olhe as linhas em que `y` vale 1. Em todas elas, algum par de entradas está todo em 1.

## Dica 2

Há três pares possíveis: `a` com `b`, `a` com `c` e `b` com `c`. Faça o E de cada par e junte os três resultados com OU.
