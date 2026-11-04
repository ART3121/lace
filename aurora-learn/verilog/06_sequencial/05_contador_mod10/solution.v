module contador_mod10 (
    input            clk,
    input            reset,
    input            en,
    output reg [3:0] q
);
    always @(posedge clk)
        if (reset)
            q <= 4'd0;
        else if (en) begin
            if (q == 4'd9)
                q <= 4'd0;          // depois do 9 vem o 0
            else
                q <= q + 4'd1;
        end
endmodule
