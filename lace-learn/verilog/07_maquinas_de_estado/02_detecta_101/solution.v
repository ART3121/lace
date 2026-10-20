module detecta_101 (
    input  clk,
    input  reset,
    input  x,
    output achou
);
    localparam NADA   = 2'd0;  // nenhum começo do padrão
    localparam VIU_1  = 2'd1;  // os últimos bits terminam em 1
    localparam VIU_10 = 2'd2;  // terminam em 1, 0
    localparam ACHOU  = 2'd3;  // terminam em 1, 0, 1

    reg [1:0] estado;
    reg [1:0] proximo;

    // O próximo estado, pelo bit amostrado nesta borda.
    always @(*) begin
        proximo = estado;
        case (estado)
            NADA:    if (x) proximo = VIU_1;
            VIU_1:   if (!x) proximo = VIU_10;
            VIU_10:  proximo = x ? ACHOU : NADA;
            ACHOU:   proximo = x ? VIU_1 : VIU_10;  // o último 1 começa outro
            default: proximo = NADA;
        endcase
    end

    // O registrador de estado, com reset síncrono.
    always @(posedge clk)
        if (reset)
            estado <= NADA;
        else
            estado <= proximo;

    // Moore: a saída depende só do estado.
    assign achou = (estado == ACHOU);
endmodule
