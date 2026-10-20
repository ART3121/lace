# Soma com sinal e estouro

Em complemento de dois, 8 bits guardam de -128 a 127. A soma 100 + 100 dá 200, que não cabe: os 8 bits do resultado são `8'hC8`, que em complemento de dois vale -56. Dois positivos somados deram um negativo. Com 8 bits na saída não há como guardar o 200, mas o circuito pode avisar que o resultado está errado, e quem usa a soma decide o que fazer.

O módulo `soma_overflow` soma `a` e `b` em complemento de dois. A saída `s` tem sempre os 8 bits de baixo da soma, e `overflow` vale 1 quando o resultado verdadeiro não cabe em 8 bits com sinal.

## Portas

| Porta      | Direção | Bits | Descrição                                  |
|------------|---------|------|--------------------------------------------|
| `a`        | entrada | 8    | primeira parcela, em complemento de dois   |
| `b`        | entrada | 8    | segunda parcela, em complemento de dois    |
| `s`        | saída   | 8    | os 8 bits de baixo de `a + b`              |
| `overflow` | saída   | 1    | 1 quando `a + b` não cabe entre -128 e 127 |

## Exemplo

| `a`            | `b`            | soma verdadeira | `s`           | `overflow` |
|----------------|----------------|-----------------|---------------|------------|
| `8'h32` (50)   | `8'h14` (20)   | 70              | `8'h46` (70)  | 0          |
| `8'h64` (100)  | `8'h64` (100)  | 200             | `8'hC8` (-56) | 1          |
| `8'h9C` (-100) | `8'h9C` (-100) | -200            | `8'h38` (56)  | 1          |
| `8'hFF` (-1)   | `8'hFF` (-1)   | -2              | `8'hFE` (-2)  | 0          |
| `8'h7F` (127)  | `8'h80` (-128) | -1              | `8'hFF` (-1)  | 0          |
| `8'h80` (-128) | `8'hFF` (-1)   | -129            | `8'h7F` (127) | 1          |

Repare na linha de -1 + -1. Lidos sem sinal, os mesmos bits são 255 + 255, que tem vai-um; com sinal, o resultado, -2, cabe. O vai-um do bit 7 não diz se houve estouro com sinal.

## Em Verilog

As portas declaradas `signed` fazem as contas e as comparações tratarem os bits como complemento de dois. A soma de 8 bits dá os mesmos bits com ou sem sinal; o que muda é a leitura deles.

## Dica

Somar um positivo com um negativo sempre cabe: o resultado fica entre os dois. O estouro só acontece quando as duas parcelas têm o mesmo sinal, que é o bit 7 de cada uma.

## Dica 2

Com as parcelas de mesmo sinal, houve estouro quando o bit 7 de `s` é diferente do bit 7 delas. Compare `a[7]`, `b[7]` e `s[7]` com `==` e `!=` e junte as duas condições com `&&`.
