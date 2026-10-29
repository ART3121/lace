module deslocamento (
    input            clk,
    input            reset,
    input            serial,
    output reg [3:0] q
);
    // Cada bit anda uma posição para a esquerda e serial entra em q[0].
    always @(posedge clk)
        if (reset)
            q <= 4'd0;
        else
            q <= {q[2:0], serial};
endmodule
