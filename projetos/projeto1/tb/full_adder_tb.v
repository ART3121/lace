`timescale 1ns / 1ps

module full_adder_tb;

    reg  a;
    reg  b;
    reg  cin;
    wire cout;
    wire sum;

    full_adder dut (
        .a   (a),
        .b   (b),
        .cin (cin),
        .cout(cout),
        .sum (sum)
    );

    initial begin
        $dumpfile("full_adder_tb.fst");
        $dumpvars(0, full_adder_tb);

        a = 0;
        b = 0;
        cin = 0;
        #10;

        a = 0;
        b = 0;
        cin = 1;
        #10;

        a = 0;
        b = 1;
        cin = 0;
        #10;

        a = 0;
        b = 1;
        cin = 1;
        #10;
        $finish;
    end

endmodule
