module registrador_en (
    input            clk,
    input            reset,
    input            en,
    input      [7:0] d,
    output reg [7:0] q
);
    // Com reset e en em 0, nenhuma atribuição: q guarda o valor.
    always @(posedge clk)
        if (reset)
            q <= 8'd0;
        else if (en)
            q <= d;
endmodule
