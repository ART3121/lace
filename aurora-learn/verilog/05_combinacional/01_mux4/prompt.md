# Multiplexador de 4 entradas

Um multiplexador escolhe, entre várias entradas, qual delas vai para a saída. Num processador, é ele que decide se a unidade lógica e aritmética recebe um registrador, uma constante da instrução ou um dado da memória. Na eletrônica de leitura de um detector, vários canais podem dividir um mesmo conversor, e um multiplexador escolhe qual canal é lido em cada momento.

O módulo `mux4` tem quatro entradas de 8 bits e um seletor de 2 bits. O valor de `sel` diz qual entrada aparece em `y`.

## Portas

| Porta | Direção | Bits | Descrição                   |
|-------|---------|------|-----------------------------|
| `a`   | entrada | 8    | vai para `y` com `sel` em 0 |
| `b`   | entrada | 8    | vai para `y` com `sel` em 1 |
| `c`   | entrada | 8    | vai para `y` com `sel` em 2 |
| `d`   | entrada | 8    | vai para `y` com `sel` em 3 |
| `sel` | entrada | 2    | escolhe a entrada           |
| `y`   | saída   | 8    | a entrada escolhida         |

## Em Verilog

O bloco `always @(*)` descreve um circuito combinacional com comandos, como num programa. O `*` diz que o bloco é recalculado sempre que algum sinal lido dentro dele muda. Dentro do bloco, a atribuição se escreve sem `assign`, com `=`, e o sinal atribuído precisa ser declarado como `reg`. Por isso a saída já vem como `output reg`. Apesar do nome, num bloco `always @(*)` um `reg` não guarda nada: ele vira fios e portas lógicas como os de um `assign`.

O `case` compara um valor com uma lista de constantes e executa a linha da primeira que for igual. A linha `default` vale para todos os valores que não estão na lista:

```verilog
always @(*) begin
    case (modo)
        2'd0:    saida = 4'd5;
        2'd1:    saida = entrada;
        default: saida = 4'd0;
    endcase
end
```

## Dica

Escreva um `case` sobre `sel` dentro de um `always @(*)`, com uma linha para cada entrada.

## Dica 2

Os valores de `sel` são `2'd0`, `2'd1`, `2'd2` e `2'd3`. A última linha pode ser o `default`, que então cobre o 3 e garante que `y` recebe um valor em todos os casos.
