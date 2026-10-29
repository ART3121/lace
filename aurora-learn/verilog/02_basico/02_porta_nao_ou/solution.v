module porta_nao_ou (
    input  a,
    input  b,
    output y
);
    assign y = ~(a | b);
endmodule
