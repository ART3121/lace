# Fios internos

Uma prensa industrial tem dois postos de comando, e cada posto tem dois botões, um para cada mão do operador. Um posto está pronto quando os seus dois botões estão apertados ao mesmo tempo: assim as duas mãos ficam longe da prensa. A prensa desce quando pelo menos um posto está pronto. A saída `parada` acende a lâmpada de prensa parada e é sempre o contrário de `desce`.

Escreva o módulo com dois fios internos, um por posto, que valem 1 quando o posto está pronto.

## Portas

| Porta    | Direção | Bits | Descrição                 |
|----------|---------|------|---------------------------|
| `esq1`   | entrada | 1    | botão esquerdo do posto 1 |
| `dir1`   | entrada | 1    | botão direito do posto 1  |
| `esq2`   | entrada | 1    | botão esquerdo do posto 2 |
| `dir2`   | entrada | 1    | botão direito do posto 2  |
| `desce`  | saída   | 1    | a prensa desce            |
| `parada` | saída   | 1    | a prensa está parada      |

## Em Verilog

Um fio interno é declarado com `wire` dentro do módulo e recebe valor com `assign`, como uma saída:

```verilog
wire ligado;
assign ligado = chave & energia;
```

Depois de declarado, o fio entra em outras expressões pelo nome. Fios internos dão nome a resultados intermediários, deixam as expressões mais curtas e evitam repetir a mesma conta. Uma saída também pode ser lida dentro do módulo, como qualquer outro sinal.

## Dica

Cada fio interno é o E dos dois botões de um posto.

## Dica 2

`desce` é o OU dos dois fios internos, e `parada` é `desce` invertido.
