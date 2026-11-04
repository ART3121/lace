# Uma ROM de quadrados

Uma ROM, memória só de leitura, é uma tabela fixa: o conteúdo é decidido quando o circuito é projetado e não muda durante o uso. Ela guarda constantes que custariam caro para calcular a cada vez, como uma tabela de senos, os coeficientes de um filtro ou o programa de um processador pequeno.

O módulo `rom` guarda 8 palavras de 8 bits. A palavra no endereço `addr` é o quadrado de `addr`, e a saída `dado` mostra essa palavra.

## Portas

| Porta  | Direção | Bits | Descrição                |
|--------|---------|------|--------------------------|
| `addr` | entrada | 3    | o endereço lido          |
| `dado` | saída   | 8    | a palavra nesse endereço |

## Conteúdo

| `addr` | `dado` |
|--------|--------|
| 0      | 0      |
| 1      | 1      |
| 2      | 4      |
| 3      | 9      |
| 4      | 16     |
| 5      | 25     |
| 6      | 36     |
| 7      | 49     |

## Dica

A saída depende só do endereço, então o circuito é combinacional. Um `always @(*)` com um `case` sobre `addr` dá uma linha por palavra da tabela.

## Dica 2

Cada linha do `case` tem a forma `3'd2: dado = 8'd4;`. Termine com um `default`, para que `dado` receba valor em todos os casos.
