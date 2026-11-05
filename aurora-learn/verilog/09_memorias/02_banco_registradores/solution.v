module banco_registradores (
    input        clk,
    input        reset,
    input        we,
    input  [1:0] waddr,
    input  [1:0] raddr,
    input  [7:0] wdata,
    output [7:0] rdata
);
    reg [7:0] regs [0:3];  // os quatro registradores
    integer k;

    // Escrita: só na borda de subida do clock.
    always @(posedge clk) begin
        if (reset) begin
            for (k = 0; k < 4; k = k + 1)
                regs[k] <= 8'd0;
        end else if (we) begin
            regs[waddr] <= wdata;
        end
    end

    // Leitura: combinacional, a qualquer momento.
    assign rdata = regs[raddr];
endmodule
