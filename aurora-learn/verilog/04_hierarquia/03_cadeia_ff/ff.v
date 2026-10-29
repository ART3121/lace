// Flip-flop D: na borda de subida de clk, q passa a valer d.
module ff (
    input      clk,
    input      d,
    output reg q
);
    always @(posedge clk)
        q <= d;
endmodule
