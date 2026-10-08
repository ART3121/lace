module cadeia_ff (
    input  clk,
    input  d,
    output q
);
    wire q1;  // saída do primeiro flip-flop
    wire q2;  // saída do segundo flip-flop

    ff estagio1 (.clk(clk), .d(d),  .q(q1));
    ff estagio2 (.clk(clk), .d(q1), .q(q2));
    ff estagio3 (.clk(clk), .d(q2), .q(q));
endmodule
