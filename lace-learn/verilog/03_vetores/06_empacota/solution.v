module empacota (
    input  [2:0]  canal,   // o canal do dado, de 0 a 7
    input  [7:0]  dado,    // o byte
    input         valido,  // o byte deve ser usado
    output [11:0] quadro,
    output [15:0] pacote
);
    assign quadro = {valido, canal, dado};

    // O quadro entre as marcas de começo e de fim.
    assign pacote = {2'b10, quadro, 2'b01};
endmodule
