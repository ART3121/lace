# Instância com ligação por nome

O arquivo `meio_somador.v`, ao lado do seu, traz um módulo pronto, o meio somador. Ele soma dois bits, `a` e `b`, e dá o resultado em duas saídas: `soma`, o bit menos significativo, e `vai_um`, o mais significativo.

```verilog
module meio_somador (
    input  a,
    input  b,
    output soma,
    output vai_um
);
```

Somar dois bits é o mesmo que contar quantos deles valem 1. O módulo `instancia_por_nome` conta os bits em 1 entre `x` e `y` com uma instância de `meio_somador`, e dá o total, de 0 a 2, na saída `contagem`, de 2 bits.

## Portas

| Porta      | Direção | Bits | Descrição                    |
|------------|---------|------|------------------------------|
| `x`        | entrada | 1    | primeiro bit                 |
| `y`        | entrada | 1    | segundo bit                  |
| `contagem` | saída   | 2    | quantos de `x` e `y` valem 1 |

## Tabela verdade

| `x` | `y` | `contagem` |
|-----|-----|------------|
| 0   | 0   | `2'd0`     |
| 0   | 1   | `2'd1`     |
| 1   | 0   | `2'd1`     |
| 1   | 1   | `2'd2`     |

## Em Verilog

Uma instância põe uma cópia de um módulo dentro de outro. Ela traz o nome do módulo, um nome para a cópia e a lista de ligações. Na ligação por nome, cada porta do módulo instanciado aparece com um ponto na frente, e entre parênteses vai o sinal ligado a ela:

```verilog
// Uma instância do módulo filtro, chamada f0.
filtro f0 (
    .entrada (sinal_bruto),
    .ganho   (4'd3),
    .saida   (sinal_limpo)
);
```

Como cada ligação diz o nome da porta, a ordem delas na lista não importa. O sinal ligado pode ser uma constante, um bit ou uma faixa de um vetor, como `v[0]` ou `v[3:0]`.

## Dica

As entradas do meio somador recebem `x` e `y`. Cada saída dele vai para um bit de `contagem`.

## Dica 2

O vai-um é o bit mais significativo da contagem: `.vai_um` se liga a `contagem[1]`.
