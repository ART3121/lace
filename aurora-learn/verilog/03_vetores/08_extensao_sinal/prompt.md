# Extensão de sinal

Em complemento de dois, o bit mais significativo de um número diz o sinal: 0 para positivo, 1 para negativo. Para passar um número de 8 bits para 32 bits sem mudar o valor, não basta completar com zeros à esquerda. Os 24 bits novos devem repetir o bit de sinal. Isso se chama extensão de sinal.

## Portas

| Porta       | Direção | Bits | Descrição                                 |
|-------------|---------|------|-------------------------------------------|
| `numero`    | entrada | 8    | inteiro com sinal, em complemento de dois |
| `estendido` | saída   | 32   | o mesmo valor, em 32 bits                 |

## Exemplos

| `numero` | valor | `estendido`    |
|----------|-------|----------------|
| `8'h05`  | 5     | `32'h00000005` |
| `8'h7F`  | 127   | `32'h0000007F` |
| `8'hFB`  | -5    | `32'hFFFFFFFB` |
| `8'h80`  | -128  | `32'hFFFFFF80` |

## Em Verilog

A replicação repete um sinal um número fixo de vezes: `{4{x}}` é o mesmo que `{x, x, x, x}`. O número de vezes é uma constante, e a replicação pode entrar numa concatenação como qualquer item: `{{2{a}}, b}` é `{a, a, b}`.

## Dica

A saída é a concatenação de duas partes: os 24 bits novos, à esquerda, e o número original, à direita.

## Dica 2

Os 24 bits novos são cópias de `numero[7]`. Repare nas chaves duplas do exemplo `{{2{a}}, b}`: a replicação tem as suas chaves, e a concatenação em volta tem as dela.
