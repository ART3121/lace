`timescale 1ns / 1ps

// Na pasta do projeto, mas fora do .spf: aparece em Não registrados.
module sobra (
    input  wire a,
    output wire y
);

    assign y = ~a;

endmodule
