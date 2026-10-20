module detecta_1101_mealy (
    input  clk,
    input  reset,
    input  x,
    output achou
);
    localparam NADA    = 2'd0;  // nenhum começo do padrão
    localparam VIU_1   = 2'd1;  // os últimos bits terminam em 1
    localparam VIU_11  = 2'd2;  // terminam em 1, 1
    localparam VIU_110 = 2'd3;  // terminam em 1, 1, 0

    reg [1:0] estado;
    reg [1:0] proximo;

    // O próximo estado, pelo bit amostrado nesta borda.
    always @(*) begin
        proximo = estado;
        case (estado)
            NADA:    if (x) proximo = VIU_1;
            VIU_1:   proximo = x ? VIU_11 : NADA;
            VIU_11:  if (!x) proximo = VIU_110;
            VIU_110: proximo = x ? VIU_1 : NADA;  // o último 1 começa outro
            default: proximo = NADA;
        endcase
    end

    // O registrador de estado, com reset síncrono.
    always @(posedge clk)
        if (reset)
            estado <= NADA;
        else
            estado <= proximo;

    // Mealy: a saída depende do estado e da entrada, sem esperar a borda.
    assign achou = (estado == VIU_110) & x;
endmodule
