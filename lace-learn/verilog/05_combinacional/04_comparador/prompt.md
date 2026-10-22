# Comparador

Comparar dois números é uma das operações mais comuns em hardware. Na eletrônica de leitura de um detector, cada amostra é comparada com um limiar, e só as que passam dele seguem para o sistema de trigger. Num processador, as instruções de desvio condicional comparam dois registradores para decidir se o programa salta.

O módulo `comparador` recebe dois números de 4 bits sem sinal, de 0 a 15, e diz qual é a relação entre eles. Em cada combinação de entradas, exatamente uma das três saídas vale 1.

## Portas

| Porta   | Direção | Bits | Descrição                    |
|---------|---------|------|------------------------------|
| `a`     | entrada | 4    | primeiro número, sem sinal   |
| `b`     | entrada | 4    | segundo número, sem sinal    |
| `menor` | saída   | 1    | 1 quando `a` é menor que `b` |
| `igual` | saída   | 1    | 1 quando `a` é igual a `b`   |
| `maior` | saída   | 1    | 1 quando `a` é maior que `b` |

## Exemplos

| `a`  | `b`  | `menor` | `igual` | `maior` |
|------|------|---------|---------|---------|
| 3    | 9    | 1       | 0       | 0       |
| 7    | 7    | 0       | 1       | 0       |
| 15   | 0    | 0       | 0       | 1       |
| 8    | 7    | 0       | 0       | 1       |

## Em Verilog

Os operadores de comparação são `<`, `<=`, `>`, `>=`, `==` (igual) e `!=` (diferente). Cada um compara dois vetores como números sem sinal e dá um único bit: 1 quando a relação vale, 0 quando não vale. O resultado pode ir direto para uma saída de 1 bit:

```verilog
assign acima = (amostra > limiar);
```

O igual tem dois sinais, `==`. Um `=` sozinho é atribuição.

## Dica

Cada saída é um `assign` com um operador de comparação entre `a` e `b`.

## Dica 2

Use `<` para `menor`, `==` para `igual` e `>` para `maior`. Como os três casos se excluem, a terceira saída também poderia sair das outras duas, com `~(menor | igual)`.
