# Operadores bit a bit

Os operadores `&`, `|`, `^` e `~` também funcionam com vetores. Aplicados a vetores de 4 bits, eles fazem quatro operações independentes, uma em cada posição, e o resultado também tem 4 bits. Por isso se chamam operadores bit a bit.

Os operadores lógicos `&&`, `||` e `!` são outra coisa. Cada operando vale como um único valor de verdade: falso quando todos os bits são 0, verdadeiro quando algum bit é 1. O resultado tem sempre 1 bit.

O módulo `bit_a_bit` calcula as três operações bit a bit entre `a` e `b`, a negação de `a` e, para comparar, um E lógico.

## Portas

| Porta          | Direção | Bits | Descrição                                    |
|----------------|---------|------|----------------------------------------------|
| `a`            | entrada | 4    | primeiro operando                            |
| `b`            | entrada | 4    | segundo operando                             |
| `e`            | saída   | 4    | E bit a bit de `a` e `b`                     |
| `ou`           | saída   | 4    | OU bit a bit de `a` e `b`                    |
| `ou_exclusivo` | saída   | 4    | OU exclusivo bit a bit de `a` e `b`          |
| `nao_a`        | saída   | 4    | `a` com todos os bits invertidos             |
| `ambos`        | saída   | 1    | 1 quando `a` e `b` são ambos diferentes de 0 |

## Exemplo

Com `a = 4'b0110` e `b = 4'b0011`:

| Saída          | Valor     |
|----------------|-----------|
| `e`            | `4'b0010` |
| `ou`           | `4'b0111` |
| `ou_exclusivo` | `4'b0101` |
| `nao_a`        | `4'b1001` |
| `ambos`        | `1'b1`    |

Com `a = 4'b0100` e `b = 4'b0011`, `e` vale `4'b0000`, mas `ambos` continua em 1: nenhum dos dois vetores é zero.

## Dica

Cada saída de 4 bits é um `assign` com um operador bit a bit. Para `ambos`, use o operador lógico.

## Dica 2

Escrever `a & b` em `ambos` é um erro comum. O resultado tem 4 bits e, ligado a uma saída de 1 bit, só o bit 0 sobra: com `a = 4'b0100` e `b = 4'b0011`, isso dá 0 em vez de 1.
