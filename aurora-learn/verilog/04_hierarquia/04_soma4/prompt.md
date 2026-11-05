# Somador de 4 bits

O arquivo `somador_completo.v` traz um somador completo pronto. Ele soma três bits, `a`, `b` e o vai-um que chega, `cin`, e dá o bit da soma, `s`, e o vai-um que sai, `cout`.

```verilog
module somador_completo (
    input  a,
    input  b,
    input  cin,
    output s,
    output cout
);
```

O módulo `soma4` soma dois números de 4 bits sem sinal, `a` e `b`, com quatro somadores completos em cadeia, como na conta feita à mão: cada coluna soma os seus dois bits e o vai-um da coluna anterior, e passa o próprio vai-um para a coluna seguinte. O resultado tem 5 bits, porque a soma de dois números de 4 bits chega a 30.

## Portas

| Porta | Direção | Bits | Descrição                                    |
|-------|---------|------|----------------------------------------------|
| `a`   | entrada | 4    | primeiro número                              |
| `b`   | entrada | 4    | segundo número                               |
| `s`   | saída   | 5    | `a + b`; o bit 4 é o vai-um da última coluna |

## Exemplo

Com `a = 4'b1011` (11) e `b = 4'b0111` (7), cada coluna faz:

| Coluna | `a` | `b` | `cin` | `s` | `cout` |
|--------|-----|-----|-------|-----|--------|
| 0      | 1   | 1   | 0     | 0   | 1      |
| 1      | 1   | 1   | 1     | 1   | 1      |
| 2      | 0   | 1   | 1     | 0   | 1      |
| 3      | 1   | 0   | 1     | 0   | 1      |

O resultado é `5'b10010` (18): o `cout` da coluna 3 é o bit 4, e o `s` de cada coluna é o bit de mesmo número.

## Dica

Os vai-uns entre as colunas precisam de fios internos: um vetor de 3 bits, ou três fios. A coluna 0 não recebe vai-um, então o `cin` dela é a constante `1'b0`.

## Dica 2

O `cout` da coluna 3 não precisa de fio interno: ligue-o direto em `s[4]`.
