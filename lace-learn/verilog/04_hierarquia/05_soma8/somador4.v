// Somador de 4 bits, com vai-um de entrada (cin) e de saída (cout).
module somador4 (
    input  [3:0] a,
    input  [3:0] b,
    input        cin,
    output [3:0] s,
    output       cout
);
    assign {cout, s} = a + b + cin;
endmodule
