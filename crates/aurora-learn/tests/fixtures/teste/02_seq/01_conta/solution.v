module conta (
    input            clk,
    input            reset,
    input            en,
    output reg [3:0] q
);
    always @(posedge clk)
        if (reset)
            q <= 4'd0;
        else if (en)
            q <= q + 4'd1;
endmodule
