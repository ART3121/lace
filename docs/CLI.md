# `lace`: a linha de comando

A CLI é uma casca fina sobre o `lace-core` (ver [API.md](API.md)): cada
comando abre o projeto, chama uma função do Core e formata o resultado. O que
a CLI faz, uma GUI faz chamando as mesmas funções.

```
cargo install --path crates/lace-cli     # instala o binário `lace`
lace --help                              # visão geral
lace <comando> --help                    # detalhes de um comando
```

## Primeiro uso

Um projeto do Lace é um projeto Verilog. Processadores SAPHO, quando
existem, são uma etapa a mais: o YANC os compila antes de verificar, simular
ou sintetizar. Os dois fluxos usam os mesmos comandos e cabem no mesmo
projeto.

Verilog:

```
lace tools                       # confere o bundle instalado
lace new contador
cd contador
lace add contador.v              # cria o módulo-modelo; o primeiro módulo vira o topo
# edite contador.v
lace add contador_tb.v           # cria o testbench, que já instancia contador
lace check                       # Icarus: o design inteiro e cada testbench
lace hierarchy                   # a árvore de instâncias do design e do testbench
lace sim                         # simula o testbench e mostra o que ele escreve
lace wave                        # abre a onda da simulação no surfer-aurora
lace synth --svg                 # Yosys + Graphviz: o esquemático do topo
```

Processador SAPHO:

```
lace new demo
cd demo
lace proc add soma               # cria soma/Software/soma.cmm
# edite soma/Software/soma.cmm
# ponha os valores de entrada em soma/Simulation/input_0.txt, um por linha
lace build                       # C± -> Verilog, memórias e testbench, pelo YANC
lace sim -p soma                 # simula e mostra as saídas
lace wave -p soma                # abre a onda da simulação do processador
```

### Regras

**De onde rodar:** de qualquer pasta do projeto, não só da raiz. O Lace
procura o `.spf` na pasta atual e, se não achar, nas de cima, como o git
acha o repositório. Fora de qualquer projeto, o erro é `project_not_found`.
`-C` aponta outro lugar para começar a busca.

**Dentro da pasta de um processador** (`soma/`, `soma/Software/`,
`soma/Hardware/`, `soma/Simulation/` ou `.lace/Temp/soma/`), os comandos que
recebem um processador e não receberam nenhum agem sobre ele: `build`,
`check`, `sim`, `wave`, `synth` e `proc set`. No resto do projeto, eles agem
sobre o projeto inteiro (`proc set` pede o nome). `lace status` marca o
processador da pasta. Um nome dado (`-p`, `proc set NOME`), um `ARQUIVO` do
`check` ou um `TESTBENCH` do `sim` vale de qualquer pasta e vence a
pasta; para agir sobre o projeto inteiro de dentro de um processador, use
`-C` com a raiz.

**Caminhos:** os da linha de comando são relativos ao diretório atual do
shell, não à raiz do projeto. De fora da pasta do projeto:
`lace -C contador add contador/rtl/alu.v`.

**Arquivos Verilog:** entram no projeto por `lace add`, que os registra no
`.spf`. O papel de um arquivo que já existe vem do conteúdo, pela regra da
AURORA: `$dumpfile`, `$finish`, módulo sem portas, `initial`, `$display` e
atrasos como `#10` somam pontos de testbench (a tabela está em
[API.md, seção 4.2](API.md#42-o-módulo-verilog)). `--tb` registra como
testbench. Um arquivo fica numa lista só: registrado de novo com outro
papel, ele muda de lista.

**Arquivo novo:** `lace add` o cria a partir de um modelo. É testbench com
`--tb` ou quando o nome tem `tb`, `test` ou `testbench` como palavra
(`alu_tb.v`, `tb_alu.v`, `test_alu.v`; `contest.v` e `latest.v` não);
senão é um módulo com o nome do arquivo e portas de exemplo que compilam. O
testbench-modelo instancia o módulo testado com todas as portas (com clock e
reset, se o módulo tiver `clk`/`clock` e `rst`/`reset`), grava a onda com
todos os sinais e termina com `$finish`. O módulo testado é o `X` de
`X_tb.v` ou `tb_X.v`, se algum módulo registrado o declara; senão, o módulo
de topo; sem nenhum dos dois, o testbench não instancia nada: grava a onda,
espera `#100` e chama `$finish`.

**Topo:** o primeiro módulo registrado vira o topo, e o primeiro testbench,
o testbench simulado. `lace top` troca o topo por arquivo
(`lace top rtl/alu.v`) ou por nome de módulo (`lace top alu`);
`lace sim <testbench>` troca o testbench.

**Processadores:** a biblioteca SAPHO do YANC só entra na verificação, na
simulação e na síntese quando o projeto tem processadores. Um projeto só de
Verilog não precisa do YANC.

## Opções globais

Valem em qualquer comando, antes ou depois dele.

| Opção | Padrão | Efeito |
|---|---|---|
| `-C, --project <CAMINHO>` | `.` | o `.spf` do projeto, ou uma pasta dentro dele: o Lace sobe até achar o `.spf` |
| `--json` | não | um objeto JSON no stdout, nada mais ([JSON](#json)) |
| `--events` | não | um objeto JSON por linha no stdout: os eventos das ferramentas enquanto rodam e, na última linha, o resultado ([Eventos](#eventos)). Com `--json` junto, vale o `--events` |
| `-v`, `-vv` | não | `-v`: o comando e toda a saída de cada passo enquanto roda, mensagens `info`, artefatos intermediários, módulos da síntese e log `info` no stderr; `-vv`: log de depuração |

Bundle (o `--compiler` fica entre as opções globais do `--help`; o `--toolchain`, de desenvolvimento, não aparece na ajuda):

| Opção | Variável | Efeito |
|---|---|---|
| `--toolchain <DIR>` | `LACE_TOOLCHAIN` | outro bundle no lugar do instalado (desenvolvimento) |
| `--compiler <DIR>` | `LACE_COMPILER` | onde está o compilador para o Verilator, quando não está no local padrão; no Windows, troca o do bundle (veja abaixo) |

## De onde vêm as ferramentas

Do bundle instalado com o Lace (`toolchain/` ao lado de `bin/`), e só dele.
`lace tools` mostra o bundle, as versões e onde está cada ferramenta;
`lace tools --verify` confere os hashes. Como instalar: [INSTALL.md](INSTALL.md). O que o bundle traz e como montá-lo:
[BUNDLE.md](BUNDLE.md).

A exceção é o compilador do sistema, que o Verilator usa no Linux e no
macOS. O Lace o procura nos locais padrão ([BUNDLE.md](BUNDLE.md), seção 4)
e nunca no `PATH`. Fora deles, `--compiler <DIR>` diz onde ele está: o
diretório com `perl`, `make` e `g++` ou `clang++`. No Windows o compilador
vem no bundle, com o Verilator, e `--compiler` recebe a raiz de um MSYS2
para usá-lo no lugar, só para desenvolvimento. Um diretório sem os três é
erro (código 2), e não volta em silêncio aos locais padrão. Para não
repetir a opção, defina a variável `LACE_COMPILER` no perfil do shell:

```sh
export LACE_COMPILER=/opt/gcc/bin                    # bash, zsh: no ~/.bashrc ou ~/.zshrc
set -Ux LACE_COMPILER /opt/gcc/bin                   # fish: vale para as próximas sessões
```

```powershell
[Environment]::SetEnvironmentVariable("LACE_COMPILER", "D:\msys64", "User")
```

`lace tools` mostra qual compilador foi encontrado e se ele é do bundle ou
do sistema. Não há arquivo de configuração.

## Comandos

### Projeto

| Comando | Faz | Função do Core |
|---|---|---|
| `lace new <NOME> [--dir DIR]` | cria `<DIR>/<NOME>/<NOME>.spf`. O nome usa só letras sem acento, dígitos, `_` e `-`, começando por letra, até 64 caracteres, e não pode diferir só na caixa de outro da mesma pasta. Criado dentro da pasta de outro projeto, avisa | `Project::create`, `Project::issues` |
| `lace status` | os avisos do `.spf` (`warning: ...`), os módulos registrados (o topo marcado com o nome do módulo), os testbenches (o simulado marcado), os `.v` da pasta que não estão registrados, com a dica `lace add <arquivo>`, e os processadores (linguagem, frequência, clocks, arrays, se está compilado, e `(this folder)` no da pasta atual). Num projeto vazio, os primeiros passos dos dois fluxos | `Project::open`, `Project::issues`, `Project::files`, `Project::top_module`, `Project::unregistered_verilog`, `Processor::is_built`, `Project::processor_at` |

Os `.v` não registrados são procurados na pasta do projeto e nas subpastas,
menos `.lace/`, as pastas dos processadores e as ocultas.

Os avisos do `.spf` (`issues` no JSON) são o que está estranho sem impedir
de abrir: um caminho de outra máquina (`C:\...`) achado pela cauda dentro da
pasta do projeto, que a próxima mudança grava como o caminho de hoje; um
`topLevelFile` ou `testbenchFile` fora da lista, ou um topo com nome de
testbench, que é ignorado; um processador com nome que não compila.

### Arquivos Verilog

| Comando | Faz | Função do Core |
|---|---|---|
| `lace add <ARQUIVO>... [--tb]` | registra cada arquivo; o que não existe é criado a partir do modelo. Confere todos antes de registrar o primeiro: um nome que não serve recusa o comando inteiro, e nada é criado nem registrado | `Project::check_add_verilog`, `Project::add_verilog` |
| `lace remove <ARQUIVO>...` | tira do projeto, e do topo ou do testbench simulado se era um deles; não apaga do disco | `Project::remove_verilog` |
| `lace top` | mostra o arquivo de topo e o módulo de topo, ou diz que não há topo e como escolher | `Project::top_level`, `Project::top_module` |
| `lace top <ARQUIVO\|MÓDULO>` | escolhe o topo: se o argumento é um arquivo, ele (qualquer `.v` ou `.sv`, inclusive o `Hardware/<proc>.v` gerado; registrado se não estava); senão, o arquivo registrado que declara esse módulo. Recusa nome de testbench (`tb_<nome>.v`, `<nome>_tb.v`, `tb.v`), arquivo que não é Verilog (`invalid_name`) e módulo declarado em mais de um arquivo (`ambiguous_module`, com os arquivos); nada é gravado | `Project::set_top` |
| `lace move <ORIGEM>... <DESTINO>` | move ou renomeia arquivos e pastas do projeto; os registrados continuam no projeto, com o topo e o testbench simulado acompanhando | `Project::move_path` |
| `lace order <ARQUIVO> (--first \| --last \| --before <OUTRO> \| --after <OUTRO>)` | muda a posição do arquivo na lista dele e mostra a lista numerada; os compiladores leem nessa ordem, e um `` `define `` só vale para os de baixo | `Project::reorder_file` |

Para cada arquivo, `add` diz se o criou ou só o registrou, se ele ficou como
módulo ou como testbench e se virou o topo ou o testbench simulado. Um
arquivo sem módulo (só `` `define ``) não vira o topo. Um arquivo de fora da
pasta do projeto é gravado no `.spf` com caminho relativo (`../../rtl/x.v`)
quando está no mesmo repositório git do projeto, e absoluto quando não
(ADR 0012). As portas
que o testbench-modelo instancia são lidas pelo Yosys, quando ele está
instalado; sem ele, por um leitor de portas no estilo ANSI do próprio Lace.

`remove` avisa quais arquivos não estavam registrados e sai com código 2 se
nenhum estava.

`move` segue o `mv`: se `DESTINO` é uma pasta que existe, ou termina com
`/`, cada origem vai para dentro dela; senão, a única origem passa a ter o
caminho `DESTINO` (renomear). As pastas que faltam são criadas. Com várias origens,
`DESTINO` precisa ser uma pasta (código 2). Para cada origem, diz de onde
para onde foi e quais arquivos continuam no projeto, com o papel de cada um.
Ficam no lugar o `.spf`, a pasta `.lace` e, de cada processador, a pasta,
`Software/`, `Hardware/`, `Simulation/` e o fonte (`cannot_move`); o destino
que existe não é sobrescrito (`path_exists`). As origens anteriores à que
falhou já foram movidas.

O módulo de topo é o único módulo do arquivo de topo; num arquivo com vários
módulos, o que tem o nome do arquivo. Sem nenhum dos dois, é erro
(`module_not_found`, com os módulos do projeto na mensagem).

### Processadores: `lace proc`

| Comando | Faz | Função do Core |
|---|---|---|
| `proc add <NOME> [--lang cmm\|cpp] [--inputs N] [--outputs N] [--nubits N] [--nbmant N] [--nbexpo N] [--nugain N] [--ndstac N] [--sdepth N]` | cria diretórios, fonte-modelo e entrada no `.spf` | `Project::add_processor` |
| `proc set [NOME] [--freq MHZ] [--clocks N] [--arrays true\|false]` | grava frequência, clocks e exportação de arrays no `.spf`; valem a partir do próximo build. Sem `NOME`, o processador da pasta | `Project::configure_processor` |

As opções `--nubits` a `--sdepth` são só de C±; com `--lang cpp`, o comando
recusa (código 2). Os padrões são os da AURORA (23, 16, 6, 128, 5, 5). Com
só `--nbmant` ou `--nbexpo`, `--nubits` acompanha (`#NBMANT + #NBEXPO + 1`,
o único que o `asmcomp` aceita). O Core recusa, antes de criar qualquer
coisa, nome que não compila (palavra do C± ou do Verilog, módulo da
biblioteca SAPHO, mais de 64 caracteres) e parâmetro fora do que o YANC
compila (`invalid_parameter`: `#NUBITS` diferente da soma ou acima de 32,
`#NBEXPO` fora de 2 a 8, `#NUGAIN` que não é potência de dois, pilha 0, mais
de 256 portas). `proc set` recusa `--freq` fora de 1 a 500000 e `--clocks`
fora de 1 a 2147483647, com o motivo. Os processadores do projeto aparecem
no `lace status`.

### Compilar, verificar, simular, sintetizar

| Comando | Faz | Função do Core |
|---|---|---|
| `lace build [-p NOME]...` | compila os processadores pedidos; sem `-p`, o da pasta ou, fora deles, todos. Num projeto sem processadores, avisa e sai com 0 | `build_processors` (`Continue`) |
| `lace check [ARQUIVO] [--lint]` | compila os processadores que têm fonte e verifica o Verilog (ver abaixo). Dentro da pasta de um processador, sem `ARQUIVO`, é o mesmo que `-p` com ele | `buildable_processors`, `build_processors` (`Stop`), `check` |
| `lace check -p NOME [--lint]` | compila o processador e verifica só o Verilog dele e o testbench gerado pelo YANC | `build_processors` (`Stop`), `check` (`CheckOptions::processor`) |
| `lace sim [TESTBENCH] [--verilator] [--timeout S] [--open]` | compila os processadores que têm fonte e simula o testbench do projeto. Com `TESTBENCH`, ele passa a ser o testbench simulado (e é registrado, se não estava). Dentro da pasta de um processador, sem `TESTBENCH`, é o mesmo que `-p` com ele | `set_testbench`, `buildable_processors`, `build_processors` (`Stop`), `simulate_project` |
| `lace sim -p NOME [--verilator] [--timeout S] [--open]` | compila o processador e o simula com o testbench gerado pelo YANC | `build_processors` (`Stop`), `simulate` |
| `lace synth [--svg] [--module M]` | compila os processadores e sintetiza o módulo de topo do projeto; com `--svg`, desenha o esquemático (Yosys `show` + `dot`). Dentro da pasta de um processador, é o mesmo que `-p` com ele | `build_processors` (`Stop`), `synthesize`, `render_schematic` |
| `lace synth -p NOME [--svg] [--module M]` | compila o processador e o sintetiza sozinho | `build_processors` (`Stop`), `synthesize`, `render_schematic` |

`TESTBENCH` e `-p` não andam juntos, nem `ARQUIVO` e `-p` no `check`. `--verilator` simula com o Verilator no
lugar do Icarus; ele usa o compilador do sistema no Linux e no macOS e o
do bundle no Windows (ver [BUNDLE.md](BUNDLE.md)) e grava a onda sempre em
VCD.
`--timeout <SEGUNDOS>` (inteiro, a partir de 1) encerra a simulação que
passar desse tempo, sem contar a elaboração e a compilação; sem ele, não há
limite, e um testbench sem `$finish` roda até o Ctrl+C. No Linux e no
macOS, o prazo pede ao simulador que saia (SIGTERM), e o que o testbench
escreveu até ali aparece. No Windows não há esse pedido: o `taskkill` encerra
o simulador na hora, e o que ainda estava no buffer de saída dele se perde
(um `$fflush` no testbench o escreve antes). `--open` abre a onda
no surfer-aurora ao terminar. O esquemático sempre traz a largura dos
barramentos. Para ver os nomes de módulo que `--module` aceita, rode
`lace synth -v`.

O esquemático recusa um módulo com mais de 120 ligações (portas das células
mais portas do módulo), porque o Graphviz levaria minutos: o comando sai com
código 2 (`schematic_too_large`) e sugere desenhar um submódulo com
`--module`. `--no-schematic-limit` desenha mesmo assim. O passo do `dot` tem
prazo de 60 s. Um `--module` fora da árvore do topo (que a síntese não
inclui) sai com código 2 (`module_not_found`, com os módulos sintetizados na
mensagem). Nos dois casos o relatório da síntese é gravado antes do erro.

Um build ou uma simulação de um processador que já está em outro build ou
simulação (outro terminal, o Studio) recusa na hora com
`operation_in_progress`, código 2.

`check`, `sim` e `synth` param no primeiro build que falhar, com código 1,
sem verificar, simular nem sintetizar. `build` compila todos os pedidos mesmo
que um falhe, para mostrar todos os erros de uma vez.

#### O que `check` verifica

Sem `ARQUIVO`, três passos:

1. `iverilog -tnull -Wall` (com `-g2012` se houver `.sv`) sobre o design inteiro (os módulos
   registrados, o Verilog dos processadores e o `TopLevel/*.v` legado da
   AURORA), sem escolher topo. O Icarus elabora cada raiz, então um módulo
   que nenhum outro instancia também é verificado.
2. Um `iverilog` por testbench registrado, com o design e `-s` no módulo do
   testbench.
3. Com `--lint`, `verilator --lint-only -Wall` sobre o design, com o módulo
   de topo, se houver. O Verilator acha o que o Icarus deixa passar, como
   larguras que não batem e sinais sem uso. Sem o Verilator instalado, os
   dois primeiros passos rodam do mesmo jeito, um aviso diz que o lint não
   rodou e a CLI sugere `lace install verilator`.

Uma macro indefinida no ponto de uso (o `` `define `` num arquivo que vem
depois na lista do projeto) faz o `check` falhar, com a explicação; o
Icarus sozinho só avisa e compila com a macro vazia. Com recursão
parametrizada (um módulo que se instancia num `generate`), as raízes vão com
`-s`. Com muitas raízes, o título diz quantas e nomeia as cinco primeiras.

Com `ARQUIVO`, só os módulos daquele arquivo são elaborados como raiz, com o
resto do design para resolver as instâncias; um testbench é elaborado com o
design.

Com `-p NOME`, ou dentro da pasta do processador, o design é só o
`<NOME>/Hardware/<NOME>.v` que o build gerou, e o testbench é o dele,
`<NOME>/Simulation/<NOME>_tb.v`; os passos são os mesmos, e o lint usa o
processador como topo. Os outros processadores não são compilados nem
verificados.

A saída lista os módulos elaborados como raiz e os diagnósticos de cada
passo. Avisos, do Icarus ou do Verilator, não fazem o `check` falhar. O
`check` não gera arquivo nenhum e sai com 1 se algum passo falhou.

#### O que `sim` mostra

- enquanto roda, o que o testbench escreve (`$display`, `$monitor`), linha a
  linha, cada uma depois de `|`, sem precisar de `-v`. As linhas que o
  próprio simulador escreve (`$finish called at`, `VCD info:` ou
  `FST info:`, o `ERROR:` de um `$error`) não saem aí: ficam para o resumo,
  como diagnósticos, e as `info` só aparecem com `-v`;
- com `-v`, enquanto roda, o comando de cada passo (depois de `$`) e tudo o
  que ele escreve, inclusive as mensagens das ferramentas;
- depois, o resumo: a linha de título, os diagnósticos e a onda;
- com `-p`, depois de uma simulação que deu certo, os valores de cada porta
  de saída, uma linha por porta: `Output 0: 5 -2 26`;
- quando falta um arquivo de entrada do processador, qual criar:
  `Warning: Create soma/Simulation/input_0.txt with one value per line (the testbench reads it)`;
- quando passa do `--timeout`, o título
  `timed out, vvp stopped at the 1 s limit` e a dica
  `Does the testbench reach $finish? Without it the simulation never ends.
  Otherwise, raise --timeout`;
- com `-p`, um aviso quando o programa não chega ao fim nos clocks
  simulados (as saídas param ali; suba com `lace proc set <nome> --clocks N`),
  e outro para um arquivo de entrada vazio ou com valor que não cabe em
  `#NUBITS` bits. Uma linha de entrada que não é inteiro recusa a simulação
  antes de rodar (`invalid_data_file`, com o arquivo e a linha);
- com `-p`, um aviso quando o programa lê mais valores do que um
  `input_<n>.txt` tem, com o arquivo e a linha (dali em diante a porta
  repete o último valor);
- sem `-p`, um aviso quando a onda VCD passa de 100 MB, com a sugestão de
  `$dumpfile("<nome>.fst")`;
- uma linha `ERROR: ...` que o testbench escreve com `$display` sai como as
  outras (`| ERROR: ...`) e não reprova; só o `$error` e o `$fatal` do
  Verilog reprovam;
- sem `-p`, um aviso quando o testbench chama `$dumpfile` sem `$dumpvars`
  (a onda não sai). Um `$dumpfile(ONDA)` com `localparam`, `parameter` ou
  `` `define `` de texto vale como o nome entre aspas.

```
  | q = 10
Simulation of contador_tb (Icarus): finished in 0.09 s
    done      elaborate     iverilog       65 ms
    done      simulate      vvp            20 ms
  Generated:
    Icarus image        .lace/Temp/contador_tb.vvp
    waveform            contador_tb.fst
  Open the waveform with: lace wave
Report run-000002: lace report show 2
```

A simulação falha, com código 1, quando o Verilog não elabora, quando o
simulador sai com código diferente de 0 e quando o `vvp` escreve uma linha
`ERROR:` ou `FATAL:`, que é o que `$error` e `$fatal` fazem no Icarus. O
`vvp` sai com 0 depois de um `$error`; o Lace conta isso como falha. Passar
do `--timeout` também sai com 1.

#### Onde fica a onda

Quem decide é o `$dumpfile` do testbench: o nome, relativo à raiz do
projeto, e o formato, pela extensão (`.fst` grava FST; qualquer outra, VCD).
O testbench-modelo do `lace add` grava `<testbench>.vcd`. Num testbench sem
`$dumpfile`, o Lace simula uma cópia com `$dumpfile("<tb>.fst")` (`<tb>.vcd`
com o Verilator) e `$dumpvars(0, <tb>)`, em que `<tb>` é o módulo do
testbench: a onda fica na raiz, com todos os sinais, inclusive os do módulo
testado. O arquivo do usuário não muda.

Com `-p`, o testbench é o que o YANC gerou e o build copiou para
`<NOME>/Simulation/<NOME>_tb.v`, como a AURORA, e a onda fica em
`.lace/Temp/<NOME>/<NOME>_tb.vcd`.

### Hierarquia

| Comando | Faz | Função do Core |
|---|---|---|
| `lace hierarchy` | a árvore de instâncias do design e de cada testbench (os registrados e o de cada processador compilado), como o Icarus as elabora. Dentro da pasta de um processador, é o mesmo que `-p` com ele | `hierarchy` |
| `lace hierarchy -p NOME` | a do processador e do testbench gerado pelo YANC | `hierarchy` (`HierarchyOptions::processor`) |

Cada linha é `instância: módulo`, ou só o nome quando os dois são iguais,
recuada pela profundidade; instâncias dentro de blocos `generate` levam o
nome do bloco na frente (`op_add.my_add`). Os módulos da biblioteca SAPHO
saem marcados `SAPHO` e fechados, com o número de instâncias dentro; `-v`
abre todos e mostra o arquivo e a linha de cada definição. No fim, o resumo
das elaborações, como no `check`.

`hierarchy` não compila os processadores nem grava relatório: mostra o que
está no disco. Um processador ainda não compilado fica de fora, com um
aviso de como compilar. Uma elaboração que falha (código 1) não esconde as
outras: o design aparece mesmo com um testbench quebrado, e o erro sai com
arquivo e linha.

### Onda

| Comando | Faz | Função do Core |
|---|---|---|
| `lace wave` | abre no surfer-aurora a onda da simulação do projeto; dentro da pasta de um processador, a dele | `waveform_path`, `open_waveform` |
| `lace wave -p NOME` | abre a onda da simulação do processador | `waveform_path`, `open_waveform` |
| `lace wave <ONDA>` | abre um arquivo de onda (VCD, FST, GHW), relativo ao diretório atual | `open_waveform` |
| `lace wave --no-layout` | abre a onda crua, sem o layout dos processadores | `open_waveform` |

Numa onda com processador SAPHO, o Surfer abre com o layout da AURORA: as
variáveis do programa, a instrução de assembly e a linha do C± de cada
ciclo, em grupos por processador (`prepare_wave_layout`, API.md 5.7.1). O
texto lista cada processador do layout. `lace sim --open` faz o mesmo.

Sem `ONDA`, o Lace abre o arquivo em que a simulação grava a onda (ver
[Onde fica a onda](#onde-fica-a-onda)); ele não simula. O comando retorna logo, sem esperar o Surfer
fechar. Se o Surfer fechar em menos de 1,5 s (sem display, onda inválida), o
comando falha com código 2 (`process_exited_early`) e mostra o fim do log do
Surfer (`RunningProcess::ensure_started`).

O log do Surfer não fica ao lado da onda: vai para `.lace/Temp/surfer/` do
projeto, como `contador_tb.fst.log`, e cada abertura da mesma onda
sobrescreve o anterior. Uma onda fora de projeto (`lace wave` com um arquivo
de outra pasta) deixa o log na pasta de cache do usuário, `~/.cache/lace/surfer`
no Linux, `~/Library/Caches/lace/surfer` no macOS e `%TEMP%\lace\surfer` no
Windows.

### Relatórios

Cada `lace build`, `check`, `sim` e `synth` que roda alguma ferramenta grava
um relatório no histórico do projeto, `.lace/reports/run-NNNNNN/`, e diz no
fim qual foi (`Report run-000042: lace report show 42`). A operação que
falha também grava; a que nem começa (projeto inválido, componente que
falta) não. Se o relatório não puder ser gravado, a operação não falha por
isso: sai um aviso.

| Comando | Faz | Função do Core |
|---|---|---|
| `lace report` | mostra o relatório mais novo | `history::latest`, `history::report_text` |
| `lace report show ID` | mostra um relatório; `ID` é `run-000042` ou só `42` | `history::load`, `history::report_text` |
| `lace report list [--limit N]` | lista os relatórios, do mais novo para o mais antigo, com o que cada um tem para comparar | `history::list` |
| `lace report compare [ID] [--against ID] [--summary]` | compara as estatísticas de síntese e os tempos de simulação de dois relatórios | `history::compare_reports` |
| `lace report clean [ID...] [--keep N] [--yes]` | apaga relatórios: sem argumento, todos; com `--keep N`, todos menos os `N` mais novos; com `ID`, só esses | `history::plan_cleanup`, `history::remove` |

Nenhum desses comandos roda ferramenta: eles só leem o histórico, menos o
`clean`, que apaga. Apagar todos, ou todos menos os mais novos, pergunta
antes (`[y/N]`, no stderr); sem terminal, exige `--yes`. Os relatórios
pedidos pelo `ID` saem sem pergunta, e um `ID` que não existe faz o comando
falhar sem apagar nenhum. O número de um relatório apagado não volta: depois
de apagar o `run-000012`, o próximo é o `run-000013`, mesmo com o histórico
vazio.

O relatório tem o status e o comando; a máquina (sistema, processador,
memória); as ferramentas do bundle que rodaram, com a versão e o caminho; o
projeto; o tempo de cada fase e de cada ferramenta; as estatísticas
genéricas de síntese, quando houve síntese; os artefatos; e, numa falha, o
erro e o fim da saída do passo que falhou.

As estatísticas de síntese saem do `stat` do Yosys sobre o netlist que a
síntese gera: módulos, fios, bits, memórias, processos, células e células
por tipo. São células genéricas do Yosys, não mapeadas para FPGA: não há
estimativa de LUT, DSP, ocupação nem temporização. Uma contagem que o Yosys
não informou aparece como `not reported`, e é diferente de zero.

`compare` sem `ID` compara o relatório mais novo; com `ID`, aquele. Sem
`--against`, a referência é o relatório mais novo, entre os anteriores, que
se compara com ele em pelo menos uma parte, de preferência uma que terminou
bem (uma simulação que estourou o prazo só vira referência sem outra); com
`--against`, é exatamente o pedido. Dois relatórios sem parte em comum (uma
simulação contra uma síntese) saem com `not_comparable`, código 2. Os
relatórios continuam se comparando depois de a pasta do projeto mudar de
lugar. A síntese se compara quando os dois têm estatísticas do mesmo topo;
a simulação, quando os dois têm tempos do mesmo simulador e testbench. O que
difere além disso (fontes, entradas, versões das ferramentas, a máquina)
aparece em `CONTEXT WARNINGS`, sem impedir a comparação. Uma mudança é só
descrita: tempo maior não é chamado de regressão. A porcentagem é da
referência; de zero para outro número, aparece `new`.

Os tempos são de relógio, em milissegundos: a compilação (`iverilog` ou
`verilator --binary`), a execução (`vvp` ou o modelo) e a simulação inteira,
medida à parte. O tempo simulado é outra grandeza, o quanto o tempo andou
dentro do modelo: vem do `$finish called at` do `vvp`, e o Verilator não o
informa. Uma rodada só não é medida de desempenho.

### Bundle

| Comando | Faz |
|---|---|
| `lace tools [--verify]` | o bundle (identificador, plataforma, componentes instalados com versão e origem, e os não instalados), cada ferramenta (`OK`, `--` se o componente dela não foi instalado, `!!` se falta o executável), o compilador do Verilator, com `(bundle)` ou `(system)`. Com `--verify`, confere o SHA-256 de cada executável e sai com 1 se algum não conferir |
| `lace install [APLICATIVO...] [--from CAMINHO]` | instala aplicativos do bundle (yanc, icarus, verilator, cocotb, yosys, graphviz, surfer-aurora, studio) na instalação de onde este `lace` roda, sem reinstalar o Lace e sem tirar nenhum. Sem nomes, abre no terminal a lista com os instalados travados; com nomes, instala direto, com o que eles exigem. Só os pedaços dos aplicativos novos são baixados (da release desta versão, conferidos pelo `SHA256SUMS`; `LACE_RELEASE_URL` troca a origem por um espelho) ou lidos de `--from` (a pasta, o `.tar.gz` ou o `payload/` de um instalador). Uma falha no meio desfaz o que entrou. O `studio` ganha o atalho no menu de aplicativos (Linux e macOS). Recusa um `lace` que não foi instalado pelo instalador, um nome que não é do bundle e um bundle diferente do instalado |
| `lace update [--check] [--yes]` | compara o Lace, o bundle e cada aplicativo instalado com a última release (o `bundle/versions.json` da tag dela) e com a última versão upstream (releases do OSS CAD Suite, do lace-toolchain e do YANC no GitHub, tags do surfer-aurora e releases do Graphviz no GitLab), e marca `(new)` o que é mais novo; `?` é uma fonte que não respondeu. Com `--check`, só mostra. Sem ele, se há Lace mais novo, pergunta (sem terminal, exige `--yes`) e baixa o instalador da release, conferido pelo `SHA256SUMS`: no Linux e no macOS reinstala com os mesmos aplicativos, a mesma pasta e o mesmo atalho; no Windows abre o assistente. Ferramenta mais nova upstream não é instalada: chega num bundle novo, numa release nova do Lace. Sem rede ou com o GitHub fora, sai com 2. Recusa atualizar um `lace` que não foi instalado pelo instalador |
| `lace uninstall [--yes]` | remove a instalação de onde este `lace` roda, com o bundle inteiro: no Linux e no macOS roda o `uninstall.sh` da pasta (sai `toolchain/`, `bin/lace`, o atalho e a pasta), no Windows abre o desinstalador do Inno (que também tira a pasta do PATH). Pergunta antes; sem terminal, exige `--yes`. Recusa um `lace` que não foi instalado pelo instalador. Também apaga o `~/.config/lace/config.json` de um build anterior à 0.2.0, e tira o atalho do Studio no menu. Os projetos ficam |

O compilador do sistema para o Verilator (Linux e macOS), quando está fora
do local padrão, é declarado com `--compiler <DIR>` ou `LACE_COMPILER`
([De onde vêm as ferramentas](#de-onde-vêm-as-ferramentas)).

`lace completions <bash|zsh|fish|elvish|powershell>` gera o script de
autocompletar. Ele não aparece na ajuda. Para instalar no bash:

```
lace completions bash > ~/.local/share/bash-completion/completions/lace
```

## Saída

### Texto

```
Build of conta (C±, 100 MHz, 2000 clocks): failed after 2 ms: cmmcomp exited with code 1
    failed    compile       cmmcomp         2 ms
    not run   pre assemble  appcomp
    not run   assemble      asmcomp
  /home/eu/projetos/com_erro/conta/Software/conta.cmm:16: error: c'mon dude, declare the variable 'total' properly!
  Written before it stopped:
    assembly            conta/Software/conta.asm
  Not generated:
    processor Verilog   conta/Hardware/conta.v
    data memory         conta/Hardware/conta_data.mif
    instruction memory  conta/Hardware/conta_inst.mif
    testbench           conta/Simulation/conta_tb.v
Simulation not run: the build of conta did not finish
Report run-000001: lace report show 1
```

- Uma linha de título por operação, com o que ela fez e como terminou:
  `finished in <tempo>`, `failed after <tempo>: <ferramenta> exited with
  code N`, `crashed` (a ferramenta morreu por sinal ou exceção),
  `incomplete` (as ferramentas rodaram, mas faltou arquivo), `cancelled
  after <tempo>, <ferramenta> stopped` (Ctrl+C; sem nenhum passo rodado,
  `cancelled before starting`) ou `timed out, <ferramenta> stopped at the
  N s limit` (`--timeout`).
- Embaixo, um passo por linha: `done`, `failed`, `crashed`, `cancelled`,
  `timed out`, com a ferramenta e o tempo; os passos que a falha impediu
  aparecem como `not run`.
- Na simulação, o que o testbench escreve sai antes do título, enquanto
  roda (ver [O que `sim` mostra](#o-que-sim-mostra)).
- Um diagnóstico por linha, no formato `arquivo:linha[:coluna]: gravidade:
  mensagem`, que editores e terminais transformam em link. A gravidade fica
  em minúscula, como no gcc (`error`, `warning`, `info`), porque é o que os
  editores reconhecem. Mensagens `info` só aparecem com `-v`.
- Os artefatos com o papel e o caminho: `Generated` lista os obrigatórios
  (com `-v`, também os intermediários); numa falha, `Written before it
  stopped` lista o que saiu, talvez pela metade, e `Not generated`, o que
  faltou, com `(left from an earlier run)` quando sobrou um de antes.
- Quando um build falha antes de `check`, `sim` ou `synth`, uma linha diz
  que a fase seguinte não rodou e por quê.
- Num terminal, uma barra mostra que um passo está rodando: o que ele faz
  (`Simulating (vvp)`, `Compiling the model (verilator)`), uma animação e o
  tempo decorrido. Ela é indeterminada, porque nem o `vvp` nem o modelo do
  Verilator informam quanto falta, e só aparece depois de 0,3 s, para os
  passos rápidos não piscarem. Fica no stderr e sai da frente quando o
  testbench escreve; num pipe, num log ou com `--json`, não há barra.
- Cores só num terminal, e nunca com `NO_COLOR` definido.

### Erros e dicas

Um erro que impede o comando de rodar sai no stderr como `Error: <mensagem>`,
com as causas depois de `: `, sem repetir uma causa que a mensagem já
termina com ela.
Os avisos começam com `Warning:`.
As mensagens do Core são neutras, sem nome de comando; para alguns códigos, a
CLI acrescenta o comando que resolve:

| `error.code` | Dica |
|---|---|
| `no_top_level` | `Choose it with: lace top <file\|module>` |
| `no_testbench` | `Create it with: lace add <name>_tb.v` |
| `empty_project` | `Add one with: lace add <file.v>, or create a processor with: lace proc add <name>` |
| `not_built` | `Build and simulate with: lace sim -p <processor>` |
| `processor_not_found` | num projeto sem processadores, `Create it with: lace proc add <name>`; com processadores, nenhuma: a mensagem lista os que existem |
| `system_compiler_missing` | `Or set its location with --compiler <DIR> or LACE_COMPILER` |
| `module_not_found` | nenhuma: a mensagem já lista os módulos do projeto |
| `ambiguous_module` | `Choose it by file: lace top <arquivo>` |
| `component_missing` | `Install it with: lace install <componente>` |
| `project_not_found` | `Create one with: lace new <name>, or point to it with -C <folder>` |
| `no_reports` | `lace build, check, sim and synth each store a report` |
| `report_not_found` | `List them with: lace report list` |

### JSON

Com `--json`, o stdout tem exatamente um objeto JSON e nada mais. Cada
comando escreve um tipo só, com JSON Schema em `docs/schema/`, gerado dos
tipos e conferido por teste
([ADR 0008](adr/0008-contrato-do-json-gerado-dos-tipos.md)). Os campos estão
no schema; os resultados de operação são os tipos do Core serializados
([API.md, seção 6](API.md#6-resultados)).

| Comando | Schema | O objeto |
|---|---|---|
| `new` | `new.json` | o projeto criado: o `.spf` e a pasta |
| `status` | `status.json` | o projeto inteiro: arquivos registrados, topo, testbench escolhido, `.v` não registrados e processadores, cada um dizendo se está compilado |
| `add` | `add.json` | o `.spf` e um `AddedFile` por arquivo, na ordem da linha de comando |
| `remove` | `remove.json` | o `.spf`, os arquivos tirados e os que não estavam registrados |
| `top` | `top.json` | o arquivo e o módulo de topo e, quando o módulo não dá para saber, o erro |
| `move` | `move.json` | o `.spf` e um `MovedPath` por origem: de onde, para onde e os arquivos do projeto que foram junto |
| `order` | `order.json` | o `.spf` e a lista do arquivo, na ordem nova |
| `proc add`, `proc set` | `proc.json` | o `Processor` criado ou atualizado |
| `build` | `build.json` | o `.spf` e um `BuildResult` por processador compilado; nenhum num projeto sem processadores |
| `check` | `check.json` | os builds feitos antes e o `CheckResult` |
| `hierarchy` | `hierarchy.json` | o `HierarchyResult`: o design e cada testbench, com a árvore de instâncias e o status de cada elaboração |
| `sim` | `sim.json` | os builds feitos antes, o `SimulationResult`, os valores das portas de saída (com `-p`, depois de uma simulação que deu certo; uma porta que não pôde ser lida, como o `x` de uma divisão por zero, vem com `values` vazio e o motivo em `error`) e o PID do surfer-aurora (com `--open`) |
| `wave` | `wave.json` | a onda aberta, o PID do surfer-aurora e o log dele; o `.surf.ron` gerado (`layout`, `null` sem processador ou com `--no-layout`) e os processadores dele (`processors`) |
| `synth` | `synth.json` | os builds feitos antes, o `SynthesisResult` (com as estatísticas do Yosys em `statistics`) e, com `--svg`, o `SchematicResult` |
| `report`, `report show` | `report.json` | o identificador, o caminho do `report.txt`, o que a comparação usa (`record`: contexto, estatísticas e tempos) e o texto |
| `report list` | `report-list.json` | um resumo por relatório, do mais novo para o mais antigo |
| `report compare` | `report-compare.json` | os dois relatórios, a síntese e a simulação comparadas (`null` quando não se comparam) e os avisos de contexto |
| `report clean` | `report-clean.json` | os relatórios apagados, do mais antigo para o mais novo, e quantos ficaram |
| `tools` | `tools.json` | o bundle, os componentes instalados e os que faltam, cada ferramenta (o caminho ou o erro), o compilador do Verilator (`system_compiler`, com `bundled` verdadeiro no Windows) e, com `--verify`, os executáveis que não conferem |
| `install` | `install.json` | a pasta, os aplicativos instalados depois e os que entraram. Exige nomes: com `--json` não há lista |
| `update` | `update.json` | o Lace (instalado, último publicado, se é mais novo), o bundle (instalado e o da última release), cada aplicativo com a versão instalada, a da release e a upstream (`null` quando a consulta falhou), e o que o comando fez (`checked`, `up_to_date`, `updated`, `wizard_opened`). O que o instalador escreve vai para o stderr |
| `uninstall` | `uninstall.json` | a pasta da instalação e se ela já saiu (`false` no Windows, onde o desinstalador termina depois que o `lace` sai). O que o `uninstall.sh` escreve vai para o stderr |
| qualquer erro (código 2) | `error.json` | o erro, com o código estável e, quando há, a dica |
| cada linha do `--events` | `events.json` | um evento do Core, ou a linha final com o resultado ([Eventos](#eventos)) |

O que o schema não mostra:

- `build`, `check`, `sim` e `synth` trazem em `report` o relatório que
  gravaram (`run-000042`), ou `null` se nada rodou ou se ele não pôde ser
  gravado.
- Em `check`, `sim` e `synth`, se um build falha ou é cancelado antes, o
  resultado da operação sai `null`, e `sim` escreve `outputs: []` e
  `surfer_pid: null`: os campos aparecem sempre.
- Em `tools`, as chaves de `tools` (o nome de cada executável) saem em ordem
  alfabética.
- O erro dentro de `top` (`module_error`) e o de uma ferramenta em `tools`
  (`tools.<nome>.error`) têm o mesmo formato do erro de um comando, com
  `hint` quando há dica.
- Um comando cancelado (código 130) também escreve o objeto, com
  `status: cancelled` no resultado que estava rodando.
- O objeto sai indentado; o de erro, numa linha só.

`error.code` é o `LaceError::code()` do Core (tabela em
[API.md, seção 8](API.md#8-erros)), ou `"cli"` para erros da própria linha de
comando (`--compiler` sem os três programas, `remove` sem nenhum arquivo
registrado, `proc set` sem nada para mudar).

A exceção são os erros de argumento (opção desconhecida, valor inválido): o
`clap` os detecta antes de o comando começar e escreve a mensagem no stderr,
em texto, com código 2.

Com `--json`, o Lace não acompanha a saída das ferramentas enquanto rodam:
o `vvp` fica com o buffer do stdout, que é mais rápido, e o `$display` do
testbench só aparece no fim, no `stdout` do passo `simulate`. Em texto e com
`--events`, o `vvp` roda com `-i`, sem buffer, para cada linha sair na hora.
Para ler a saída enquanto roda a partir de outro programa, use `--events`.

### Eventos

Com `--events`, o stdout tem um objeto JSON por linha, e nada mais. Enquanto
as ferramentas rodam, cada linha é um evento do Core: `step_started`,
`output` (uma por linha que a ferramenta escreve) e `step_finished`
([API.md, seção 5.8](API.md#58-cancelamento-prazo-e-saída-ao-vivo-control)).
A última linha é `{"event": "result", "result": <objeto>}`, em que
`<objeto>` é o mesmo que o comando escreveria com `--json`. Um erro que
impede o comando de rodar vira essa linha final, com o objeto de erro em
`result`. Um comando que não roda ferramenta (`new`, `status`, `add`)
escreve só a linha final.

Cada linha confere com `docs/schema/events.json`. Em `output`, `diagnostic`
separa o que é mensagem da ferramenta (volta interpretado em `diagnostics`
no resultado) do que é saída do programa, como o `$display` do testbench.

`lace --events sim` no projeto `contador` (caminhos encurtados, resultado
cortado):

```
{"event":"step_started","step":"elaborate","tool":"iverilog","command":{"program":"/bin/bash","args":["..."],"cwd":"/home/eu/contador","env":[["PATH","/usr/bin:/bin"]],"inherit":[]}}
{"event":"step_finished","step":"elaborate","tool":"iverilog","termination":{"kind":"exited","value":0},"duration_ms":111}
{"event":"step_started","step":"simulate","tool":"vvp","command":{"program":"/bin/bash","args":["/opt/lace/toolchain/oss-cad-suite/bin/vvp","-n","-i","/home/eu/contador/.lace/Temp/contador_tb.vvp","-fst"],"cwd":"/home/eu/contador","env":[["PATH","/usr/bin:/bin"]],"inherit":[]}}
{"event":"output","step":"simulate","tool":"vvp","stream":"stdout","line":"FST info: dumpfile contador_tb.fst opened for output.","diagnostic":true}
{"event":"output","step":"simulate","tool":"vvp","stream":"stdout","line":"q = 10","diagnostic":false}
{"event":"output","step":"simulate","tool":"vvp","stream":"stdout","line":"/home/eu/contador/.lace/Temp/instr_contador_tb.v:15: $finish called at 112000 (1ps)","diagnostic":true}
{"event":"step_finished","step":"simulate","tool":"vvp","termination":{"kind":"exited","value":0},"duration_ms":49}
{"event":"result","result":{"builds":[],"simulation":{"top":"contador_tb","simulator":"icarus","status":"succeeded","...":"..."},"outputs":[],"surfer_pid":null}}
```


### Interromper

Ctrl+C (SIGINT), SIGTERM e, no Unix, SIGHUP (o do terminal que fecha)
cancelam o comando: o Lace encerra a ferramenta que está rodando, com tudo
o que ela iniciou, não começa os passos seguintes e mostra o que chegou a
rodar, com o título `Cancelled, <tool> stopped`. O código
de saída é 130. Com `--json` e `--events`, o objeto do resultado sai do
mesmo jeito. Um segundo sinal sai na hora, também com 130.

```
  | comecou
Simulation infinito_tb: Cancelled, vvp stopped (icarus)
```

Quem roda o `lace` a partir de outro programa e precisa pará-lo deve
mandar SIGTERM. Um SIGKILL mata só o `lace`: a ferramenta roda num grupo
de processos próprio e continua rodando sozinha.

### Códigos de saída

| Código | Significado |
|---|---|
| 0 | deu certo |
| 1 | a operação rodou e falhou: erro de compilação, Verilog que não elabora, simulação que falhou (inclusive por `$error`) ou que passou do `--timeout` |
| 2 | o Lace não conseguiu rodar: bundle, projeto, arquivo, opção inválida |
| 130 | cancelado: o Lace recebeu Ctrl+C, SIGTERM ou, no Unix, SIGHUP durante o comando |

### Log

O log do Core sai no stderr: avisos por padrão, `info` com `-v`, `debug` com
`-vv`. `RUST_LOG` tem precedência (por exemplo `RUST_LOG=lace_core=debug`).
