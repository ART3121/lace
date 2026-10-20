# Média de quatro amostras

O sinal que sai de um detector chega com ruído: cada amostra fica um pouco acima ou um pouco abaixo do valor certo, ao acaso. A média das últimas amostras reduz esse ruído, porque os erros para cima e para baixo se compensam em parte. É um dos filtros mais simples da leitura de um calorímetro, e com quatro amostras a divisão sai de graça: dividir por 4 é deslocar dois bits para a direita.

O módulo `media4` guarda as quatro últimas amostras de `x`. A cada borda de subida de `clk`, a amostra nova entra e a mais velha sai; com `reset` em 1 na borda, as quatro vão para 0. A saída `media` é a soma das quatro amostras guardadas, dividida por 4 e arredondada para baixo. Ela depende só das amostras guardadas, então muda logo depois de cada borda.

## Portas

| Porta   | Direção | Bits | Descrição                                     |
|---------|---------|------|-----------------------------------------------|
| `clk`   | entrada | 1    | o clock                                       |
| `reset` | entrada | 1    | síncrono, ativo em 1: zera as quatro amostras |
| `x`     | entrada | 8    | a amostra nova, sem sinal                     |
| `media` | saída   | 8    | a soma das amostras guardadas, dividida por 4 |

## Exemplo

Cada linha mostra os valores logo depois de uma borda de subida do clock. As amostras guardadas estão da mais nova para a mais velha.

| Borda | `reset` | `x` | amostras guardadas | soma | `media` |
|-------|---------|-----|--------------------|------|---------|
| 1     | 1       | 9   | 0, 0, 0, 0         | 0    | 0       |
| 2     | 0       | 100 | 100, 0, 0, 0       | 100  | 25      |
| 3     | 0       | 102 | 102, 100, 0, 0     | 202  | 50      |
| 4     | 0       | 98  | 98, 102, 100, 0    | 300  | 75      |
| 5     | 0       | 101 | 101, 98, 102, 100  | 401  | 100     |
| 6     | 0       | 255 | 255, 101, 98, 102  | 556  | 139     |

A partir da borda 5, a soma passa de 255. Com as quatro amostras em 255, ela chega a 1020, e `media` vale 255.

## Dica

Guarde as amostras em quatro registradores de 8 bits, um passando o valor para o outro a cada borda, como na cadeia de flip-flops. A soma precisa de 10 bits: `(h0 + h1 + h2 + h3) >> 2` ligado direto numa saída de 8 bits faz a soma com 8 bits e perde os vai-uns antes de dividir.

## Dica 2

Faça a soma num fio de 10 bits: `wire [9:0] soma = h0 + h1 + h2 + h3;`. Com o destino de 10 bits, a conta é feita com 10 bits. Dividir por 4 é ficar com os bits 9 a 2 desse fio.
