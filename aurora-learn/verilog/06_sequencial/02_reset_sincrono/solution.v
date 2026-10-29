module reset_sincrono (
    input      clk,
    input      reset,
    input      d,
    output reg q
);
    // O reset só é olhado na borda de subida do clock.
    always @(posedge clk)
        if (reset)
            q <= 1'b0;
        else
            q <= d;
endmodule
