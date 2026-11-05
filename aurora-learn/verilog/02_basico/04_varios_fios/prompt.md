# Vários fios

O módulo `varios_fios` liga o painel de uma máquina. Ele recebe três sinais de estado e controla três lâmpadas e uma buzina:

- `led_verde` acende com a máquina ligada (`ligada`);
- `led_amarelo` acende enquanto a máquina aquece (`aquecendo`);
- `led_vermelho` acende quando há uma falha (`falha`);
- `buzina` toca quando há uma falha, junto com o `led_vermelho`.

## Portas

| Porta          | Direção | Bits | Descrição                  |
|----------------|---------|------|----------------------------|
| `ligada`       | entrada | 1    | a máquina está ligada      |
| `aquecendo`    | entrada | 1    | a máquina está aquecendo   |
| `falha`        | entrada | 1    | há uma falha               |
| `led_verde`    | saída   | 1    | lâmpada verde do painel    |
| `led_amarelo`  | saída   | 1    | lâmpada amarela do painel  |
| `led_vermelho` | saída   | 1    | lâmpada vermelha do painel |
| `buzina`       | saída   | 1    | buzina de alarme           |

## Em Verilog

Um módulo pode ter quantos `assign` forem necessários, e todos valem ao mesmo tempo, como fios soldados numa placa. Por isso a ordem das linhas não muda o circuito.

Uma entrada pode alimentar várias saídas. O contrário não vale: cada saída deve ter um único `assign`. Dois `assign` no mesmo sinal são duas fontes ligadas no mesmo fio, e quando elas discordam o valor fica indefinido (X).

## Dica

São quatro saídas, então são quatro linhas `assign`, uma para cada saída.

## Dica 2

A entrada `falha` aparece em duas linhas: na do `led_vermelho` e na da `buzina`.
