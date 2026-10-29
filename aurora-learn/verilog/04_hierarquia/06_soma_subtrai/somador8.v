// Somador de 8 bits, com vai-um de entrada (cin) e de saída (cout).
module somador8 (
    input  [7:0] a,
    input  [7:0] b,
    input        cin,
    output [7:0] s,
    output       cout
);
    assign {cout, s} = a + b + cin;
endmodule
