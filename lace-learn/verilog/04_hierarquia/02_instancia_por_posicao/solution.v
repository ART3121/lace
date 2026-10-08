module instancia_por_posicao (
    input  a,
    input  b,
    input  troca,   // 1 cruza os sinais
    output p,
    output q
);
    // As portas de mux2, na ordem: sel, d0, d1, y.
    mux2 mux_p (troca, a, b, p);
    mux2 mux_q (troca, b, a, q);
endmodule
