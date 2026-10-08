# Parâmetros

O arquivo `somador.v` traz um somador de largura configurável. A largura é o parâmetro `N`, que vale 4 quando a instância não pede outro valor:

```verilog
module somador #(
    parameter N = 4
) (
    input  [N-1:0] a,
    input  [N-1:0] b,
    output [N:0]   s    // o bit N é o vai-um
);
```

O módulo `soma12` soma dois números de 12 bits sem sinal com uma instância de `somador` em que `N` vale 12.

## Portas

| Porta | Direção | Bits | Descrição                    |
|-------|---------|------|------------------------------|
| `a`   | entrada | 12   | primeiro número              |
| `b`   | entrada | 12   | segundo número               |
| `s`   | saída   | 13   | `a + b`; o bit 12 é o vai-um |

## Exemplo

Com `a = 12'hFFF` e `b = 12'h001`, `s` vale `13'h1000`.

## Em Verilog

Um parâmetro é uma constante do módulo que cada instância pode trocar. O valor novo vai entre `#(` e `)`, depois do nome do módulo e antes do nome da instância, e se escreve como uma ligação por nome:

```verilog
fila #(.PROFUNDIDADE(32)) fila_rx (
    .entrada (byte_recebido),
    .saida   (byte_lido)
);
```

Sem o `#( )`, a instância usa o valor padrão da declaração. Aqui isso daria um somador de 4 bits ligado a sinais de 12, e o compilador só avisaria da diferença de largura, sem erro: o circuito compila e soma errado.

## Dica

As portas de `somador` têm os mesmos nomes das portas do seu módulo: `a`, `b` e `s`.

## Dica 2

O `#(.N(12))` fica entre o nome do módulo, `somador`, e o nome que você der à instância.
