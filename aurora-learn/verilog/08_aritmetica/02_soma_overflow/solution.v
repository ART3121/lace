module soma_overflow (
    input  signed [7:0] a,
    input  signed [7:0] b,
    output signed [7:0] s,
    output              overflow
);
    assign s = a + b;

    // Estouro: as parcelas têm o mesmo sinal, e o resultado tem o outro.
    assign overflow = (a[7] == b[7]) && (s[7] != a[7]);
endmodule
