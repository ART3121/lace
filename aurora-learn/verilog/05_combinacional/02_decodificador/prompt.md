# Decodificador 2 para 4

Um decodificador recebe um número e acende uma única linha de saída, a de mesmo índice. Num processador, os bits de endereço passam por um decodificador para escolher qual memória ou qual periférico responde ao acesso. A entrada de habilitação permite desligar todas as saídas de uma vez, por exemplo quando o endereço não pertence a esse grupo de dispositivos.

O módulo `decodificador` tem um seletor de 2 bits e quatro saídas. Com `en` em 0, todas as saídas ficam em 0. Com `en` em 1, só o bit `y[sel]` vai a 1.

## Portas

| Porta | Direção | Bits | Descrição                    |
|-------|---------|------|------------------------------|
| `sel` | entrada | 2    | o índice da saída que acende |
| `en`  | entrada | 1    | 1 habilita, 0 apaga todas    |
| `y`   | saída   | 4    | no máximo uma linha em 1     |

## Tabela verdade

| `en` | `sel`    | `y`       |
|------|----------|-----------|
| 0    | qualquer | `4'b0000` |
| 1    | 0        | `4'b0001` |
| 1    | 1        | `4'b0010` |
| 1    | 2        | `4'b0100` |
| 1    | 3        | `4'b1000` |

## Em Verilog

Dentro de um `always @(*)`, o `if` executa um comando quando a condição vale 1, e o `else`, quando ela vale 0. Para mais de um comando, use `begin` e `end`:

```verilog
always @(*) begin
    if (liga)
        saida = entrada;
    else
        saida = 8'd0;
end
```

Num bloco combinacional, cada saída precisa receber um valor em todos os caminhos. Se faltar o `else` acima, com `liga` em 0 o bloco não diz o que `saida` vale, e a ferramenta de síntese cria um latch, um elemento de memória que guarda o último valor. Quase sempre isso é um erro, e a correção avisa quando acontece. Um jeito simples de evitar é dar um valor de partida à saída na primeira linha do bloco e depois mudar só o que for preciso, porque dentro do bloco vale a última atribuição.

Um vetor também aceita um índice variável: `y[sel]` é o bit de `y` cujo número está em `sel`.

## Dica

Comece o bloco pondo `4'b0000` em `y`. Depois, só quando `en` vale 1, mude um bit.

## Dica 2

Com o valor de partida já dado, basta um `if (en)` sem `else`, que atribui `1'b1` a `y[sel]`.
