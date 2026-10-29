module flip_flop (
    input      clk,
    input      d,
    output reg q
);
    // Na borda de subida, q passa a valer d e guarda o valor até a próxima.
    always @(posedge clk)
        q <= d;
endmodule
