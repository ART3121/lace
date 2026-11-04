module soma_satura (
    input  [7:0] a,
    input  [7:0] b,
    output [7:0] s
);
    // Com 9 bits, a soma não perde o vai-um.
    wire [8:0] soma = a + b;

    // O bit 8 diz que a soma passou de 255.
    assign s = soma[8] ? 8'd255 : soma[7:0];
endmodule
