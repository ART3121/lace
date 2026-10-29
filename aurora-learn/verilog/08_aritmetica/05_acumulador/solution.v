module acumulador (
    input             clk,
    input             reset,
    input             en,
    input      [7:0]  x,
    output reg [11:0] acc
);
    always @(posedge clk)
        if (reset)
            acc <= 12'd0;
        else if (en)
            acc <= acc + x;  // x vai estendido até 12 bits; passa de 4095 e dá a volta
endmodule
