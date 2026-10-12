`timescale 1ns / 1ps

module somador_tb;

    reg  [7:0] a;
    wire [7:0] y;

    somador dut (
        .a(a),
        .y(y)
    );

    initial begin
        $dumpfile("somador_tb.fst");
        $dumpvars(0, somador_tb);

        a = 0;

        #100;
        $finish;
    end

endmodule
