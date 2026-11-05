module paridade_serial (
    input  clk,
    input  reset,
    input  bit_in,
    output impar
);
    localparam PAR   = 1'b0;  // chegou um número par de 1s
    localparam IMPAR = 1'b1;  // chegou um número ímpar de 1s

    reg estado;
    reg proximo;

    // O próximo estado: cada 1 recebido troca a paridade.
    always @(*) begin
        proximo = estado;
        case (estado)
            PAR:     if (bit_in) proximo = IMPAR;
            IMPAR:   if (bit_in) proximo = PAR;
            default: proximo = PAR;
        endcase
    end

    // O registrador de estado, com reset síncrono.
    always @(posedge clk)
        if (reset)
            estado <= PAR;
        else
            estado <= proximo;

    // Moore: a saída depende só do estado.
    assign impar = (estado == IMPAR);
endmodule
