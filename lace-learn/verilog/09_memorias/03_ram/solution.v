module ram (
    input            clk,
    input            we,
    input      [3:0] addr,
    input      [7:0] wdata,
    output reg [7:0] rdata
);
    reg [7:0] mem [0:15];  // 16 palavras de 8 bits

    always @(posedge clk) begin
        if (we)
            mem[addr] <= wdata;
        // Não bloqueante: lê a palavra de antes da escrita.
        rdata <= mem[addr];
    end
endmodule
