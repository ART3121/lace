module varios_fios (
    input  ligada,
    input  aquecendo,
    input  falha,
    output led_verde,
    output led_amarelo,
    output led_vermelho,
    output buzina
);
    assign led_verde    = ligada;
    assign led_amarelo  = aquecendo;
    assign led_vermelho = falha;
    assign buzina       = falha;
endmodule
