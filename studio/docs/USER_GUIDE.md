# Guia do usuário

Como usar o Lace Studio: a janela, os dois fluxos de trabalho (Verilog e
processador SAPHO), o que aparece em cada console, os atalhos e as
preferências. Quem vem da AURORA encontra os mesmos botões com os mesmos
nomes e teclas (C± F6, Verilog F7, Wave F8, PRISM F10).

Sumário:

1. [Antes de começar](#1-antes-de-começar)
2. [A janela](#2-a-janela)
3. [Projeto Verilog](#3-projeto-verilog)
4. [Processador SAPHO](#4-processador-sapho)
5. [Os botões do fluxo](#5-os-botões-do-fluxo)
6. [Consoles, problemas e terminal](#6-consoles-problemas-e-terminal)
7. [Esquemático, síntese e relatórios](#7-esquemático-síntese-e-relatórios)
8. [Ferramentas do Lace](#8-ferramentas-do-lace)
9. [Preferências](#9-preferências)
10. [Atalhos](#10-atalhos)
11. [Problemas comuns](#11-problemas-comuns)

---

## 1. Antes de começar

O Studio usa as ferramentas do Lace, e só elas: precisa do Lace instalado
com o bundle ([instalação do Lace](https://github.com/ART3121/lace#instalar)).
Ele acha a instalação sozinho; o estado aparece na tela inicial e na barra de
status (`Bundle 2026.09.29`, ou `Lace não encontrado`). Se o Lace estiver
numa pasta fora do padrão, indique-a em Preferências > Bundle do Lace.

Os projetos são os `.spf` da AURORA: um projeto da AURORA abre no Studio, e
o que o Studio grava continua legível pela AURORA. Os temporários ficam em
`.lace/` (a AURORA usava `.aurora/`).

## 2. A janela

```
 menus        Arquivo Editar Exibir Projeto Fluxo Ferramentas Ajuda
 ferramentas  Novo Abrir Salvar | Alvo [Projeto] | C± Verilog Wave Rápida Onda PRISM | Parar
 ┌──┬────────────────┬──────────────────────────────────────────────┐
 │  │ barra lateral  │ abas: arquivos e vistas                       │
 │at│ (explorador,   │                                              │
 │iv│  fluxo, busca, │                                              │
 │id│  relatórios)   ├──────────────────────────────────────────────┤
 │ad│                │ painel: C±, ASM, Verilog, Wave, PRISM, Lace, │
 │es│                │ Problemas, Terminal                          │
 └──┴────────────────┴──────────────────────────────────────────────┘
 status       projeto, topo, testbench | operação ou último resultado | problemas, cursor, simulador, bundle
```

- **Barra de atividades**, à esquerda: Explorador, Fluxo, Busca e
  Relatórios em cima; Ferramentas do Lace e Preferências embaixo. Clicar na
  vista aberta esconde a barra lateral (Ctrl+B faz o mesmo).
- **Explorador**, com três modos (seção 2.1): *Fontes*, *Hierarquia* e
  *Arquivos*. Os botões de cada modo ficam na linha do título, à direita.
- **Fluxo**: as etapas do projeto em ordem, como o Flow Navigator do
  Vivado, cada uma com um ponto que diz como terminou a última execução.
- **Abas**: um clique no explorador abre o arquivo numa aba provisória
  (título em itálico), que a próxima abertura troca; editar ou dar duplo
  clique a fixa. Vistas como o esquemático e as preferências também abrem
  em abas. O editor se divide em até três grupos lado a lado (seção 2.2).
- **Painel inferior**: os consoles de cada etapa, Problemas e o terminal.
  Ctrl+J esconde e mostra; o botão de maximizar dá a altura toda a ele.
- **Barra de status**: o que está sendo feito agora e há quanto tempo, ou
  como terminou a última operação. Clicar nos contadores de problemas abre
  o painel Problemas; clicar no bundle abre as Ferramentas do Lace.

Todo divisor entre as áreas pode ser arrastado.

### 2.1 Explorador

**Fontes** mostra o projeto como o Lace o vê:

- *Módulos*: os sintetizáveis registrados, com o topo marcado pelo nome do
  módulo, e o Verilog que o build de cada processador gerou
  (`<proc>/Hardware/<proc>.v`), marcado como **Gerado**.
- *Testbenches*: os registrados, com o simulado marcado, e o testbench que o
  YANC gerou para cada processador (`<proc>/Simulation/<proc>_tb.v`).
- *Processadores SAPHO*: cada um com o **Programa** (o fonte e o assembly
  gerado), as **Memórias** (`.mif`), a **Simulação** (entradas, saídas e a
  onda) e, recolhidos, os **Intermediários do YANC** (logs e traduções da
  pasta temporária).
- *Fora do projeto*: os `.v` da pasta que ainda não estão no `.spf`.

**Hierarquia** mostra a árvore de instâncias como o Icarus a elabora: em
*Design*, os módulos que ninguém instancia (o topo, marcado) com tudo o que
eles instanciam; em *Simulação*, cada testbench com o que ele instancia. Os
módulos da biblioteca SAPHO aparecem apagados, marcados SAPHO e fechados;
instâncias dentro de blocos `generate` levam o caminho do bloco no nome
(`op_add.my_add`). Clicar abre a definição do módulo; o menu leva também à
linha da instância e copia o caminho hierárquico. A hierarquia atualiza
sozinha depois de cada Verilog (F7), simulação, build ou síntese que passa,
e o botão do título elabora na hora. Quando um `.v` muda depois da última
elaboração, o aviso no topo diz que ela está desatualizada. Uma elaboração
que falha (um testbench com erro de sintaxe, por exemplo) aparece com o erro
e a linha, e não esconde as outras. Ela precisa do Icarus instalado,
qualquer que seja o simulador escolhido. É a mesma árvore do
`lace hierarchy`.

**Arquivos** mostra a pasta do projeto: criar, renomear (F2), mandar para a
lixeira (Delete), mostrar os ocultos. Renomear e mover são o `lace move`:
um `.v` registrado continua no projeto com o nome novo. O topo e o testbench simulado têm a
marca deles, e um ponto azul marca os outros arquivos registrados no `.spf`.
O arquivo da aba ativa aparece na árvore, com as pastas dele abertas, e as
pastas que você abre ficam lembradas por projeto.

**Arrastar e soltar:**

| De | Para | O que acontece |
|---|---|---|
| arquivo ou pasta da árvore de arquivos | uma pasta (ou o espaço vazio, para a raiz) | move; os `.v` registrados continuam registrados, com o topo e o testbench escolhido acompanhando |
| um `.v` das Fontes | Módulos ou Testbenches | muda o papel do arquivo |
| arquivos do gerenciador de arquivos do sistema | uma pasta da árvore de arquivos | copia para lá |
| `.v` ou `.sv` do sistema | Módulos ou Testbenches | copia para a raiz do projeto, se vier de fora, e registra com esse papel |
| um `.spf` do sistema | qualquer lugar | abre o projeto |
| outros arquivos do sistema | o editor | abrem em abas |

Com um arquivo do mesmo nome no destino, o Studio pergunta antes de
substituir, e o que estava lá vai para a lixeira. O `.spf`, a pasta `.lace`
e as pastas dos processadores não se movem: o Lace os acha pelo lugar.

Nas três vistas, as setas andam pela árvore: cima e baixo de linha em
linha, direita abre (ou entra), esquerda fecha (ou sobe para o pai), Home e
End vão ao começo e ao fim, Enter abre e Ctrl+Enter abre no grupo do editor
ao lado.

O último botão da linha do título recolhe a árvore inteira e, clicado de
novo, expande tudo. Nas Fontes e na Hierarquia vale para as seções e os
nós; na árvore de arquivos, para as pastas (o "expandir" abre até 400,
contando só as ocultas que estiverem à mostra). Para esconder a barra
lateral inteira, clique de novo no ícone da vista aberta ou use Ctrl+B.

### 2.2 Editor dividido

O editor se divide em até três grupos lado a lado, como os painéis da
AURORA. Cada grupo tem as suas abas e o seu editor; o grupo com o foco tem
a aba ativa marcada em azul, e é dele que falam os atalhos (Ctrl+S, Ctrl+W,
localizar) e a barra de status.

| Para | Faça |
|---|---|
| abrir a aba ativa também à direita | `Ctrl+\`, o botão de colunas no fim da barra de abas, ou "Dividir à direita" no menu da aba |
| abrir um arquivo do explorador ao lado | "Abrir ao lado" no menu do arquivo, ou Ctrl+Enter na árvore |
| mudar uma aba de grupo | arraste a aba para a barra de abas de outro grupo (na posição) ou para o conteúdo dele (no fim); ou "Mover para o grupo da direita/esquerda" no menu da aba |
| criar um grupo arrastando | solte a aba na faixa da direita do conteúdo de um grupo (enquanto houver menos de três) |
| ir para o grupo 1, 2 ou 3 | Ctrl+1, Ctrl+2, Ctrl+3, ou clique nele |
| fechar um grupo | o X no fim da barra de abas dele, ou Exibir > Fechar o grupo do editor |

O mesmo arquivo pode estar em dois grupos: é o mesmo texto nos dois, o que
se digita num aparece no outro, e cada um guarda o seu cursor e a sua
rolagem. Fechar a aba num grupo não pergunta nada enquanto o arquivo estiver
aberto em outro; a pergunta de salvar vem quando ele sai do último. Uma
vista (preferências, esquemático, relatórios) fica num grupo só: dividir a
leva para o lado. Um grupo que fica sem abas desaparece, e os grupos
abertos voltam com o projeto.

### 2.3 Modo zen

Ctrl+K Z (aperte Ctrl+K, solte, aperte Z), Exibir > Modo zen ou a paleta.
Some tudo menos o editor: barras de menus, de ferramentas, de atividades e
de status, a barra lateral, o painel inferior e as abas. A janela entra em
tela cheia e, com um grupo só, o editor fica numa coluna de umas 110
colunas no meio da tela, sem o minimapa.

| Para | Faça |
|---|---|
| sair | Ctrl+K Z de novo, ou Esc duas vezes seguidas |
| usar o terminal | Ctrl+` abre o shell numa gaveta embaixo do editor, o mesmo do painel (o `lace` está no `PATH`); Ctrl+` de novo fecha e o foco volta ao editor. A divisa entre os dois se arrasta |
| trocar de arquivo | Ctrl+P, ou a paleta (Ctrl+Shift+P) |
| rodar o fluxo | F5 a F10, como sempre |
| ver o painel ou a barra lateral | Ctrl+J, Ctrl+B; valem só enquanto o zen durar |

Durante uma operação, um indicador pequeno no canto inferior direito mostra
a etapa e o tempo; ao terminar, mostra o resultado e os erros e avisos por
alguns segundos, e clicar nele abre Problemas. Os marcadores de erro
continuam no editor. Com a gaveta aberta o indicador sobe para o canto de
cima.

O Esc duas vezes não sai do zen com o foco no terminal (o Esc é do shell,
de programas como `vim` e `less`) nem no editor com o modo Vim ligado (o Esc
é do Vim); nesses casos, Ctrl+K Z. Ao sair, a barra lateral, o painel e a
janela voltam como estavam; se a janela já estava em tela cheia antes,
continua. O Studio sempre abre fora do zen.

Preferências > Modo zen: entrar em tela cheia, centralizar o editor,
mostrar as abas, esconder os números de linha.

## 3. Projeto Verilog

1. **Arquivo > Novo projeto** (Ctrl+Alt+N): nome e pasta. O Studio cria
   `<pasta>/<nome>/<nome>.spf` e abre o projeto. O nome usa só letras sem
   acento, números, `_` e `-`, começando por letra: ele vira pasta, `.spf`
   e parte do caminho que as ferramentas recebem. A regra é a do Lace
   (`lace new` recusa igual), e o diálogo avisa enquanto você digita.
2. **Projeto > Novo módulo Verilog**: dê o nome do arquivo (`contador.v`).
   O Lace cria o módulo a partir do modelo; o primeiro módulo vira o topo.
3. Escreva o módulo e salve (Ctrl+S).
4. **Projeto > Novo testbench**: o nome sugerido é `<topo>_tb.v`. Um
   testbench `X_tb.v` ou `tb_X.v` já sai instanciando o módulo `X` com todas
   as portas, com clock e reset se ele tiver `clk` e `rst`, gravando a onda
   e terminando com `$finish`.
5. **Verilog** (F7): verifica o design e cada testbench com o Icarus. Os
   erros aparecem no console Verilog, no painel Problemas e sublinhados no
   editor.
6. **Wave** (F8): simula e abre a onda no surfer-aurora, numa aba (seção
   4.1). O que o testbench escreve (`$display`) aparece no console Wave
   enquanto roda.
7. **PRISM** (F10): sintetiza o topo com o Yosys e abre o esquemático.

Arquivos que já existem entram por **Projeto > Adicionar arquivos Verilog**
ou arrastando-os para Módulos ou Testbenches, no explorador (seção 2.1). O
Lace decide pelo conteúdo se cada um é módulo ou testbench (a regra da
AURORA); soltar em Testbenches marca o arquivo como testbench. Um arquivo de
fora da pasta do projeto é registrado no lugar, sem cópia: no mesmo
repositório git do projeto, o `.spf` guarda o caminho relativo
(`../../rtl/x.v`), que vale em qualquer clone; fora de repositório, o
absoluto.

**A ordem dos arquivos** em Módulos e em Testbenches é a ordem em que os
compiladores os leem, e um `` `define `` só vale para os arquivos de baixo
dele (a verificação acusa a macro indefinida). Para mudar, arraste o
arquivo sobre outro da mesma seção (ele entra antes desse), ou use **Subir
na lista** e **Descer na lista** no menu do arquivo.

**O topo** pode ser qualquer arquivo Verilog, inclusive o `.v` que o build
de um processador gerou (`<proc>/Hardware/<proc>.v`) e um `.v` que ainda não
está no projeto, que passa a ser registrado. A exceção é o nome de
testbench: `tb_<nome>.v`, `<nome>_tb.v` e `tb.v` não viram topo. Escolha
pelo botão direito no arquivo (nas Fontes, na árvore de arquivos ou na
hierarquia) ou por **Projeto > Escolher o topo**, que lista todos os
candidatos e diz de onde cada um vem. O `.v` gerado escolhido como topo
continua sendo refeito a cada build; registrado, ele aparece uma vez só em
Módulos, com as marcas Gerado e Topo. Para trocar o topo ou o testbench
simulado, clique com o botão direito no arquivo, no explorador.

## 4. Processador SAPHO

1. Crie ou abra um projeto.
2. **Projeto > Novo processador SAPHO** (Ctrl+Alt+P): nome (letras,
   números e `_`, sem começar por número), linguagem (C± ou C), portas de
   entrada e de saída e, em C±, a arquitetura (largura, mantissa, expoente,
   ganho, pilhas). Os padrões são os da AURORA. O Studio abre o fonte criado.
3. Escreva o programa. `#PRNAME` precisa ser o nome do processador: o Lace
   confere antes de compilar.
4. Entradas: no explorador, botão direito no processador > **Novo arquivo
   de entrada**. É o `Simulation/input_<n>.txt` da porta `n` de `in(n)`, um
   valor por linha.
5. Escolha o processador no **Alvo** da barra de ferramentas. Com o alvo
   num processador, os botões agem só nele; com o alvo em "Projeto", agem no
   projeto inteiro (todos os processadores e o testbench do projeto).
6. **C±** (F6): compila (cmmcomp, appcomp, asmcomp). Os erros do YANC
   apontam a linha do fonte.
7. **Wave** (F8) ou **Rápida** (F9): simula o processador com o testbench
   que o YANC gerou. No fim, o console Wave mostra os valores de cada porta
   de saída (`Saída 0: 55`), e a aba do processador também.

A **aba do processador** (duplo clique nele no explorador) tem a frequência,
o número de clocks, a exportação de arrays para a onda e o tempo simulado
que eles dão, as entradas e as saídas da última simulação. A frequência e os
clocks valem a partir do próximo build.

### 4.1 A onda

Wave (F8), ou Onda (Ctrl+F8) depois de simular, abre a onda no Surfer. A
onda de um processador abre arrumada como na AURORA: um grupo por
processador com o `clk` e o `rst`, as portas (I/O), as **instruções** e as
**variáveis** do programa, e as flags (pilhas e ULA, fechadas).

- **Assembly**: a instrução que o processador executa em cada ciclo
  (`LOD_V main_x 2`, `SET main_soma`), no lugar do número do PC.
- **C±**: a linha do fonte daquela instrução, com o texto da linha.
- **Variáveis**: `int` com sinal, `float` como número real, complexo como
  `re imi`; cada array num grupo fechado (só com a exportação de arrays
  ligada, na aba do processador).

O layout sai das tabelas que o YANC deixa ao compilar; sem elas (um
processador compilado fora do Lace), o PC e a linha aparecem como números.
Também aparecem como números quando o processador foi compilado de novo
depois da simulação, porque as tabelas novas deslocariam o assembly e a
linha: o resumo da aba avisa "compilado depois desta simulação", e basta
simular de novo. A onda de um projeto só de Verilog abre com o grupo
Top-level, com os sinais do testbench; o resto do design fica na
hierarquia do Surfer (só ondas VCD; a `.fst` que o Lace grava quando o
testbench não tem `$dumpfile` abre sem grupos).

Onde a onda abre é uma preferência (Preferências > Simulação): **numa
aba** do Studio (o padrão) ou **em janela separada**. Na aba, o Surfer roda
dentro do Studio, com os menus e os atalhos dele; com o foco na aba, letras,
setas, espaço e as teclas de edição são do Surfer, e as teclas de função
(F5 a F10) e as combinações com Ctrl, Alt ou Cmd continuam sendo do Studio
(Ctrl+W fecha a aba, Ctrl+B esconde a barra lateral); a F11 e o Ctrl+K
ficam com o Surfer. No alto da aba ficam o arquivo, o
resumo de cada processador, **Ler a onda de novo** e **Abrir em janela**.
Trocar de aba não perde nada: a aba de onda guarda o zoom, o cursor e os
sinais acrescentados enquanto estiver aberta (cada aba de onda aberta ocupa
memória; feche as que não usa). Simular de novo recarrega a aba aberta da
onda. A aba precisa do cliente
web do Surfer no bundle e de uma onda de até 256 MB; fora disso, a onda abre
em janela.

## 5. Os botões do fluxo

| Botão | Tecla | Faz | Comando equivalente do Lace |
|---|---|---|---|
| C± | F6 | compila os processadores (o alvo, ou todos) | `lace build [-p NOME]` |
| Verilog | F7 | compila os processadores que têm fonte e verifica o Verilog; com um processador no alvo, compila só ele e verifica o Verilog dele com o testbench do YANC | `lace check [-p NOME]` |
| (menu Fluxo) | Shift+F7 | o mesmo, com o lint do Verilator | `lace check [-p NOME] --lint` |
| Wave | F8 | compila, simula e abre a onda | `lace sim [-p NOME] --open` |
| Rápida | F9 | compila e simula, sem abrir a onda | `lace sim [-p NOME]` |
| Onda | Ctrl+F8 | abre a onda da última simulação | `lace wave [-p NOME]` |
| PRISM | F10 | compila, sintetiza e desenha o esquemático | `lace synth [-p NOME] --svg` |
| (menu Fluxo) | F5 | verifica e, se passar, simula | |
| Parar | Shift+F5 | cancela a operação; a ferramenta é encerrada com tudo o que iniciou. A atualização do Lace não para no meio | Ctrl+C na CLI |

Antes de cada operação, o Studio salva os arquivos abertos: compila o que
está na tela. Uma operação por vez; os botões ficam desabilitados enquanto
uma roda. O simulador (Icarus ou Verilator) é escolhido no menu Fluxo ou
em Preferências > Simulação.

Um botão que não pode fazer nada fica desabilitado, em vez de dar erro:
Fluxo > Verilator e o lint (Shift+F7) sem o Verilator instalado, Onda antes
de existir a onda do alvo.

Um testbench sem `$finish` não termina sozinho: use Parar, ou defina um
prazo em Preferências > Simulação.

Cada operação grava um relatório no histórico do projeto (`.lace/reports/`),
como a CLI. O número aparece no fim do console.

## 6. Consoles, problemas e terminal

Cada etapa escreve no seu console, com os nomes da AURORA:

| Console | Na AURORA | O que aparece |
|---|---|---|
| C± | TCMM | o compilador de C± ou de C (cmmcomp, cpppp, cppcomp) |
| ASM | TASM | o pré-montador e o montador (appcomp, asmcomp) e o resumo do build |
| Verilog | TVERI | a verificação do Icarus e o lint do Verilator |
| Wave | TWAVE | a elaboração e a simulação: o que o testbench escreve, as saídas do processador, onde ficou a onda |
| PRISM | TPRISM | o Yosys e o Graphviz |
| Lace | | os comandos que o Studio rodou, a instalação de componentes |

O painel troca sozinho para o console da etapa que está rodando (a não ser
que você esteja no Terminal). Uma aba com saída nova ganha um ponto. Um
`arquivo:linha` no console é um link: clique para abrir o arquivo na linha.
Com a preferência "Mostrar o comando de cada passo", aparece também a linha
de comando exata de cada ferramenta, para repetir à mão.

**Problemas** lista os erros e avisos da última operação, com filtro;
clicar abre o arquivo na linha. As mensagens `info` aparecem marcando a
caixa "info". No topo ficam os avisos do `.spf` do projeto aberto (ferramenta
`.spf`), que valem enquanto ele está aberto: um caminho de outra máquina
(`C:\...`) achado dentro da pasta do projeto, que o Lace grava como o de
hoje na próxima mudança; um topo ou testbench escolhido fora da lista, que é
ignorado; um processador com nome que não compila.

**Terminal** é um shell de verdade, aberto na pasta do projeto, com o
`lace` no `PATH`: `lace status`, `lace report compare`, tudo o que a CLI
faz. O botão `+` abre outro no lugar do atual. No Linux e no macOS é o seu
shell (a variável `SHELL`). No Windows, Preferências > Terminal escolhe
entre o PowerShell (o padrão) e o Prompt de Comando (`cmd`); trocar
reinicia o terminal aberto com o shell novo.

## 7. Esquemático, síntese e relatórios

- **Esquemático**: o SVG do `show` do Yosys desenhado pelo Graphviz. Roda do
  mouse dá zoom, arrastar move, duplo clique ajusta à janela. O seletor
  troca o módulo desenhado sem sintetizar de novo; "Largura dos
  barramentos" escreve os bits em cada aresta. O botão de abrir manda o SVG
  para o programa do sistema. Um módulo com mais de 120 ligações não é
  desenhado, porque o Graphviz levaria minutos: a síntese e as estatísticas
  saem mesmo assim, e o seletor deixa escolher um submódulo.
- **Síntese** (Fluxo > Estatísticas da síntese): células, fios, memórias e
  processos do `stat` do Yosys, e as células por tipo. São células
  genéricas, sem mapeamento para FPGA: não há LUT, DSP nem temporização.
  "não informado" é diferente de zero.
- **Relatórios** (barra de atividades): o histórico do projeto, do mais novo
  para o mais antigo. Abrir mostra o relatório (máquina, ferramentas e
  versões, tempo de cada passo, estatísticas); **Comparar** mostra a
  diferença de síntese e de tempos contra o anterior comparável, com os
  avisos de contexto (fontes, versões, máquina diferentes).
- **Limpar relatórios** (a lixeira no título dos Relatórios, ou Ferramentas
  > Limpar relatórios): o `lace report clean`. Escolha manter os N mais
  novos ou apagar todos; o diálogo mostra quais saem antes de apagar. O
  menu de cada relatório (botão direito) apaga só ele. O número de um
  relatório apagado não volta a ser usado. Com uma operação rodando, a
  limpeza espera.

## 8. Ferramentas do Lace

A aba **Ferramentas do Lace** (chave inglesa na barra de atividades) é o
`lace tools` com botões:

- o bundle, a plataforma, a pasta e como o Studio a achou;
- os componentes instalados e os que faltam, com **Instalar** (roda o
  `lace install` da sua instalação; o progresso aparece no console Lace);
- cada executável e onde está;
- o compilador do Verilator (do sistema no Linux e no macOS, do bundle no
  Windows);
- **Conferir os hashes**: o `lace tools --verify`;
- **Procurar atualizações**: o `lace update --check`, com a versão
  instalada, a da última release e a upstream de cada componente. O
  resultado aparece no alto da tela;
- **Atualizar para X**: aparece depois da busca, quando há um Lace mais
  novo publicado. Pede confirmação e roda o `lace update` da sua
  instalação. No Linux e no macOS, o instalador da release troca o `lace`
  e o bundle inteiro, com os mesmos componentes, e a saída dele aparece no
  console Lace. No Windows, abre o assistente de instalação; depois de
  terminar nele, clique em **Atualizar** no alto da tela para reler o
  bundle.

A atualização não para no meio: o Parar some e o Shift+F5 não faz nada
enquanto ela roda, porque parar o `lace` não pararia o instalador que ele
abriu. Um `lace` que não veio do instalador (um build local) não se
atualiza por aqui. O Lace Studio não é atualizado junto: se o bundle novo
vier num formato de manifesto que esta versão do Studio não lê, a tela de
ferramentas mostra o erro, e o Studio também precisa ser atualizado.

## 9. Preferências

Ctrl+, ou a engrenagem. Tudo vale na hora e fica gravado.

| Grupo | Preferência |
|---|---|
| Geral | idioma (do sistema, português, inglês), reabrir o último projeto |
| Aparência | o tema (ver abaixo) |
| Terminal | no Windows, o shell: PowerShell ou Prompt de Comando (`cmd`) |
| Editor | tamanho da fonte, tabulação, quebra de linha, minimapa, salvar ao trocar de aba, modo Vim |
| Modo zen | tela cheia, centralizar o editor, mostrar as abas, esconder os números de linha (seção 2.3) |
| Simulação | simulador padrão, onde abrir a onda (numa aba ou em janela separada), abrir a onda depois de simular (o botão Wave), prazo da simulação, consoles detalhados |
| Bundle do Lace | a pasta do bundle (o `--toolchain` da CLI) e a do compilador do Verilator (o `--compiler`) |

As preferências ficam em `settings.json`, na pasta de configuração do
aplicativo: `~/.config/com.nipscern.lace-studio/` no Linux,
`%APPDATA%\com.nipscern.lace-studio\` no Windows,
`~/Library/Application Support/com.nipscern.lace-studio/` no macOS. O
caminho exato aparece em Ajuda > Sobre.

### Temas

Em Preferências > Aparência, cada tema tem um cartão com uma prévia nas
cores dele; clicar aplica na hora. Também dá para escolher em Exibir >
Selecionar tema, que abre a lista na paleta. O tema pinta a interface, o
editor (inclusive C± e o assembly) e os consoles.

| Tema | Esquema | Origem |
|---|---|---|
| Atlas (padrão) | escuro | o do Studio: cinzas neutros, azul do CERN nos detalhes |
| Atlas Branco | claro | o Atlas claro |
| Aurora Legacy | escuro | as cores da AURORA |
| Dark Modern, Light Modern | escuro, claro | os padrões do VS Code |
| Dracula | escuro | |
| Gruvbox Dark, Gruvbox Light | escuro, claro | |
| Monokai | escuro | |
| Nord | escuro | |
| One Dark Pro | escuro | o do Atom |
| Solarized Dark, Solarized Light | escuro, claro | |
| Catppuccin Mocha | escuro | |
| Tokyo Night | escuro | |

**Do sistema** fica com o Atlas quando o sistema está no modo escuro e com o
Atlas Branco no claro, e troca junto com ele. Exibir > Alternar tema claro e
escuro vai para o par do tema (Atlas e Atlas Branco, Dark e Light Modern,
Gruvbox, Solarized) ou, num tema sem par, para o Atlas Branco ou o Atlas.

## 9.1 Sobre

Ajuda > Sobre abre uma aba com a versão do Studio, a do bundle do Lace e a
de cada componente (com os que faltam instalar), as do Tauri e do motor de
páginas do sistema (WebKitGTK, WebView2 ou WKWebView), as pastas das
preferências e do bundle (o botão ao lado abre a pasta) e os links da
documentação do Lace, do manual do SAPHO e do NIPS-CERN. **Copiar
informações** põe tudo isso, em inglês, na área de transferência, para
anexar a um relato de problema.

## 10. Atalhos

No macOS, Ctrl é Cmd. A lista também está em Ajuda > Atalhos de teclado.

| Ação | Teclas |
|---|---|
| Paleta de comandos | Ctrl+Shift+P |
| Abrir arquivo do projeto pelo nome | Ctrl+P |
| Novo projeto, abrir projeto | Ctrl+Alt+N, Ctrl+Shift+O |
| Novo processador | Ctrl+Alt+P |
| Novo arquivo, abrir arquivo | Ctrl+N, Ctrl+O |
| Salvar, salvar todos | Ctrl+S, Ctrl+Shift+S |
| Fechar aba, reabrir aba fechada | Ctrl+W, Ctrl+Shift+T |
| Dividir o editor, ir para o grupo 1, 2, 3 | `Ctrl+\`, Ctrl+1, Ctrl+2, Ctrl+3 |
| Abrir ao lado (na árvore do explorador) | Ctrl+Enter |
| Preferências | Ctrl+, |
| C±, Verilog, lint | F6, F7, Shift+F7 |
| Wave, Rápida, abrir a onda | F8, F9, Ctrl+F8 |
| PRISM | F10 |
| Fluxo completo, Parar | F5, Shift+F5 |
| Localizar, substituir, ir para a linha | Ctrl+F, Ctrl+H, Ctrl+G |
| Localizar nos arquivos | Ctrl+Shift+F |
| Formatar documento | Shift+Alt+F |
| Explorador, barra lateral, painel inferior | Ctrl+Shift+E, Ctrl+B, Ctrl+J |
| Terminal, Problemas | Ctrl+`, Ctrl+Shift+M |
| Modo zen (entrar e sair) | Ctrl+K Z; Esc Esc também sai |
| Histórico de relatórios | Ctrl+Shift+H |
| Zoom | Ctrl+=, Ctrl+-, Ctrl+0 |

Dentro do Terminal, as teclas com Ctrl sozinho (Ctrl+C, Ctrl+W, Ctrl+P) vão
para o shell; só as teclas de função e as combinações com Ctrl+Shift
continuam do Studio.

### Modo Vim

Editar > Alternar modo Vim, ou a caixa nas Preferências. O editor passa a
usar as teclas do Vim (o `monaco-vim`, que adapta as do CodeMirror), e a
barra de status mostra o modo (`--NORMAL--`, `--INSERT--`, `--VISUAL--`) e
a linha de comando. Além dos comandos do Vim:

| Comando | Faz |
|---|---|
| `:w` | grava a aba |
| `:wa` | grava todas |
| `:q` | fecha a aba |
| `:wq`, `:x` | gravam e fecham |

Com o Vim ligado e o foco no editor, Ctrl+B e Ctrl+O ficam com o Vim
(página para cima, voltar no histórico de saltos) em vez de esconder a
barra lateral e abrir arquivo. Os outros atalhos do Studio continuam. No
modo zen, a linha do Vim aparece no indicador do canto, porque a barra de
status está escondida; Esc duas vezes não sai do zen, Ctrl+K Z sai.

## 11. Problemas comuns

| Sintoma | O que fazer |
|---|---|
| "Lace não encontrado" | instale o Lace, ou indique a pasta do bundle em Preferências > Bundle do Lace (a que tem `bundle.json`) |
| "Falta um componente do bundle" | Ferramentas do Lace > Não instalados > Instalar |
| A simulação não termina | o testbench chega ao `$finish`? Use Parar, ou defina um prazo em Preferências > Simulação |
| "O projeto não tem módulo de topo" | botão direito num módulo, no explorador > Definir como topo; ou Projeto > Escolher o módulo de topo |
| "O processador ainda não foi compilado" | C± (F6) com o processador no alvo |
| O Verilator falha por falta de compilador | instale g++ (ou clang++), make e Perl; ou indique a pasta deles em Preferências > Compilador do Verilator |
| O surfer-aurora fecha logo ao abrir | sem display gráfico, ou onda inválida; o log dele fica em `.lace/Temp/surfer/<onda>.log` |
| A aba da onda diz que o bundle não traz o cliente web | o bundle é anterior ao cliente web do Surfer; a onda abre em janela até um bundle novo |
| A aba da onda recusa uma onda grande | acima de 256 MB a onda abre em janela (o ícone de janela, no alto da aba) |
| "O arquivo mudou no disco" ao salvar | outro programa alterou o arquivo; escolha sobrescrever ou recarregar |
| "O fonte do processador não está como o YANC espera" | o arquivo precisa se chamar `<processador>.cmm` e ter `#PRNAME <processador>` |
| "O caminho é longo demais para o YANC" | no Windows o YANC aceita até 259 caracteres; mova o projeto para uma pasta mais curta |
