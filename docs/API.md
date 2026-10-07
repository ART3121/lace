# Lace-Core: guia da API

Este guia explica como a API funciona como um todo: conceitos, o que cada
operação executa, o que lê e escreve no disco, o formato dos resultados e dos
erros. A referência item a item está na documentação do código:

```
cargo doc -p lace-core --no-deps --open
```

Todo item público do `lace-core` tem documentação (o crate compila com
`#![deny(missing_docs)]`), e os exemplos da documentação rodam como teste em
`cargo test`.

Sumário:

1. [Conceitos](#1-conceitos)
2. [Toolchain](#2-toolchain)
3. [Projeto](#3-projeto)
4. [Arquivos do projeto](#4-arquivos-do-projeto)
5. [Operações](#5-operações)
6. [Resultados](#6-resultados)
7. [Diagnósticos](#7-diagnósticos)
8. [Erros](#8-erros)
9. [Garantias e limites](#9-garantias-e-limites)
10. [Diferenças em relação à AURORA](#10-diferenças-em-relação-à-aurora)

---

## 1. Conceitos

A API tem quatro peças:

| Peça | Tipo | O que é |
|---|---|---|
| Toolchain | `Toolchain` | O bundle de ferramentas instalado com o Lace, aberto e conferido. É o único lugar de onde o Lace executa algo. |
| Projeto | `Project` | Um `.spf` da AURORA aberto, com seus arquivos Verilog e processadores. |
| Processador | `Processor` | Um processador SAPHO do projeto: nome, linguagem, caminhos, parâmetros de simulação. |
| Operação | funções livres | `build`, `simulate`, `simulate_project`, `check`, `synthesize`, `render_schematic`, `open_waveform`. Cada uma recebe a toolchain e o que operar, e devolve um resultado. As que executam ferramentas, todas menos `open_waveform`, recebem por último um `&Control`, que cancela a operação e mostra o que as ferramentas escrevem enquanto rodam (5.8). |

Um projeto é Verilog; processadores SAPHO são opcionais. Quando existem, o
cliente os compila com `build` antes de verificar, simular ou sintetizar,
como a CLI faz. O módulo `lace_core::verilog` (4.2) trabalha sobre o texto
Verilog: classifica arquivos, acha módulos, lê portas e gera os
arquivos-modelo.

As operações não guardam estado entre si. O estado está no disco: `build`
grava o Verilog, `simulate` lê esse Verilog. Chamar `simulate` sem `build`
antes é erro (`NotBuilt`).

O ciclo de uso é sempre o mesmo:

```rust
use lace_core::*;

let exe = camino::Utf8PathBuf::try_from(std::env::current_exe()?)?;
let toolchain = Toolchain::locate(&exe)?;      // <instalação>/toolchain
let project = Project::open("/home/eu/projetos/soma")?;
let soma = project.require_processor("soma")?;

let built = build(&toolchain, soma, &BuildOptions::default(), &Control::default())?;
if !built.succeeded() {
    for d in &built.diagnostics {
        println!("{:?}:{:?}: {}", d.file, d.line, d.message);
    }
}
```

---

## 2. O bundle (`Toolchain`)

O Lace só executa ferramentas de um bundle versionado instalado com ele:
YANC, surfer-aurora e uma release fixa do OSS CAD Suite (Icarus, Verilator,
Yosys, `dot`), mais o Graphviz no Windows. Nunca do `PATH`, nunca de caminho
configurado. O que vem no bundle, as versões, como ele é montado e o que vem
do sistema estão em [BUNDLE.md](BUNDLE.md); os instaladores, em
[INSTALL.md](INSTALL.md).

O manifesto tem duas partes: o cabeçalho `bundle.json` (formato, versão do
bundle, plataforma) e um `components/<nome>.json` por componente instalado
(versão, origem, SHA-256 dos executáveis). O instalador deixa o usuário
escolher os componentes; um componente que não foi escolhido não tem arquivo
em `components/`.

### 2.1 Abrir

| Função | Faz |
|---|---|
| `Toolchain::locate(&executável)` | acha `toolchain/` ao lado de `bin/` (ou ao lado do executável) e abre |
| `Toolchain::open(dir)` | abre um bundle num diretório (desenvolvimento e testes) |

Abrir lê o `bundle.json` e os manifestos em `components/`
(`COMPONENTS_DIR`), confere o formato (`BUNDLE_SCHEMA`, hoje 2), que o bundle
é da plataforma atual, que cada `components/<nome>.json` descreve o
componente `<nome>` e que o diretório de cada componente existe e, resolvidos
os symlinks, fica dentro do bundle. Um bundle pode ser parcial (os
componentes que o usuário escolheu, ou só o YANC para testes de build): a
ferramenta de um componente não instalado dá `ComponentMissing` quando for
usada. `locate` resolve os symlinks do caminho do executável antes de
procurar, para o atalho `~/.local/bin/lace` do instalador funcionar.

### 2.2 Consultar

| Método | Devolve |
|---|---|
| `manifest()` | o `BundleManifest`: identificador, plataforma e os componentes instalados, na ordem de `component::ALL` (`BundleComponent`: nome, versão, diretório, origem, SHA-256 do pacote e o hash de cada executável) |
| `component(nome)` | um componente instalado (nomes em `lace_core::component`), ou `None` |
| `verified_files()` | quantos executáveis os manifestos listam |
| `tool(Tool)` | o caminho do executável, conferido no disco agora; recusa um symlink que sai do bundle (`InvalidBundle`) e, para o Verilator, a falta do `bin/verilator_bin` que o script Perl chama (sem ele, o script o procuraria no `PATH`) |
| `available()` | as ferramentas presentes |
| `hdl_dir()`, `macros_dir()`, `headers_dir()` | os diretórios do YANC (`SAPHO/`, `Macros/`, `Header/`) |
| `surfer_web_dir()` | o cliente web do Surfer (`surfer-aurora/web/`: `index.html`, `surfer.js`, `surfer_bg.wasm`); `ToolchainIncomplete` num bundle que não o traz |
| `system_compiler()` | o `SystemCompiler` do Verilator, se encontrado |
| `verify()` | os executáveis cujo SHA-256 não confere com o manifesto (`FileMismatch`, com o componente que o lista) |
| `platform()` | a `Platform` |

### 2.3 Ferramentas

| `Tool` | Componente (`Tool::component()`) | Diretório |
|---|---|---|
| `Cmmcomp`, `Appcomp`, `Asmcomp`, `Cpppp`, `Cppcomp` | `yanc` | `yanc/` |
| `Iverilog`, `Vvp` | `icarus` | `oss-cad-suite/` (Linux, macOS) ou `msys/` (Windows) |
| `Verilator` | `verilator` | `oss-cad-suite/` (Linux, macOS) ou `msys/` (Windows) |
| `Yosys` | `yosys` | `oss-cad-suite/` |
| `Dot` | `graphviz` | `oss-cad-suite/` (Linux, macOS) ou `graphviz/` (Windows) |
| `Surfer` | `surfer-aurora` (o nome do programa é `surfer-aurora`) | `surfer-aurora/` |
| `Perl` | nenhum: o do `SystemCompiler` (sistema no Linux e no macOS, bundle no Windows) | |

Icarus, Verilator, cocotb, Yosys e o `dot` do Linux e do macOS dividem o
diretório `oss-cad-suite/`: cada componente traz a parte do pacote da sua
ferramenta, e as bibliotecas em comum vêm com qualquer um deles. No
Windows, Icarus e
Verilator dividem do mesmo jeito o `msys/`, o bloco MSYS2 do lace-toolchain,
com os programas em `ucrt64/bin`. O caminho de cada ferramenta dentro do
diretório é fixo por plataforma, no código; o diretório vem do manifesto do
componente. Nada é configurável.

### 2.4 O compilador do Verilator

O Verilator compila o modelo em C++, chamado pelo `make`, e o script dele
roda pelo Perl. Os três ficam num `SystemCompiler`. No Windows vêm do bundle,
no componente verilator (`msys/ucrt64/bin` e `msys/usr/bin`), e `bundled` é
verdadeiro. No Linux e no macOS, onde o OSS CAD Suite não traz compilador,
vêm do sistema: a exceção à regra do bundle.

| Função | Faz |
|---|---|
| `SystemCompiler::detect()` | procura nos locais padrão do sistema (chamado por `open` no Linux e no macOS; no Windows devolve `None`) |
| `SystemCompiler::in_dir(dir)` | os três num diretório |
| `SystemCompiler::in_msys2(raiz)` | uma instalação do MSYS2, no lugar do compilador do bundle no Windows |
| `toolchain.with_system_compiler(Some(c))` | troca o achado por um declarado |

Sem ele, só o que roda o Verilator falha: a simulação com Verilator e o
`check` com `lint`. No Linux e no macOS, com `SystemCompilerMissing`; no
Windows, com o erro do componente verilator (`ComponentMissing` ou
`ToolchainIncomplete`).

## 3. Projeto

### 3.1 No disco

```
<raiz>/
  <projeto>.spf                  arquivo de projeto (JSON, formato AURORA)
  *.v                            Verilog do usuário, em qualquer subpasta, registrado no .spf
  <tb>.fst ou <tb>.vcd           onda da simulação do projeto (o $dumpfile do testbench)
  <processador>/
    Software/<processador>.cmm   fonte (ou .cpp); o .asm gerado fica aqui
    Hardware/                    <processador>.v, _data.mif, _inst.mif gerados
    Simulation/                  input_<n>.txt (usuário), output_<n>.txt (simulação)
  .lace/Temp/<processador>/      intermediários do YANC, testbench, onda, .vvp, obj_dir
  .lace/Temp/                    simulação do projeto (.vvp, obj_dir, testbench instrumentado)
  .lace/Temp/synth/<topo>/       síntese: script, log, netlist, esquemáticos
  .lace/Temp/hierarchy/          hierarquia: um .vvp por elaboração, refeita a cada chamada
```

A raiz é sempre o diretório do `.spf`. O `basePath` gravado dentro do arquivo
é ignorado na leitura, para que um projeto copiado de outra máquina funcione.

### 3.2 Abrir e criar

| Função | Faz |
|---|---|
| `Project::open(caminho)` | Aceita o `.spf` ou o diretório (usa `<dir>/<nome-do-dir>.spf` ou o único `.spf` dele). Só lê. |
| `Project::discover(caminho)` | Acha o projeto que contém o caminho: o `.spf` dele ou da primeira pasta acima que tenha um, com a regra de `open` em cada pasta. É o que a CLI usa, para os comandos funcionarem de qualquer pasta do projeto. Sem nenhum até a raiz do sistema, `ProjectNotFound`. |
| `project.processor_at(caminho)` | O processador cuja pasta (ou a temporária dele) contém o caminho; `None` fora delas. A CLI usa para `sim` e `wave` sem `-p`. |
| `Project::create(pai, nome)` | Cria `<pai>/<nome>/<nome>.spf` vazio, no formato da AURORA. Recusa se já existir. |
| `project.add_processor(&NewProcessor)` | Cria diretórios, fonte-modelo e entrada no `.spf`. Recusa nome repetido, nome que não passa em `validate_processor_name` (3.4), parâmetro que não passa em `NewProcessor::validate` (`InvalidParameter`, abaixo) e fonte que já exista no disco. |
| `project.configure_processor(nome, &ProcessorConfig)` | Grava `clk`, `numClocks` e `showArrays` no `.spf` (campos `None` ficam como estão). Uma entrada no formato antigo (só o nome) vira objeto. Valem a partir do próximo `build`. Frequência de 1 a `MAX_FREQUENCY_MHZ` (500000) e clocks de 1 a `MAX_CLOCKS` (2147483647); fora disso, `InvalidParameter`, e nada é gravado. |
| `project.issues()` | O que está estranho no `.spf` sem impedir de abrir (`Vec<ProjectIssue>`, abaixo). Só lê. |
| `processor.is_built()` | O `Hardware/<nome>.v` e o testbench existem. Não diz se estão atualizados em relação ao fonte. |
| `project.buildable_processors()` | Os processadores que têm o fonte no disco, na ordem do `.spf`: os que o botão Wave da AURORA compila antes de simular o projeto. |

Todo método que altera o projeto grava o `.spf` na hora (gravação atômica:
arquivo temporário e `rename`). Não existe "salvar". Um `Project` aberto não
percebe mudanças que outro programa faça no `.spf`: abra de novo.

`NewProcessor::validate()` confere os parâmetros contra o que o YANC
compila, e é o que `add_processor` aplica. Portas de entrada e de saída: de 0
a `MAX_PORTS` (256, teto do Lace; o `asmcomp` gera testbench corrompido com
100000). Só em C±: `#NBEXPO` de 2 a 8 (acima disso o `cmmcomp` converte
constantes com o `float` da máquina), `#NBMANT` de pelo menos 2, `#NUBITS`
até 32 e igual a `#NBMANT + #NBEXPO + 1` (o `asmcomp` exige), `#NUGAIN`
potência de dois e `#NDSTAC` e `#SDEPTH` de pelo menos 1 (pilha vazia passa
no build e quebra a simulação). O erro é `InvalidParameter`, com o
parâmetro (`#NUBITS`), o valor e a regra. Em C, só as portas contam.

`ProjectIssue` (de `issues()`) tem `kind`, `path` (absoluto), `detail` e
`message` (em inglês). Os tipos (`IssueKind`):

| `kind` | Quando | `detail` |
|---|---|---|
| `rescued_path` | um absoluto de outra máquina que não existe aqui foi achado pela cauda dentro da raiz (4.1); a próxima gravação conserta o `.spf` | o caminho como estava gravado |
| `selection_not_registered` | `topLevelFile` ou `testbenchFile` aponta para um arquivo fora da lista do papel; é ignorado | o campo |
| `testbench_as_top` | `topLevelFile` tem nome de testbench (3.4); é ignorado | o campo |
| `invalid_processor_name` | um processador do `.spf` com nome que o YANC não compila (`x-y`, `void`); o `build` dele recusa com `InvalidName` | o nome |
| `duplicate_file` | um arquivo repetido numa lista (também por outro caminho, `rtl/../rtl/a.v`, `rtl\a.v`) ou nas duas; conta uma vez (4.1), e a próxima gravação tira as entradas que sobram | a lista |
| `nested_project` | uma pasta acima da raiz tem `.spf`: o projeto está dentro de outro; o `path` é o `.spf` de fora | nenhum |

### 3.3 O `.spf`

| Campo | Lido | Gravado |
|---|---|---|
| `metadata.projectName`, `createdAt`, `computerName`, `appVersion` | não | na criação |
| `metadata.lastModified` | não | em toda gravação |
| `metadata.projectPath`, `structure.basePath` | não (a raiz é o diretório do `.spf`) | em toda gravação, com a raiz atual |
| `structure.processors` | sim | `add_processor` |
| `structure.synthesizableFiles`, `testbenchFiles` | sim | `add_verilog`, `remove_verilog`, `set_top`, `add_file`, `remove_file`, `set_top_level`, `set_testbench` |
| `structure.topLevelFile`, `testbenchFile` | sim | `add_verilog` (o primeiro de cada papel), `remove_verilog`, `set_top`, `set_top_level`, `set_testbench` |
| qualquer outro (`commandOverrides`, `folders`, ...) | não | preservado como estava |

A leitura é tolerante como a da AURORA: JSON estrito primeiro; se falhar, tira
comentários `//` e `/* */`, BOM e vírgula antes de `}` ou `]`, e tenta de novo.
A ordem das chaves é preservada ao gravar.

Um tipo errado nos campos de arquivos é `InvalidProjectFile`, com o campo, e
não um campo ignorado (a próxima gravação o trocaria por uma lista vazia):
`synthesizableFiles` e `testbenchFiles` são listas de objetos, `path` é
texto, `isTopLevel` é booleano (ou o texto `"true"`/`"false"`), e
`topLevelFile`, `testbenchFile` e `topLevelModule` são texto. Ausente ou
`null` vale como vazio. `structure` que não é objeto também é
`InvalidProjectFile`.

Cada processador é uma string (formato antigo) ou um objeto:

| Campo | Tipo | Padrão | Uso |
|---|---|---|---|
| `name` | texto | obrigatório | nome do processador |
| `language` | `"cpp"` ou ausente | C± | linguagem |
| `sourceFile` / `cmmFile` | texto | `<nome>.<ext>` | só o nome do arquivo importa; a extensão ajuda a decidir a linguagem |
| `clk` | inteiro positivo (número ou texto) | 100 | `frequency_mhz`, `-f` do `asmcomp` |
| `numClocks` | inteiro positivo | 2000 | `clocks`, `-c` do `asmcomp` |
| `showArrays` | booleano | `false` | `show_arrays`, `-A` do `cmmcomp` |

`clk: 12.5` é erro (`InvalidProjectFile`). A AURORA trunca para 12 em
silêncio; o Lace recusa para não compilar com um valor que ninguém escolheu.
Também é `InvalidProjectFile` ao abrir: `clk` acima de 500000 ou `numClocks`
acima de 2147483647 (as faixas de `configure_processor`), nome de
processador vazio (tomaria a raiz como pasta dele), com `/`, `\` ou `:`, ou
repetido (sem diferença de caixa). Um nome que só não compila (`x-y`) abre,
com aviso em `issues()`, e o `build` recusa.

A linguagem é resolvida nesta ordem: `language` declarada; extensão de
`sourceFile`/`cmmFile`; o arquivo que existir em `Software/` (`.cmm` antes de
`.cpp`); C±.

### 3.4 Nomes

| Nome | Regra | Por quê |
|---|---|---|
| Processador | `[A-Za-z_][A-Za-z0-9_]*`, até `MAX_PROCESSOR_NAME` (64) caracteres, fora as palavras do C± (`void`, `in`, `out`, ...), do Verilog e do SystemVerilog (`module`, `logic`, ...), os módulos da biblioteca SAPHO (`core`, `processor`, `ula`, ...) e os nomes reservados do Windows (`validate_processor_name`) | vira `#PRNAME` (lexer do `cmmcomp`), módulo Verilog e nome de arquivo; acima de 97 caracteres o `cmmcomp` morre com sinal 11, e `core` instanciaria a si mesmo |
| Módulo de topo | o escolhido pelo nome com `set_top` (campo `topLevelModule`), senão o único módulo do arquivo de topo, ou o que tem o nome do arquivo (`top_module`, 4.1) | vai em `hierarchy -top` da síntese e no `--top-module` do lint |
| Arquivo de topo | qualquer Verilog, inclusive o `Hardware/<nome>.v` gerado de um processador, menos nome de testbench: `tb_<nome>.v`, `<nome>_tb.v`, `tb.v` (`verilog::is_testbench_name`) | o testbench não é o design |
| Módulo do testbench | a mesma regra, no testbench (`testbench_module`) | vai em `-s` e `--top-module` da simulação do projeto e do `check` |
| Projeto | letras sem acento (`A-Z`, `a-z`), dígitos, `_` e `-`, começando por letra, até `MAX_PROJECT_NAME` (64) caracteres; fora `CON`, `COM1` e os outros nomes reservados do Windows (`validate_project_name`). `create` também recusa um nome que só difere na caixa de outro da mesma pasta (`Ok1` ao lado de `ok1`), e não deixa pasta para trás se não conseguir gravar o `.spf` | vira a pasta, o `.spf` e parte do caminho de tudo que as ferramentas recebem; espaço e acento exigem que cada script e ferramenta cite o argumento direito |

A AURORA aceita `-` em nome de processador; o Lace não, porque o `cmmcomp`
não aceita no `#PRNAME`. `validate_processor_name(nome)` é pública, como a
de projeto, para avisar enquanto o usuário digita.

A regra de projeto só vale para criar (`Project::create`): um projeto que
já existe com outro nome, como os da AURORA, abre normalmente.
`validate_project_name(nome)` é pública, para uma interface avisar enquanto
o usuário digita; o erro é `InvalidName`, com o motivo em `reason`.

### 3.5 Fonte-modelo de `add_processor`

C± (padrões da AURORA em `NewProcessor::new`):

```
#PRNAME soma
#NUBITS 23
#NDSTAC 5
#SDEPTH 5
#NUIOIN 1
#NUIOOU 1
#NBMANT 16
#NBEXPO 6
#NUGAIN 128

void main()
{
    // out(porta, valor) escreve numa porta de saída
    out(0, 0);
}
```

O `cmmcomp` recusa função de corpo vazio, e um corpo só com comentário conta
como vazio (a fonte-modelo da AURORA, que é assim, não compila no YANC 5.6).
Por isso o modelo traz uma instrução. Com `output_ports = 0` não há porta
para o `out`, e o corpo é `int x = 0;`, que compila com um aviso de variável
não usada.

C:

```
#pragma yanc prname filtro
#pragma yanc nuioin 1
#pragma yanc nuioou 1

void main(void)
{
}
```

No fluxo C, largura, mantissa, expoente e pilhas são padrões de compilação do
`cppcomp` e não vão para o fonte.

---

## 4. Arquivos do projeto

### 4.1 Verilog e testbenches

Cada arquivo Verilog registrado é sintetizável (`FileRole::Synthesizable`) ou
testbench (`FileRole::Testbench`). `add_verilog` é a forma normal de
registrar: decide o papel pelo conteúdo, cria o arquivo que não existe e
escolhe topo e testbench. `add_file`, `remove_file` e `set_top_level` são a
forma de baixo nível: recebem o papel de quem chama e não classificam nem
criam modelo. Um `.py` só entra como testbench: é um testbench cocotb, como
na AURORA (4.4).

| Método | Faz |
|---|---|
| `add_verilog(toolchain, caminho, testbench) -> Result<AddedFile>` | registra um arquivo (`.v`, `.sv`, ou `.py`, testbench cocotb); se ele não existe, cria a partir do modelo |
| `remove_verilog(caminho) -> Result<bool>` | tira das duas listas e das escolhas de topo e testbench, sem apagar do disco; diz se estava registrado |
| `set_top(alvo) -> Result<Utf8PathBuf>` | escolhe o topo por arquivo ou por nome de módulo; devolve o arquivo |
| `top_module() -> Result<Option<String>>` | o nome do módulo de topo |
| `testbench_module() -> Result<Option<String>>` | o nome do módulo do testbench escolhido; num `.py`, o módulo Python dos testes (o nome do arquivo) |
| `unregistered_verilog() -> Vec<Utf8PathBuf>` | os `.v` e `.sv` da pasta do projeto que não estão registrados, e os `.py` com `@cocotb.test` |
| `files(FileRole)` | lista os arquivos de um papel, na ordem do `.spf`, com caminho absoluto; um arquivo repetido conta uma vez (abaixo) |
| `reorder_file(caminho, &ListPosition) -> Result<Vec<ProjectFile>>` | muda a posição do arquivo na lista dele (`ListPosition::First`, `Last`, `Before(arquivo)`, `After(arquivo)`) e devolve a lista; a ordem é a dos compiladores, e um `` `define `` só vale para os que vêm depois |
| `top_level()` | o arquivo de topo: `topLevelFile`, se está na lista dos sintetizáveis e não tem nome de testbench; senão o sintetizável marcado |
| `testbench()` | `testbenchFile`, se está na lista dos testbenches; senão o testbench marcado, senão o primeiro |
| `check_add_verilog(caminho)` | confere, sem mudar nada, se `add_verilog` aceitaria o arquivo (extensão e, se não existe, o nome); quem registra vários confere todos antes |
| `set_testbench(caminho)` | testbench da simulação do projeto: `testbenchFile` + `isTopLevel` exclusivo; registra se não estava |
| `add_file(papel, caminho, conteúdo)` | registra com o papel dado; com `Some(texto)` cria o arquivo (recusa se existir), com `None` exige que exista; só `.v` e `.sv`, e `.py` como testbench (`InvalidName`, sem gravar nada) |
| `remove_file(papel, caminho)` | tira de uma lista, não apaga do disco |
| `set_top_level(caminho)` | arquivo de topo: `topLevelFile` + `isTopLevel` exclusivo; registra se não estava; recusa nome de testbench (3.4) |
| `resolve_path(texto)` | converte um caminho como está no `.spf` para absoluto |
| `move_path(de, para) -> Result<MovedPath>` | move ou renomeia um arquivo ou pasta do projeto; os arquivos registrados continuam registrados no lugar novo |

`add_verilog(toolchain, caminho, testbench)`:

- **Arquivo que existe:** o papel vem de `verilog::classify` (4.2); com
  `testbench`, é testbench. Se ele estava na outra lista, muda de lista: um
  arquivo fica numa lista só.
- **Arquivo que não existe:** é testbench com `testbench` ou se o nome
  tiver `tb`, `test` ou `testbench` como palavra (`alu_tb.v`, `tb_alu.v`,
  `test_alu.v`; `contest.v` não); senão, módulo, com
  `verilog::module_template(<nome do arquivo>)`. O testbench é
  `verilog::testbench_template` e instancia o módulo `X` de `X_tb.v` ou
  `tb_X.v` se algum sintetizável registrado o declara; senão, o módulo de
  topo; senão, nenhum. As portas vêm de `verilog::read_interfaces` com a
  `toolchain` recebida: com `None`, ou sem o Yosys instalado, o leitor de
  portas embutido.
- **`.py`:** é sempre testbench. Novo, sai de `cocotb::testbench_template`
  (4.4), com o módulo testado pela mesma regra (`test_alu.py` testa `alu`)
  na diretiva `# aurora-toplevel:`; o nome do arquivo precisa ser
  identificador do Python (`InvalidName`, sem criar nada).
- O primeiro sintetizável registrado vira o topo, a não ser que tenha nome
  de testbench (3.4) ou não declare módulo (um arquivo só de `` `define ``);
  o primeiro testbench, o testbench escolhido.

`caminho` é absoluto ou relativo à raiz do projeto. A CLI sempre passa
absoluto, resolvido a partir do diretório atual do shell.

`AddedFile`:

| Campo | Conteúdo |
|---|---|
| `path` | caminho absoluto |
| `role` | `synthesizable` ou `testbench`: em qual lista ficou |
| `created` | o arquivo não existia e foi criado a partir do modelo |
| `selected` | virou o topo (era o primeiro sintetizável) ou o testbench escolhido |

`set_top(alvo)` usa `alvo` como arquivo (absoluto ou relativo à raiz) se ele
existir, registrando-o como sintetizável (um arquivo que estava entre os
testbenches muda de lista). Qualquer Verilog serve, inclusive o
`Hardware/<nome>.v` que o build de um processador gerou: registrado, ele
continua entrando uma vez só no design, porque `check`, a simulação, a
síntese e a hierarquia tiram os repetidos. Nome de testbench (`tb_<nome>.v`,
`<nome>_tb.v`, `tb.v`) é `InvalidName`, e nada muda; senão, procura um módulo com esse nome nos
sintetizáveis registrados e grava o arquivo e o nome do módulo
(`structure.topLevelModule`, campo do Lace que a AURORA ignora), para
arquivos com vários módulos. Se nenhum declara, é `ModuleNotFound`, com os
módulos do projeto em `available` (sem repetição); se mais de um declara, é
`AmbiguousModule`, com os arquivos, e o topo se escolhe pelo arquivo. Um
alvo que é arquivo mas não `.v` nem `.sv` é `InvalidName`.

`top_module()` é o módulo de `topLevelModule`, se o arquivo de topo ainda o
declara; senão, os módulos do arquivo de topo (`verilog::modules_in`): com um
só, é ele; com vários, o que tem o nome do arquivo; senão, `ModuleNotFound`.
Sem topo, `Ok(None)`. `testbench_module()` é a mesma regra, sem o campo,
sobre `testbench()`, com mais um caso: vários módulos e nenhum com o nome do
arquivo, vale o único que nenhum outro do arquivo instancia
(`bancada_tb.v` com `gerador` e `principal`, que instancia `gerador`,
simula `principal`).

`unregistered_verilog()` procura na pasta do projeto e nas subpastas (até 4
níveis), menos `.lace/`, as ocultas, as pastas dos processadores e a
`TopLevel/` legada, cujos `.v` já entram sozinhos.

`move_path(de, para)` move ou renomeia um arquivo ou uma pasta dentro do
projeto, como arrastar na árvore de uma IDE, e mantém o `.spf` em dia.
`para` é o caminho final, e não a pasta de destino (`rtl/a.v` para
`src/a.v` move; `a.v` para `b.v` renomeia); as pastas que faltam nele são
criadas. Os dois são absolutos ou relativos à raiz.

- Cada entrada do `.spf` que estava em `de` (o próprio arquivo, ou os de
  dentro da pasta) troca de caminho e de nome na mesma posição da lista, e
  `topLevelFile`, `testbenchFile` e `topLevelModule` acompanham. O papel e a
  marca de topo ou de testbench escolhido não mudam.
- Ficam onde estão, porque o Core os acha pelo lugar: o `.spf`, a pasta
  `.lace` e, de cada processador, a pasta, `Software/`, `Hardware/`,
  `Simulation/` e o fonte (`CannotMove`). O resto de dentro delas (entradas,
  saídas, gerados) pode mudar de lugar. Nada vai para dentro de `.lace`, e
  uma pasta não vai para dentro dela mesma (`CannotMove`).
- O destino que já existe não é sobrescrito (`PathExists`), inclusive um
  link quebrado; renomear só a caixa (`a.v` para `A.v`) funciona também num
  sistema que não distingue maiúsculas. Uma origem que não existe é
  `InvalidProject`. Caminho fora da
  pasta do projeto é `OutsideProject`. Em todos os erros, nada mudou.
- O movimento é um `rename`; entre discos diferentes, copiar e apagar. Se o
  `.spf` não puder ser gravado depois, o movimento é desfeito antes do erro.
- `de` igual a `para` não faz nada.

`MovedPath`:

| Campo | Conteúdo |
|---|---|
| `from` | onde estava, absoluto |
| `to` | onde ficou, absoluto |
| `files` | os `ProjectFile` que mudaram de lugar, já com o caminho novo; vazio para um arquivo que não está no `.spf` |

```rust
let moved = project.move_path("contador.v", "rtl/contador.v")?;
for file in &moved.files {
    println!("{} continua no projeto ({:?})", file.path, file.role);
}
```

Caminhos são comparados depois de normalização léxica: `rtl/../top.v` e
`top.v` são o mesmo arquivo. Um `.spf` editado à mão com o mesmo arquivo
repetido numa lista conta uma vez, marcado se alguma das entradas está;
nas duas listas, conta como testbench se o nome indica (`_tb`, `tb_`,
`test`), e como sintetizável senão. A marca de testbench escolhido aceita
também o `isMarkedTestbench` legado da AURORA, que ela lê como
`isTopLevel`; escolher um testbench tira a marca legada dos outros. O `.spf` guarda, sempre com `/`:

- relativo à raiz, quando o arquivo está dentro dela, como a AURORA;
- relativo com `..` (`../../rtl/x.v`), quando está fora dela mas no mesmo
  repositório git (a primeira pasta acima da raiz com `.git`), para o
  projeto funcionar em qualquer clone;
- absoluto, no resto.

Na leitura aceita `\` e `/`. Um absoluto que não existe nesta máquina (de
outra, `C:\Users\...` num `.spf` aberto no Linux) é procurado pela cauda
dentro da raiz, como a AURORA (`resgatarPelaCauda`): `C:\velho\p\rtl\x.v`
tenta `velho/p/rtl/x.v`, `p/rtl/x.v`, `rtl/x.v` e `x.v`, e o primeiro que
existir vale para todas as operações. O caminho achado é um aviso
(`rescued_path` em `issues()`), e a próxima gravação do `.spf` o troca pelo
caminho de hoje.

### 4.2 O módulo `verilog`

`lace_core::verilog` trabalha sobre texto. Só `read_interfaces` executa
uma ferramenta, o Yosys, e só quando ele está instalado.

| Função | Devolve |
|---|---|
| `classify(texto, nome_do_arquivo) -> FileRole` | sintetizável ou testbench, pela regra da AURORA |
| `modules_in(texto) -> Vec<String>` | os nomes dos módulos declarados, na ordem, sem os que estão em comentário |
| `read_interfaces(toolchain, arquivos) -> Result<Vec<ModuleInterface>>` | as portas de cada módulo declarado nos arquivos |
| `module_template(nome) -> String` | o módulo-modelo de um arquivo novo |
| `testbench_template(testbench, dut) -> String` | o testbench-modelo, instanciando `dut` quando há um |
| `is_testbench_name(nome_do_arquivo) -> bool` | o nome é de testbench pela regra do topo: `tb_<nome>`, `<nome>_tb` ou `tb`, com qualquer extensão; mais estreita que a dica de nome do `classify` (`test_alu.v` não conta) |

`classify` segue `js/project/verilog_classifier.ts` da AURORA: soma pontos, e
3 ou mais é testbench. Cada indício conta uma vez.

| Indício | Pontos |
|---|---|
| `$dumpfile`, `$dumpvars`, `$dumpon`, `$dumpoff`, `$dumpall`, `$dumplimit` ou `$dumpflush` | 3 |
| `$finish` ou `$stop` | 3 |
| módulo sem portas (`module tb;`, `module tb();`, também com `#(...)` de parâmetros) | 3 |
| `initial` | 2 |
| `$display`, `$write`, `$monitor`, `$strobe`, `$time`, `$realtime`, `$random`, `$sformat` ou `$sformatf` | 1 |
| atraso `#<dígito>` (`#10`; o `#(` de parâmetro não conta) | 1 |
| nome do arquivo com `tb`, `test` ou `testbench` como palavra (`alu_tb.v`, `test_alu.v`) | 2 |

Comentários e strings não contam. Texto vazio é sintetizável. Na dúvida,
sintetizável: `initial` sozinho, que aparece em módulo sintetizável para
inicializar memória, não basta; o nome sozinho também não.

`read_interfaces` lê só sintetizáveis, porque o Yosys não lê testbench. Com
o Yosys do bundle, roda `read_verilog -sv -lib` e `write_json`; sem ele
(`toolchain` `None` ou componente não instalado), usa um leitor de portas no
estilo ANSI, as declaradas no cabeçalho do módulo. `ModuleInterface` tem
`name`, `file` e `ports`, na ordem da declaração; cada `Port` tem `name`,
`direction` (`input`, `output`, `inout`), `width` em bits, com os parâmetros
nos valores padrão, e `signed`.

`module_template(nome)` dá um módulo com `input wire [7:0] a`,
`output wire [7:0] y` e `assign y = a;`: compila, e tem portas para não ser
classificado como testbench.

`testbench_template(testbench, dut)`, com `dut`, instancia o módulo com
todas as portas (`reg` para as entradas, `wire` para as saídas), gera clock
se houver uma porta `clk` ou `clock` e reset se houver `rst` ou `reset`,
grava a onda com `$dumpfile("<testbench>.fst")` (o Verilator grava
`<testbench>.vcd`, 5.2) e
`$dumpvars(0, <testbench>)` e termina com `$finish`. Sem `dut`, o testbench
não instancia nada: grava a onda com o mesmo `$dumpfile` e `$dumpvars`,
espera `#100` e chama `$finish`.

### 4.3 Entradas e saídas da simulação

O testbench que o `asmcomp` gera lê `Simulation/input_<n>.txt` e escreve
`Simulation/output_<n>.txt`, por caminho absoluto embutido no testbench. Os
dois têm um valor decimal com sinal por linha.

| Método de `Processor` | Faz |
|---|---|
| `input_path(n)`, `output_path(n)` | os caminhos |
| `write_input_values(n, &[i64])` | grava a entrada, um valor por linha (substitui a anterior) |
| `write_input(n, texto)` | grava a entrada como texto livre |
| `read_output_values(n)` | lê a saída como `Vec<i64>` |
| `read_output(n)` | lê a saída como texto |

`read_data_file(caminho)` lê qualquer arquivo nesse formato. Uma linha que não
seja inteiro é erro (`InvalidDataFile`, com a linha), nunca descartada: um
valor perdido mudaria o resultado da simulação.

A porta `n` corresponde ao endereço usado em `in(n)` / `out(n)` no programa.
O testbench só lê uma entrada se o arquivo existir; arquivo ausente não é
erro, e a porta simplesmente não recebe dado. `missing_inputs(processador)`
lista os que faltam antes de simular.

`simulate` (5.2) confere as entradas que existem antes de rodar, porque o
testbench do YANC lê com `$fscanf` sem olhar o resultado: uma linha que não
é inteiro é `InvalidDataFile`, e a simulação não roda; arquivo vazio, ou
valor que não cabe em `#NUBITS` bits (com ou sem sinal), é aviso, e a
simulação roda. Um arquivo com menos valores do que o programa lê não é
conferido (o número de leituras só se sabe rodando; o último valor se
repete).

### 4.4 O módulo `cocotb`

Um `.py` em `testbenchFiles` é um testbench cocotb: só os testes
(`@cocotb.test()`), e o Lace monta a simulação (5.3.2). O módulo que os
testes recebem como `dut` vem da diretiva da AURORA, uma linha só de
comentário `# aurora-toplevel: <módulo>` (com `:` ou `=`, sem distinção de
caixa), ou, sem ela, do topo do projeto, com um aviso. O nome do arquivo é o
módulo Python que o cocotb importa.

| Função ou tipo | Faz |
|---|---|
| `is_testbench(caminho) -> bool` | o arquivo é testbench cocotb (termina em `.py`) |
| `toplevel_directive(texto) -> Option<String>` | o módulo da diretiva `# aurora-toplevel:`, se houver |
| `testbench_template(dut) -> String` | o testbench-modelo: com `dut`, a diretiva e um teste que liga o clock (`clk`, 10 ns), segura o reset nos primeiros 20 ns (`rst`, ou `rst_n` ativo em baixo), põe as outras entradas em zero e escreve as saídas no log; sem, um teste que só espera. `Timer` e `Clock` com a unidade posicional, que vale no cocotb 1 e no 2 |
| `TestReport` | o resultado dos testes: `results` (o `results.xml`), `cases` e as contagens `passed`, `failed`, `skipped` |
| `TestCase` | um teste: `name` (`<módulo>.<teste>`), `status` (`TestStatus`: `passed`, `failed`, `skipped`), `message`, `file`, `line` (numa falha, a linha mais funda do traceback dentro do `.py`) |

---

## 5. Operações

Todas bloqueiam até as ferramentas terminarem, exceto `open_waveform`. As
que executam ferramentas recebem por último um `&Control`;
`&Control::default()` roda até o fim sem cancelar nem avisar nada. Cancelar,
pôr prazo e acompanhar a saída estão em 5.8.

### 5.1 `build(&Toolchain, &Processor, &BuildOptions, &Control) -> Result<BuildResult>`

Do fonte ao Verilog sintetizável.

Antes de executar qualquer coisa:

1. o fonte precisa existir e se chamar `<processador>.<ext>`;
2. em C±, o fonte precisa declarar `#PRNAME <processador>`; em C, um
   `#pragma yanc prname`, se houver, precisa ser o nome do processador;
3. nenhum caminho pode passar de `YANC_PATH_LIMIT` (259 no Windows, 1000
   nos outros sistemas; ver 9.1);
4. `Software/`, `Hardware/`, `Simulation/` e o diretório temporário são
   criados.

O `asmcomp` grava o testbench na pasta temporária (`T/<nome>_tb.v`). No fim
de um build que deu certo, o Lace o copia para `Simulation/<nome>_tb.v`
(`Processor::testbench_path`), como a AURORA (`processor_compiler.ts`), e é
essa cópia que a simulação roda; um build novo a sobrescreve.

Os itens 1 e 2 existem porque o YANC tem duas armadilhas: o `cmmcomp` reabre
o fonte pelo nome do processador no fim da compilação, e o `asmcomp` dá aos
arquivos de `Hardware/` o nome do `#PRNAME`, não o do `-n`. Um `#PRNAME`
diferente gera `Hardware/<outro>.v`; sem `#PRNAME`, um nome de lixo.

Comandos, com `P` = diretório do processador e `T` = diretório temporário:

| Passo | Comando | CWD |
|---|---|---|
| `preprocess` (só C) | `cpppp -i <fonte> -o T/pp.cpp -I <headers> -I P/Software` | `T` |
| `compile` (C±) | `cmmcomp -i <nome>.cmm -n <nome> -p P -m <macros> -t T [-A] -en` | `P` |
| `compile` (C) | `cppcomp -i T/pp.cpp -p P -n <nome> -t T` | `P` |
| `pre_assemble` | `appcomp -i P/Software/<nome>.asm -t T -en` | `T` |
| `assemble` | `asmcomp -i <asm> -p P -d <hdl> -m <macros> -t T -f <MHz> -c <clocks> -en` | `T` |

O Lace sempre passa `-en` aos três compiladores do fluxo C±, para que o
parser de mensagens trabalhe sobre um idioma só. `cpppp` e `cppcomp` não
conhecem a flag.

Tudo é compilado no destino final: o `asmcomp` embute caminhos absolutos no
Verilog e no testbench, então mover os artefatos depois quebra a simulação.

`BuildOptions` sobrepõe `frequency_mhz`, `clocks` e `show_arrays` só neste
build, sem gravar no `.spf`.

Artefatos obrigatórios: `assembly`, `verilog`, `data_memory`,
`instruction_memory`, `testbench`. Intermediários: `program_counter_map`,
`source_translation`, `opcode_translation`, `compiler_log`,
`pre_assembler_log`, `preprocessed_source` (C).

### 5.1.1 `build_processors(&Toolchain, processadores, &BuildOptions, OnFailure, &Control, on_result) -> Result<Vec<BuildResult>>`

Compila vários processadores em sequência. `on_result` é chamado depois de
cada um, para a interface mostrar o progresso. Um build cancelado encerra a
sequência mesmo com `OnFailure::Continue`.

| `OnFailure` | Uso |
|---|---|
| `Continue` | "compilar tudo": mostra todos os erros de uma vez |
| `Stop` | antes de simular ou sintetizar: para no primeiro que falhar |

O fluxo do botão Wave da AURORA é:

```rust
let control = Control::default();
let builds = build_processors(&tc, project.buildable_processors(), &BuildOptions::default(),
                              OnFailure::Stop, &control, |r| mostrar(r))?;
if builds.iter().all(BuildResult::succeeded) {
    let options = SimulationOptions::new(Simulator::Icarus);
    let sim = simulate_project(&tc, &project, &options, &control)?;
}
```

### 5.2 `simulate(&Toolchain, &Processor, &SimulationOptions, &Control) -> Result<SimulationResult>`

Simula um processador com o testbench que o `build` gerou. Duração e
frequência já estão no testbench (vêm de `clocks` e `frequency_mhz` do build).

Com `<SAPHO>` = a biblioteca SAPHO (`yanc/SAPHO/` do bundle):

| Simulador | Passo | Comando | CWD |
|---|---|---|---|
| Icarus | `elaborate` | `iverilog -y <SAPHO> -s <nome>_tb -o T/<nome>_tb.vvp P/Hardware/<nome>.v T/instr_<nome>_tb.v` | `T` |
| Icarus | `simulate` | `vvp -n [-i] T/<nome>_tb.vvp -fst`, com `-i` quando o `Control` tem receptor de eventos (5.8); sem `-fst` só quando a onda não vai para um `.fst` (o formato da onda, abaixo) | `T` |
| Verilator | `verilate` | `perl verilator --binary --main --trace -j 0 ... --top-module <nome>_tb -Mdir T/obj_dir_<nome>_tb -y <SAPHO> <arquivos>` | `T` |
| Verilator | `simulate` | `T/obj_dir_<nome>_tb/V<nome>_tb` | `T` |

O CWD é o diretório temporário porque o `.v` do processador lê
`pc_<nome>_mem.txt` por nome relativo e o testbench grava a onda em
`<nome>_tb.fst` (`.vcd` no Verilator), também relativo. As memórias `.mif` e os arquivos de
`Simulation/` são abertos por caminho absoluto.

O caminho do processador não pode ter caractere fora do ASCII
(`NonAsciiPath`): o YANC grava no `.v` e no testbench o caminho absoluto das
memórias e das entradas, e o `vvp` recusa nome de arquivo com acento ("contains
non-printable characters"), seguindo sem os dados. O aviso do `vvp`, se
aparecer, também vira erro.

Em toda chamada do Icarus (simulação, `check`, `hierarchy`), um `include`
procura primeiro na pasta do arquivo que inclui (`-grelative-include`) e
depois na raiz do projeto (`-I <raiz>`), a mesma ordem da síntese.

O `-n` faz `$stop` e Ctrl-C terminarem a simulação, em vez de abrir o prompt
interativo do `vvp`.

Antes de rodar, as entradas do processador são conferidas (4.3), e a
simulação usa uma cópia do testbench (`T/instr_<nome>_tb.v`) que confere o
resultado de cada `$fscanf` da entrada: na primeira leitura além do fim do
arquivo, o `vvp` escreve `WARNING: <entrada>:<linha>: the program reads more
values than this file has (N)`, que vira aviso com o arquivo e a linha
seguinte à última (antes, a porta repetia o último valor em silêncio). A
cópia tem as mesmas linhas, e os diagnósticos dela apontam para o
testbench original. A mesma cópia leva a extensão da onda trocada (o formato
da onda, abaixo); um testbench sem o trecho esperado e com a extensão certa
roda como está.
Depois, se
a simulação deu certo mas o testbench não escreveu `end of program` (o aviso
que o testbench do `asmcomp` dá quando o programa termina), um diagnóstico de
aviso diz que os clocks acabaram antes do fim do programa e que as saídas
param ali; o status continua `succeeded`.

Um build (5.1) ou uma simulação de um processador trava
`.lace/Temp/<nome>.lock` enquanto roda, e `simulate_project` trava a do
projeto (`.lace/Temp/.project.lock`) e a de cada processador. Uma segunda
operação no mesmo processador, de outro processo (o Studio e a CLI) ou da
mesma, não espera: é `OperationInProgress`, e nada é alterado. Antes, as duas
regravavam `Hardware/` e a pasta temporária ao mesmo tempo e falhavam juntas.

A biblioteca SAPHO entra por `-y`: o simulador carrega só os módulos que
faltarem. Ela vem do YANC (`Toolchain::sapho_library`) e só entra quando há
processador SAPHO: sempre em `simulate`; em `simulate_project`, `check` e
`synthesize`, só se o projeto tem processadores. Um projeto só de Verilog
não usa a biblioteca e não precisa do YANC; um projeto com processador e sem
o YANC instalado dá `ComponentMissing("yanc")`.

Flags do Verilator (as da AURORA, sem o arquivo `.vlt` de monitores dela, e
com três mudanças): `--binary --main --trace -j 0
-MAKEFLAGS OBJCACHE= -MAKEFLAGS PYTHON3=<Python do bundle> -Wno-fatal
-Wno-TIMESCALEMOD -Wno-DECLFILENAME -Wno-STMTDLY --timing --x-assign fast
--no-trace-top --autoflush +define+YANC_TRACE -CFLAGS -O3 -CFLAGS -march=native -CFLAGS
-fstrict-aliasing -CFLAGS -pipe -CFLAGS -Wno-attributes` (sem `-march=native`
no macOS). As três mudanças:

- `--trace` (VCD) em vez de `--trace-fst`: no Linux e no macOS, o FST do
  Verilator do bundle compila contra lz4 e zlib do sistema, fora da exceção
  do compilador; o Windows segue o mesmo formato;
- `PYTHON3=`: o `make` gerado chama `python3` pelo nome, e sem isso usaria o
  do sistema;
- `--autoflush`: o modelo escreve o `$display` na hora, e não em blocos no
  fim, o que permite mostrar a saída enquanto roda (5.8). Entra sempre, com
  ou sem receptor de eventos.

Na simulação rápida saem `--trace` e `--no-trace-top` (abaixo).

O script `verilator` roda pelo Perl do `SystemCompiler`, com `PATH` = o
diretório do script (`bin/` do OSS CAD Suite, `ucrt64/bin` do `msys/`),
depois os diretórios do compilador, e `LC_ALL=C`. No Windows ele recebe
também `--no-unlimited-stack` e `VERILATOR_BIN=verilator_bin.exe`: sem a
primeira, o script tenta `ulimit -s unlimited 2>/dev/null` pelo `cmd.exe`,
que escreve um erro de caminho no stderr a cada chamada; sem a segunda, o
Verilator anota entre as entradas do modelo (`__verFiles.dat`) um
`verilator_bin` sem extensão, que não existe, e o `--skip-identical` nunca
reconhece um modelo em dia. A primeira compilação do modelo leva dezenas de
segundos. Depois, com os fontes e as flags iguais, o Verilator não gera o
C++ de novo, o `make` não recompila nada e a simulação só roda o executável
(medido no Windows em 2026-10-07, num contador: 0,4 s, contra 9 s antes); o
`verilated_model` conta como `fresh`, porque está em dia.

`SimulationOptions`:

| Campo | Padrão | Efeito |
|---|---|---|
| `simulator` | (obrigatório em `new`) | `Icarus` ou `Verilator` |
| `build_jobs` | `None` (`-j 0`, todos os núcleos) | paralelismo da compilação C++ do Verilator |
| `timeout` | `None` (sem limite) | prazo do passo `simulate` (o `vvp` ou o modelo do Verilator), sem contar elaboração e compilação. Passou dele, o Lace encerra a simulação e o resultado vem com `status: timed_out` e o que o testbench escreveu até ali (5.8) |
| `fast` | `false` | a simulação rápida, sem onda (abaixo) |

O formato da onda não é opção: o Icarus grava FST (`vvp -fst`) e o
Verilator, VCD (`--trace`), e a onda sai com o nome do `$dumpfile` do
testbench e a extensão do formato. Um `$dumpfile` com outra
extensão é trocado numa cópia do testbench, com as mesmas linhas: o
`<nome>_tb.vcd` que o testbench do `asmcomp` pede vira `<nome>_tb.fst` no
Icarus, e um `saida.fst` vira `saida.vcd` no Verilator. A cópia é a que o
Lace já simula, `T/instr_<nome>_tb.v` num processador e
`.lace/Temp/instr_<testbench>` no projeto (5.3), e o arquivo do usuário não
muda. Uma chamada de `$dumpfile` quebrada em linhas não é trocada, e o
Icarus grava VCD no nome que ela der. `SimulationResult.waveform` traz o
caminho e o formato.

**A simulação rápida.** Com `fast`, a simulação é o Fast Sim da AURORA
(`runFastSim`): roda sem gravar onda, para ver a saída do testbench, as
portas do processador e os testes cocotb na velocidade do simulador. O
testbench Verilog, de `simulate` ou de `simulate_project`, roda no
Verilator, qualquer que seja o `simulator`: o passo `verilate` não leva
`--trace` nem `--no-trace-top`, o `-Mdir` é `obj_dir_fast_<topo>`, ao lado
do `obj_dir_<topo>` da simulação com onda (alternar entre as duas não
recompila tudo a cada vez), e o modelo ignora os `$dumpfile` e `$dumpvars`
do testbench (escreve na saída `$dumpvar ignored, as Verilated without
--trace`). O Lace não injeta o dump padrão nem troca a extensão do
`$dumpfile`; o `+define+YANC_TRACE` continua, porque o testbench que o YANC
gera lê os sinais de simulação do processador para achar o fim do programa.
Um testbench cocotb roda os testes no `simulator` (5.3.2). O resultado vem
com `fast: true`, o simulador que rodou em `simulator` e `waveform: null`.
A AURORA comenta os `$dumpfile` e `$dumpvars` numa cópia do testbench; o
Lace não precisa: o Verilator 5 do bundle os ignora sozinho.

O `vvp` sai com código 0 depois de um `$error`. Por isso as linhas `ERROR:`
e `FATAL:` que ele escreve (as de `$error` e `$fatal`) viram diagnóstico de
erro e a simulação termina como `failed`, com `failed_step: simulate`.
Sem elas, uma simulação que chega ao `$finish` com código 0 é `succeeded`,
mesmo que o circuito esteja errado: conferir o comportamento é olhar as
saídas e a onda. O stdout do testbench (`$display`) fica no `stdout` do passo
`simulate`.

### 5.3 `simulate_project(&Toolchain, &Project, &SimulationOptions, &Control) -> Result<SimulationResult>`

Simula o testbench do projeto, como o botão Wave da AURORA.

1. O testbench é `project.testbench()`. O módulo simulado (`-s`,
   `--top-module`) é `project.testbench_module()`: vem do conteúdo do
   testbench, não só do nome do arquivo.
2. Arquivos: os sintetizáveis do `.spf`, depois o `Hardware/<nome>.v` de cada
   processador compilado que não esteja na lista, e o testbench por último.
3. Copia para a raiz o `pc_<nome>_mem.txt` de cada processador compilado.
4. Copia para a raiz os arquivos que o testbench lê por nome relativo
   (`$readmemb("x")`, `$readmemh("x")`, `$fopen("x", "r")`), a partir da pasta
   do testbench, com duas restrições:
   - nada é escrito fora da raiz: um `$readmemh("../dados/x.hex")` lê, como
     o Icarus faz, o caminho relativo à raiz, e se ele não existe entra em
     `missing_inputs`;
   - um arquivo do usuário na raiz não é sobrescrito: se a raiz já tem um
     `x` diferente, ele fica, a simulação lê esse, e sai um aviso. O Lace só
     atualiza as cópias que ele mesmo fez, anotadas em
     `.lace/Temp/data_copies.txt`.
5. O `$dumpfile` vale com texto entre aspas ou com um `localparam`,
   `parameter` ou `` `define `` de texto (`$dumpfile(ONDA)`). Com uma
   expressão que o Lace não resolve, nada é injetado, a onda não entra no
   resultado, e `waveform_path` dá `InvalidProject` (antes, o dump injetado e
   o `-fst` dele gravavam FST no `.vcd` do usuário). Um `$dumpfile` sem
   `$dumpvars` dá um aviso: nenhum sinal é gravado e a onda não sai. Um
   `$dumpfile` com a extensão de outro formato vai para uma cópia em
   `.lace/Temp/instr_<testbench>`, com a extensão trocada (5.2): `saida.vcd`
   vira `saida.fst` no Icarus.
   Se o testbench não tem `$dumpfile`, simula uma cópia em
   `.lace/Temp/instr_<testbench>` com `$dumpfile("<tb>.fst");
   $dumpvars(0, <tb>);` antes do `endmodule` do módulo do testbench
   (comentário e texto entre aspas não contam), em que `<tb>` é esse módulo
   (`<tb>.vcd` no Verilator). O bloco entra sem quebrar linha, e os
   diagnósticos da cópia apontam para o arquivo do usuário, que não muda. A
   profundidade 0 grava todos os sinais da hierarquia, inclusive os do
   módulo testado. A onda injetada não é exigida: um testbench que chega ao
   `$finish` no tempo 0, antes do bloco injetado, termina como sucesso, sem
   onda. Na simulação rápida (5.2), nada disso: o testbench roda como está,
   e não há onda.
6. Roda os mesmos comandos de 5.2, com CWD na raiz e `.vvp`/`obj_dir` em
   `.lace/Temp/`, e o mesmo `timeout`. A onda fica na raiz, com o nome do
   `$dumpfile` e a extensão do formato.

Os processadores não são compilados aqui. Chame `build` para cada um antes.

Uma onda VCD do Icarus acima de 100 MB vem com um aviso: um `$dumpfile`
terminado em `.fst` grava os mesmos sinais várias vezes menor. Só acontece
quando o Lace não troca a extensão do `$dumpfile` (a chamada quebrada em
linhas).

Com processador no projeto, a raiz não pode ter caractere fora do ASCII
(`NonAsciiPath`), pelo mesmo motivo de 5.2.

### 5.3.2 Testbench cocotb

Com um testbench `.py`, `simulate_project` roda os testes cocotb no Icarus
ou no Verilator, pelos passos de sempre.
Com `<B>` = `<raiz>/.lace/Temp/cocotb/<módulo de teste>` e `<dut>` o módulo
da diretiva (ou o topo):

| Simulador | Passo | Comando | CWD |
|---|---|---|---|
| Icarus | `elaborate` | `iverilog -grelative-include -I <raiz> [-g2012] [-y <SAPHO>] -s <dut> -f <B>/cmds.f -s lace_cocotb_dump -o <B>/<dut>.vvp <design> <B>/lace_cocotb_dump.v` | raiz |
| Icarus | `simulate` | `vvp -n [-i] -m <VPI do cocotb> <B>/<dut>.vvp -fst` | raiz |
| Verilator | `verilate` | `perl verilator --cc --exe --build --vpi --public-flat-rw --prefix Vtop -o V<dut> --timescale 1ns/1ps -LDFLAGS <VPI do cocotb> ... --trace --no-trace-top <as flags de 5.2> --top-module <dut> -Mdir <B>/obj_dir_<dut> [-y <SAPHO>] <verilator.cpp do cocotb> <design>` | `<B>` |
| Verilator | `simulate` | `<B>/obj_dir_<dut>/V<dut> --trace --trace-file <raiz>/<módulo de teste>.vcd` | raiz |

`<design>` é o mesmo da simulação de um testbench Verilog (5.3, passo 2),
sem o testbench, e os `pc_<nome>_mem.txt` dos processadores também vão para
a raiz. O `cmds.f` traz `+timescale+1ns/1ps`, o padrão do runner do cocotb,
para os módulos sem `` `timescale `` (no Verilator, `--timescale`). O
`lace_cocotb_dump` grava `<raiz>/<módulo de teste>.fst` com
`$dumpvars(0, <dut>)`.

No Verilator, o modelo tem o `main` do cocotb (`share/lib/verilator/verilator.cpp`),
que carrega a VPI dele e deixa os testes dirigirem o tempo, e grava a onda
com `--trace` em `<raiz>/<módulo de teste>.vcd`. A VPI do cocotb entra na
ligação: no Windows a estática, `libcocotbvpi_verilator.a`, com `-L<libs>
-lgpi`; no Linux e no macOS a compartilhada, com `-Wl,-rpath,<libs>
-L<libs>`, como no `Makefile.verilator` do cocotb. No Windows, o
`PYTHONPATH` começa por `.lace/Temp/cocotb/site/`, com um
`sitecustomize.py` que põe a pasta do Python entre as que o Windows procura
ao carregar DLLs (`os.add_dll_directory`): o Python dentro do modelo não
acha as DLLs das extensões dele pelo `PATH` (o `binascii` carrega a do
`zlib`), e o modelo, ao contrário do `vvp`, não fica na pasta do Python.

Na simulação rápida (5.2) os testes rodam sem onda: no Icarus sem o
`lace_cocotb_dump` e com `vvp ... -none`; no Verilator com o modelo sem
`--trace`, em `<B>/obj_dir_fast_<dut>`.

O que o simulador precisa para carregar o cocotb vem de uma sonda
(`cocotb_probe.py`) rodada com o Python do componente `cocotb` e guardada
em `.lace/Temp/cocotb/probe.json` até o bundle mudar: a VPI para o Icarus,
a VPI e o `verilator.cpp` para o Verilator, a biblioteca do Python, o ponto
de entrada e o `sys.path`. Com isso o simulador recebe `PYGPI_PYTHON_BIN`, `PYTHONPATH` (a pasta do `.py`, a raiz e o
`sys.path` do Python), `TOPLEVEL_LANG=verilog`, `COCOTB_RESULTS_FILE`, as
variáveis do cocotb 2 (`GPI_USERS`, `COCOTB_TOPLEVEL`,
`COCOTB_TEST_MODULES`) ou do 1 (`LIBPYTHON_LOC`, `TOPLEVEL`, `MODULE`), a
pasta das bibliotecas do cocotb no `PATH`, `PYTHONUTF8=1` e
`PYTHONPYCACHEPREFIX` na pasta de cache do usuário: os `.pyc` não vão para o
bundle nem para o projeto.

O `results.xml` vira `SimulationResult::tests` (`TestReport`, 4.4). Cada
teste que falha é um diagnóstico de erro do simulador (`vvp`, ou
`verilator` no Verilator) no `.py`, na linha mais funda do traceback dentro
dele, e a simulação termina `failed`, com `failed_step: simulate`; um `.py`
sem teste nenhum também. Um diagnóstico `info` resume as contagens. A onda
vem em `waveform` mesmo com teste falhando, desde que a simulação tenha ido
até o fim: é nela que se vê a falha.

Erros: `NoCocotbToplevel` sem diretiva e sem topo; `InvalidName` se o nome
do `.py` não é identificador do Python; `ComponentMissing` sem o
componente `cocotb`; `CocotbUnavailable` se a sonda falha ou o cocotb não
traz a biblioteca do simulador. Testado no Windows com o cocotb 2.1.0 e o
Verilator 5.050 do bundle, nos dois simuladores; no Linux e no macOS, com o
2.1.0.dev0 do OSS CAD Suite, não foi rodado.

### 5.3.1 `waveform_path(&Project, Option<&Processor>) -> Result<Utf8PathBuf>`

Onde a simulação grava a onda, sem simular: é o que `lace wave` abre sem
argumento. Com um processador, a do testbench que o `asmcomp` gerou
(`.lace/Temp/<nome>/<nome>_tb.fst` ou `.vcd`); sem, a do testbench do
projeto: o nome do `$dumpfile` dele, ou, sem `$dumpfile`, a onda que o
Lace injeta na raiz (5.3, passo 5). Das duas extensões, `.fst` do Icarus e
`.vcd` do Verilator, vale a mais recente (a do Icarus se nenhuma existe
ainda). Com um testbench cocotb, `<raiz>/<módulo de teste>.fst` (Icarus)
ou `.vcd` (Verilator), a mais recente (5.3.2).

Erros: `NoTestbench` sem testbench; `NotBuilt` se o processador ainda não
foi compilado; `InvalidProject` se o `$dumpfile` do testbench é uma
expressão que o Lace não resolve (5.3, passo 5).

### 5.4 `check(&Toolchain, &Project, &CheckOptions, &Control) -> Result<CheckResult>`

Confere se o Verilog do projeto elabora, sem simular nem sintetizar. Não
gera nada. O design é o conjunto dos sintetizáveis do `.spf`, do
`Hardware/<nome>.v` de cada processador e dos `.v` de `<raiz>/TopLevel/`
(legado da AURORA), sem testbenches. A biblioteca SAPHO entra por `-y` só se
o projeto tem processadores.

Sem `file`, com `<design>` = esses arquivos:

| Passo | Comando | Quando |
|---|---|---|
| `check_syntax` | `iverilog [-g2012] -tnull -Wall [-y <SAPHO>] <design>` | sempre |
| `check_syntax` | `iverilog [-g2012] -tnull -Wall [-y <SAPHO>] -s <módulo do testbench> <design> <testbench>` | um por testbench registrado |
| `lint` | `verilator --lint-only -Wall --quiet -Wno-fatal [--top-module <topo>] [-y <SAPHO>] <design>` | com `lint` |

O primeiro passo não tem `-s`: o Icarus elabora cada módulo que ninguém
instancia como raiz, então um módulo solto também é verificado, e o projeto
não precisa ter topo. A exceção é a recursão parametrizada (um módulo que se
instancia num `generate`): aí nenhum módulo sobra como raiz, o Icarus diria
"No top level modules", e as raízes vão com `-s`: o topo do projeto, ou os
módulos que só o próprio arquivo cita. As raízes saem de uma contagem de
palavras num passe por arquivo (um projeto de 1500 arquivos leva
milissegundos), e entram em `CheckResult.targets`.

O `iverilog` só avisa de uma macro indefinida ("macro NBITS undefined (and
assumed null)") e segue com a macro vazia; o Lace trata como erro, com a
explicação de que um `` `define `` só vale para os arquivos depois dele na
lista do projeto, e o passo falha mesmo com o `iverilog` saindo com 0.
Antes, `check` e `sim` passavam com a largura errada, e só a síntese
quebrava. O `-g2012` (SystemVerilog) só entra quando há um `.sv`
no projeto: ele reserva nomes como `bit` e `logic`, que um Verilog-2001 pode
usar como sinal; a simulação segue a mesma regra. Um passo que falha
interrompe os seguintes. O `iverilog` roda com CWD na raiz. O Verilator roda
numa pasta vazia, com `--top-module` quando o projeto tem topo; com
`-Wno-fatal`, os avisos dele não fazem o `check` falhar.

Com `file`, só os módulos daquele arquivo são elaborados como raiz (um `-s`
para cada um), com o design para resolver as instâncias. Se o arquivo é
testbench, o passo é a elaboração dele com o design.

Com `processor`, o design é só o `Hardware/<nome>.v` do processador, e o
único testbench é o que o build gerou (`Simulation/<nome>_tb.v`, se
existe); os passos são os mesmos, e o lint usa o processador como topo. Os
arquivos do `.spf` e os outros processadores ficam de fora. O processador
precisa ter sido compilado (`NotBuilt`).

`CheckOptions` é `#[non_exhaustive]` e implementa `Default`: construa com
`CheckOptions::default()` e atribua os campos.

| Campo | Padrão | Efeito |
|---|---|---|
| `file` | `None` | só os módulos deste arquivo; `None`, o projeto inteiro |
| `processor` | `None` | só este processador e o testbench dele, no lugar do design do projeto |
| `lint` | `false` | acrescenta o `verilator --lint-only` |

```rust
let mut options = CheckOptions::default();
options.lint = true;
let result = check(&toolchain, &project, &options, &Control::default())?;
println!("{}: {:?}", result.targets.join(", "), result.status);
```

`CheckResult.targets` lista os módulos elaborados como raiz: os do design
(ou os do arquivo pedido) e o de cada testbench. O módulo de um testbench é
o de `testbench_module` (4.1): com dois módulos e nenhum com o nome do
arquivo, o único que o outro não instancia.

Com `lint` e sem o Verilator instalado, a verificação roda do mesmo jeito,
sem o passo `lint`, e um diagnóstico de aviso diz que o lint não rodou.

Os processadores não são compilados aqui. Compile antes com
`build_processors`, como a CLI faz.

Erros: `EmptyProject` sem nenhum arquivo Verilog registrado e sem
processadores; `InvalidProject` se um sintetizável registrado não existe;
`ComponentMissing` sem o Icarus; `SystemCompilerMissing` com `lint` e sem o
compilador do sistema (Linux e macOS).

### 5.4.1 `hierarchy(&Toolchain, &Project, &HierarchyOptions, &Control) -> Result<HierarchyResult>`

A hierarquia do design depois da elaboração: que módulo instancia qual, com
os nomes das instâncias, os parâmetros resolvidos e os blocos `generate`
expandidos. Elabora com o Icarus os mesmos arquivos do `check` (5.4), mas
grava a imagem (`-o`, sem `-tnull`) e lê dela a árvore. Não compila os
processadores, não grava relatório no histórico (é uma consulta, como
`Project::files`) e não gera nada fora da pasta de trabalho.

Com `<design>` = os arquivos do `check` e `<T>` = `<raiz>/.lace/Temp/hierarchy`,
refeita a cada chamada:

| Passo | Comando | Quando |
|---|---|---|
| `elaborate` | `iverilog [-g2012] [-y <SAPHO>] -o <T>/design.vvp <design>` | com design |
| `elaborate` | `iverilog [-g2012] [-y <SAPHO>] -s <módulo do testbench> -o <T>/tb-<n>.vvp <design> <testbench>` | um por testbench registrado |
| `elaborate` | `iverilog [-g2012] -y <SAPHO> -s <nome>_tb -o <T>/proc-<nome>.vvp Hardware/<nome>.v Simulation/<nome>_tb.v` | um por processador compilado |

Como no `check`, o design vai sem `-s`, e o Icarus elabora como raiz cada
módulo que ninguém instancia; com recursão parametrizada, as raízes vão com
`-s`, pela mesma regra. Ao contrário do `check`, uma elaboração que
falha não interrompe as outras: a árvore do design aparece mesmo com um
testbench quebrado. O cancelamento para tudo. Um processador nunca compilado
fica de fora, sem erro.

Com `processor`, o design é só o `Hardware/<nome>.v` do processador, e o
único testbench é o que o build gerou; o processador precisa ter sido
compilado (`NotBuilt`).

`HierarchyOptions` é `#[non_exhaustive]` e implementa `Default`:

| Campo | Padrão | Efeito |
|---|---|---|
| `processor` | `None` | só este processador e o testbench dele, no lugar do projeto |

`HierarchyResult` tem a estrutura comum (6.1, com um `elaborate` por
elaboração e um `icarus_image` em `artifacts` para cada uma) e:

| Campo | Conteúdo |
|---|---|
| `design` | `Elaboration` do design, ou `null` sem Verilog de design |
| `testbenches` | uma `Elaboration` por testbench: os registrados, na ordem do `.spf`, e depois o de cada processador compilado |

`Elaboration`:

| Campo | Conteúdo |
|---|---|
| `testbench` | o testbench elaborado; `null` no design |
| `processor` | o processador, quando o testbench é o que o build dele gerou |
| `status` | como esta elaboração terminou |
| `roots` | `ModuleInstance[]`: os módulos que ninguém instancia, com o que eles instanciam; vazio numa falha |
| `diagnostics` | os erros e avisos desta elaboração (também no `diagnostics` do resultado) |

`ModuleInstance`:

| Campo | Conteúdo |
|---|---|
| `name` | o nome da instância (`dut`); numa raiz, o do módulo. Dentro de blocos `generate`, `begin` ou `fork`, com o nome deles na frente (`op_add.my_add`): os blocos não viram nós |
| `module` | o módulo instanciado |
| `file`, `line` | onde o módulo é definido |
| `instance_file`, `instance_line` | onde está a instância, no módulo de cima; `null` numa raiz |
| `library` | o módulo vem da biblioteca SAPHO |
| `children` | as instâncias de dentro, na ordem do fonte |

A árvore sai do `.vvp`: cada escopo elaborado é uma linha `.scope` com o
tipo, a instância, o módulo, os índices de arquivo e linha da instância e da
definição e o escopo pai; os arquivos vêm da tabela `:file_names`. Funções e
tarefas ficam de fora. Num processador, a árvore mostra as unidades da
biblioteca SAPHO que o programa realmente usa, porque o processador só
instancia essas.

```rust
let result = hierarchy(&toolchain, &project, &HierarchyOptions::default(), &Control::default())?;
for tb in &result.testbenches {
    for root in &tb.roots {
        println!("{}: {} instâncias", root.module, root.children.len());
    }
}
```

Erros: os do `check` (`EmptyProject`, `InvalidProject`, `ModuleNotFound`
para um testbench sem módulo que dê para usar); `ProcessorNotFound` e
`NotBuilt` com `processor`; `ComponentMissing` sem o Icarus, ou sem o YANC
num projeto com processadores.

### 5.5 `synthesize(&Toolchain, &Project, &DesignTarget, &Control) -> Result<SynthesisResult>`

Síntese de visualização (a do PRISM da AURORA), sem mapear para FPGA.

| Alvo | Topo | Arquivos |
|---|---|---|
| `DesignTarget::TopLevel` | o nome do arquivo de topo (`project.top_level()`, ver 3.4) | o design de `check` (5.4) |
| `DesignTarget::Processor(nome)` | o processador | `Hardware/<nome>.v` |

Quando o projeto tem processadores, todo `.v` da biblioteca SAPHO
(`yanc/SAPHO/` do bundle) que não é testbench entra primeiro.

Script gravado em `.lace/Temp/synth/<topo>/yosys_script.ys`:

```
read_verilog -setattr src "<arquivo>"     (um por arquivo)
hierarchy -top <topo>
proc
tee -q -o "<dir>/check.log" check
setundef -zero
opt_clean -purge
write_json "<dir>/hierarchy.json"
tee -q -o "<dir>/stat.json" stat -json -top <topo>
```

Cada `read_verilog` leva `-I "<raiz>"` e, num `.sv`, `-sv`. Execução: `yosys
-q -l <dir>/yosys.log -s <dir>/yosys_script.ys`, com CWD na raiz do projeto,
como a simulação: um `$readmemh("rtl/rom.mif")` relativo à raiz vale nas
duas (o Yosys também procura na pasta do arquivo que lê).
`SynthesisResult.modules` lista os módulos do netlist que dá para desenhar:
sem as caixas-pretas (um módulo vazio vira `blackbox` e fica no netlist
mesmo fora da árvore do topo).

O `check` do Yosys vai só para o `check.log` (`tee -q`): no processador SAPHO
ele avisa de cada fio sem driver da biblioteca. Do `check.log`, viram aviso
(com o arquivo e a linha do primeiro `source:`) só o laço combinacional
("found logic loop") e os drivers em conflito, que nem o Icarus nem a
síntese apontam.

Um módulo do design que se instancia sem `if`, `case` nem `for` no corpo
nunca para: o Icarus recusa, e o Yosys geraria um nível atrás do outro até
ser interrompido. A síntese falha antes de rodar o Yosys, com um erro que
diz o módulo. Se o Yosys cai (sinal 11) e um módulo do design se instancia,
um erro relaciona a queda com a recursão.

O `write_json` do Yosys grava cada byte fora do ASCII como `\uFFFFFFxx`, o que
estraga os caminhos com acento nos atributos `src` e faz o `read_json` do
próprio Yosys recusar o netlist. Depois da síntese, o Lace devolve esses
bytes ao texto UTF-8 que eram.

`SynthesisResult.statistics` é o que o `stat` do Yosys contou no mesmo
netlist (`SynthesisStatistics`, em `stats.rs`): módulos, fios, bits de fio,
fios e bits com nome do usuário, memórias, bits de memória, processos,
células e as células por tipo, da seção `design` do `stat -json`, que soma
os submódulos (as instâncias de submódulo ficam fora das células). Uma
contagem que o Yosys não informou é `None`, diferente de zero. As células
são as genéricas do Yosys, depois de `proc` e `opt_clean`: nada aqui estima
LUT, DSP, ocupação ou temporização. `None` quando a síntese falhou ou o
`stat.json` desta síntese não existe.

### 5.6 `render_schematic(&Toolchain, &netlist, módulo, &SchematicOptions, &Control) -> Result<SchematicResult>`

1. Confere que o módulo está no netlist; um módulo fora da árvore do topo
   não é sintetizado, e o erro é `ModuleNotFound`, com os módulos do
   netlist em `available` (antes, `InvalidNetlist`, como se o netlist
   estivesse quebrado).
2. Passo `graph`: `yosys -q -s show.ys`, CWD `<dir>`, com o script
   `read_json "<netlist>"` e `show -format dot -prefix <módulo> [-width] <módulo>`.
   Gera `<dir>/<módulo>.dot`.
3. Passo `render`: `dot -Tsvg <módulo>.dot -o <módulo>.svg`, CWD `<dir>`. Uma
   mensagem `Error:` do `dot` conta como falha mesmo com código 0. No Linux,
   o Lace grava `<dir>/fonts.conf` a partir do modelo do OSS CAD Suite e
   passa `FONTCONFIG_FILE` e `FONTCONFIG_PATH`: o `dot` usa as fontes do
   bundle, com o cache em `<dir>/fontconfig-cache`. Sem isso ele leria a
   configuração de fontes do sistema, e o SVG mudaria de máquina para
   máquina. No macOS e no Windows o pacote não traz fontes e o `dot` usa as
   do sistema.

`SchematicOptions::bus_widths` (padrão: sim) escreve a largura dos
barramentos nas arestas. O visual é o do Graphviz, não o do netlistsvg da
AURORA.

Não há teto de ligações: qualquer módulo desenha. O `dot` fica lento com
muitas ligações e portas compartilhadas (40 instâncias de um registrador de 8
bits ligadas ao mesmo `clk` e à mesma entrada, 160 ligações, levaram mais de
40 s), e o prazo dele é a única proteção:

| Opção | Padrão | O que faz |
|---|---|---|
| `timeout` | `Some(SCHEMATIC_TIMEOUT)`, 60 s | prazo do passo do `dot`; vencido, o passo termina como `timed_out` |

Nomes de módulo com caracteres fora de `[A-Za-z0-9_.-]` (por exemplo
`$paramod\processor\NUBITS=23`) viram `_` no nome dos arquivos.

### 5.7 `open_waveform(&Toolchain, &onda, &ViewerOptions) -> Result<RunningProcess>`

`surfer-aurora <onda> [-c <arquivo.sucl> | -s <arquivo.surf.ron>]`, destacado,
CWD na pasta da onda. Retorna na hora. O stdout e o stderr vão para
`<onda>.log` (`contador_tb.fst.log`) numa pasta oculta, nunca ao lado da
onda: `.lace/Temp/surfer/` do projeto que contém a onda; com a onda fora de
projeto, a pasta de cache do usuário (`$XDG_CACHE_HOME/lace/surfer` ou
`~/.cache/lace/surfer` no Linux, `~/Library/Caches/lace/surfer` no macOS,
`%TEMP%\lace\surfer` no Windows). Abrir a mesma onda de novo sobrescreve o
log; `RunningProcess::log_file()` diz onde ele ficou.
Não recebe `Control`: o surfer-aurora não é um passo da operação, e o
cancelamento não o fecha.

O surfer-aurora recebe só as variáveis de ambiente que uma aplicação gráfica precisa
(`DISPLAY`, `WAYLAND_DISPLAY`, `HOME`, `XDG_*`, `DBUS_SESSION_BUS_ADDRESS` no
Linux; `USERPROFILE`, `APPDATA`, `LOCALAPPDATA` no Windows).

`RunningProcess`:

| Método | Faz |
|---|---|
| `id()` | PID |
| `try_wait()` | `Some(Termination)` se já terminou, sem bloquear |
| `wait_timeout(d)` | espera até `d` |
| `ensure_started(d)` | erro `ProcessExitedEarly`, com o fim do log, se o processo terminou dentro de `d` |
| `wait()` | bloqueia até terminar |
| `kill()` | encerra (não é erro se já tinha terminado) |
| `log_file()` | o log |

Soltar o `RunningProcess` não fecha o Surfer. Sem display, o Surfer morre em
menos de um segundo com código 101; `ensure_started` de um ou dois segundos
logo depois de abrir detecta isso (a CLI usa 1,5 s).

`ViewerOptions::working_dir` troca a pasta onde o Surfer roda (padrão: a da
onda). É nela que ele procura `.surfer/mappings/`, os tradutores de valor de
um layout preparado (5.7.1).

### 5.7.1 O layout dos processadores SAPHO: `wave_layout(&onda) -> Result<Option<WaveLayout>>`

Monta, em memória, o estado do Surfer que mostra cada processador SAPHO da
onda como a AURORA mostrava. O YANC já põe na onda as variáveis
do programa, o PC (`valr2`) e a linha do fonte (`linetabs`); o layout dá
nome a esses números e os arruma em grupos:

| Grupo | O que tem |
|---|---|
| `Top-level` | os sinais da raiz do testbench (o `clk` e o `rst`) |
| `<instância> (<processador>)` | um por processador, aberto |
| ↳ (sem grupo) | `clk`, `rst` e `itr` do núcleo |
| ↳ `I/O` | `req_in N`, `input N`, `out_en N`, `output N`, com `N` o número da porta, lido do nome do sinal (`in_sim_2` é `input 2` mesmo que seja a única entrada usada) |
| ↳ `Instructions` | `Assembly (<proc>)`: o `valr2` como a instrução (`LOD_V main_x 2`); `C± (<proc>)`: o `linetabs` como a linha do fonte |
| ↳ `Variables` | `int` com sinal, `float` como o número real que o YANC calcula na simulação, complexo como `re imi`; cada array num grupo fechado |
| ↳ `Flags` | fechado: pilha de dados e de instruções (`pointeri` em degrau), ULA (`delta_int`, `delta_float`) |

O processador é o escopo com `valr2` e `linetabs`. O nome dele sai do
subescopo `p_<nome>.core` que o YANC cria; sem ele, do pai `<nome>_inst`, da
raiz `<nome>_tb` ou da instância. As tabelas vêm da pasta temporária do
processador com esse nome no projeto que contém a onda (fora de projeto, da
pasta da onda, se ela tem `pc_<nome>_mem.txt`):

| Tabela do YANC | Tradutor | Sinal |
|---|---|---|
| `trad_opcode.txt` (`<índice> <mnemônico> <operando>`) | `lace_asm_<proc>`, `Bits` = largura do `valr2` | `valr2` |
| `trad_cmm.txt` (`<linha> <texto>`; -1, -2 e -3 viram `0xFFFFF`, `0xFFFFE`, `0xFFFFD` em 20 bits) | `lace_src_<proc>` | `linetabs` |
| os valores dos complexos no corpo da onda | `lace_complex`, um `0b<bits> <re> <im>i` por valor | `comp_me3_*`, `comp_arr_me3_*` |

Linha de tabela sem texto sai: o Surfer recusa o tradutor inteiro por ela.
Sem a tabela, o sinal aparece como número (`Unsigned`, `Signed`).

Uma tabela (ou o `pc_<nome>_mem.txt`) gravada depois da onda é de outro
build: o processador foi compilado de novo depois da simulação, e o assembly
e a linha do C± sairiam deslocados. Nenhuma tabela é usada, o PC e a linha
aparecem como números, e `WaveProcessor::outdated` diz por quê, para a
interface avisar ("simule de novo").

O tradutor dos complexos lê o corpo da onda inteiro, só nos sinais
complexos: num VCD, em bytes e só nas linhas `b<bits> <id>` de um id
complexo; num FST, pelo filtro de sinais do leitor. Para em
`MAX_COMPLEX_VALUES` (16384) valores distintos. Um complexo que muda a cada clock passa disso, e os valores
seguintes aparecem em binário. Numa onda de 100 MB com um complexo, a
varredura custa alguns segundos antes de abrir o Surfer.

Detalhes dos nomes: o rótulo da linha do fonte é `C (<proc>)` num
processador em C e `C± (<proc>)` em C±; o elemento de um array leva o índice
lido inteiro depois do último `_e_` (o `asmcomp` usa `%04d`, então o 10000
tem cinco dígitos e fica no mesmo grupo); a variável de uma função com
`_v_` no nome (`get_v_x`) é desfeita pelos pares do `cmm_log.txt` do build;
um escopo com `.` no nome (o identificador escapado `\u.pa `) continua um
componente só no caminho do `.surf.ron`; `delta_int` e `delta_float`, que o
Icarus grava como `real`, ficam sem formato de bits.

Uma onda sem processador SAPHO (um projeto só de Verilog) recebe só o grupo
`Top-level`, com os sinais da raiz do testbench, como a AURORA; o resto do
design fica na hierarquia do Surfer. O FST é lido pela crate `fst-reader`,
e a hierarquia dele vira o mesmo texto do cabeçalho de um VCD.
`Ok(None)`: a onda não é VCD nem FST (o GHW abre sem layout), ou não tem
nem sinal na raiz nem processador.
`WaveLayout`: `state` (o `.surf.ron`), `mappings` (`MappingTranslator`:
`name`, `content`) e `processors` (`WaveProcessor`: `instance`,
`processor`, `variables`, `assembly`, `source`, `outdated`).

### 5.7.2 `prepare_wave_layout(&onda) -> Result<Option<PreparedLayout>>`

Grava o layout em `.lace/Temp/surfer/<onda sem extensão>-<hash do caminho>/` do projeto que
contém a onda (fora de projeto, na pasta de cache do usuário, como o log do
Surfer): `<onda>.surf.ron` e os tradutores em `.surfer/mappings/<nome>`, sem
os de antes. `ViewerOptions::with_layout(&preparado)` abre com ele:

```rust
if let Some(layout) = lace_core::prepare_wave_layout(&onda)? {
    open_waveform(&toolchain, &onda, &ViewerOptions::with_layout(&layout))?;
}
```

O formato do `.surf.ron` é o do surfer-aurora do bundle (base Surfer 0.7.0):
os identificadores dos sinais são marcadores, e o Surfer reacha cada sinal
pelo caminho e pelo nome.

### 5.8 Cancelamento, prazo e saída ao vivo: `Control`

Toda operação que executa ferramentas (`build`, `build_processors`, `check`,
`simulate`, `simulate_project`, `synthesize`, `render_schematic`) recebe um
`&Control` como último argumento; em `build_processors`, antes de
`on_result`. Ele leva o pedido de cancelamento e, se houver, quem recebe os
eventos. `Control::default()` não cancela e não avisa nada.

| Tipo | O que é |
|---|---|
| `CancelToken` | o pedido de cancelamento. `cancel()` pede, de qualquer thread; `is_cancelled()` consulta. Os clones são o mesmo pedido. Um pedido feito não se desfaz: para a operação seguinte, crie outro. `CancelToken::from(Arc<AtomicBool>)` liga o pedido a uma flag que outro código marca, como a de um tratador de sinal |
| `Control` | `Control::new()`, `.with_cancel(token)` e `.on_event(receptor)`; `cancel_token()` e `is_cancelled()` consultam |
| `Event` | o que aconteceu, avisado na hora: `StepStarted`, `Output`, `StepFinished` |
| `Stream` | de qual saída veio uma linha: `stdout` ou `stderr` |

`Event` em JSON, com o tipo no campo `event` (schema em
`docs/schema/events.json`):

| `event` | Campos | Quando |
|---|---|---|
| `step_started` | `step`, `tool`, `command` (a `Invocation`) | o processo do passo vai ser iniciado |
| `output` | `step`, `tool`, `stream`, `line`, `diagnostic` | a cada linha que o processo escreve, sem o fim de linha |
| `step_finished` | `step`, `tool`, `termination`, `duration_ms` | o processo do passo terminou |

`diagnostic` diz se a linha volta como `Diagnostic` no resultado (seção 7):
tudo do stderr, todo o stdout dos compiladores (YANC e `iverilog`) e, no
stdout de uma simulação, as linhas do próprio simulador (`$finish called
at`, `VCD info:` ou `FST info:`, o `ERROR:` de um `$error`). `false` é saída
do programa, como o `$display` do testbench. A decisão é linha a linha
(`diagnostics::is_message`). Os eventos só adiantam o que chega depois no
resultado (`StepReport`). As linhas de stdout e de stderr chegam na ordem em
que o Lace as leu, que pode não ser a ordem exata em que a ferramenta as
escreveu.

O receptor roda na thread da operação, no meio dela: deve ser rápido
(mandar o evento por um canal, escrever uma linha) e não pode chamar outra
operação do Lace.

Numa GUI, a operação roda numa thread de trabalho, e a interface fica com um
clone do `CancelToken` (`Control`, `CancelToken` e `Event` são
`Send + Sync`):

```rust
use std::sync::mpsc;
use lace_core::*;

// toolchain e project abertos como na seção 1.
let cancel = CancelToken::new();
let (events, received) = mpsc::channel();
let control = Control::new()
    .with_cancel(cancel.clone())
    .on_event(move |event| {
        let _ = events.send(event.clone());
    });

let worker = std::thread::spawn(move || {
    let options = SimulationOptions::new(Simulator::Icarus);
    simulate_project(&toolchain, &project, &options, &control)
});
// Na thread da interface: mostra cada linha; o botão de parar chama
// cancel.cancel().
for event in received {
    if let Event::Output { line, .. } = event {
        println!("{line}");
    }
}
let result = worker.join().expect("a thread terminou")?;
```

**O que acontece com os processos.** O Lace lê stdout e stderr linha a
linha, em duas threads, e confere o cancelamento e o prazo a cada 20 ms,
mesmo enquanto a ferramenta escreve sem parar. Encerrar é encerrar tudo o
que a ferramenta iniciou: o `iverilog` roda o `ivlpp` e o `ivl`, o
Verilator roda o `make`, que roda o compilador C++.

- **Unix:** cada passo roda num grupo de processos próprio. O Lace manda
  SIGTERM ao grupo e, se o processo não sair em 1 s, SIGKILL. Fora do grupo
  do Lace, os filhos não recebem o Ctrl+C do terminal: quem os encerra é o
  Lace, ao ver o pedido.
- **Windows:** cada passo roda num Job Object com
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (`ProcessJob`): encerrar é fechar o
  job, e tudo o que a ferramenta iniciou termina junto. O que sobrar quando o
  passo acaba também termina, e o mesmo vale se o processo do Lace morrer,
  mesmo à força: o sistema fecha o handle do job. Se o sistema recusar o
  job, o `taskkill /T /F` do `System32` encerra a árvore. O `taskkill.exe` é
  o único programa do sistema que o Lace roda fora da exceção do Verilator,
  e só serve para encerrar ([BUNDLE.md](BUNDLE.md), seção 4).
  Nenhuma ferramenta abre janela de console (`hide_console`): se o Lace tem
  console (um terminal, ou o que quem o criou lhe deu), ela divide esse
  console, e o Ctrl+C chega a ela também; se não tem (o Studio), ela leva
  `CREATE_NO_WINDOW` e ganha um console próprio, sem janela, ao custo de um
  `conhost.exe`, uns 16 ms por ferramenta. Quem cria processo fora do Core,
  como o Studio ao chamar a CLI, usa o mesmo `ProcessJob` e o mesmo
  `hide_console`.

O passo encerrado termina com `Termination::Cancelled` ou
`Termination::TimedOut`, e a operação, com `Status::Cancelled` ou
`Status::TimedOut` e `failed_step` nesse passo. É `Ok`, não `Err`: a
operação rodou, e o resultado traz os passos que chegaram a rodar, com o que
as ferramentas escreveram até ali. Cancelada, nenhum passo seguinte começa;
um pedido feito antes da operação devolve `cancelled` sem passo nenhum, com
`failed_step` no primeiro, que não rodou. Só uma simulação que deu certo
traz `waveform` no resultado: uma cancelada ou que passou do prazo não traz,
mesmo que o arquivo da onda exista.

**Prazo.** Só a simulação tem prazo (`SimulationOptions::timeout`, 5.2), e
só no passo `simulate`: é onde um código do usuário pode não terminar, como
um testbench sem `$finish`. Sem prazo por padrão.

**O custo da saída ao vivo.** Num pipe, o `vvp` guarda o stdout em buffer:
o `$display` só sai quando o buffer enche ou no fim, e se perde se o
processo for morto. Com um receptor de eventos, o Lace roda o `vvp` com `-i`, que
tira o buffer, ao custo de uma escrita por linha. Sem receptor, fica o
buffer, que é mais rápido. O Verilator compila sempre com `--autoflush` (5.2):
trocar a flag conforme o receptor recompilaria o modelo.

### 5.9 Relatório e histórico: o módulo `history`

Depois de uma operação, a interface chama `history::record` com o que cada
fase devolveu, e o Lace grava um relatório no histórico do projeto. Nenhuma
operação grava sozinha: quem decide é quem chama (a CLI grava depois de
`build`, `check`, `sim` e `synth`).

```rust
use lace_core::history::{self, Operation};

let started = std::time::SystemTime::now();
let builds = lace_core::build_processors(&toolchain, project.buildable_processors(), &opts, OnFailure::Stop, &control, |_| {})?;
let sim = lace_core::simulate_project(&toolchain, &project, &options, &control)?;
let operation = Operation::new("lace sim", started)
    .with_builds(&builds)
    .with_simulation(&sim);
let record = history::record(&project, &toolchain, &operation)?;   // run-000042
```

```
<projeto>/.lace/reports/
  run-000001/
    report.txt     o relatório, para gente
    record.json    o que a comparação usa (RunRecord)
  sequence         o último número reservado
```

O número só cresce, e o de um relatório apagado não volta. A pasta é
montada com outro nome e renomeada no fim: quem lê nunca vê um relatório
pela metade. Duas operações gravando ao mesmo tempo no mesmo projeto ficam
com números seguidos.

Apagar é em dois passos, para a interface mostrar e confirmar antes:
`plan_cleanup` escolhe, sem apagar, e `remove` apaga exatamente a lista
escolhida. Um relatório gravado entre os dois não sai sem ter sido mostrado.

```rust
use lace_core::history::{self, Cleanup};

let doomed = history::plan_cleanup(&project, &Cleanup::KeepLatest(10))?;
// mostrar `doomed` e confirmar
let removed = history::remove(&project, &doomed)?;
```

| Função | Faz |
|---|---|
| `record(&Project, &Toolchain, &Operation) -> Result<RunRecord>` | grava e devolve o relatório |
| `list(&Project) -> Result<Vec<RunSummary>>` | os relatórios, do mais novo para o mais antigo; um `record.json` ilegível aparece com `readable: false` |
| `load(&Project, id)`, `latest(&Project)` | um relatório (`run-000042` ou `42`), o mais novo |
| `report_text(&Project, id)`, `report_path(&Project, id)` | o `report.txt` e onde ele fica |
| `compare_reports(&Project, atual, referência)` | escolhe e compara, como `lace report compare`; os dois vêm do histórico do projeto, então se comparam mesmo depois de a pasta mudar de lugar; sem parte nenhuma em comum (uma simulação contra uma síntese), `NotComparable` |
| `previous_comparable(&Project, &RunRecord)` | o anterior mais novo que se compara em pelo menos uma parte, de preferência um que terminou bem (uma simulação que estourou o prazo só serve sem outra) |
| `compare(&referência, &atual) -> Result<RunComparison>` | compara dois relatórios; não lê nem roda nada |
| `compare_metric(Option<u64>, Option<u64>)` | compara dois números |
| `parse_id(texto)` | `42` ou `run-000042` para `run-000042` |
| `plan_cleanup(&Project, &Cleanup) -> Result<Vec<String>>` | os relatórios que `Cleanup::All`, `KeepLatest(n)` ou `Reports(ids)` escolhe, do mais antigo para o mais novo; `ReportNotFound` se um id pedido não existe, e então nada é escolhido |
| `remove(&Project, &ids) -> Result<Vec<String>>` | apaga e devolve os que saíram; um que já não existe é pulado. Grava em `sequence` o maior número publicado antes de apagar, e renomeia cada pasta para um nome oculto antes de apagá-la |

`RunRecord` tem `metadata` (`RunMetadata`: comando, status, duração, versão
do Lace, um hash da raiz do projeto, bundle, topo, a máquina e o contexto da
síntese e da simulação, com hashes dos fontes e das entradas), `synthesis`
(as `SynthesisStatistics` da síntese) e `simulation` (`SimulationTimings`:
compilação, execução e total em milissegundos, e o tempo simulado do
`$finish` do `vvp` em femtossegundos).

A comparação segue a do Alpha-Solar. Relatórios de projetos diferentes não
se comparam (`NotComparable`). A síntese se compara quando os dois têm
estatísticas do mesmo topo; a simulação, quando os dois têm tempos do mesmo
simulador e testbench. O resto que difere vira aviso em `warnings`, sem
invalidar a comparação: fontes, entradas, versões das ferramentas, onda
ligada num só, tempo simulado diferente, a máquina (um hash do sistema, da
arquitetura, do processador, do número de processadores e do nome da
máquina), simulação que não terminou bem, versão do Lace. Cada número
comparado é um `MetricComparison`: os dois valores, a `Change`
(`unchanged`, `increased`, `decreased`, `added`, `removed`,
`not_comparable`), a diferença e a porcentagem da referência, que é `None`
de zero para outro número. Os tipos de célula vêm da maior mudança para a
menor e, no empate, pelo nome.

A máquina vem do que o sistema informa sem rodar outro programa: o `uname`
no Linux e no macOS, `/etc/os-release`, `/proc/cpuinfo` e `/proc/meminfo` no
Linux. O que não dá para saber assim (o modelo do processador e a memória
fora do Linux) fica `None`.

---

## 6. Resultados

Todo resultado implementa `Serialize`, e o JSON da CLI é essa serialização.
O JSON Schema de cada tipo é gerado do próprio tipo e fica em
`docs/schema/`, nos `$defs` do schema do comando da CLI que o escreve:
`BuildResult` em `build.json`, `CheckResult` em `check.json`,
`SimulationResult` em `sim.json`, `SynthesisResult` e `SchematicResult` em
`synth.json`, `HierarchyResult` em `hierarchy.json`, `MovedPath` em
`move.json`, `Event` em `events.json`. Um teste confere que os arquivos são
o que os tipos geram.

### 6.1 Estrutura comum

| Campo | Tipo | Conteúdo |
|---|---|---|
| `status` | `Status` | `succeeded`, `failed`, `crashed`, `incomplete`, `cancelled`, `timed_out` |
| `failed_step` | `Option<Step>` | o passo que falhou ou que o Lace encerrou; num cancelamento pedido antes de um passo começar, esse passo, que não rodou |
| `steps` | `Vec<StepReport>` | um por programa executado, na ordem |
| `diagnostics` | `Vec<Diagnostic>` | mensagens interpretadas de todos os passos |
| `artifacts` | `Vec<Artifact>` | arquivos esperados, com `fresh` |
| `duration_ms` | `u64` | quanto a operação levou, do começo ao fim, com a preparação do Lace |

`CheckResult` não tem `artifacts` e tem `targets`, os módulos elaborados
como raiz (5.4). `HierarchyResult` tem `design` e `testbenches`, e o
`status` de cada elaboração dentro deles (5.4.1).

`Status`:

| Valor | Quando |
|---|---|
| `succeeded` | todos os passos com código 0 e todo artefato obrigatório gerado agora |
| `failed` | um passo terminou com código diferente de 0, ou o `vvp` escreveu `ERROR:` ou `FATAL:` (5.2) |
| `crashed` | um passo morreu por sinal (Unix) ou exceção NTSTATUS (Windows): defeito da ferramenta |
| `incomplete` | os passos deram certo, mas falta artefato obrigatório |
| `cancelled` | o cancelamento foi pedido (`CancelToken`): o passo que rodava foi encerrado e os seguintes não rodaram (5.8) |
| `timed_out` | um passo passou do prazo e o Lace o encerrou; numa simulação, é o testbench que não chega ao `$finish` (5.8) |

Os passos param no primeiro que falhar e quando o cancelamento é pedido. A
exceção é `hierarchy`, em que cada elaboração roda mesmo que uma anterior
tenha falhado (5.4.1).

### 6.2 Passos por operação

| Operação | `Step` / `Tool` |
|---|---|
| `build` C± | `compile`/`cmmcomp`, `pre_assemble`/`appcomp`, `assemble`/`asmcomp` |
| `build` C | `preprocess`/`cpppp`, `compile`/`cppcomp`, `pre_assemble`, `assemble` |
| `simulate*` Icarus | `elaborate`/`iverilog`, `simulate`/`vvp` |
| `simulate*` Verilator | `verilate`/`verilator`, `simulate`/`verilator` |
| `check` | `check_syntax`/`iverilog` (um para o design, um por testbench), `lint`/`verilator` (com `lint`) |
| `hierarchy` | `elaborate`/`iverilog` (um para o design, um por testbench) |
| `synthesize` | `synthesize`/`yosys` |
| `render_schematic` | `graph`/`yosys`, `render`/`dot` |

`StepReport` guarda `command` (programa, argumentos, CWD, ambiente),
`termination`, `stdout`, `stderr` e `duration_ms`. Com ele dá para reproduzir
qualquer passo à mão.

`Termination` em JSON: `{"kind": "exited", "value": 1}`,
`{"kind": "signaled", "value": 11}`, `{"kind": "exception", "value": ...}`,
`{"kind": "cancelled"}`, `{"kind": "timed_out"}`, `{"kind": "unknown"}`.
`cancelled` e `timed_out` são processos que o próprio Lace encerrou (5.8);
`signaled` é um processo que morreu por um sinal que não veio do Lace.

### 6.3 Artefatos

`fresh` compara o horário de modificação antes e depois da operação: um
arquivo que sobrou de uma execução anterior aparece com `fresh: false`.
Obrigatórios aparecem sempre; intermediários, só se existirem.

| `ArtifactKind` | Arquivo | Produzido por |
|---|---|---|
| `assembly` | `Software/<nome>.asm` | `build` |
| `verilog` | `Hardware/<nome>.v` | `build` |
| `data_memory` | `Hardware/<nome>_data.mif` | `build` |
| `instruction_memory` | `Hardware/<nome>_inst.mif` | `build` |
| `testbench` | `Simulation/<nome>_tb.v`, copiado de `T/<nome>_tb.v` | `build` |
| `preprocessed_source` | `T/pp.cpp` | `build` C |
| `program_counter_map` | `T/pc_<nome>_mem.txt` | `build` |
| `source_translation` | `T/trad_cmm.txt` | `build` |
| `opcode_translation` | `T/trad_opcode.txt` | `build` |
| `compiler_log` | `T/cmm_log.txt` | `build` |
| `pre_assembler_log` | `T/app_log.txt` | `build` |
| `icarus_image` | `<trabalho>/<topo>.vvp`; na hierarquia, `hierarchy/design.vvp`, `hierarchy/tb-<n>.vvp`, `hierarchy/proc-<nome>.vvp` | `simulate*` Icarus, `hierarchy` |
| `verilated_model` | `<trabalho>/obj_dir_<topo>/V<topo>`; na simulação rápida, `obj_dir_fast_<topo>` | `simulate*` Verilator |
| `waveform` | nome do `$dumpfile`, ou o da onda injetada (5.3) | `simulate*` |
| `simulation_output` | `Simulation/output_<n>.txt` | `simulate*` |
| `netlist` | `synth/<topo>/hierarchy.json` | `synthesize` |
| `synthesis_statistics` | `synth/<topo>/stat.json` (não obrigatório) | `synthesize` |
| `schematic_graph` | `synth/<topo>/<módulo>.dot` | `render_schematic` |
| `schematic` | `synth/<topo>/<módulo>.svg` | `render_schematic` |

### 6.4 Exemplo: build com erro

Um elemento de `results` na saída real de
`lace -C com_erro build -p conta --json` (caminhos encurtados, bundle em
`/opt/lace/toolchain`, artefatos cortados):

```json
{
  "processor": "conta",
  "language": "cmm",
  "status": "failed",
  "failed_step": "compile",
  "frequency_mhz": 100,
  "clocks": 2000,
  "steps": [
    {
      "step": "compile",
      "tool": "cmmcomp",
      "command": {
        "program": "/opt/lace/toolchain/yanc/bin/cmmcomp",
        "args": ["-i", "conta.cmm", "-n", "conta", "-p", "/home/eu/projetos/com_erro/conta",
                 "-m", "/opt/lace/toolchain/yanc/Macros", "-t", "/home/eu/projetos/com_erro/.lace/Temp/conta", "-en"],
        "cwd": "/home/eu/projetos/com_erro/conta",
        "env": [],
        "inherit": []
      },
      "termination": { "kind": "exited", "value": 1 },
      "stdout": "",
      "stderr": "Error on line 16: c'mon dude, declare the variable 'total' properly!\n",
      "duration_ms": 3
    }
  ],
  "diagnostics": [
    {
      "tool": "cmmcomp",
      "severity": "error",
      "message": "c'mon dude, declare the variable 'total' properly!",
      "file": "/home/eu/projetos/com_erro/conta/Software/conta.cmm",
      "line": 16,
      "column": null,
      "raw": "Error on line 16: c'mon dude, declare the variable 'total' properly!"
    }
  ],
  "artifacts": [
    { "kind": "assembly", "path": "/home/eu/projetos/com_erro/conta/Software/conta.asm", "required": true, "fresh": true },
    { "kind": "verilog", "path": "/home/eu/projetos/com_erro/conta/Hardware/conta.v", "required": true, "fresh": false }
  ]
}
```

O `.asm` aparece como `fresh` porque o `cmmcomp` o abre para escrita antes de
achar o erro; o `.v` não, porque o `asmcomp` nem rodou.

### 6.5 Exemplo: simulação

Exemplo de `lace -C soma sim -p soma --json`, campo `simulation` (caminhos
encurtados, bundle em `/opt/lace/toolchain`). No Linux e no macOS, o
`iverilog` e o `vvp` do bundle são lançadores em bash, que o Lace roda pelo
`/bin/bash` com `PATH=/usr/bin:/bin` (9.1):

```json
{
  "top": "soma_tb",
  "simulator": "icarus",
  "status": "succeeded",
  "failed_step": null,
  "steps": [
    {
      "step": "elaborate",
      "tool": "iverilog",
      "command": {
        "program": "/bin/bash",
        "args": ["/opt/lace/toolchain/oss-cad-suite/bin/iverilog",
                 "-grelative-include", "-I", "/home/eu/projetos/soma/soma/Simulation", "-I", "/home/eu/projetos/soma/.lace/Temp/soma",
                 "-y", "/opt/lace/toolchain/yanc/SAPHO", "-s", "soma_tb", "-o", "/home/eu/projetos/soma/.lace/Temp/soma/soma_tb.vvp",
                 "/home/eu/projetos/soma/soma/Hardware/soma.v", "/home/eu/projetos/soma/.lace/Temp/soma/instr_soma_tb.v"],
        "cwd": "/home/eu/projetos/soma/.lace/Temp/soma",
        "env": [["PATH", "/usr/bin:/bin"]],
        "inherit": []
      },
      "termination": { "kind": "exited", "value": 0 },
      "stdout": "",
      "stderr": "",
      "duration_ms": 144
    },
    {
      "step": "simulate",
      "tool": "vvp",
      "command": {
        "program": "/bin/bash",
        "args": ["/opt/lace/toolchain/oss-cad-suite/bin/vvp", "-n", "/home/eu/projetos/soma/.lace/Temp/soma/soma_tb.vvp", "-fst"],
        "cwd": "/home/eu/projetos/soma/.lace/Temp/soma",
        "env": [["PATH", "/usr/bin:/bin"]],
        "inherit": []
      },
      "termination": { "kind": "exited", "value": 0 },
      "stdout": "FST info: dumpfile soma_tb.fst opened for output.\nInfo: end of program!\n/home/eu/projetos/soma/.lace/Temp/soma/instr_soma_tb.v:61: $finish called at 985000 (1ps)\n",
      "stderr": "",
      "duration_ms": 66
    }
  ],
  "diagnostics": [
    { "tool": "vvp", "severity": "info", "message": "FST info: dumpfile soma_tb.fst opened for output.",
      "file": null, "line": null, "column": null, "raw": "FST info: dumpfile soma_tb.fst opened for output." },
    { "tool": "vvp", "severity": "info", "message": "$finish called at 985000 (1ps)",
      "file": "/home/eu/projetos/soma/soma/Simulation/soma_tb.v", "line": 61, "column": null,
      "raw": "/home/eu/projetos/soma/.lace/Temp/soma/instr_soma_tb.v:61: $finish called at 985000 (1ps)" }
  ],
  "artifacts": [
    { "kind": "icarus_image", "path": "/home/eu/projetos/soma/.lace/Temp/soma/soma_tb.vvp", "required": true, "fresh": true },
    { "kind": "waveform", "path": "/home/eu/projetos/soma/.lace/Temp/soma/soma_tb.fst", "required": true, "fresh": true },
    { "kind": "simulation_output", "path": "/home/eu/projetos/soma/soma/Simulation/output_0.txt", "required": false, "fresh": true }
  ],
  "waveform": { "path": "/home/eu/projetos/soma/.lace/Temp/soma/soma_tb.fst", "format": "fst" },
  "outputs": ["/home/eu/projetos/soma/soma/Simulation/output_0.txt"],
  "missing_inputs": []
}
```

---

## 7. Diagnósticos

`Diagnostic` tem `tool`, `severity`, `message`, `file`, `line`, `column` e
`raw` (o texto original, com as linhas de continuação do Verilator). Os três
campos de localização são opcionais.

| Ferramenta | Formato reconhecido | Arquivo |
|---|---|---|
| `cmmcomp`, `appcomp`, `asmcomp` | `Error on line N: ...`, `Syntax error on line N. ...`, `Heads up on line N: ...`, `Warning on line N: ...`, `Error: ...`, `Info: ...`, `Error in function f: ...` (sem linha); `asmcomp`: `Error: line N of file 'x' ...` | o fonte do processador (preenchido pelo Lace), ou o arquivo de dados citado |
| `cpppp`, `cppcomp` | `<arquivo>:<linha>: error: ...`, `<ferramenta>: ...` | o `pp.cpp`: o `cpppp` não emite `#line` |
| `iverilog`, `vvp` | `<arquivo>:<linha>: error: ...`, `<arquivo>:<linha>: syntax error`, `<arquivo>: No such file or directory`, `ERROR: ...`, `FATAL: ...` | o que a ferramenta cita |
| `verilator` | `%Error: <arquivo>:<linha>:<coluna>: ...`, `%Warning-<CÓDIGO>: ...` (o código vai para o fim da mensagem entre colchetes); erros do `g++` no formato C | o que a ferramenta cita |
| `yosys` | `<arquivo>:<linha>: ERROR: ...`, `ERROR: ...`, `Warning: ...`, com ou sem espaço depois dos dois-pontos; a gravidade no começo da linha vem antes do local (o aviso de latch traz um `proc_dlatch.cc:542:` do Yosys no meio da mensagem) | o que a ferramenta cita |
| `dot` | `Error: <arquivo>: syntax error in line N ...`, `Warning: ...` | o `.dot` citado |

Regras:

- Linha do stderr que não casa com nada vira `severity: unknown`. Nunca é
  descartada.
- Linha do stdout que não casa só vira diagnóstico nas ferramentas cujo
  stdout é feito de mensagens (YANC e `iverilog`). No `vvp`, no modelo do
  Verilator, no Yosys e no `dot`, o stdout é saída do programa (o
  `$display` do testbench, o log do `make`) e fica só no `StepReport`.
- Resumos (`2 error(s) during elaboration.`, `%Error: Exiting due to ...`)
  e `$finish called` são `info`.
- No `vvp`, `ERROR:` e `FATAL:` (de `$error` e `$fatal`) são erro e fazem a
  simulação falhar, mesmo com código 0 (5.2). O `$error` do Verilog sempre
  traz o local (`ERROR: tb.v:12: ...`); um `ERROR:` sem local é o testbench
  escrevendo (`$display("ERROR: ...")`) e fica como saída dele, sem
  reprovar. A linha seguinte do `vvp` (`Time: 1000  Scope: tb`) entra na
  mensagem.
- No `iverilog`, a continuação com local de um aviso
  (`x.v:5:        : Padding 4 high bits of the port.`) entra na mensagem
  anterior, e o bloco `*** These modules were missing:` ... `***` vira um
  resumo `info` só.
- No `iverilog`, a macro indefinida é erro (5.4), e um erro faz o passo
  falhar mesmo com código 0.
- As mensagens do YANC vêm em inglês, porque o Lace pede `-en`.

---

## 8. Erros

`LaceError` é para quando a operação não pôde rodar. `code()` devolve um
identificador estável, o mesmo que a CLI põe em `error.code` no JSON.

| Variante | `code()` | Quando | O que fazer |
|---|---|---|---|
| `BundleNotFound` | `bundle_not_found` | não há `toolchain/` com `bundle.json` ao lado do executável | reinstalar o Lace |
| `InvalidBundle` | `invalid_bundle` | manifesto ausente ou inválido, formato novo demais, outra plataforma, componente ausente ou fora do bundle, executável que é symlink para fora do bundle | reinstalar, ou montar o bundle certo |
| `UnsupportedPlatform` | `unsupported_platform` | fora de Linux x64, macOS arm64, Windows x64 | não há bundle |
| `ComponentMissing` | `component_missing` | o componente da ferramenta não foi instalado; a mensagem não diz como instalar, que é de cada interface | `lace install <componente>`, ou o botão Instalar do Studio |
| `ToolchainIncomplete` | `toolchain_incomplete` | o componente está lá, mas falta o executável (ou o `verilator_bin`), ou um diretório do YANC | reinstalar |
| `SystemCompilerMissing` | `system_compiler_missing` | Linux e macOS: Verilator (simulação ou `check` com `lint`) sem compilador C++, `make` e Perl do sistema | instalar (ver BUNDLE.md) ou, na CLI, declarar com `--compiler <DIR>` ou `LACE_COMPILER` |
| `InvalidProject` | `invalid_project` | diretório ou arquivo do projeto ausente ou em conflito | ver `path` e `reason` |
| `ProjectExists` | `project_exists` | `create` sobre um `.spf` existente | abrir em vez de criar |
| `OutsideProject` | `outside_project` | `move_path` com um caminho fora da pasta do projeto | usar um caminho do projeto |
| `CannotMove` | `cannot_move` | `move_path` do `.spf`, de `.lace`, da pasta de um processador, das pastas dele ou do fonte; para dentro de `.lace`; de uma pasta para dentro dela mesma | ver `reason` |
| `PathExists` | `path_exists` | o destino de `move_path` já existe | outro destino, ou tirar o que está lá |
| `ProcessorExists` | `processor_exists` | `add_processor` com nome repetido | outro nome |
| `ProcessorNotFound` | `processor_not_found` | nome inexistente | ver `available` |
| `InvalidName` | `invalid_name` | nome de projeto, processador ou topo da síntese fora das regras de 3.4; arquivo com nome de testbench como topo; arquivo que não é `.v` nem `.sv` nas listas, ou `.py` fora dos testbenches; `.py` novo ou simulado com nome que não é identificador do Python; `check` de um `.py`; `build` de processador do `.spf` com nome que não compila | outro nome, ou outro arquivo |
| `InvalidProjectFile` | `invalid_project_file` | `.spf` ilegível, campo com tipo errado (3.3), processador com nome vazio, com barra ou repetido, ou `clk`/`numClocks` fora da faixa | corrigir o `.spf` |
| `InvalidSource` | `invalid_source` | fonte ausente, nome errado, `#PRNAME` ausente ou diferente | corrigir o fonte |
| `NotBuilt` | `not_built` | simular, sintetizar ou elaborar a hierarquia de processador sem `build`; `waveform_path` de processador não compilado | rodar `build` |
| `NoTestbench` | `no_testbench` | `simulate_project` ou `waveform_path` sem testbench | `add_verilog` de um testbench, ou `set_testbench` |
| `NoCocotbToplevel` | `no_cocotb_toplevel` | `simulate_project` de um testbench cocotb sem a diretiva `# aurora-toplevel:` num projeto sem topo | a diretiva no `.py`, ou `set_top` |
| `CocotbUnavailable` | `cocotb_unavailable` | o Python do componente `cocotb` não carregou o cocotb (a sonda de 5.3.2 falhou), ou o cocotb dele não traz a biblioteca do simulador; `reason` traz o fim da saída da sonda ou a biblioteca que falta | reinstalar o componente |
| `NoTopLevel` | `no_top_level` | `synthesize(TopLevel)` sem topo | `set_top` |
| `EmptyProject` | `empty_project` | `check` sem nenhum arquivo Verilog registrado e sem processadores; `hierarchy` sem nada para elaborar | `add_verilog` ou `add_processor` |
| `ModuleNotFound` | `module_not_found` | `set_top` com um nome que nenhum sintetizável registrado declara; `top_module` ou `testbench_module` num arquivo sem um módulo que dê para usar; `render_schematic` de um módulo fora do netlist | ver `available` |
| `AmbiguousModule` | `ambiguous_module` | `set_top` com um nome que mais de um sintetizável declara | escolher pelo arquivo (`files`) |
| `InvalidParameter` | `invalid_parameter` | `add_processor` com parâmetro que o YANC não compila (`NewProcessor::validate`); `configure_processor` com frequência ou clocks fora da faixa | ver `name`, `value` e `reason` |
| `OperationInProgress` | `operation_in_progress` | outro build ou outra simulação do mesmo processador, ou do projeto, está rodando (de outro processo ou do mesmo) | esperar a outra terminar |
| `InvalidNetlist` | `invalid_netlist` | netlist ilegível | ver `modules` da síntese |
| `InvalidDataFile` | `invalid_data_file` | linha de `input_<n>.txt`/`output_<n>.txt` que não é inteiro | corrigir a linha indicada |
| `ProcessExitedEarly` | `process_exited_early` | o Surfer fechou logo ao abrir | ver o fim do log (`tail`) |
| `NonUtf8Path` | `non_utf8_path` | caminho não UTF-8 | renomear |
| `PathTooLong` | `path_too_long` | caminho acima de `YANC_PATH_LIMIT` (259 no Windows, 1000 nos outros) | mover o projeto |
| `NonAsciiPath` | `non_ascii_path` | processador num caminho com caractere fora do ASCII: o YANC grava o caminho absoluto das memórias e das entradas, e o `vvp` do Icarus não abre nome com acento | mover o projeto para uma pasta sem acento |
| `Spawn` | `spawn` | processo não iniciou | ver permissão e caminho |
| `Io` | `io` | leitura ou escrita do Lace falhou | ver `context` e `path` |
| `ProjectNotFound` | `project_not_found` | `Project::discover` sem `.spf` na pasta nem nas de cima | rodar de dentro de um projeto, ou criar um |
| `NoReports` | `no_reports` | `latest` ou `compare_reports` num projeto sem relatório guardado | rodar uma operação e gravar |
| `ReportNotFound` | `report_not_found` | `load`, `report_text` com um identificador que não existe | ver `list` |
| `InvalidReport` | `invalid_report` | `record.json` ilegível, que não é JSON, ou de outro formato | ver `path` e `reason` |
| `NotComparable` | `not_comparable` | `compare` de projetos diferentes; `compare_reports` sem estatísticas nem tempos, sem anterior compatível, ou entre dois sem parte em comum | escolher outro relatório |

O enum é `#[non_exhaustive]`: todo `match` precisa de `_`.

---

## 9. Garantias e limites

Garantias:

- **Sem I/O de console.** O Core não escreve no terminal, não chama `exit`,
  não entra em pânico em erro esperado. O log sai por `tracing`, com spans
  por operação (`build`, `simulate`, `simulate_project`, `check`,
  `hierarchy`, `synthesize`, `render_schematic`); o cliente configura o
  subscriber.
- **Só o bundle.** Toda ferramenta sai do bundle instalado com o Lace, num
  caminho fixo por plataforma. Nada do `PATH`, nada configurável. A exceção é
  a do Verilator no Linux e no macOS (compilador, `make` e Perl do sistema,
  em locais fixos); no Windows os três vêm no bundle.
- **Ambiente explícito.** Todo filho parte de um ambiente vazio. No Windows
  recebe `SystemRoot`, `windir`, `ComSpec`, `TEMP`, `TMP`; as ferramentas do OSS CAD
  Suite recebem `PATH` com `bin;lib` do pacote, e as do `msys/`, com
  `ucrt64/bin`. No Linux e no macOS, os lançadores do pacote recebem
  `PATH=/usr/bin:/bin`. O Verilator recebe o diretório do script e os do
  compilador, e `LC_ALL=C`; o
  surfer-aurora recebe as variáveis de display. O `StepReport` registra o que
  foi definido.
- **stdin fechado.** Todo filho recebe stdin nulo, e o `vvp` roda com `-n`:
  um `$stop` num testbench termina a simulação em vez de esperar entrada.
- **Cancelar não deixa processo para trás.** Uma operação cancelada ou que
  passou do prazo encerra o processo do passo com tudo o que ele iniciou:
  no Unix, o grupo de processos do passo (SIGTERM, depois SIGKILL); no
  Windows, o Job Object do passo, que também encerra a árvore quando o
  processo do Lace morre (5.8).
- **Nunca sobrescreve código.** `add_processor` e `add_file` com conteúdo
  recusam se o arquivo já existir; `add_verilog` só cria o arquivo que não
  existe e registra como está o que existe. `move_path` recusa um destino
  que já existe. `write_input` substitui a entrada, que é estímulo.
- **Threads.** `Toolchain`, `Project`, os resultados, `LaceError`,
  `Control`, `CancelToken` e `Event` são `Send + Sync`; `RunningProcess` é
  `Send`. Uma GUI roda as operações numa thread de trabalho e cancela da
  thread da interface (5.8).
- **FFI.** A superfície pública só expõe tipos do Lace, da `std`, de
  `camino` (caminhos), de `serde` e de `schemars` (derive). Os enums são
  `#[non_exhaustive]`.

Limites desta versão:

- As operações bloqueiam até o fim, menos `open_waveform`. Cancelar e
  acompanhar a saída é pelo `Control` (5.8), de outra thread.
- Só a simulação tem prazo (`SimulationOptions::timeout`). Build, `check`,
  hierarquia, síntese e esquemático rodam até o fim ou até o cancelamento.
- No Unix, um SIGKILL no processo que usa a biblioteca não tem como
  encerrar os filhos: eles rodam num grupo de processos próprio e continuam
  rodando. O mesmo vale para um SIGTERM que o processo não trata. Quem
  encerra um cliente do Lace de fora deve mandar SIGTERM, e o cliente deve
  transformá-lo em `cancel()`, como a CLI faz (`cancel_on_signals`, em
  `crates/lace-cli/src/main.rs`). No Windows os filhos estão no Job Object
  do passo e terminam junto com o processo, de qualquer jeito que ele
  morra.
- Sem build incremental: cada chamada roda tudo de novo (o modelo do
  Verilator em dia é reaproveitado pelo próprio Verilator e pelo `make`,
  5.2).
- O `StepReport` guarda de cada pipe os primeiros e os últimos 2 MiB; o meio
  de uma saída maior (um testbench que imprime sem parar) sai, e uma linha
  `[Lace: N bytes of output left out here]` marca o corte. A saída ao vivo do
  `Control` (5.8) passa inteira. Os diagnósticos saem do que foi guardado.
- No fluxo C, a linha dos erros e a da onda são as do `pp.cpp`: o `cpppp`
  do YANC tira as diretivas e junta os `#include` sem marcar a linha de
  origem (`#line`), e o Lace não tem como voltar ao fonte sem adivinhar.
- Plataformas: ver 9.1.

### 9.1 Plataformas

O Lace é escrito para Linux, macOS e Windows. O que foi conferido e como:

| Verificação | Linux | Windows | macOS |
|---|---|---|---|
| compila, `clippy -D warnings` | sim | sim (MSVC e GNU, cruzado) | sim (Intel e Apple Silicon, cruzado) |
| testes de unidade | sim | sim, no Wine | não |
| build com o YANC real, regressão do YANC (66 + 10 casos), CLI | sim | sim, no Wine, com o YANC compilado para Windows | não |
| Icarus, Verilator, Yosys, `dot`, surfer-aurora do bundle | sim (bundle 2026.09.29: Icarus 14.0 devel, Verilator 5.053 devel, Yosys 0.69+156, Graphviz 2.43, surfer-aurora v0.7.0-nips.10) | não | não |

O Wine executa a API do Windows (criação de processo, caminhos `C:\`,
`.exe`, ambiente), mas não é o Windows. A verificação completa nos três
sistemas, com todas as ferramentas, é à mão: os testes contra uma
instalação ([CONTRIBUTING.md](../CONTRIBUTING.md)).

O que muda por sistema, e como o Lace trata:

| Assunto | Tratamento |
|---|---|
| extensão de executável | `.exe` no Windows para o YANC e para o modelo do Verilator (`EXE_SUFFIX`) |
| caminhos | `camino` em toda a API; `dunce` tira o prefixo `\\?\` que os compiladores C não entendem; `.spf` aceita `\` e `/` |
| limite de caminho | 259 caracteres no Windows (MAX_PATH dos `.exe` do YANC), 1000 nos outros (buffers do YANC); conferido por arquivo antes de compilar |
| nomes reservados | `CON`, `PRN`, `AUX`, `NUL`, `COM0-9`, `LPT0-9` recusados em todo sistema |
| maiúsculas | processadores que diferem só na caixa são recusados (Windows e macOS não distinguem) |
| ambiente do filho | Windows: `SystemRoot`, `windir`, `ComSpec`, `TEMP`, `TMP`. Surfer: `HOME`, `DISPLAY`, `WAYLAND_DISPLAY`, `XDG_*`, `TMPDIR` (Unix) ou `USERPROFILE`, `APPDATA`, `LOCALAPPDATA` (Windows) |
| término de processo | sinal no Unix (`Signaled`), NTSTATUS no Windows (`Exception`) |
| encerrar um passo (cancelamento, prazo) | Unix: grupo de processos próprio por passo, SIGTERM ao grupo e SIGKILL depois de 1 s. Windows: Job Object por passo (`KILL_ON_JOB_CLOSE`), que encerra a árvore também quando o Lace morre; `taskkill /T /F` do `System32` se o sistema recusar o job |
| janela de console | Windows: nenhuma (`hide_console`); com console o filho divide o do Lace, sem console leva `CREATE_NO_WINDOW` |
| ferramentas do OSS CAD Suite | lançadores bash via `/bin/bash` (Linux, macOS); `.exe` com `PATH=bin;lib` (Windows) |
| Verilator | script Perl pelo Perl do sistema; Python do bundle no `make`; sem `-march=native` no macOS |
| fins de linha | CRLF aceito em toda leitura (`.spf`, fontes, saídas); no Windows o YANC grava CRLF |

---

## 10. Diferenças em relação à AURORA

| Assunto | AURORA | Lace |
|---|---|---|
| Abrir projeto | regrava o `.spf` (`lastOpened`, `exists`) | só lê |
| `clk` fracionário | trunca | recusa |
| `-` no nome do processador | aceita | recusa (o `cmmcomp` não aceita) |
| `#PRNAME` ausente ou diferente | compila e gera nomes errados | recusa antes de compilar |
| Diretório temporário | `<raiz>/.aurora/Temp` | `<raiz>/.lace/Temp` |
| Idioma do YANC | o da interface (`-pt` ou `-en`) | sempre `-en` |
| Ambiente do filho | herda o da AURORA | vazio + o mínimo |
| Classificação de arquivos | por conteúdo, refeita a cada carga do projeto | por conteúdo, com a mesma regra, quando `add_verilog` registra; o papel fica gravado no `.spf` |
| Verificação | `iverilog -tnull -s <topo>` | `iverilog` sem `-s` (todas as raízes), um por testbench e, com `lint`, `verilator --lint-only` |
| Hierarquia | montada pelo Yosys | elaborada pelo Icarus, do design e de cada testbench, com a biblioteca SAPHO (`hierarchy`) |
| Nome de projeto | aceita espaço e acento | só letras sem acento, dígitos, `_` e `-`, começando por letra; os projetos que já existem abrem |
| Formato da onda (Icarus) | `vvp -fst` sempre, no nome do `$dumpfile` (um `.vcd` com FST dentro) | FST, com a extensão `.fst` trocada numa cópia do testbench |
| Testbench cocotb | o runner Python do cocotb, num processo, no Icarus ou no Verilator | os passos `elaborate` (ou `verilate`) e `simulate` do Lace, no Icarus ou no Verilator, com os testes em `SimulationResult::tests` |
| Simulação rápida (Fast Sim) | Verilator sem `--trace-fst`, com os `$dumpfile`/`$dumpvars` comentados numa cópia do testbench; o botão só habilita com o Verilator escolhido; cocotb no simulador escolhido | Verilator sem `--trace`, com o testbench como está (o Verilator ignora o dump), qualquer que seja o simulador escolhido; cocotb no simulador escolhido (`SimulationOptions::fast`) |
| Testbench sem `$dumpfile` | injeta `$dumpvars(1, <topo>)`: só o nível do testbench | injeta `$dumpvars(0, <tb>)`: todos os sinais, inclusive os do módulo testado |
| Simulação de processador | só pelo modo projeto | `simulate` direto, além de `simulate_project` |
| Ferramentas | `components/` baixados pela AURORA | bundle versionado instalado com o Lace |
| Esquemático | netlistsvg (fork próprio), dentro do processo, ~50 skins | `show` do Yosys + `dot` do Graphviz; o Studio desenha o próprio, do `hierarchy.json`, com o ELK |
| Onda do Verilator | FST | VCD (o FST do Verilator do bundle exige lz4 e zlib do sistema) |
| Layout do Surfer | gera `.surf.ron` e tradutores na pasta de configuração do usuário | gera o mesmo layout (`wave_layout`), com os tradutores em `.lace/Temp/surfer/` |
| Sinais fora dos processadores no layout | todos os escopos | só os da raiz do testbench |
| Teste de hardware (THTEST) | harness C++ próprio | não implementado |
