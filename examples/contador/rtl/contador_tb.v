`timescale 1ns/1ps
// Testbench sem $dumpfile: o Lace injeta o dump padrão.
module contador_tb;
    reg clk = 0, rst = 1, en = 0;
    wire [3:0] q;

    contador dut (.clk(clk), .rst(rst), .en(en), .q(q));

    always #5 clk = ~clk;

    initial begin
        #12 rst = 0; en = 1;
        #100;
        $display("q = %0d", q);
        $finish;
    end
endmodule
