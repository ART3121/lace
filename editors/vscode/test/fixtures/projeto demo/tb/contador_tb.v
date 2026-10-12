`timescale 1ns / 1ps

module contador_tb;

    reg  [7:0] a;
    wire [7:0] y;

    contador dut (
        .a(a),
        .y(y)
    );

    initial begin
        $dumpfile("contador_tb.fst");
        $dumpvars(0, contador_tb);

        a = 0;

        #10 $display("a = %0d, y = %0d", a, y);
        #90;
        $finish;
    end

endmodule
