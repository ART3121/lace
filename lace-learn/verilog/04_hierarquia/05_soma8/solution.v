module soma8 (
    input  [7:0] a,
    input  [7:0] b,
    output [8:0] s
);
    wire c4;  // o vai-um da metade de baixo para a de cima

    somador4 baixo (.a(a[3:0]), .b(b[3:0]), .cin(1'b0), .s(s[3:0]), .cout(c4));
    somador4 alto  (.a(a[7:4]), .b(b[7:4]), .cin(c4),   .s(s[7:4]), .cout(s[8]));
endmodule
