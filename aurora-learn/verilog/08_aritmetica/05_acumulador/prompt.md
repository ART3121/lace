# Acumulador

Somar as amostras de um pulso dá uma medida da carga que ele trouxe, e com ela da energia depositada no detector. O acumulador faz essa soma ao longo do tempo: a cada borda de clock, ele soma a amostra nova ao que já guardava. O registrador tem largura fixa, e quem projeta escolhe uma que baste para a janela de soma: 12 bits guardam até 4095, o bastante para 16 amostras de 8 bits (16 × 255 = 4080). Passando de 4095, a conta dá a volta e recomeça do zero, como um hodômetro.

O módulo `acumulador` soma `x` em `acc` a cada borda de subida de `clk` em que `en` vale 1. O `reset` é síncrono e tem prioridade: na borda em que ele vale 1, `acc` vai para 0.

## Portas

| Porta   | Direção | Bits | Descrição                          |
|---------|---------|------|------------------------------------|
| `clk`   | entrada | 1    | o clock                            |
| `reset` | entrada | 1    | síncrono, ativo em 1: zera `acc`   |
| `en`    | entrada | 1    | 1 para somar a amostra nesta borda |
| `x`     | entrada | 8    | a amostra, sem sinal               |
| `acc`   | saída   | 12   | a soma acumulada                   |

## Exemplo

Cada linha mostra os valores logo depois de uma borda de subida do clock.

| Borda | `reset` | `en` | `x` | `acc` |
|-------|---------|------|-----|-------|
| 1     | 1       | 0    | 7   | 0     |
| 2     | 0       | 1    | 200 | 200   |
| 3     | 0       | 1    | 100 | 300   |
| 4     | 0       | 0    | 50  | 300   |
| 5     | 0       | 1    | 50  | 350   |
| 6     | 1       | 1    | 9   | 0     |

Com `acc` em 4000 e `x` em 200 numa borda com `en` em 1, a soma verdadeira é 4200, e `acc` fica com 4200 - 4096 = 104.

## Dica

Um único bloco `always @(posedge clk)` resolve: primeiro o `reset`, depois o `en`. Na borda em que nenhum dos dois vale 1, não atribua nada a `acc`: o registrador guarda o valor sozinho.

## Dica 2

Na soma `acc + x`, o Verilog estende `x` com zeros até os 12 bits de `acc`. A conta tem 12 bits, e o que passa de 4095 se perde, que é a volta pedida.
