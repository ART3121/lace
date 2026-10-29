module inverte_bits (
    input  [7:0] entrada,
    output [7:0] saida
);
    // O bit 0 da entrada vai para o bit 7 da saída, e assim por diante.
    assign saida = {entrada[0], entrada[1], entrada[2], entrada[3],
                    entrada[4], entrada[5], entrada[6], entrada[7]};
endmodule
