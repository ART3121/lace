# Verilog

Uma trilha de exercícios curtos de Verilog, do primeiro fio até máquinas de
estado, aritmética e memórias. Cada exercício pede um módulo: as portas já
estão no arquivo, e você escreve o que vai dentro.

Ao gravar o arquivo, o Lace compila o seu módulo e o simula ao lado de uma
solução de referência, com as mesmas entradas. Se alguma saída diferir, ele
mostra quantas amostras erraram, a primeira amostra errada (as entradas, o
valor esperado e o seu) e a onda das duas versões lado a lado. Com o
Verilator e o Yosys instalados, ele também avisa do que simula certo mas não
é o circuito pedido, como um latch esquecido num circuito combinacional.
