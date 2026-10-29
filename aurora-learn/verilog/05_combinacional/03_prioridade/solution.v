module prioridade (
    input      [3:0] req,     // um bit por pedido
    output reg [1:0] idx,     // o pedido atendido
    output           valid    // algum pedido ativo
);
    assign valid = |req;

    // O primeiro teste que der certo decide: o pedido 3 vem antes de todos.
    always @(*) begin
        if (req[3])
            idx = 2'd3;
        else if (req[2])
            idx = 2'd2;
        else if (req[1])
            idx = 2'd1;
        else
            idx = 2'd0;
    end
endmodule
