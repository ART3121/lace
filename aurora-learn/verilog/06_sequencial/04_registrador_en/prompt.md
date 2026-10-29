# Registrador com habilitação

Os registradores de um processador não mudam a cada ciclo. Um registrador só recebe valor novo quando uma instrução escreve nele, e nos outros ciclos guarda o que tinha. A entrada `en` (de *enable*, habilitar) faz esse controle.

Na borda de subida, com `en` em 1, `q` passa a valer `d`; com `en` em 0, `q` fica como está. O reset é síncrono e vale mais que o `en`: com `reset` em 1 na borda, `q` vai a 0.

## Portas

| Porta   | Direção | Bits | Descrição                  |
|---------|---------|------|----------------------------|
| `clk`   | entrada | 1    | o clock                    |
| `reset` | entrada | 1    | em 1 na borda, zera `q`    |
| `en`    | entrada | 1    | em 1 na borda, carrega `d` |
| `d`     | entrada | 8    | o byte a guardar           |
| `q`     | saída   | 8    | o byte guardado            |

## Exemplo

Cada linha mostra os valores logo depois de uma borda de subida do clock.

| Borda | `reset` | `en` | `d`     | `q`     |
|-------|---------|------|---------|---------|
| 1     | 1       | 0    | `8'h3A` | `8'h00` |
| 2     | 0       | 1    | `8'h3A` | `8'h3A` |
| 3     | 0       | 0    | `8'hFF` | `8'h3A` |
| 4     | 0       | 0    | `8'h12` | `8'h3A` |
| 5     | 0       | 1    | `8'h12` | `8'h12` |
| 6     | 1       | 1    | `8'h77` | `8'h00` |

## Dica

Pense na ordem das decisões: primeiro o reset, depois o `en`. Quando nenhuma das duas condições vale, `q` não precisa de atribuição: o registrador guarda o valor sozinho.

## Dica 2

Use `if (reset) ... else if (en) ...`, sem `else` no fim. Num bloco com clock, isso não cria latch: o caso que falta é justamente o de guardar o valor. O zero de 8 bits é `8'd0`.
