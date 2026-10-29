# Somador de 8 bits

O arquivo `somador4.v` traz um somador de 4 bits pronto, com vai-um de entrada e de saída:

```verilog
module somador4 (
    input  [3:0] a,
    input  [3:0] b,
    input        cin,   // vai-um que chega
    output [3:0] s,
    output       cout   // vai-um que sai
);
```

O módulo `soma8` soma dois números de 8 bits sem sinal com dois `somador4`. Um soma os 4 bits de baixo de `a` e `b`, o outro soma os 4 bits de cima, e o vai-um passa do primeiro para o segundo. O resultado tem 9 bits.

## Portas

| Porta | Direção | Bits | Descrição                         |
|-------|---------|------|-----------------------------------|
| `a`   | entrada | 8    | primeiro número                   |
| `b`   | entrada | 8    | segundo número                    |
| `s`   | saída   | 9    | `a + b`; o bit 8 é o vai-um final |

## Exemplo

Com `a = 8'hC8` (200) e `b = 8'h64` (100), a metade de baixo soma `4'h8` e `4'h4`: dá `4'hC`, sem vai-um. A de cima soma `4'hC`, `4'h6` e esse vai-um 0: dá `4'h2` e gera vai-um, que vira o bit 8. O resultado é `9'h12C` (300).

## Dica

Ligue as faixas `a[3:0]` e `b[3:0]` a uma instância, e `a[7:4]` e `b[7:4]` à outra. Um fio interno leva o vai-um de uma para a outra.

## Dica 2

A instância de baixo recebe `1'b0` no `cin`, e o `cout` da de cima vai direto para `s[8]`.
