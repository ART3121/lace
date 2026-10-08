module soma4 (
    input  [3:0] a,
    input  [3:0] b,
    output [4:0] s
);
    wire [3:0] c;
    somador_completo s0 (.a(a[0]), .b(b[0]), .cin(1'b0), .s(s[0]), .cout(c[0]));
    somador_completo s1 (.a(a[1]), .b(b[1]), .cin(c[0]), .s(s[1]), .cout(c[1]));
    somador_completo s2 (.a(a[2]), .b(b[2]), .cin(c[1]), .s(s[2]), .cout(c[2]));
    somador_completo s3 (.a(a[3]), .b(b[3]), .cin(c[2]), .s(s[3]), .cout(c[3]));
    assign s[4] = c[3];
endmodule
