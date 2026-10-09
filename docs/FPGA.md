# Placas FPGA

O Lace leva um projeto para uma placa FPGA: confere as ligações das portas
do topo aos sinais da placa, gera o topo da placa e compila pelo Quartus
Prime, sem abrir a interface dele. Gravar pelo cabo USB da placa vem na
próxima etapa. Este documento descreve o que já existe: as placas, o
`fpga.json`, `lace fpga boards`, `lace fpga check`, `lace fpga build`, a
detecção do Quartus e o componente `openfpgaloader`.

## Placas

| Placa | `board` | FPGA | Cabo |
|---|---|---|---|
| Terasic DE2-115 | `de2-115` | Cyclone IV E EP4CE115F29C7 | USB-Blaster |
| Terasic DE10-Nano | `de10-nano` | Cyclone V SE 5CSEBA6U23I7 | USB-Blaster II |

Os pinos vêm dos manuais da Terasic; cada placa cita o manual, a versão e as
tabelas no campo `source`. Em 2026-10-09 os pinos e os padrões de I/O das
duas placas foram comparados, um a um, com o `.qsf` que a própria Terasic
distribui ("golden top", gerado pelo System Builder): 110 de 110 na DE2-115
e 17 de 17 na DE10-Nano, sem diferença. Os LEDs 6 e 7 da DE10-Nano, que a
tabela do manual não lista, vêm desse arquivo. `lace fpga boards` lista as
placas, e `lace fpga boards <placa>` mostra os sinais com os pinos, o padrão
de I/O e quais são ativos em nível baixo. Com `--json`, os dados completos.

Os dados de cada placa ficam em `crates/lace-core/boards/<placa>.json`,
embutidos no Lace. Uma placa nova é um arquivo novo nessa pasta e uma linha
em `board.rs`; os testes conferem nomes, pinos repetidos e clocks.

## O `fpga.json`

Fica na raiz do projeto, ao lado do `.spf`. A AURORA não o lê.

```json
{
  "board": "de2-115",
  "top": "media_movel",
  "connect": {
    "clk": "CLOCK_50",
    "rst": "!KEY[0]",
    "in[3:0]": "SW[3:0]",
    "out[7:0]": "LEDR[7:0]"
  }
}
```

- `board`: a placa.
- `top`: o módulo que vai para a placa. Sem ele, o topo do projeto; sem
  topo, o processador, se o projeto tiver um só.
- `connect`: cada porta do topo (com faixa ou não) e o sinal da placa que
  liga nela.

Cada ligação é `porta[faixa]` → `[!]SINAL[faixa]`. Uma entrada do topo
também pode receber `0` ou `1`. O `!` inverte o sinal: o `rst` do SAPHO é
ativo em nível alto e os botões das placas da Terasic, em nível baixo, então
`"rst": "!KEY[0]"` dá reset com o botão apertado. As faixas vão do bit mais
alto ao mais baixo (`[7:0]`) ou são um bit só (`[3]`); sem faixa, vale a
porta ou o sinal inteiro.

Regras:

- Uma entrada do topo só recebe sinal de entrada da placa (clock, botão,
  chave); uma saída do topo só dirige sinal de saída da placa (LED,
  display). Porta `inout` não é ligada.
- Com faixa dos dois lados, as larguras têm de ser iguais. Com faixa de um
  lado só, ou de nenhum, ligam-se os bits de baixo: a entrada que sobra
  recebe 0 e a saída que sobra não chega à placa. O Lace avisa cada ajuste.
- Um bit de entrada do topo tem uma ligação só; um bit de saída da placa
  também. Uma saída do topo pode dirigir mais de um sinal da placa.
- Entrada do topo sem ligação recebe 0, com aviso. Bit de saída da placa
  sem ligação fica no nível inativo: apagado (0) num LED, apagado (1) num
  segmento de display, que é ativo em nível baixo.

`lace fpga check` confere o `fpga.json` contra a placa e as portas do topo
e mostra o que vai em cada porta e em cada sinal da placa. Todos os
problemas aparecem de uma vez, um por linha:

```
$ lace fpga check
OK media_movel on Terasic DE2-115 (EP4CE115F29C7)
  clk         <-  CLOCK_50
  rst         <-  ~KEY[0]
  in[15:0]    <-  {12'b0, SW[3:0]}
  LEDR[17:0]  <-  {10'b0, out[7:0]}
  clock CLOCK_50 (50 MHz) on clk
  note: in[15:4] is tied to 0 (no connection)
```

Com `"in": "SW"` (sem faixa dos dois lados), as 16 chaves de baixo iriam
para `in` e não haveria nota; com `"in": "SW[3:0]"`, a nota seria
`in[15:4] receives 0 (SW[3:0] has 4 bits)`.

As portas do topo vêm do Verilog: um processador SAPHO precisa estar
compilado (`lace build`).

## O topo da placa

O Lace gera um módulo, `lace_board_top`, que tem como portas só os sinais
da placa usados e instancia o topo do projeto com as ligações. É ele que
vai para o Quartus, com um pino por sinal. `lace fpga check --show-top`
mostra o Verilog. Ele é gerado de novo a cada compilação: não edite.

## O Quartus Prime

Para as placas Intel, o bitstream sai do Quartus Prime, que não tem
alternativa aberta para o Cyclone IV. O Lace roda o Quartus sem abrir a
interface. Ele não vai no bundle (é proprietário e grande): é a segunda
exceção à regra do bundle, depois do compilador do Verilator. O Lace o
procura nesta ordem:

1. `--quartus <DIR>` ou `LACE_QUARTUS`: a pasta da versão
   (`C:\intelFPGA_lite\22.1std`), a `quartus` dentro dela ou a dos
   programas. Pasta declarada sem o `quartus_sh` é erro.
2. `QUARTUS_ROOTDIR`, que o instalador da Intel cria.
3. As pastas padrão do instalador: `C:\intelFPGA_lite`, `C:\intelFPGA`,
   `C:\altera_lite` e `C:\altera` no Windows; as mesmas na pasta do usuário
   e em `/opt` no Linux. Com mais de uma versão, a mais nova.

`lace tools` mostra o Quartus encontrado. O Quartus não existe para macOS.

A instalação precisa do Quartus Prime Lite e do suporte à família da placa,
que a Intel distribui à parte: Cyclone IV para a DE2-115 e Cyclone V para a
DE10-Nano. O Questa (simulador) e as outras famílias não são usados.

## Compilar: `lace fpga build`

```
lace fpga build
```

1. Confere o `fpga.json`, a placa e o Quartus antes de compilar qualquer
   coisa: faltar um deles não custa o build dos processadores.
2. Compila os processadores com o YANC, como `lace synth`: o Verilog de um
   processador aponta para as memórias (`.mif`) pelo caminho absoluto, e
   um projeto copiado de outra pasta precisa de um build novo.
3. Confere as ligações (`lace fpga check`) e grava o projeto do Quartus na
   pasta da placa.
4. Roda os quatro programas que a interface do Quartus roda ao compilar,
   um por passo.

```text
<raiz>/.lace/fpga/<placa>/
  lace_board_top.v      o topo da placa
  lace_board_top.qsf    a FPGA, os fontes e os pinos
  lace_board_top.sdc    os clocks da placa
  output_files/         o .sof, o .rbf, o .svf e os relatórios do Quartus
  db/, incremental_db/  o banco de dados do Quartus

synthesize  quartus_map --read_settings_files=on --write_settings_files=off lace_board_top -c lace_board_top
fit         quartus_fit --read_settings_files=off --write_settings_files=off lace_board_top -c lace_board_top
bitstream   quartus_asm --read_settings_files=off --write_settings_files=off lace_board_top -c lace_board_top
timing      quartus_sta lace_board_top -c lace_board_top
```

O `.qsf` traz a família e o modelo da FPGA, os fontes (a biblioteca SAPHO,
se o projeto tem processadores, os arquivos do projeto e o topo da placa),
a raiz do projeto como pasta de `include` e um pino por bit de cada sinal
usado, com o padrão de I/O do manual. Os pinos sem uso ficam como entrada,
em alta impedância, com o pull-up fraco: é o padrão do Quartus, escrito no
`.qsf` para não depender da versão. Pede também o `.rbf` e o `.svf` sem
compressão, as três linhas que a documentação do openFPGALoader indica: pela
JTAG a FPGA não descomprime. O `.sdc` cria um clock por sinal de clock da
placa ligado ao topo, com o período do oscilador. Os três arquivos são
gravados de novo a cada compilação, em inglês e só com ASCII, por
precaução: não se sabe em que codificação o Quartus os lê.

Depois do Fitter, antes de gerar o arquivo de gravação, o Lace lê o `.pin`
(onde o Quartus de fato pôs cada sinal) e confere cada bit contra a placa:
o pino, a direção, o padrão de I/O e que a posição veio da atribuição, e
não de uma escolha do Fitter. Qualquer diferença reprova a compilação no
passo `fit`, com uma mensagem por bit, e o `.sof` não é gerado.

No fim, a compilação grava o `lace-build.json` na pasta da placa: o SHA-256
do `.sof` e de tudo o que entrou nele (os fontes, a biblioteca SAPHO, as
memórias `.mif` dos processadores, o `fpga.json` e a definição da placa).
É por ele que a gravação sabe se o `.sof` ainda descreve o projeto.

Depois de compilar, o Lace mostra os erros e avisos do Quartus com arquivo e
linha (os `Info`, centenas, ficam de fora; o aviso repetido em cada canto de
tempo aparece uma vez), os recursos da FPGA do resumo do Fitter e, por
clock, a frequência pedida, a Fmax e as folgas de setup e de hold no pior
canto. Folga negativa não reprova a compilação: o Quartus gera o `.sof` do
mesmo jeito, e o Lace avisa que o design pode falhar na placa nessa
frequência.

Saída real de `lace fpga build` no projeto `bcd_prot`, com o Quartus Prime
25.1 Lite (o começo, com o build e os passos, foi cortado):

```
  quartus: warning: 1 pins must meet Intel FPGA requirements for 3.3-, 3.0-, and 2.5-V interfaces. For more information, refer to AN 447: Interfacing Cyclone IV E Devices with 3.3/3.0/2.5-V LVTTL/LVCMOS I/O Systems. [169177]
  Generated:
    bitstream (.sof)    .lace\fpga\de2-115\output_files\lace_board_top.sof
  Resources:
    Total logic elements                677 / 114480 (<1%)
    Total registers                     39
    Total pins                          67 / 529 (13%)
    Total virtual pins                  0
    Total memory bits                   0 / 3981312 (0%)
    Embedded Multiplier 9-bit elements  2 / 532 (<1%)
    Total PLLs                          0 / 4 (0%)
  Timing: met
    CLOCK_50  needs 50 MHz, reaches 250 MHz, setup slack 16.985 ns, hold slack 0.192 ns
  Program the Terasic DE2-115 with: lace fpga program
```

O aviso 169177 vem com a submensagem `Pin CLOCK_50 uses I/O standard 3.3-V
LVTTL at Y2`: é o oscilador de 50 MHz da placa, com o mesmo padrão que o
`.qsf` da Terasic usa, e o Quartus lembra os requisitos de interfaces de
3,3 V (a nota de aplicação AN 447).

O número entre colchetes é o da mensagem do Quartus, que é o que se procura
na base de conhecimento da Intel; `critical` marca os avisos críticos. Com
`--json`, o relatório `fpga-build` (`docs/schema/fpga-build.json`).

Conferido em 2026-10-09 com o Quartus Prime 25.1 Lite no Windows e uma
DE2-115, compilando e gravando. Ainda não conferido:

- a DE10-Nano com o Quartus, e o Quartus no Linux, em que as variáveis de
  ambiente que o Lace passa (a pasta e o nome do usuário, o idioma e a pasta
  temporária) são dedução; o relatório de cada passo mostra quais foram;
- caminhos com acento: o simulador já os recusa num projeto com
  processadores; no Quartus, não se sabe.

## Gravar: `lace fpga program`

```
lace fpga program            # o primeiro cabo ligado
lace fpga program --list     # os cabos que o Quartus vê
```

Grava o `.sof` da última `lace fpga build` pelo Quartus Programmer, na
pasta da placa:

```
quartus_pgm -c "USB-Blaster [USB-0]" -m jtag -o "p;output_files/lace_board_top.sof@1"
```

O `@1` é a posição da FPGA na cadeia JTAG da placa: 1 na DE2-115, 2 na
DE10-Nano, em que o HPS vem antes. A gravação vai para a SRAM da FPGA e
some quando a placa é desligada.

Antes de procurar o cabo, a gravação confere o `lace-build.json` contra o
projeto de agora, pelo conteúdo: um fonte, uma memória, o `fpga.json` ou a
definição da placa que mudou depois da compilação, um `.sof` trocado ou uma
compilação que não terminou recusam a gravação (`stale_bitstream`), com
cada diferença. Regravar os mesmos arquivos (um `lace build` que gera o
mesmo Verilog) não conta como mudança.

Conferido em 2026-10-09 com o Quartus Prime 25.1 Lite no Windows e uma
DE2-115: o projeto `bcd_prot` compilou em 35 s e gravou em 9 s. Antes de
gravar pela primeira vez:

- ligue a placa pela porta USB BLASTER, com a chave RUN/PROG em RUN;
- no Windows, o driver do USB-Blaster está na pasta `drivers` do Quartus
  (`quartus/drivers/usb-blaster`); o Gerenciador de Dispositivos instala
  por ele.

Sem cabo, o erro é `no_cable`; sem a compilação, `no_bitstream`.

No Linux, não testado: o Lace procura o Quartus em `~/intelFPGA_lite`,
`~/intelFPGA`, `~/altera_lite`, `~/altera` e nas mesmas pastas em `/opt`, e
roda os programas de `quartus/bin`. O código compila para Linux sem aviso.
Para gravar, o Quartus no Linux pede uma regra do udev que dê ao usuário
acesso ao USB-Blaster (sem ela, o `quartus_pgm -l` não vê o cabo e o erro é
`no_cable`); a regra e o serviço `jtagd` são os da documentação da Intel
para o cabo no Linux, e pedem root.

## Segurança da placa

Conferido em 2026-10-09 na DE2-115, com os relatórios da compilação real do
`bcd_prot` e o manual da placa:

| Risco | O que impede |
|---|---|
| Uma saída da FPGA num pino que a placa liga a uma chave, a um botão ou ao oscilador: curto quando os dois lados divergem | Cada sinal da placa tem direção no arquivo dela, e o Lace só liga uma saída do topo a um sinal de saída da placa (LEDs, displays). Porta `inout` não é ligada. |
| Pino errado no arquivo da placa | Os pinos e os padrões de I/O foram conferidos com o manual e com o `.qsf` da Terasic, um a um. |
| Uma atribuição que o Quartus não aplica, e o Fitter escolhe o pino | A conferência do `.pin` depois do Fitter reprova a compilação antes do `.sof`. |
| Um pino sem uso dirigindo o que está ligado nele | Os pinos sem uso ficam como entrada em alta impedância, com pull-up fraco, escrito no `.qsf`. No `.pin` da compilação real: 447 pinos `RESERVED_INPUT_WITH_WEAK_PULLUP`. |
| Gravar um design diferente do que está no projeto | A gravação recusa um `.sof` velho, trocado ou de outra placa (`stale_bitstream`). |
| Perder o projeto de fábrica da placa | A gravação é pela JTAG, direto na FPGA (`.sof`), e some ao desligar. A memória EPCS64, que guarda o projeto que a placa carrega ao ligar, só é gravada com a chave em PROG e um `.pof` (manual da DE2-115, seção 4.1); o Lace nunca faz isso. |

Com a chave JP3 na posição de fábrica (pinos 1 e 2), a cadeia JTAG da
DE2-115 tem só a FPGA, e o `@1` da gravação é ela. Na gravação real, o
Quartus Programmer leu o código JTAG da FPGA (`Device 1 contains JTAG ID
code 0x020F70DD`) antes de configurar.

O que o Lace não detecta: um jumper mudado. Os padrões de I/O das chaves,
dos botões, dos LEDs e de parte dos displays seguem o JP7 de fábrica (2,5 V
nos bancos 5 e 6); com o JP7 em outra tensão, esses padrões deixam de
corresponder à placa.

## Gravação: o componente `openfpgaloader`

O openFPGALoader grava um `.rbf` ou um `.svf` pelo cabo da placa. Vem do
OSS CAD Suite, nas três plataformas, no componente opcional
`openfpgaloader` (`lace install openfpgaloader`).

- Na DE10-Nano, o USB-Blaster II só funciona no openFPGALoader com o
  firmware `blaster_6810.hex`, que vem com o Quartus: gravar nela pede o
  Quartus instalado.
- No Linux, o acesso ao cabo pede uma regra do udev, que precisa de root. O
  OSS CAD Suite não traz a do openFPGALoader.
- No Windows, o driver do USB-Blaster que o Quartus instala não é o que o
  openFPGALoader usa (libusb). Por isso, quando há Quartus, o Lace grava
  pelo `quartus_pgm`. Falta testar com as placas.

## O que falta

- Compilar e gravar a DE10-Nano com o Quartus de verdade (a DE2-115 foi
  conferida).
- Gravar pelo openFPGALoader, sem o Quartus.
- Guardar a compilação para a placa no histórico (`lace report`).
- O fluxo aberto para o Cyclone V (Yosys e `nextpnr-mistral`), adiado: o
  `nextpnr-mistral` não vem no OSS CAD Suite 2026-09-29, em nenhuma
  plataforma, então teria de ser compilado pelo Lace.
- No Studio, um console próprio da placa: a aba Placa FPGA monta o
  `fpga.json`, compila e grava, com a saída no console da síntese.
