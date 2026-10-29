# Cadeia de flip-flops

O arquivo `ff.v` traz um flip-flop D pronto. A cada borda de subida do clock, a saída `q` passa a valer o que está na entrada `d`, e guarda esse valor até a borda seguinte.

```verilog
module ff (
    input      clk,
    input      d,
    output reg q
);
```

O módulo `cadeia_ff` liga três desses flip-flops em série, todos com o mesmo clock: a entrada `d` alimenta o primeiro, cada um alimenta o seguinte, e a saída `q` é a do terceiro. É um registrador de deslocamento: cada valor de `d` atravessa a cadeia, um flip-flop por borda de clock.

## Portas

| Porta | Direção | Bits | Descrição                           |
|-------|---------|------|-------------------------------------|
| `clk` | entrada | 1    | o clock                             |
| `d`   | entrada | 1    | o bit que entra na cadeia           |
| `q`   | saída   | 1    | o bit que sai do terceiro flip-flop |

## Exemplo

Cada linha mostra os valores logo depois de uma borda de subida do clock.

| Borda | `d` na borda | 1º flip-flop | 2º flip-flop | `q` |
|-------|--------------|--------------|--------------|-----|
| 1     | 1            | 1            | X            | X   |
| 2     | 0            | 0            | 1            | X   |
| 3     | 0            | 0            | 0            | 1   |
| 4     | 1            | 1            | 0            | 0   |
| 5     | 1            | 1            | 1            | 0   |

O 1 que entrou na borda 1 chega a `q` na borda 3. Os flip-flops não têm reset: cada um fica em X, valor desconhecido, até receber o primeiro valor. Enquanto a saída da referência está em X, a correção não confere a sua.

## Dica

Declare dois fios internos para levar a saída de um flip-flop à entrada do seguinte. Os três recebem o mesmo `clk`.

## Dica 2

Cada instância precisa de um nome próprio. A primeira recebe `d` na entrada, e a terceira entrega `q` na saída.
