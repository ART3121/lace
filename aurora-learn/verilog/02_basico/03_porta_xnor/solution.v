module porta_xnor (
    input  a,
    input  b,
    output iguais
);
    assign iguais = ~(a ^ b);
endmodule
