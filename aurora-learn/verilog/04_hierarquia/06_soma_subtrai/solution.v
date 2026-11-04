module soma_subtrai (
    input  [7:0] a,
    input  [7:0] b,
    input        sub,        // 0 soma, 1 subtrai
    output [7:0] resultado
);
    // Na subtração, b entra invertido e o cin soma o 1 que falta:
    // a - b = a + ~b + 1.
    wire [7:0] operando_b;

    assign operando_b = b ^ {8{sub}};

    somador8 conta (
        .a    (a),
        .b    (operando_b),
        .cin  (sub),
        .s    (resultado),
        .cout ()
    );
endmodule
