module comparador (
    input  [3:0] a,
    input  [3:0] b,
    output       menor,   // a < b
    output       igual,   // a == b
    output       maior    // a > b
);
    assign menor = (a < b);
    assign igual = (a == b);
    assign maior = (a > b);
endmodule
