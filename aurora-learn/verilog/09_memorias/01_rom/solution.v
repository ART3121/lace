module rom (
    input      [2:0] addr,
    output reg [7:0] dado
);
    // Cada endereço guarda o seu quadrado.
    always @(*) begin
        case (addr)
            3'd0:    dado = 8'd0;
            3'd1:    dado = 8'd1;
            3'd2:    dado = 8'd4;
            3'd3:    dado = 8'd9;
            3'd4:    dado = 8'd16;
            3'd5:    dado = 8'd25;
            3'd6:    dado = 8'd36;
            default: dado = 8'd49;
        endcase
    end
endmodule
