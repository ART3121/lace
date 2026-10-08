# Alarme

O alarme de uma casa recebe quatro sinais: se ele está armado, se a porta está aberta, se a janela está aberta e se a senha certa foi digitada no teclado. Ele controla duas saídas:

- `aviso` acende quando a porta ou a janela está aberta, com o alarme armado ou não. É a lâmpada que lembra o morador de fechar tudo antes de sair.
- `sirene` toca quando o alarme está armado e a porta ou a janela está aberta, a menos que a senha certa tenha sido digitada.

## Portas

| Porta           | Direção | Bits | Descrição                  |
|-----------------|---------|------|----------------------------|
| `armado`        | entrada | 1    | o alarme está armado       |
| `porta_aberta`  | entrada | 1    | a porta está aberta        |
| `janela_aberta` | entrada | 1    | a janela está aberta       |
| `senha_ok`      | entrada | 1    | a senha certa foi digitada |
| `aviso`         | saída   | 1    | há porta ou janela aberta  |
| `sirene`        | saída   | 1    | a sirene toca              |

## Exemplos

| `armado` | `porta_aberta` | `janela_aberta` | `senha_ok` | `aviso` | `sirene` |
|----------|----------------|-----------------|------------|---------|----------|
| 0        | 1              | 0               | 0          | 1       | 0        |
| 1        | 0              | 0               | 0          | 0       | 0        |
| 1        | 0              | 1               | 0          | 1       | 1        |
| 1        | 1              | 1               | 1          | 1       | 0        |

## Dica

Traduza a regra palavra por palavra: "e" vira `&`, "ou" vira `|`, e "a menos que" vira um E com a condição invertida.

## Dica 2

O `&` é aplicado antes do `|`: `x & y | z` é o mesmo que `(x & y) | z`. Ponha o OU da porta e da janela entre parênteses, ou calcule-o antes num fio interno, que também serve à saída `aviso`.
