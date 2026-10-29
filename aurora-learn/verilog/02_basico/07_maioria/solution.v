module maioria (
    input  a,
    input  b,
    input  c,
    output y
);
    // 1 quando algum dos três pares está todo em 1.
    assign y = (a & b) | (a & c) | (b & c);
endmodule
