module conta_uns (
    input  [7:0] x,
    output [3:0] n
);
    // A conta é feita com os 4 bits de n: cada bit entra estendido com zeros.
    assign n = x[0] + x[1] + x[2] + x[3] + x[4] + x[5] + x[6] + x[7];
endmodule
