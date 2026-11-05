# Memórias

Uma memória em Verilog é um vetor de registradores: `reg [7:0] mem [0:15];` declara 16 palavras de 8 bits, e `mem[3]` é a palavra no endereço 3. Numa FPGA, uma memória escrita assim vira um bloco de RAM do próprio chip, a block RAM. O SAPHO, o processador soft-core do laboratório, guarda o programa e os dados em memórias como estas. Neste capítulo você escreve uma tabela fixa, um banco de registradores e uma RAM síncrona.
