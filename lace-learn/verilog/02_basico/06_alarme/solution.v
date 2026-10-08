module alarme (
    input  armado,
    input  porta_aberta,
    input  janela_aberta,
    input  senha_ok,
    output aviso,
    output sirene
);
    wire aberta;  // a porta ou a janela está aberta

    assign aberta = porta_aberta | janela_aberta;
    assign aviso  = aberta;
    assign sirene = armado & aberta & ~senha_ok;
endmodule
