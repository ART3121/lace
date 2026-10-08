module troca_bytes (
    input  [31:0] entrada,
    output [31:0] saida
);
    // O byte 0 da entrada vira o byte 3 da saída, e assim por diante.
    assign saida[31:24] = entrada[7:0];
    assign saida[23:16] = entrada[15:8];
    assign saida[15:8]  = entrada[23:16];
    assign saida[7:0]   = entrada[31:24];
endmodule
