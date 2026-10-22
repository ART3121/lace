module semaforo (
    input  clk,
    input  reset,
    output verde,
    output amarelo,
    output vermelho
);
    localparam VERDE    = 2'd0;  // 4 ciclos
    localparam AMARELO  = 2'd1;  // 1 ciclo
    localparam VERMELHO = 2'd2;  // 3 ciclos

    reg [1:0] estado;
    reg [1:0] conta;          // ciclos já passados na cor, a partir de 0
    reg [1:0] proximo;
    reg [1:0] proxima_conta;

    // A próxima cor e a próxima conta: a cor troca quando o tempo dela acaba.
    always @(*) begin
        proximo       = estado;
        proxima_conta = conta + 2'd1;
        case (estado)
            VERDE:
                if (conta == 2'd3) begin
                    proximo       = AMARELO;
                    proxima_conta = 2'd0;
                end
            AMARELO: begin
                proximo       = VERMELHO;
                proxima_conta = 2'd0;
            end
            VERMELHO:
                if (conta == 2'd2) begin
                    proximo       = VERDE;
                    proxima_conta = 2'd0;
                end
            default: begin
                proximo       = VERDE;
                proxima_conta = 2'd0;
            end
        endcase
    end

    // Os registradores, com reset síncrono.
    always @(posedge clk)
        if (reset) begin
            estado <= VERDE;
            conta  <= 2'd0;
        end else begin
            estado <= proximo;
            conta  <= proxima_conta;
        end

    // Moore: cada luz é uma comparação do estado.
    assign verde    = (estado == VERDE);
    assign amarelo  = (estado == AMARELO);
    assign vermelho = (estado == VERMELHO);
endmodule
