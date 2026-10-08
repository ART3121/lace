module soma12 (
    input  [11:0] a,
    input  [11:0] b,
    output [12:0] s
);
    somador #(.N(12)) conta (
        .a (a),
        .b (b),
        .s (s)
    );
endmodule
