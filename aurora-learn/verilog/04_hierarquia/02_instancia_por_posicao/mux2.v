// Multiplexador de duas entradas: y é d0 quando sel vale 0, e d1 quando
// sel vale 1.
module mux2 (
    input  sel,
    input  d0,
    input  d1,
    output y
);
    assign y = sel ? d1 : d0;
endmodule
