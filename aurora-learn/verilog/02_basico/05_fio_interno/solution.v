module fio_interno (
    input  esq1,    // botão esquerdo do posto 1
    input  dir1,    // botão direito do posto 1
    input  esq2,    // botão esquerdo do posto 2
    input  dir2,    // botão direito do posto 2
    output desce,   // a prensa desce
    output parada   // a prensa está parada
);
    wire pronto1;   // os dois botões do posto 1 apertados
    wire pronto2;   // os dois botões do posto 2 apertados

    assign pronto1 = esq1 & dir1;
    assign pronto2 = esq2 & dir2;
    assign desce   = pronto1 | pronto2;
    assign parada  = ~desce;
endmodule
