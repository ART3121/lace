module reducao (
    input  [7:0] dado,
    output       paridade,
    output       todos,
    output       algum
);
    assign paridade = ^dado;
    assign todos    = &dado;
    assign algum    = |dado;
endmodule
