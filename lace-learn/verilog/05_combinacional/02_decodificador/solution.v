module decodificador (
    input      [1:0] sel,   // qual saída acende
    input            en,    // habilitação
    output reg [3:0] y
);
    always @(*) begin
        y = 4'b0000;        // valor de partida: tudo apagado
        if (en)
            y[sel] = 1'b1;  // acende só o bit escolhido
    end
endmodule
