module borda (
    input  clk,
    input  reset,
    input  sinal,
    output pulso
);
    reg atual;     // sinal amostrado na última borda
    reg anterior;  // sinal amostrado na borda de antes

    always @(posedge clk)
        if (reset) begin
            atual    <= 1'b0;
            anterior <= 1'b0;
        end else begin
            atual    <= sinal;
            anterior <= atual;
        end

    // A saída vem só dos registradores.
    assign pulso = atual & ~anterior;
endmodule
