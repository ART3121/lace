module somador4 (
    input  [3:0] a,
    input  [3:0] b,
    output [3:0] sum,
    output       cout
);
wire c0,c1,c2;

half_adder u_ha0(
     a[0],
     b[0],
     sum[0],
     c0
);
full_adder u_fa0(
     a[1],
     b[1],
     c0,
     c1,
     sum[1]
);
full_adder u_fa1(
     a[2],
     b[2],
     c1,
     c2,
     sum[2]
);
full_adder u_fa2(
     a[3],
     b[3],
     c2,
     cout,
     sum[3]
);


endmodule
