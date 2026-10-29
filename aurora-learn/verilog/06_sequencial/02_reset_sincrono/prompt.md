# Reset síncrono

Quando a placa liga, cada flip-flop pode começar em 0 ou em 1, e não há como saber qual. O reset põe o circuito num estado conhecido antes de ele começar a trabalhar.

No reset síncrono, o sinal `reset` só é olhado na borda de subida do clock, como mais uma entrada: se ele vale 1 na borda, `q` passa a 0; se vale 0, `q` passa a valer `d`. Se `reset` sobe no meio do ciclo, `q` só vai a 0 na borda seguinte.

## Portas

| Porta   | Direção | Bits | Descrição                      |
|---------|---------|------|--------------------------------|
| `clk`   | entrada | 1    | o clock                        |
| `reset` | entrada | 1    | em 1 na borda, zera `q`        |
| `d`     | entrada | 1    | o bit a guardar                |
| `q`     | saída   | 1    | o bit guardado na última borda |

## Exemplo

Cada linha mostra os valores logo depois de uma borda de subida do clock.

| Borda | `reset` na borda | `d` na borda | `q` |
|-------|------------------|--------------|-----|
| 1     | 1                | 1            | 0   |
| 2     | 0                | 1            | 1   |
| 3     | 0                | 0            | 0   |
| 4     | 0                | 1            | 1   |
| 5     | 1                | 1            | 0   |
| 6     | 0                | 1            | 1   |

## Dica

O reset é mais uma condição dentro do mesmo bloco `always @(posedge clk)`. A lista de sensibilidade não muda.

## Dica 2

Use `if (reset) ... else ...`. Escreva o zero com o tamanho: `1'b0`.
