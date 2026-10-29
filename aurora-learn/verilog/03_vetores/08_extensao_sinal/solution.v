module extensao_sinal (
    input  [7:0]  numero,     // inteiro com sinal, em complemento de dois
    output [31:0] estendido
);
    // Os 24 bits novos repetem o bit de sinal.
    assign estendido = {{24{numero[7]}}, numero};
endmodule
