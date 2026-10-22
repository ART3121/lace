# RAM síncrona

As block RAMs de uma FPGA são síncronas: o endereço é registrado na borda do clock, e a palavra lida só aparece na saída depois dessa borda, um ciclo depois de o endereço ser apresentado. Quem usa a memória precisa contar com esse ciclo de atraso, e o SAPHO lê o programa e os dados assim.

O módulo `ram` tem 16 palavras de 8 bits e faz tudo na borda de subida do clock. Se `we` vale 1, a palavra no endereço `addr` passa a guardar `wdata`. Na mesma borda, com ou sem escrita, `rdata` recebe a palavra que estava em `addr` antes da borda. Quando há escrita, `rdata` recebe o valor antigo, e o novo só sai numa leitura seguinte. Entre as bordas, `rdata` não muda.

A memória não tem reset, como costuma ser numa block RAM: uma palavra ainda não escrita vale X, e enquanto a referência entrega X a correção aceita qualquer valor na sua saída.

## Portas

| Porta   | Direção | Bits | Descrição                              |
|---------|---------|------|----------------------------------------|
| `clk`   | entrada | 1    | o clock                                |
| `we`    | entrada | 1    | em 1 na borda, grava `wdata` em `addr` |
| `addr`  | entrada | 4    | o endereço lido e escrito              |
| `wdata` | entrada | 8    | o valor escrito                        |
| `rdata` | saída   | 8    | a palavra lida na última borda         |

## Exemplo

Cada linha mostra os valores logo depois de uma borda de subida do clock. As entradas são as que estavam aplicadas na borda.

| Borda | `we` | `addr` | `wdata` | `mem[addr]` depois | `rdata` |
|-------|------|--------|---------|--------------------|---------|
| 1     | 1    | 4      | 10      | 10                 | X       |
| 2     | 1    | 5      | 20      | 20                 | X       |
| 3     | 0    | 4      | 99      | 10                 | 10      |
| 4     | 1    | 4      | 30      | 30                 | 10      |
| 5     | 0    | 4      | 0       | 30                 | 30      |

Na borda 1, a palavra 4 ainda não tinha sido escrita, e `rdata` recebe X. Na borda 4, a escrita do 30 e a leitura acontecem juntas, e `rdata` recebe o 10 que estava lá antes.

## Dica

Declare a memória com `reg [7:0] mem [0:15];` e faça a escrita e a leitura no mesmo `always @(posedge clk)`.

## Dica 2

Com atribuições não bloqueantes, todas leem os valores de antes da borda. `mem[addr] <= wdata;` sob um `if (we)` e `rdata <= mem[addr];` fora dele dão a leitura do valor antigo.
