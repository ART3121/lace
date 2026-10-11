`timescale 1ns / 1ps

module somador4_tb;

    reg  [3:0] a;
    reg  [3:0] b;
    wire [3:0] sum;
    wire       cout;

    somador42 dut (
        .a   (a),
        .b   (b),
        .sum (sum),
        .cout(cout)
    );

    initial begin
        $dumpfile("somador4_tb.fst");
        $dumpvars(0, somador4_tb);

        a = 4'd5;
        b = 4'd7;

        #10;

        a = 4'd10;
        b = 4'd4;

        #10;

        a = 4'hA;
        b = 4'h3;
        
        #10;


        $finish;
    end

endmodule
