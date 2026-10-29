# Banco de registradores

Um processador guarda os operandos das contas num banco de registradores: algumas palavras de acesso rápido, cada uma com um número. Numa instrução como "some o registrador 1 com o 2", o banco entrega os dois valores na hora, e o resultado é gravado na borda de clock seguinte.

O módulo `banco_registradores` tem 4 registradores de 8 bits. A escrita é síncrona: na borda de subida do clock, se `we` vale 1, o registrador `waddr` passa a guardar `wdata`. A leitura é combinacional: `rdata` mostra sempre o valor guardado no registrador `raddr` e muda assim que `raddr` muda, sem esperar o clock. Uma escrita só aparece em `rdata` depois da borda que a grava. O reset é síncrono: com `reset` em 1 na borda de subida, os quatro registradores voltam a 0, e nada é escrito nessa borda.

## Portas

| Porta   | Direção | Bits | Descrição                                   |
|---------|---------|------|---------------------------------------------|
| `clk`   | entrada | 1    | o clock                                     |
| `reset` | entrada | 1    | em 1 na borda, zera os quatro registradores |
| `we`    | entrada | 1    | em 1 na borda, grava `wdata` em `waddr`     |
| `waddr` | entrada | 2    | o registrador escrito                       |
| `raddr` | entrada | 2    | o registrador lido                          |
| `wdata` | entrada | 8    | o valor escrito                             |
| `rdata` | saída   | 8    | o valor guardado no registrador `raddr`     |

## Exemplo

Cada linha mostra os valores logo depois de uma borda de subida do clock. As entradas são as que estavam aplicadas na borda; r0 a r3 são os registradores.

| Borda | `reset` | `we` | `waddr` | `wdata` | `raddr` | r0 | r1 | r2 | r3 | `rdata` |
|-------|---------|------|---------|---------|---------|----|----|----|----|---------|
| 1     | 1       | 0    | 0       | 0       | 0       | 0  | 0  | 0  | 0  | 0       |
| 2     | 0       | 1    | 1       | 7       | 1       | 0  | 7  | 0  | 0  | 7       |
| 3     | 0       | 1    | 2       | 5       | 1       | 0  | 7  | 5  | 0  | 7       |
| 4     | 0       | 0    | 3       | 9       | 2       | 0  | 7  | 5  | 0  | 5       |
| 5     | 0       | 1    | 1       | 3       | 1       | 0  | 3  | 5  | 0  | 3       |

Na borda 4, `we` vale 0 e nada é gravado. Entre a borda 4 e a 5, quando `raddr` muda para 1, `rdata` passa a 7 na hora; o 3 só aparece depois da borda 5.

## Dica

Declare os registradores como uma memória, `reg [7:0] regs [0:3];`. A escrita fica num `always @(posedge clk)`, e a leitura, fora dele, num `assign`.

## Dica 2

No `always`, o reset vem primeiro e zera as quatro palavras, uma a uma ou com um `for`; depois vem `else if (we) regs[waddr] <= wdata;`. A leitura é `assign rdata = regs[raddr];`.
