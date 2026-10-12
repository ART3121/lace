`timescale 1ns / 1ps

// Fora do .spf e com nome de testbench: não pode ser topo.
module velho_tb;

    initial begin
        #10;
        $finish;
    end

endmodule
