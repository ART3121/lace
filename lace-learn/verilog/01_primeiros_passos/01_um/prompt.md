# Uma saída constante

O módulo `um` não tem entradas e tem uma saída, `saida`, que deve valer 1 o tempo todo.

O cabeçalho do módulo já está no arquivo: o nome, as portas entre parênteses e o `endmodule` no fim. O seu código vai entre o `);` do cabeçalho e o `endmodule`.

## Portas

| Porta   | Direção | Bits | Descrição |
|---------|---------|------|-----------|
| `saida` | saída   | 1    | sempre 1  |

## Em Verilog

Uma saída recebe valor com `assign`, a atribuição contínua:

```verilog
assign sinal = expressao;
```

O sinal da esquerda passa a valer a expressão da direita durante toda a simulação. Não é uma cópia feita uma vez, como a atribuição de uma linguagem de programação: é uma ligação permanente.

Uma constante pode dizer quantos bits tem e em que base está escrita. `1'b0` é um número de 1 bit, em binário, que vale 0; `4'b0110` tem 4 bits; `8'hA5` tem 8 bits, escritos em hexadecimal.

## Dica

A linha que falta começa com `assign`, tem o nome da saída à esquerda do `=` e termina com `;`.

## Dica 2

À direita do `=` vai a constante de 1 bit que vale 1, escrita como os exemplos de constante do enunciado.
