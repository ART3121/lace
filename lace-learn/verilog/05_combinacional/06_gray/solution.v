module gray (
    input  [3:0] bin,    // número em binário
    output [3:0] codigo  // o mesmo número em código Gray
);
    // Cada bit de codigo é o OU exclusivo de um bit com o vizinho da esquerda.
    // O bit 3 não tem vizinho e passa igual: bin >> 1 põe um 0 ali.
    assign codigo = bin ^ (bin >> 1);
endmodule
