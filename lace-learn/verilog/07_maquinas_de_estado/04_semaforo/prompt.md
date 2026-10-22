# Semáforo

O semáforo de um cruzamento não tem entrada de dados: ele só conta o tempo. Cada cor dura um número fixo de ciclos do clock, e a máquina passa de uma cor para a seguinte quando a conta chega ao fim. Num semáforo de verdade, o clock que move a máquina é lento, de um ciclo por segundo ou menos.

O módulo `semaforo` acende uma cor de cada vez. Depois de uma borda de subida com o reset em 1, a cor é verde, e a sequência a partir daí é: verde por 4 ciclos, amarelo por 1, vermelho por 3, e de novo verde por 4, para sempre, até o próximo reset. A cada instante, exatamente uma das três saídas vale 1.

## Portas

| Porta      | Direção | Bits | Descrição                  |
|------------|---------|------|----------------------------|
| `clk`      | entrada | 1    | o clock                    |
| `reset`    | entrada | 1    | reset síncrono, ativo em 1 |
| `verde`    | saída   | 1    | 1 com a luz verde acesa    |
| `amarelo`  | saída   | 1    | 1 com a luz amarela acesa  |
| `vermelho` | saída   | 1    | 1 com a luz vermelha acesa |

## Estados

| Estado     | Duração  | Saída em 1 |
|------------|----------|------------|
| `VERDE`    | 4 ciclos | `verde`    |
| `AMARELO`  | 1 ciclo  | `amarelo`  |
| `VERMELHO` | 3 ciclos | `vermelho` |

O reset leva a máquina para o primeiro ciclo de `VERDE`, qualquer que seja a cor e a conta naquele momento. Um ciclo vai de uma borda de subida à seguinte.

## Exemplo

Cada linha mostra o reset na borda de subida e a cor logo depois dela. A última coluna diz em qual ciclo da cor o semáforo está.

| Borda | `reset` | `verde` | `amarelo` | `vermelho` | ciclo da cor |
|-------|---------|---------|-----------|------------|--------------|
| 1     | 1       | 1       | 0         | 0          | 1º           |
| 2     | 1       | 1       | 0         | 0          | 1º           |
| 3     | 0       | 1       | 0         | 0          | 2º           |
| 4     | 0       | 1       | 0         | 0          | 3º           |
| 5     | 0       | 1       | 0         | 0          | 4º           |
| 6     | 0       | 0       | 1         | 0          | 1º           |
| 7     | 0       | 0       | 0         | 1          | 1º           |
| 8     | 0       | 0       | 0         | 1          | 2º           |
| 9     | 0       | 0       | 0         | 1          | 3º           |
| 10    | 0       | 1       | 0         | 0          | 1º           |
| 11    | 0       | 1       | 0         | 0          | 2º           |
| 12    | 1       | 1       | 0         | 0          | 1º           |
| 13    | 0       | 1       | 0         | 0          | 2º           |

O reset da borda 12 recomeça a conta no meio do verde.

## Dica

Use dois registradores: o estado, com a cor, e um contador pequeno com os ciclos já passados nessa cor. Os dois são atualizados na mesma borda e zerados pelo mesmo reset. Na troca de cor, o contador volta a 0. As saídas são comparações do estado, como nas máquinas de Moore anteriores.

## Dica 2

Um contador de 2 bits basta, porque nenhuma cor passa de 4 ciclos. Com o contador em 0 no primeiro ciclo de cada cor, o verde termina quando ele vale 3, e o vermelho quando ele vale 2. No `always @(*)`, comece com `proximo = estado;` e `proxima_conta = conta + 2'd1;`, e no `case (estado)` troque de cor e zere a conta quando o tempo da cor acabar. O amarelo troca sempre.
