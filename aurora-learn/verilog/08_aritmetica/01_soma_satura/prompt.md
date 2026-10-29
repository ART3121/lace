# Soma com saturação

Numa soma de 8 bits sem sinal, 200 + 100 dá 44. O resultado verdadeiro, 300, precisa de 9 bits, e o nono bit se perde. Numa amostra de sinal, um pulso grande vira um valor pequeno, e o filtro que vem depois leva o erro adiante. A saturação evita isso: quando a soma passa do maior valor que cabe nos 8 bits, a saída fica nesse valor, 255.

O módulo `soma_satura` soma `a` e `b`, dois números sem sinal, e satura em 255.

## Portas

| Porta | Direção | Bits | Descrição                                  |
|-------|---------|------|--------------------------------------------|
| `a`   | entrada | 8    | primeira parcela, sem sinal                |
| `b`   | entrada | 8    | segunda parcela, sem sinal                 |
| `s`   | saída   | 8    | `a + b`, ou 255 quando a soma passa de 255 |

## Exemplo

| `a` | `b` | soma verdadeira | `s` |
|-----|-----|-----------------|-----|
| 10  | 20  | 30              | 30  |
| 200 | 55  | 255             | 255 |
| 200 | 56  | 256             | 255 |
| 200 | 100 | 300             | 255 |
| 255 | 255 | 510             | 255 |

## Em Verilog

A largura com que uma conta é feita vem dos operandos e do destino: vale a maior delas. Com `s` de 8 bits, `assign s = a + b;` faz a soma com 8 bits e perde o vai-um. Num fio de 9 bits, a mesma soma guarda o vai-um no bit 8.

## Dica

Calcule a soma num fio interno de 9 bits. O bit 8 desse fio vale 1 justamente quando a soma passa de 255.

## Dica 2

Use o operador condicional, `condicao ? valor_se_1 : valor_se_0`, com o bit 8 da soma como condição. Com ele em 1, a saída é a constante `8'd255`; com ele em 0, são os 8 bits de baixo do fio.
