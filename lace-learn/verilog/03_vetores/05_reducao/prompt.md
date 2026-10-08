# Operadores de redução

Na transmissão serial, um byte pode seguir acompanhado de um bit de paridade, escolhido para que o total de bits em 1, contando o da paridade, seja par. Quem recebe conta os bits em 1 de novo: se der ímpar, algum bit trocou no caminho.

O módulo `reducao` calcula três resumos de um byte, `dado`:

- `paridade` vale 1 quando `dado` tem um número ímpar de bits em 1. É o bit de paridade do byte.
- `todos` vale 1 quando todos os bits de `dado` valem 1.
- `algum` vale 1 quando pelo menos um bit de `dado` vale 1.

## Portas

| Porta      | Direção | Bits | Descrição                 |
|------------|---------|------|---------------------------|
| `dado`     | entrada | 8    | o byte                    |
| `paridade` | saída   | 1    | número ímpar de bits em 1 |
| `todos`    | saída   | 1    | todos os bits em 1        |
| `algum`    | saída   | 1    | pelo menos um bit em 1    |

## Exemplos

| `dado`        | `paridade` | `todos` | `algum` |
|---------------|------------|---------|---------|
| `8'b00000000` | 0          | 0       | 0       |
| `8'b00010110` | 1          | 0       | 1       |
| `8'b10010110` | 0          | 0       | 1       |
| `8'b11111111` | 0          | 1       | 1       |

## Em Verilog

Um operador de redução tem um único operando, um vetor, e combina todos os bits dele num bit só. Ele se escreve com o símbolo da operação antes do vetor: `&v` é o E de todos os bits de `v`, `|v` é o OU e `^v` é o OU exclusivo. Entre dois operandos, o mesmo símbolo volta a ser o operador bit a bit.

## Dica

Cada saída é um operador de redução aplicado a `dado`.

## Dica 2

O OU exclusivo de vários bits vale 1 quando o número de bits em 1 é ímpar.
