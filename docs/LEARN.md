# `lace learn`: exercícios de Verilog

O `lace learn` é uma trilha de exercícios curtos de Verilog, no estilo do
rustlings: cada exercício pede um módulo, com as portas já escritas, e o
Lace corrige a cada gravação, simulando o módulo ao lado de uma solução de
referência com as mesmas entradas. Quando uma saída difere, ele diz em
quantas amostras, em que instante foi o primeiro erro, e mostra a onda das
duas versões lado a lado.

O código está no Lace (o crate `lace-learn`, usado pela CLI e pelo Studio).
Os exercícios vêm no componente `lace-learn` do bundle, que é opcional.

## Para quem faz os exercícios

### Instalar

```
lace install lace-learn
```

ou marque "Lace Learn" no instalador. O componente exige o Icarus.

### Na linha de comando

```
lace learn init          # cria a pasta lace-learn/ com todos os exercícios
cd lace-learn
lace learn               # o modo watch
```

O modo watch é uma tela de terminal: no alto, o progresso da trilha; embaixo
dele, o capítulo, o exercício e o arquivo a editar; no meio, o enunciado com
cores (os títulos, as tabelas alinhadas, o código Verilog destacado e as
dicas pedidas), que rola, e ao lado dele (ou embaixo, num terminal estreito)
o resultado; no rodapé, as teclas que valem naquela hora.

Ele não corrige ao abrir nem ao trocar de exercício: o resultado começa em
"not checked yet" e vem quando você grava o arquivo do exercício (em
qualquer editor) ou aperta `r`. A correção roda sem travar a tela, com uma
animação enquanto dura, e o resultado mostra o veredito num selo colorido,
uma linha por saída que errou (quantas amostras, o primeiro erro, quantas em
X ou Z), os erros de compilação com arquivo e linha, a causa provável e o que
fazer em seguida.

| Tecla | Faz |
|---|---|
| gravar o arquivo | corrige |
| `r` | corrige agora |
| `h` | mostra a próxima dica, no fim do enunciado |
| `n` | passa ao próximo exercício por resolver (só depois de resolver o atual) |
| `l` | a lista dos exercícios por capítulo, com os resolvidos: setas ou `j`/`k` andam, Enter abre, `x` restaura o arquivo do selecionado, Esc fecha |
| `w` | abre a onda da última correção no surfer-aurora |
| `c` | corrige todos, com o progresso numa janela (Esc para) |
| `x` | restaura o arquivo do exercício ao começo (pergunta antes) |
| setas, PgUp, PgDn, Home, End | rolam o enunciado |
| `?` | mostra as teclas |
| `q`, Ctrl+C | sai |

Com `NO_COLOR` definida, a tela sai sem cores. Fora do Windows, sem
`COLORTERM=truecolor` (ou `24bit`), as cores saem da paleta de 256 do xterm,
que os terminais sem cores de 24 bits também mostram.

Os subcomandos fazem cada coisa uma vez, para scripts ou sem terminal
interativo. Todos aceitam `--json`, com o schema em `docs/schema/learn-*.json`,
e todos acham a pasta de exercícios a partir da pasta atual (ou do `-C`),
como os outros comandos acham o projeto.

| Comando | Faz |
|---|---|
| `lace learn init [PASTA] [--track verilog]` | cria a pasta de exercícios (padrão `lace-learn`); recusa uma que exista e não esteja vazia |
| `lace learn check [NOME]` | corrige o exercício (sem nome, o atual) e grava se está resolvido; sai com 0 se está |
| `lace learn check --all` | corrige todos |
| `lace learn list` | os exercícios por capítulo, com o atual (`*`) e os resolvidos (`done`) |
| `lace learn hint [NOME]` | as dicas do exercício |
| `lace learn reset NOME [--yes]` | volta o arquivo do exercício ao começo; sem terminal, exige `--yes` |
| `lace learn wave [NOME]` | abre a onda da última correção, arrumada |
| `lace learn dev check [TRILHA]` | confere uma trilha (para quem escreve exercícios, abaixo) |

### No Lace Studio

A vista Exercícios (Exibir > Exercícios, ou Ferramentas > Exercícios) cria
ou abre a pasta de exercícios e lista os capítulos, com o que já foi
resolvido. Escolher um exercício abre o projeto dele, o arquivo à esquerda e
o enunciado à direita, sem corrigir: a correção vem quando você grava o
arquivo (a chave "Corrigir ao gravar" desliga isso) ou pede, com Ctrl+Alt+L
ou o botão Corrigir.

Com um exercício aberto, o Studio fica numa sessão de exercícios: trocar de
aba não grava sozinho, mesmo com a preferência "Salvar ao trocar de aba ou
de janela" ligada, e o painel de baixo (consoles, Problemas, terminal) fica escondido
até você chamá-lo (Ctrl+J). Abrir outro projeto, ou fechar a pasta de
exercícios, encerra a sessão, e o painel volta como estava.

Na aba do enunciado ficam a correção (o veredito, uma linha por saída que
errou, os erros de compilação, que levam à linha) e os botões: a próxima
dica, a onda (na aba, já com as saídas lado a lado e o primeiro erro
marcado), o esquemático do seu circuito no PRISM, restaurar o arquivo e,
depois de resolvido, a solução de referência e o próximo exercício. Os
erros também vão para Problemas e para o editor, como os de qualquer
verificação.

Sem o componente instalado, a vista oferece instalá-lo.

### A pasta de exercícios

```
lace-learn/
  .lace-learn.json                   o estado: a trilha, o exercício atual, os resolvidos
  exercises/02_basico/porta_e/       cada exercício é um projeto Lace
    porta_e.spf
    porta_e.v                        o seu arquivo
    README.md                        o enunciado, sem as dicas
    .lace-learn/tb_porta_e.v         o testbench
    .lace-learn/porta_e_ref.v        a referência
    .lace-learn/wave.fst             a onda da última correção
    .lace-learn/wave.sucl            o layout dela
  solutions/02_basico/porta_e.v      a solução, liberada quando o exercício é resolvido
```

O seu arquivo só é escrito quando o exercício é criado e quando você pede
para restaurar. O que está em `.lace-learn/` e o `README.md` são da trilha:
quando o componente é atualizado, o `lace learn` (e o Studio, ao abrir a
pasta) grava de novo o que mudou e cria os exercícios novos. Como cada
exercício é um projeto Lace comum, `lace sim`, `lace wave`, `lace synth` e o
Studio inteiro funcionam nele.

### Como a correção funciona

1. O `check` do projeto do exercício (o `iverilog -tnull -Wall`): um erro de
   compilação para aqui, com arquivo e linha.
2. A simulação no Icarus, com um prazo (10 s, se o exercício não disser
   outro): o testbench aplica as mesmas entradas no seu módulo (`dut`) e na
   referência (`reference`) e compara cada saída a cada amostra.
3. O resumo que o testbench escreve diz quantas amostras houve, quantas
   erraram e, por saída, os erros, o primeiro instante e quantas amostras
   erradas estavam em X ou Z.

Um bit em X na referência aceita qualquer valor; um bit seu em X ou Z com a
referência em 0 ou 1 é erro. O veredito é um destes: `solved`,
`compile_error`, `mismatch`, `timed_out` (um laço que não acaba, ou um laço
combinacional que não estabiliza), `incomplete` (a simulação parou antes do
fim do testbench) ou `cancelled`. Quando dá para dizer, a correção aponta a
causa: o módulo ou uma porta com outro nome (o testbench não achou), uma
saída sempre em X ou Z (não atribuída), ou um erro que só acontece com o
reset ativo (síncrono no lugar de assíncrono, ou o contrário).

Os avisos de `timescale` não aparecem: o testbench declara um, e o seu
arquivo não precisa.

## Para quem escreve exercícios

### A trilha

As trilhas estão em `lace-learn/` neste repositório, uma por pasta, e o
`bundle.py` as copia para o componente `lace-learn`:

```
lace-learn/verilog/            a trilha (o nome da pasta é o id)
  track.json                   {"format": 1}
  track.md                     o título (# na primeira linha) e as boas-vindas
  final.md                     a mensagem do fim (opcional)
  01_primeiros_passos/         um capítulo; o número dá a ordem
    chapter.md                 o título e a introdução
    01_um/                     um exercício; o nome é o da pasta sem o número
      exercise.json            como o testbench gerado testa
      prompt.md                o enunciado e as dicas
      start.v                  o que o aluno recebe
      solution.v               a referência
      tb.v                     um testbench escrito à mão (opcional)
      <outro>.v                módulos dados ao aluno (opcional)
```

O nome do exercício é também o do projeto e, sem `module` no
`exercise.json`, o do módulo: letras, números e `_`, começando por letra,
sem acento, único na trilha e que não seja palavra do Verilog.

### `exercise.json`

| Campo | Padrão | O que é |
|---|---|---|
| `kind` | (obrigatório) | `combinational` ou `sequential` |
| `module` | o nome do exercício | o módulo, quando é outro |
| `stimulus` | exaustivo até 12 bits de entrada | `exhaustive` (todas as combinações, até 16 bits) ou `random` (tudo 0, tudo 1 e amostras aleatórias) |
| `samples` | 200 | as amostras aleatórias |
| `seed` | 1 | a semente do `$random` |
| `clock` | `clk` | sequencial: a entrada de clock |
| `reset` | nenhum | sequencial: `{"name": "reset", "active": 1, "every": 16}`; o reset fica ativo nos dois primeiros ciclos e depois volta em média a cada `every` ciclos (0 desliga) |
| `cycles` | 200 | sequencial: os ciclos |
| `timeout_s` | 10 | o prazo da simulação |

O testbench gerado lê as portas da `solution.v`. No combinacional, cada
amostra aplica as entradas, espera 5 ns, compara e espera mais 5 ns. No
sequencial, o clock tem período de 10 ns, sobe em 5 ns, e cada ciclo compara
duas vezes: 2 ns depois da subida (o estado novo) e 2 ns antes da próxima,
depois de as entradas mudarem na descida (as saídas de Mealy e o reset
assíncrono). As entradas que não são clock nem reset mudam a cada ciclo.

O gerador recusa porta `inout`, clock ou reset que não seja entrada de 1
bit, uma saída `<x>` ao lado de uma porta `<x>_ref`, uma porta chamada
`mismatch` ou começada por `lace_`, e mais de 16 bits de entrada no
estímulo exaustivo.

### `prompt.md`

Markdown simples: títulos, parágrafos, listas, blocos de código
(` ```verilog `), tabelas, código, negrito e itálico no texto. Nada de HTML,
imagem nem link. A primeira linha com texto é o título (`# Título`). As
dicas vêm no fim, cada uma numa seção cujo título começa por "Dica"
(`## Dica`, `## Dica 2`); da primeira dica em diante, cada seção `##` é uma
dica. As seções antes delas (`## Portas`, `## Exemplos`) são do enunciado.

Uma versão em inglês fica ao lado, em `prompt.en.md` (e `chapter.en.md`,
`track.en.md`, `final.en.md`); sem ela, vale o português.

### Os arquivos `.v`

- `start.v` declara o módulo com todas as portas, iguais às da
  `solution.v`, e compila: o `dev check` exige que ele não passe.
- `solution.v` é Verilog-2001 que o Icarus aceita. Cada módulo que ela
  declara ganha `_ref` na cópia da referência, nas declarações e nas
  instâncias.
- Um módulo dado ao aluno (um somador para os exercícios de hierarquia) é
  um `.v` a mais na pasta. Ele vai para a pasta do aluno, entra no projeto e
  é usado pelo aluno e pela referência; nem `start.v` nem `solution.v` o
  declaram.

### Um testbench escrito à mão

Quando o gerado não serve (uma sequência de entrada que importa, um
protocolo), o `tb.v` da pasta do exercício vai no lugar dele, como está.
Ele precisa:

- incluir a referência com `` `include "<módulo>_ref.v" `` e instanciar o
  módulo do aluno e o `<módulo>_ref`;
- gravar a onda com `$dumpfile(".lace-learn/wave.fst")` e `$dumpvars`;
- escrever o resumo no stdout e terminar com `$finish`:

```
LACE-LEARN samples <amostras>
LACE-LEARN mismatched <amostras com alguma saída errada>
LACE-LEARN output <saída> <erros> <primeiro erro em ns, ou -1> <erradas em X ou Z>
LACE-LEARN reset <erradas com o reset ativo>       (opcional)
LACE-LEARN end
```

O que ele escrever além disso aparece para o aluno na correção.

### Conferir

```
LACE_LEARN_DIR=lace-learn lace learn dev check lace-learn/verilog
```

Para cada exercício: as portas de `start.v` e `solution.v` são as mesmas, o
enunciado tem dica, o testbench sai, o `start.v` não passa e a `solution.v`
passa. O teste `crates/lace-learn/tests/track.rs` faz o mesmo para todas as
trilhas do repositório (com `LACE_TEST_BUNDLE`).

`LACE_LEARN_DIR` aponta a pasta das trilhas no lugar do componente, para
testar uma trilha sem instalá-la: `lace learn init`, `lace learn` e os outros
subcomandos passam a usá-la. No Studio, o mesmo é a preferência "Pasta das
trilhas de exercícios" (Preferências > Bundle do Lace), que a vista
Exercícios também oferece quando não acha trilha nenhuma.

Numa build de desenvolvimento (`cargo run`, `npm run tauri dev`), sem
`LACE_LEARN_DIR` e sem a preferência, o Lace e o Studio usam sozinhos a
pasta `lace-learn/` do repositório de onde foram compilados. Com
`LACE_LEARN_DIR` definida e vazia, isso desliga.

### Publicar

As trilhas saem com o Lace: o pacote `lace-learn` de `bundle/versions.json`
tem a versão do Lace e o caminho `lace-learn`, e o componente
`lace-learn` de `bundle/components.json` leva a pasta inteira. Uma pasta de
exercícios criada antes recebe os exercícios novos e os testbenches
corrigidos na primeira vez que é aberta depois da atualização.
