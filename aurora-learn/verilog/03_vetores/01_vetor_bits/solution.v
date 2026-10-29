module vetor_bits (
    input  [2:0] cor,       // bit 2: vermelho, bit 1: verde, bit 0: azul
    output [2:0] espelho,
    output       vermelho,
    output       verde,
    output       azul
);
    assign espelho  = cor;
    assign vermelho = cor[2];
    assign verde    = cor[1];
    assign azul     = cor[0];
endmodule
