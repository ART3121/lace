# Os bits de um vetor

Um LED RGB tem três cores, e o módulo recebe a cor desejada num vetor de 3 bits, `cor`: o bit 2 acende o vermelho, o bit 1 o verde e o bit 0 o azul. O módulo separa os três bits para as três saídas do LED, `vermelho`, `verde` e `azul`, e repassa o vetor inteiro, sem mudança, para um segundo LED, pela saída `espelho`.

## Portas

| Porta      | Direção | Bits | Descrição        |
|------------|---------|------|------------------|
| `cor`      | entrada | 3    | a cor desejada   |
| `espelho`  | saída   | 3    | cópia de `cor`   |
| `vermelho` | saída   | 1    | o bit 2 de `cor` |
| `verde`    | saída   | 1    | o bit 1 de `cor` |
| `azul`     | saída   | 1    | o bit 0 de `cor` |

## Exemplo

Com `cor` igual a `3'b110`, `vermelho` e `verde` valem 1 e `azul` vale 0: o LED fica amarelo. `espelho` vale `3'b110`.

## Em Verilog

A faixa de índices vem antes do nome, na declaração: `input [2:0] cor` tem três bits, `cor[2]`, `cor[1]` e `cor[0]`. O nome sozinho, `cor`, é o vetor inteiro; `cor[1]` é só o bit 1. Um `assign` entre dois vetores de mesma largura liga cada bit ao bit de mesma posição.

## Dica

`espelho` tem a mesma largura de `cor`, então um único `assign` liga os três bits.

## Dica 2

Nas saídas de 1 bit, selecione o bit de `cor` com o índice entre colchetes.
