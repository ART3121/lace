module media4 (
    input        clk,
    input        reset,
    input  [7:0] x,
    output [7:0] media
);
    // As quatro últimas amostras; h0 é a mais nova.
    reg [7:0] h0, h1, h2, h3;

    always @(posedge clk)
        if (reset) begin
            h0 <= 8'd0;
            h1 <= 8'd0;
            h2 <= 8'd0;
            h3 <= 8'd0;
        end else begin
            h0 <= x;
            h1 <= h0;
            h2 <= h1;
            h3 <= h2;
        end

    // A soma chega a 4 x 255 = 1020 e precisa de 10 bits: com o destino de
    // 10 bits, a conta é feita com 10 bits.
    wire [9:0] soma = h0 + h1 + h2 + h3;

    // Dividir por 4 é descartar os dois bits de baixo.
    assign media = soma[9:2];
endmodule
