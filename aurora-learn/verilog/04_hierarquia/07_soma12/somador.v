// Somador de N bits; o vai-um sai no bit N de s.
module somador #(
    parameter N = 4
) (
    input  [N-1:0] a,
    input  [N-1:0] b,
    output [N:0]   s
);
    assign s = a + b;
endmodule
