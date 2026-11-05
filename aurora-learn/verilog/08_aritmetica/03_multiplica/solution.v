module multiplica (
    input  [3:0] a,
    input  [3:0] b,
    output [7:0] p
);
    // Cada produto parcial é zero ou uma cópia de a, deslocada i posições.
    wire [7:0] p0 = b[0] ? {4'b0000, a}       : 8'd0;
    wire [7:0] p1 = b[1] ? {3'b000, a, 1'b0}  : 8'd0;
    wire [7:0] p2 = b[2] ? {2'b00, a, 2'b00}  : 8'd0;
    wire [7:0] p3 = b[3] ? {1'b0, a, 3'b000}  : 8'd0;

    assign p = p0 + p1 + p2 + p3;
endmodule
