# Display de sete segmentos

As placas de FPGA da Terasic, como a DE2-115 e a DE10, trazem displays de sete segmentos, chamados HEX0, HEX1 e assim por diante. Cada display tem sete LEDs em forma de barra, e o circuito que mostra um número precisa dizer quais barras acendem. Nessas placas, cada segmento acende quando o pino da FPGA vai a 0: os displays são ativos em nível baixo.

O módulo `sete_segmentos` recebe um dígito hexadecimal de 4 bits e entrega os sete bits do display. Os segmentos têm letras, de `a` a `g`:

```
 --a--
|     |
f     b
|     |
 --g--
|     |
e     c
|     |
 --d--
```

O `a` é a barra de cima, `b` a de cima à direita, `c` a de baixo à direita, `d` a de baixo, `e` a de baixo à esquerda, `f` a de cima à esquerda e `g` a do meio. Na saída, `seg[0]` é o segmento `a`, `seg[1]` é o `b`, e assim até `seg[6]`, que é o `g`. Um segmento aceso é um 0.

## Portas

| Porta | Direção | Bits | Descrição                                   |
|-------|---------|------|---------------------------------------------|
| `hex` | entrada | 4    | o dígito, de 0 a F                          |
| `seg` | saída   | 7    | um bit por segmento, de `a` a `g`; 0 acende |

## Tabela

Os bits de `seg` estão escritos do `g` ao `a`, na ordem em que um `7'b` os escreve, do `seg[6]` ao `seg[0]`. As letras A, C, E e F aparecem maiúsculas, e b e d minúsculas, para que o b não se confunda com o 8 nem o d com o 0.

| `hex` | Símbolo | Segmentos acesos | `seg` (g f e d c b a) |
|-------|---------|------------------|-----------------------|
| 0     | 0       | a b c d e f      | `1000000`             |
| 1     | 1       | b c              | `1111001`             |
| 2     | 2       | a b d e g        | `0100100`             |
| 3     | 3       | a b c d g        | `0110000`             |
| 4     | 4       | b c f g          | `0011001`             |
| 5     | 5       | a c d f g        | `0010010`             |
| 6     | 6       | a c d e f g      | `0000010`             |
| 7     | 7       | a b c            | `1111000`             |
| 8     | 8       | a b c d e f g    | `0000000`             |
| 9     | 9       | a b c d f g      | `0010000`             |
| A     | A       | a b c e f g      | `0001000`             |
| B     | b       | c d e f g        | `0000011`             |
| C     | C       | a d e f          | `1000110`             |
| D     | d       | b c d e g        | `0100001`             |
| E     | E       | a d e f g        | `0000110`             |
| F     | F       | a e f g          | `0001110`             |

## Dica

Um `case` sobre `hex` dentro de um `always @(*)`, com uma linha por dígito, traduz a tabela diretamente. As constantes de 4 bits em hexadecimal se escrevem `4'h0` até `4'hF`.

## Dica 2

Cada linha tem a forma `4'h2: seg = 7'b0100100;`, com os bits copiados da última coluna. Feche o `case` com uma linha `default`, que pode apagar todos os segmentos com `7'b1111111`.
