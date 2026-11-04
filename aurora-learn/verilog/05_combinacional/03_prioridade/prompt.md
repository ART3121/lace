# Codificador de prioridade

Quando vários dispositivos pedem atenção ao mesmo tempo, alguém precisa decidir quem é atendido primeiro. Num processador, cada fonte de interrupção tem uma linha de pedido, e um codificador de prioridade entrega o número do pedido mais importante que está ativo. O mesmo circuito aparece num barramento compartilhado, para escolher qual placa usa o barramento no próximo ciclo.

O módulo `prioridade` recebe quatro pedidos, um por bit de `req`. Quanto maior o índice, maior a prioridade. A saída `idx` é o índice do pedido ativo de maior índice, e `valid` vale 1 quando há pelo menos um pedido ativo. Sem nenhum pedido, `idx` e `valid` ficam em 0.

## Portas

| Porta   | Direção | Bits | Descrição                              |
|---------|---------|------|----------------------------------------|
| `req`   | entrada | 4    | `req[n]` em 1 quer dizer pedido `n`    |
| `idx`   | saída   | 2    | o índice do pedido de maior prioridade |
| `valid` | saída   | 1    | 1 quando algum pedido está ativo       |

## Exemplos

| `req`     | `idx` | `valid` |
|-----------|-------|---------|
| `4'b0000` | 0     | 0       |
| `4'b0001` | 0     | 1       |
| `4'b0110` | 2     | 1       |
| `4'b1011` | 3     | 1       |
| `4'b0011` | 1     | 1       |

Repare que `idx` vale 0 tanto sem pedido quanto com o pedido 0 sozinho. É o `valid` que distingue os dois casos.

## Em Verilog

Vários `if` podem formar uma cadeia com `else if`. Os testes são feitos em ordem, e só o comando do primeiro que der certo é executado:

```verilog
always @(*) begin
    if (temperatura > 8'd90)
        alarme = 2'd2;
    else if (temperatura > 8'd70)
        alarme = 2'd1;
    else
        alarme = 2'd0;
end
```

Essa ordem é justamente uma prioridade: quem é testado antes vence. O `else` do fim garante que a saída recebe valor em todos os caminhos.

## Dica

O `valid` não depende de prioridade e cabe num `assign` com um operador de redução. Para `idx`, teste os bits de `req` do mais importante para o menos importante.

## Dica 2

A cadeia começa com `if (req[3])` e dá `2'd3` a `idx`. Os testes seguintes são `req[2]` e `req[1]`, e o `else` final dá `2'd0`.
