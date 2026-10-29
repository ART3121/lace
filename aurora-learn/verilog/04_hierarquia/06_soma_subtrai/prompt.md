# Somador e subtrator

O arquivo `somador8.v` traz um somador de 8 bits pronto, com vai-um de entrada e de saída:

```verilog
module somador8 (
    input  [7:0] a,
    input  [7:0] b,
    input        cin,   // vai-um que chega
    output [7:0] s,
    output       cout   // vai-um que sai
);
```

O módulo `soma_subtrai` usa uma única instância de `somador8` para somar ou subtrair: com `sub` em 0, `resultado` é `a + b`; com `sub` em 1, é `a - b`. O resultado tem 8 bits, e o que passar disso se perde, como num registrador de 8 bits.

## Portas

| Porta       | Direção | Bits | Descrição                     |
|-------------|---------|------|-------------------------------|
| `a`         | entrada | 8    | primeiro operando             |
| `b`         | entrada | 8    | segundo operando              |
| `sub`       | entrada | 1    | 0 soma, 1 subtrai             |
| `resultado` | saída   | 8    | `a + b` ou `a - b`, em 8 bits |

## Exemplos

| `a`      | `b`      | `sub` | `resultado`                               |
|----------|----------|-------|-------------------------------------------|
| `8'd10`  | `8'd3`   | 0     | `8'd13`                                   |
| `8'd10`  | `8'd3`   | 1     | `8'd7`                                    |
| `8'd3`   | `8'd10`  | 1     | `8'd249`, que é -7 em complemento de dois |
| `8'd200` | `8'd100` | 0     | `8'd44`, porque 300 não cabe em 8 bits    |

## Subtrair com um somador

Em complemento de dois, `-b` é `~b + 1`: inverte-se cada bit de `b` e soma-se 1. Então `a - b` é `a + ~b + 1`, e o mesmo somador faz as duas contas. Na subtração, a entrada `b` do somador recebe `b` invertido, e o 1 a mais entra pelo `cin`.

Para inverter ou não um bit conforme um sinal de controle, use o OU exclusivo: `x ^ 1` é `~x`, e `x ^ 0` é o próprio `x`.

## Dica

Para fazer o OU exclusivo dos 8 bits de `b` com `sub`, repita `sub` 8 vezes com a replicação, como na extensão de sinal. Um fio interno de 8 bits pode guardar esse `b` já tratado.

## Dica 2

O `cin` do somador recebe o próprio `sub`. O `cout` não é usado: uma saída pode ficar desligada na instância, com `.cout()`.
