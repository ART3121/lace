module bit_a_bit (
    input  [3:0] a,
    input  [3:0] b,
    output [3:0] e,
    output [3:0] ou,
    output [3:0] ou_exclusivo,
    output [3:0] nao_a,
    output       ambos
);
    // Bit a bit: quatro operações, uma por posição.
    assign e            = a & b;
    assign ou           = a | b;
    assign ou_exclusivo = a ^ b;
    assign nao_a        = ~a;

    // Lógico: cada vetor vale verdadeiro quando é diferente de zero.
    // Dá o mesmo que a && b, com a comparação escrita por extenso.
    assign ambos = (a != 4'd0) && (b != 4'd0);
endmodule
