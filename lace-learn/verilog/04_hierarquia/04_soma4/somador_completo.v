// Somador completo: soma a, b e o vai-um que chega (cin).
module somador_completo (
    input  a,
    input  b,
    input  cin,
    output s,     // o bit da soma
    output cout   // o vai-um que sai
);
    assign s    = a ^ b ^ cin;
    assign cout = (a & b) | (cin & (a ^ b));
endmodule
