# Máquinas de estado

Uma máquina de estado tem três partes: um registrador que guarda o estado atual, a lógica que calcula o próximo estado a partir do estado e das entradas, e a lógica que calcula as saídas. Numa máquina de Moore, as saídas dependem só do estado. Numa máquina de Mealy, dependem também das entradas, e mudam assim que elas mudam, sem esperar a borda do clock. Neste capítulo cada estado ganha um nome com `localparam`, a lógica do próximo estado fica num `always @(*)` com `case`, e o registrador de estado fica num `always @(posedge clk)` com reset síncrono.
