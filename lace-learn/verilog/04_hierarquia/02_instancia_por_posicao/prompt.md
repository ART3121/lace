# Instância com ligação por posição

O arquivo `mux2.v` traz um multiplexador de duas entradas pronto. Repare na ordem das portas: o seletor vem primeiro.

```verilog
module mux2 (
    input  sel,  // escolhe a entrada
    input  d0,   // vai para y quando sel vale 0
    input  d1,   // vai para y quando sel vale 1
    output y
);
```

O módulo `instancia_por_posicao` é uma chave de cruzamento, com duas entradas e duas saídas. Com `troca` em 0, os sinais passam direto: `p` recebe `a`, e `q` recebe `b`. Com `troca` em 1, eles se cruzam: `p` recebe `b`, e `q` recebe `a`. Monte a chave com duas instâncias de `mux2`, ligadas por posição.

## Portas

| Porta   | Direção | Bits | Descrição                                  |
|---------|---------|------|--------------------------------------------|
| `a`     | entrada | 1    | primeiro sinal                             |
| `b`     | entrada | 1    | segundo sinal                              |
| `troca` | entrada | 1    | 1 cruza os sinais                          |
| `p`     | saída   | 1    | `a` com `troca` em 0, `b` com `troca` em 1 |
| `q`     | saída   | 1    | `b` com `troca` em 0, `a` com `troca` em 1 |

## Em Verilog

Na ligação por posição, a instância lista só os sinais, sem o nome das portas. O primeiro sinal vai para a primeira porta da declaração do módulo, o segundo para a segunda, e assim por diante:

```verilog
// filtro declara as portas na ordem entrada, ganho, saida.
filtro f1 (sinal_bruto, 4'd3, sinal_limpo);
```

É mais curto, mas um sinal fora do lugar não dá erro quando as larguras batem: o circuito só fica errado. Por isso, em módulos com muitas portas, a ligação por nome é mais segura.

## Dica

Cada saída da chave vem de um `mux2`. Para cada um, pergunte qual sinal deve aparecer na saída com `troca` em 0, e qual com `troca` em 1.

## Dica 2

Nos dois `mux2`, o primeiro sinal da lista é `troca`. No que gera `q`, a entrada `d0` recebe `b`, porque com `troca` em 0 a saída `q` repete `b`.
