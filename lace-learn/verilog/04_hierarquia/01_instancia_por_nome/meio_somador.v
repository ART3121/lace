// Meio somador: soma dois bits. soma é o bit menos significativo do
// resultado, e vai_um, o mais significativo.
module meio_somador (
    input  a,
    input  b,
    output soma,
    output vai_um
);
    assign soma   = a ^ b;
    assign vai_um = a & b;
endmodule
