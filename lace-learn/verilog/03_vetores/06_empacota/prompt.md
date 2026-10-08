# Montando um quadro

Um link serial manda dados em quadros. Cada quadro leva um byte, `dado`, o número do `canal` a que ele pertence, de 0 a 7, e o bit `valido`, que diz se o byte deve ser usado. O módulo `empacota` monta duas versões:

- `quadro`, de 12 bits, com `valido` no bit 11, `canal` nos bits 10 a 8 e `dado` nos bits 7 a 0;
- `pacote`, de 16 bits, com o `quadro` entre duas marcas fixas, `2'b10` antes e `2'b01` depois, que ajudam quem recebe a achar o começo e o fim.

## Portas

| Porta    | Direção | Bits | Descrição                   |
|----------|---------|------|-----------------------------|
| `canal`  | entrada | 3    | o canal do dado             |
| `dado`   | entrada | 8    | o byte                      |
| `valido` | entrada | 1    | o byte deve ser usado       |
| `quadro` | saída   | 12   | `valido`, `canal` e `dado`  |
| `pacote` | saída   | 16   | `2'b10`, o quadro e `2'b01` |

## Formato do pacote

| Bits     | 15:14   | 13       | 12:10   | 9:2    | 1:0     |
|----------|---------|----------|---------|--------|---------|
| Conteúdo | `2'b10` | `valido` | `canal` | `dado` | `2'b01` |

## Exemplo

Com `valido = 1`, `canal = 3'd5` e `dado = 8'hC3`, `quadro` vale `12'hDC3` e `pacote` vale `16'hB70D`.

## Em Verilog

A concatenação, entre chaves, junta sinais num vetor maior: `{x, y}` põe `x` nos bits mais significativos e `y` nos menos significativos. A largura do resultado é a soma das larguras dos itens, e os itens podem ser bits, faixas ou vetores inteiros. Constantes também entram, desde que tenham largura: `{4'b1111, x}` põe quatro bits em 1 na frente de `x`. Uma constante sem largura, como `2`, numa concatenação é erro de compilação.

## Dica

Na concatenação, o primeiro item ocupa os bits mais significativos. Em `quadro`, o primeiro item é `valido`.

## Dica 2

`pacote` pode usar o `quadro` já pronto como item, entre as duas constantes de 2 bits.
