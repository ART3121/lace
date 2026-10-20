module mux4 (
    input      [7:0] a,
    input      [7:0] b,
    input      [7:0] c,
    input      [7:0] d,
    input      [1:0] sel,   // escolhe a entrada
    output reg [7:0] y
);
    always @(*) begin
        case (sel)
            2'd0:    y = a;
            2'd1:    y = b;
            2'd2:    y = c;
            default: y = d;
        endcase
    end
endmodule
