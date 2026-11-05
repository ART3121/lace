# Multiplicador de 4 bits

A multiplicação binária segue a conta feita à mão, com uma simplificação: cada algarismo de `b` é 0 ou 1, então cada linha da conta é zero ou uma cópia de `a` deslocada para a esquerda. Essas linhas são os produtos parciais, e o produto é a soma delas. Dois números de 4 bits dão no máximo 15 × 15 = 225, que cabe em 8 bits: a largura do produto é a soma das larguras dos fatores.

O módulo `multiplica` multiplica `a` por `b`, os dois sem sinal.

## Portas

| Porta | Direção | Bits | Descrição          |
|-------|---------|------|--------------------|
| `a`   | entrada | 4    | o multiplicando    |
| `b`   | entrada | 4    | o multiplicador    |
| `p`   | saída   | 8    | o produto, `a × b` |

## Exemplo

Com `a = 4'b1011` (11) e `b = 4'b0110` (6):

```
        1011    a
      x 0110    b
      ------
        0000    b[0] = 0: zero
       1011     b[1] = 1: a deslocado 1 posição
      1011      b[2] = 1: a deslocado 2 posições
     0000       b[3] = 0: zero
    --------
    01000010    66
```

| `a` | `b` | `p`           |
|-----|-----|---------------|
| 0   | 15  | 0             |
| 3   | 5   | 15            |
| 11  | 6   | 66            |
| 15  | 15  | 225 (`8'hE1`) |

## Em Verilog

O operador `*` existe, e a correção aceita uma solução com ele. Vale montar os produtos parciais assim mesmo: é o circuito que o sintetizador monta quando encontra o `*`.

## Dica

Cada produto parcial tem 8 bits. O de número `i` vale zero quando `b[i]` é 0 e, quando `b[i]` é 1, vale `a` com `i` zeros à direita, completado com zeros à esquerda até os 8 bits.

## Dica 2

O produto parcial 2, por exemplo, é `b[2] ? {2'b00, a, 2'b00} : 8'd0`. Faça os quatro em fios de 8 bits e some.
