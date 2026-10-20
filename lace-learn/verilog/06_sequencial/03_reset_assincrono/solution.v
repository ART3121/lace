module reset_assincrono (
    input      clk,
    input      reset,
    input      d,
    output reg q
);
    // O bloco acorda também na subida do reset, sem esperar o clock.
    always @(posedge clk or posedge reset)
        if (reset)
            q <= 1'b0;
        else
            q <= d;
endmodule
