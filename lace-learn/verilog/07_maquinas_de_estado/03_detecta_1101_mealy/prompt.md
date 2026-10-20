# Detector de 1, 1, 0, 1 com saída de Mealy

O receptor de um enlace serial acha o começo de cada quadro procurando no fluxo de bits uma palavra de sincronismo fixa. Com uma saída de Mealy, o aviso aparece enquanto o último bit da palavra ainda está na entrada, um ciclo antes do que daria uma máquina de Moore, e o circuito seguinte já pode agir na mesma borda que amostra esse bit.

O módulo `detecta_1101_mealy` procura os bits 1, 1, 0, 1. A cada borda de subida sem reset, ele amostra `x`. A saída `achou` é combinacional e vale 1, em qualquer instante, quando os bits amostrados nas três últimas bordas foram 1, 1 e 0, nessa ordem e sem reset em nenhuma delas, e `x` vale 1 naquele momento. Ela muda assim que `x` muda, sem esperar a borda. O reset age só na borda: ele não entra direto na saída. As ocorrências podem se sobrepor: o último 1 de um padrão serve de primeiro 1 do seguinte, e então 1, 1, 0, 1, 1, 0, 1 dá dois acertos.

## Portas

| Porta   | Direção | Bits | Descrição                                     |
|---------|---------|------|-----------------------------------------------|
| `clk`   | entrada | 1    | o clock                                       |
| `reset` | entrada | 1    | reset síncrono, ativo em 1                    |
| `x`     | entrada | 1    | o bit recebido, amostrado na borda de subida  |
| `achou` | saída   | 1    | 1 enquanto o estado é `VIU_110` e `x` vale 1  |

## Estados

O último bit do padrão não precisa ser guardado, porque a saída olha para ele direto na entrada. Bastam quatro estados.

| Estado    | Os últimos bits amostrados terminam em | `achou`     |
|-----------|----------------------------------------|-------------|
| `NADA`    | nenhum começo do padrão                | 0           |
| `VIU_1`   | 1                                      | 0           |
| `VIU_11`  | 1, 1                                   | 0           |
| `VIU_110` | 1, 1, 0                                | igual a `x` |

O reset leva a máquina para `NADA`, e o bit amostrado numa borda com reset não conta.

## Exemplo

O reset esteve ativo nas bordas 1 e 2. Cada linha mostra os valores pouco antes de uma borda de subida: o bit em `x` é o que essa borda vai amostrar, e o estado é o que as bordas anteriores deixaram.

| Borda | `reset` | `x` | estado    | `achou` |
|-------|---------|-----|-----------|---------|
| 3     | 0       | 1   | `NADA`    | 0       |
| 4     | 0       | 1   | `VIU_1`   | 0       |
| 5     | 0       | 0   | `VIU_11`  | 0       |
| 6     | 0       | 1   | `VIU_110` | 1       |
| 7     | 0       | 1   | `VIU_1`   | 0       |
| 8     | 0       | 0   | `VIU_11`  | 0       |
| 9     | 0       | 1   | `VIU_110` | 1       |
| 10    | 0       | 1   | `VIU_1`   | 0       |
| 11    | 0       | 0   | `VIU_11`  | 0       |
| 12    | 0       | 0   | `VIU_110` | 0       |
| 13    | 0       | 1   | `NADA`    | 0       |

O 1 amostrado na borda 6 fecha um padrão e abre o outro, que se completa na borda 9. Logo depois da borda 6, o estado passa a `VIU_1` e `achou` volta a 0, mesmo com o 1 ainda em `x` até a descida do clock.

## Dica

O registrador de estado e a lógica do próximo estado são como os do detector de 1, 0, 1, com outros estados. A diferença está na saída: ela é um `assign` que usa o estado e a entrada `x`, fora de qualquer `always @(posedge clk)`. Uma saída registrada chegaria um ciclo atrasada.

## Dica 2

O caminho do acerto é `NADA` com 1 para `VIU_1`, `VIU_1` com 1 para `VIU_11` e `VIU_11` com 0 para `VIU_110`. De `VIU_11`, um 1 mantém o estado, porque os últimos bits continuam terminando em 1, 1. De `VIU_110`, um 1 completa o padrão e deixa os últimos bits terminando em 1, 1, 0, 1, cujo maior final que começa o padrão é 1. A saída é `(estado == VIU_110) & x`.
