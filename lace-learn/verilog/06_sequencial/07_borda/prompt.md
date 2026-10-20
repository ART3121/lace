# Detector de borda de subida

Um botão apertado fica em 1 por milhões de ciclos de clock, e a linha de trigger de um detector fica em 1 por vários ciclos a cada evento. Se o circuito contar os ciclos em 1, um aperto vira milhões. O detector de borda troca cada subida de `sinal` por um pulso de um ciclo só, e cada evento conta uma vez.

O módulo guarda `sinal` em dois registradores. A cada borda de subida, `atual` recebe `sinal` e `anterior` recebe o valor que `atual` tinha. A saída é `pulso = atual & ~anterior`: vale 1 quando a última amostra de `sinal` é 1 e a de antes era 0. Como a saída vem só dos registradores, ela não muda quando `sinal` muda no meio do ciclo. O reset é síncrono e zera os dois registradores.

## Portas

| Porta   | Direção | Bits | Descrição                                   |
|---------|---------|------|---------------------------------------------|
| `clk`   | entrada | 1    | o clock                                     |
| `reset` | entrada | 1    | em 1 na borda, zera os registradores        |
| `sinal` | entrada | 1    | o sinal observado                           |
| `pulso` | saída   | 1    | 1 durante um ciclo a cada subida de `sinal` |

## Exemplo

Cada linha mostra os valores logo depois de uma borda de subida do clock.

| Borda | `reset` | `sinal` na borda | `atual` | `anterior` | `pulso` |
|-------|---------|------------------|---------|------------|---------|
| 1     | 1       | 0                | 0       | 0          | 0       |
| 2     | 0       | 0                | 0       | 0          | 0       |
| 3     | 0       | 1                | 1       | 0          | 1       |
| 4     | 0       | 1                | 1       | 1          | 0       |
| 5     | 0       | 1                | 1       | 1          | 0       |
| 6     | 0       | 0                | 0       | 1          | 0       |
| 7     | 0       | 1                | 1       | 0          | 1       |
| 8     | 0       | 0                | 0       | 1          | 0       |

O `sinal` ficou em 1 por três bordas, da 3 à 5, e `pulso` valeu 1 só depois da borda 3. Se `sinal` já está em 1 quando o reset acaba, a primeira borda depois dele também dá um pulso, porque os dois registradores saem do reset em 0.

## Dica

Declare dois `reg` internos, `atual` e `anterior`, e atualize os dois no mesmo `always @(posedge clk)`. A saída `pulso` é um `assign` fora do bloco.

## Dica 2

Com `<=`, `anterior <= atual;` pega o valor que `atual` tinha antes da borda, que é a amostra do ciclo anterior. No reset, os dois recebem `1'b0`.
