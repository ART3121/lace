# Flip-flop D

O flip-flop D guarda um bit. Na borda de subida do clock, a saída `q` passa a valer o que está na entrada `d`, e fica com esse valor até a borda seguinte, mesmo que `d` mude no meio do ciclo. Cada célula lógica de uma FPGA tem flip-flops como este, e os registradores de um processador são feitos deles.

No capítulo anterior você usou um flip-flop pronto. Agora escreva o seu.

## Portas

| Porta | Direção | Bits | Descrição                      |
|-------|---------|------|--------------------------------|
| `clk` | entrada | 1    | o clock                        |
| `d`   | entrada | 1    | o bit a guardar                |
| `q`   | saída   | 1    | o bit guardado na última borda |

## Exemplo

Cada linha mostra os valores logo depois de uma borda de subida do clock.

| Borda | `d` na borda | `q` |
|-------|--------------|-----|
| 1     | 1            | 1   |
| 2     | 0            | 0   |
| 3     | 0            | 0   |
| 4     | 1            | 1   |

Antes da primeira borda, `q` está em X: o flip-flop não tem reset e ainda não guardou nada.

## Em Verilog

Um bloco `always @(posedge clk)` executa a cada borda de subida de `clk`. Dentro dele, a atribuição não bloqueante `<=` dá o valor que o registrador passa a ter depois da borda. Uma saída que recebe valor dentro de um `always` é declarada como `output reg`, e o `start.v` já a declara assim.

## Dica

Basta um bloco `always` sensível à borda de subida de `clk`, com uma atribuição só.

## Dica 2

Use `<=`, e não `=`: num bloco com clock, `q <= ...` diz que `q` muda na borda. O lado direito é a entrada que o flip-flop copia.
