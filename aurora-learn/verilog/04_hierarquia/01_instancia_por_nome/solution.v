module instancia_por_nome (
    input        x,
    input        y,
    output [1:0] contagem   // quantos de x e y valem 1
);
    meio_somador conta (
        .a      (x),
        .b      (y),
        .soma   (contagem[0]),
        .vai_um (contagem[1])
    );
endmodule
