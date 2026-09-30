# Solar-Core: guia da API

Este guia explica como a API funciona como um todo: conceitos, o que cada
operação executa, o que lê e escreve no disco, o formato dos resultados e dos
erros. A referência item a item está na documentação do código:

```
cargo doc -p solar-core --no-deps --open
```

Todo item público do `solar-core` tem documentação (o crate compila com
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
| Toolchain | `Toolchain` | O bundle de ferramentas instalado com o Solar, aberto e conferido. É o único lugar de onde o Solar executa algo. |
| Projeto | `Project` | Um `.spf` da AURORA aberto, com seus processadores e arquivos Verilog. |
| Processador | `Processor` | Um processador SAPHO do projeto: nome, linguagem, caminhos, parâmetros de simulação. |
| Operação | funções livres | `build`, `simulate`, `simulate_project`, `check_syntax`, `synthesize`, `render_schematic`, `open_waveform`. Cada uma recebe a toolchain e o que operar, e devolve um resultado. |

As operações não guardam estado entre si. O estado está no disco: `build`
grava o Verilog, `simulate` lê esse Verilog. Chamar `simulate` sem `build`
antes é erro (`NotBuilt`).

O ciclo de uso é sempre o mesmo:

```rust
use solar_core::*;

let exe = camino::Utf8PathBuf::try_from(std::env::current_exe()?)?;
let toolchain = Toolchain::locate(&exe)?;      // <instalação>/toolchain
let project = Project::open("/home/eu/projetos/soma")?;
let soma = project.require_processor("soma")?;

let built = build(&toolchain, soma, &BuildOptions::default())?;
if !built.succeeded() {
    for d in &built.diagnostics {
        println!("{:?}:{:?}: {}", d.file, d.line, d.message);
    }
}
```

---

## 2. O bundle (`Toolchain`)

O Solar só executa ferramentas de um bundle versionado instalado com ele:
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
procurar, para o atalho `~/.local/bin/solar` do instalador funcionar.

### 2.2 Consultar

| Método | Devolve |
|---|---|
| `manifest()` | o `BundleManifest`: identificador, plataforma e os componentes instalados, na ordem de `component::ALL` (`BundleComponent`: nome, versão, diretório, origem, SHA-256 do pacote e o hash de cada executável) |
| `component(nome)` | um componente instalado (nomes em `solar_core::component`), ou `None` |
| `verified_files()` | quantos executáveis os manifestos listam |
| `tool(Tool)` | o caminho do executável, conferido no disco agora; recusa um symlink que sai do bundle (`InvalidBundle`) e, para o Verilator, a falta do `bin/verilator_bin` que o script Perl chama (sem ele, o script o procuraria no `PATH`) |
| `available()` | as ferramentas presentes |
| `hdl_dir()`, `macros_dir()`, `headers_dir()` | os diretórios do YANC (`SAPHO/`, `Macros/`, `Header/`) |
| `system_compiler()` | o `SystemCompiler` do Verilator, se encontrado |
| `verify()` | os executáveis cujo SHA-256 não confere com o manifesto (`FileMismatch`, com o componente que o lista) |
| `platform()` | a `Platform` |

### 2.3 Ferramentas

| `Tool` | Componente (`Tool::component()`) | Diretório |
|---|---|---|
| `Cmmcomp`, `Appcomp`, `Asmcomp`, `Cpppp`, `Cppcomp` | `yanc` | `yanc/` |
| `Iverilog`, `Vvp` | `icarus` | `oss-cad-suite/` |
| `Verilator` | `verilator` | `oss-cad-suite/` |
| `Yosys` | `yosys` | `oss-cad-suite/` |
| `Dot` | `graphviz` | `oss-cad-suite/` (Linux, macOS) ou `graphviz/` (Windows) |
| `Surfer` | `surfer-aurora` (o nome do programa é `surfer-aurora`) | `surfer-aurora/` |
| `Perl` | nenhum: sistema (exceção do Verilator) | |

Icarus, Verilator, Yosys e o `dot` do Linux e do macOS dividem o diretório
`oss-cad-suite/`: cada componente traz a parte do pacote da sua ferramenta, e
as bibliotecas em comum vêm com qualquer um deles. O caminho de cada
ferramenta dentro do diretório é fixo por plataforma, no código; o diretório
vem do manifesto do componente. Nada é configurável.

### 2.4 A exceção do Verilator

O Verilator compila o modelo em C++, e o OSS CAD Suite não traz compilador. O
compilador C++, o `make` e o Perl vêm do sistema, num `SystemCompiler`:

| Função | Faz |
|---|---|
| `SystemCompiler::detect()` | procura nos locais padrão (chamado por `open`) |
| `SystemCompiler::in_dir(dir)` | os três num diretório |
| `SystemCompiler::in_msys2(raiz)` | uma instalação do MSYS2 |
| `toolchain.with_system_compiler(Some(c))` | troca o detectado por um declarado |

Sem ele, só a simulação com Verilator falha, com `SystemCompilerMissing`.

## 3. Projeto

### 3.1 No disco

```
<raiz>/
  <projeto>.spf                  arquivo de projeto (JSON, formato AURORA)
  <processador>/
    Software/<processador>.cmm   fonte (ou .cpp); o .asm gerado fica aqui
    Hardware/                    <processador>.v, _data.mif, _inst.mif gerados
    Simulation/                  input_<n>.txt (usuário), output_<n>.txt (simulação)
  .solar/Temp/<processador>/     intermediários do YANC, testbench, onda, .vvp, obj_dir
  .solar/Temp/                   simulação do projeto (.vvp, obj_dir, testbench instrumentado)
  .solar/Temp/synth/<topo>/      síntese: script, log, netlist, esquemáticos
```

A raiz é sempre o diretório do `.spf`. O `basePath` gravado dentro do arquivo
é ignorado na leitura, para que um projeto copiado de outra máquina funcione.

### 3.2 Abrir e criar

| Função | Faz |
|---|---|
| `Project::open(caminho)` | Aceita o `.spf` ou o diretório (usa `<dir>/<nome-do-dir>.spf` ou o único `.spf` dele). Só lê. |
| `Project::create(pai, nome)` | Cria `<pai>/<nome>/<nome>.spf` vazio, no formato da AURORA. Recusa se já existir. |
| `project.add_processor(&NewProcessor)` | Cria diretórios, fonte-modelo e entrada no `.spf`. Recusa nome repetido e fonte que já exista no disco. |
| `project.configure_processor(nome, &ProcessorConfig)` | Grava `clk`, `numClocks` e `showArrays` no `.spf` (campos `None` ficam como estão). Uma entrada no formato antigo (só o nome) vira objeto. Valem a partir do próximo `build`. |
| `processor.is_built()` | O `Hardware/<nome>.v` e o testbench existem. Não diz se estão atualizados em relação ao fonte. |
| `project.buildable_processors()` | Os processadores que têm o fonte no disco, na ordem do `.spf`: os que o botão Wave da AURORA compila antes de simular o projeto. |

Todo método que altera o projeto grava o `.spf` na hora (gravação atômica:
arquivo temporário e `rename`). Não existe "salvar". Um `Project` aberto não
percebe mudanças que outro programa faça no `.spf`: abra de novo.

### 3.3 O `.spf`

| Campo | Lido | Gravado |
|---|---|---|
| `metadata.projectName`, `createdAt`, `computerName`, `appVersion` | não | na criação |
| `metadata.lastModified` | não | em toda gravação |
| `metadata.projectPath`, `structure.basePath` | não (a raiz é o diretório do `.spf`) | em toda gravação, com a raiz atual |
| `structure.processors` | sim | `add_processor` |
| `structure.synthesizableFiles`, `testbenchFiles` | sim | `add_file`, `remove_file`, `set_top_level`, `set_testbench` |
| `structure.topLevelFile`, `testbenchFile` | sim | `set_top_level`, `set_testbench` |
| qualquer outro (`commandOverrides`, `folders`, ...) | não | preservado como estava |

A leitura é tolerante como a da AURORA: JSON estrito primeiro; se falhar, tira
comentários `//` e `/* */`, BOM e vírgula antes de `}` ou `]`, e tenta de novo.
A ordem das chaves é preservada ao gravar.

Cada processador é uma string (formato antigo) ou um objeto:

| Campo | Tipo | Padrão | Uso |
|---|---|---|---|
| `name` | texto | obrigatório | nome do processador |
| `language` | `"cpp"` ou ausente | C± | linguagem |
| `sourceFile` / `cmmFile` | texto | `<nome>.<ext>` | só o nome do arquivo importa; a extensão ajuda a decidir a linguagem |
| `clk` | inteiro positivo (número ou texto) | 100 | `frequency_mhz`, `-f` do `asmcomp` |
| `numClocks` | inteiro positivo | 2000 | `clocks`, `-c` do `asmcomp` |
| `showArrays` | booleano | `false` | `show_arrays`, `-A` do `cmmcomp` |

`configure_processor` com `frequency_mhz` ou `clocks` igual a 0 também é
`InvalidProjectFile`. `clk: 12.5` é erro (`InvalidProjectFile`). A AURORA trunca para 12 em
silêncio; o Solar recusa para não compilar com um valor que ninguém escolheu.

A linguagem é resolvida nesta ordem: `language` declarada; extensão de
`sourceFile`/`cmmFile`; o arquivo que existir em `Software/` (`.cmm` antes de
`.cpp`); C±.

### 3.4 Nomes

| Nome | Regra | Por quê |
|---|---|---|
| Processador | `[A-Za-z_][A-Za-z0-9_]*` | vira `#PRNAME` (lexer do `cmmcomp`), módulo Verilog e nome de arquivo |
| Módulo de topo | nome do arquivo sem extensão, mesma regra | vai em `-s` e `hierarchy -top` |
| Projeto | nome de pasta válido no Windows e no Linux, sem começar com `.` | vira diretório |

A AURORA aceita `-` em nome de processador; o Solar não, porque o `cmmcomp`
não aceita no `#PRNAME`.

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

| Método | Faz |
|---|---|
| `files(FileRole)` | lista os arquivos de um papel, na ordem do `.spf`, com caminho absoluto |
| `add_file(papel, caminho, conteúdo)` | registra; com `Some(texto)` cria o arquivo (recusa se existir), com `None` exige que exista |
| `remove_file(papel, caminho)` | tira do `.spf`, não apaga do disco |
| `set_top_level(caminho)` | módulo de topo: `topLevelFile` + `isTopLevel` exclusivo |
| `set_testbench(caminho)` | testbench da simulação do projeto: `testbenchFile` + `isTopLevel` exclusivo |
| `top_level()` | `topLevelFile`, senão o sintetizável marcado |
| `testbench()` | `testbenchFile`, senão o testbench marcado, senão o primeiro |
| `resolve_path(texto)` | converte um caminho como está no `.spf` para absoluto |

O `.spf` guarda o caminho relativo à raiz quando o arquivo está dentro dela,
com `/`. Na leitura aceita `\` e `/`.

### 4.2 Entradas e saídas da simulação

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

---

## 5. Operações

Todas bloqueiam até as ferramentas terminarem, exceto `open_waveform`.

### 5.1 `build(&Toolchain, &Processor, &BuildOptions) -> Result<BuildResult>`

Do fonte ao Verilog sintetizável.

Antes de executar qualquer coisa:

1. o fonte precisa existir e se chamar `<processador>.<ext>`;
2. em C±, o fonte precisa declarar `#PRNAME <processador>`; em C, um
   `#pragma yanc prname`, se houver, precisa ser o nome do processador;
3. nenhum caminho pode passar de `YANC_PATH_LIMIT` (900 bytes);
4. `Software/`, `Hardware/`, `Simulation/` e o diretório temporário são
   criados.

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

O Solar sempre passa `-en` aos três compiladores do fluxo C±, para que o
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

### 5.1.1 `build_processors(&Toolchain, processadores, &BuildOptions, OnFailure, on_result) -> Result<Vec<BuildResult>>`

Compila vários processadores em sequência. `on_result` é chamado depois de
cada um, para a interface mostrar o progresso.

| `OnFailure` | Uso |
|---|---|
| `Continue` | "compilar tudo": mostra todos os erros de uma vez |
| `Stop` | antes de simular ou sintetizar: para no primeiro que falhar |

O fluxo do botão Wave da AURORA é:

```rust
let builds = build_processors(&tc, project.buildable_processors(), &BuildOptions::default(),
                              OnFailure::Stop, |r| mostrar(r))?;
if builds.iter().all(BuildResult::succeeded) {
    let sim = simulate_project(&tc, &project, &SimulationOptions::new(Simulator::Icarus))?;
}
```

### 5.2 `simulate(&Toolchain, &Processor, &SimulationOptions) -> Result<SimulationResult>`

Simula um processador com o testbench que o `build` gerou. Duração e
frequência já estão no testbench (vêm de `clocks` e `frequency_mhz` do build).

| Simulador | Passo | Comando | CWD |
|---|---|---|---|
| Icarus | `elaborate` | `iverilog -y <hdl> -s <nome>_tb -o T/<nome>_tb.vvp P/Hardware/<nome>.v T/<nome>_tb.v` | `T` |
| Icarus | `simulate` | `vvp T/<nome>_tb.vvp -fst` | `T` |
| Verilator | `verilate` | `perl verilator --binary --main --trace -j 0 ... --top-module <nome>_tb -Mdir T/obj_dir_<nome>_tb -y <hdl> <arquivos>` | `T` |
| Verilator | `simulate` | `T/obj_dir_<nome>_tb/V<nome>_tb` | `T` |

O CWD é o diretório temporário porque o `.v` do processador lê
`pc_<nome>_mem.txt` por nome relativo e o testbench grava a onda em
`<nome>_tb.vcd`, também relativo. As memórias `.mif` e os arquivos de
`Simulation/` são abertos por caminho absoluto.

A biblioteca SAPHO entra por `-y`: o simulador carrega só os módulos que
faltarem. Ela vem do YANC (`Toolchain::sapho_library`): sem o YANC
instalado, um projeto só de Verilog simula, verifica e sintetiza sem ela, e
um projeto com processador SAPHO dá `ComponentMissing("yanc")`.

Flags do Verilator (as da AURORA, sem o arquivo `.vlt` de monitores dela, e
com duas mudanças para ficar no bundle): `--binary --main --trace -j 0
-MAKEFLAGS OBJCACHE= -MAKEFLAGS PYTHON3=<Python do bundle> -Wno-fatal
-Wno-TIMESCALEMOD -Wno-DECLFILENAME -Wno-STMTDLY --timing --x-assign fast
--no-trace-top +define+YANC_TRACE -CFLAGS -O3 -CFLAGS -march=native -CFLAGS
-fstrict-aliasing -CFLAGS -pipe -CFLAGS -Wno-attributes` (sem `-march=native`
no macOS). As duas mudanças:

- `--trace` (VCD) em vez de `--trace-fst`: o FST do Verilator do bundle
  compila contra lz4 e zlib do sistema, fora da exceção do compilador;
- `PYTHON3=`: o `make` gerado chama `python3` pelo nome, e sem isso usaria o
  do sistema.

O script `verilator` roda pelo Perl do sistema, com `PATH` = `bin/` do
pacote, depois os diretórios do compilador do sistema, e `LC_ALL=C`. A
primeira compilação do modelo leva dezenas de segundos; o `obj_dir` é
reaproveitado depois.

`SimulationOptions`:

| Campo | Padrão | Efeito |
|---|---|---|
| `simulator` | (obrigatório em `new`) | `Icarus` ou `Verilator` |
| `fst` | `true` | só Icarus: `-fst`; com `false`, VCD. O Verilator sempre grava VCD |
| `build_jobs` | `None` (`-j 0`, todos os núcleos) | paralelismo da compilação C++ do Verilator |

A onda sai com o nome do `$dumpfile` do testbench (`<nome>_tb.vcd`). Com FST
(Icarus), o conteúdo é FST apesar da extensão `.vcd`. `SimulationResult.waveform` traz o
caminho e o formato real.

### 5.3 `simulate_project(&Toolchain, &Project, &SimulationOptions) -> Result<SimulationResult>`

Simula o testbench do projeto, como o botão Wave da AURORA.

1. O testbench é `project.testbench()`; o módulo de topo é o nome do arquivo.
2. Arquivos: os sintetizáveis do `.spf`, depois o `Hardware/<nome>.v` de cada
   processador compilado que não esteja na lista, e o testbench por último.
3. Copia para a raiz o `pc_<nome>_mem.txt` de cada processador compilado.
4. Copia para a raiz os arquivos que o testbench lê por nome relativo
   (`$readmemb("x")`, `$readmemh("x")`, `$fopen("x", "r")`), a partir da pasta
   do testbench.
5. Se o testbench não tem `$dumpfile`, simula uma cópia em
   `.solar/Temp/instr_<testbench>` com `$dumpfile("<topo>.vcd");
   $dumpvars(1, <topo>);` antes do último `endmodule`. O arquivo do usuário
   não muda.
6. Roda os mesmos comandos de 5.2, com CWD na raiz e `.vvp`/`obj_dir` em
   `.solar/Temp/`.

Os processadores não são compilados aqui. Chame `build` para cada um antes.

### 5.4 `check_syntax(&Toolchain, &Project) -> Result<CheckResult>`

`iverilog -y <hdl> -tnull -s <topo> <arquivos>` com CWD na raiz. Os arquivos
são os sintetizáveis do `.spf`, os `.v` de `<raiz>/TopLevel/` (legado da
AURORA) e o `Hardware/*.v` de cada processador, sem testbenches. Não gera
nada: só diz se o projeto elabora a partir do topo.

### 5.5 `synthesize(&Toolchain, &Project, &DesignTarget) -> Result<SynthesisResult>`

Síntese de visualização (a do PRISM da AURORA), sem mapear para FPGA.

| Alvo | Topo | Arquivos |
|---|---|---|
| `DesignTarget::TopLevel` | `project.top_level()` | os de `check_syntax` |
| `DesignTarget::Processor(nome)` | o processador | `Hardware/<nome>.v` |

Nos dois casos, todo `.v` de `HDL/` que não é testbench entra primeiro.

Script gravado em `.solar/Temp/synth/<topo>/yosys_script.ys`:

```
read_verilog -setattr src "<arquivo>"     (um por arquivo)
hierarchy -top <topo>
proc
setundef -zero
opt_clean -purge
write_json "<dir>/hierarchy.json"
```

Execução: `yosys -q -l <dir>/yosys.log -s <dir>/yosys_script.ys`, CWD
`<dir>`. `SynthesisResult.modules` lista os módulos do netlist.

### 5.6 `render_schematic(&Toolchain, &netlist, módulo, &SchematicOptions) -> Result<SchematicResult>`

1. Confere que o módulo está no netlist.
2. Passo `graph`: `yosys -q -s show.ys`, CWD `<dir>`, com o script
   `read_json "<netlist>"` e `show -format dot -prefix <módulo> [-width] <módulo>`.
   Gera `<dir>/<módulo>.dot`.
3. Passo `render`: `dot -Tsvg <módulo>.dot -o <módulo>.svg`, CWD `<dir>`. Uma
   mensagem `Error:` do `dot` conta como falha mesmo com código 0. No Linux,
   o Solar grava `<dir>/fonts.conf` a partir do modelo do OSS CAD Suite e
   passa `FONTCONFIG_FILE` e `FONTCONFIG_PATH`: o `dot` usa as fontes do
   bundle, com o cache em `<dir>/fontconfig-cache`. Sem isso ele leria a
   configuração de fontes do sistema, e o SVG mudaria de máquina para
   máquina. No macOS e no Windows o pacote não traz fontes e o `dot` usa as
   do sistema.

`SchematicOptions::bus_widths` (padrão: sim) escreve a largura dos
barramentos nas arestas. O visual é o do Graphviz, não o do netlistsvg da
AURORA.

Nomes de módulo com caracteres fora de `[A-Za-z0-9_.-]` (por exemplo
`$paramod\processor\NUBITS=23`) viram `_` no nome dos arquivos.

### 5.7 `open_waveform(&Toolchain, &onda, &ViewerOptions) -> Result<RunningProcess>`

`surfer-aurora <onda> [-c <arquivo.sucl> | -s <arquivo.surf.ron>]`, destacado,
CWD na pasta da onda, stdout e stderr em `<onda>.surfer.log`. Retorna na hora.

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

---

## 6. Resultados

### 6.1 Estrutura comum

| Campo | Tipo | Conteúdo |
|---|---|---|
| `status` | `Status` | `succeeded`, `failed`, `crashed`, `incomplete` |
| `failed_step` | `Option<Step>` | o passo que falhou |
| `steps` | `Vec<StepReport>` | um por programa executado, na ordem |
| `diagnostics` | `Vec<Diagnostic>` | mensagens interpretadas de todos os passos |
| `artifacts` | `Vec<Artifact>` | arquivos esperados, com `fresh` |

`Status`:

| Valor | Quando |
|---|---|
| `succeeded` | todos os passos com código 0 e todo artefato obrigatório gerado agora |
| `failed` | um passo terminou com código diferente de 0 |
| `crashed` | um passo morreu por sinal (Unix) ou exceção NTSTATUS (Windows): defeito da ferramenta |
| `incomplete` | os passos deram certo, mas falta artefato obrigatório |

Os passos param no primeiro que falhar.

### 6.2 Passos por operação

| Operação | `Step` / `Tool` |
|---|---|
| `build` C± | `compile`/`cmmcomp`, `pre_assemble`/`appcomp`, `assemble`/`asmcomp` |
| `build` C | `preprocess`/`cpppp`, `compile`/`cppcomp`, `pre_assemble`, `assemble` |
| `simulate*` Icarus | `elaborate`/`iverilog`, `simulate`/`vvp` |
| `simulate*` Verilator | `verilate`/`verilator`, `simulate`/`verilator` |
| `check_syntax` | `check_syntax`/`iverilog` |
| `synthesize` | `synthesize`/`yosys` |
| `render_schematic` | `graph`/`yosys`, `render`/`dot` |

`StepReport` guarda `command` (programa, argumentos, CWD, ambiente),
`termination`, `stdout`, `stderr` e `duration_ms`. Com ele dá para reproduzir
qualquer passo à mão.

`Termination` em JSON: `{"kind": "exited", "value": 1}`,
`{"kind": "signaled", "value": 11}`, `{"kind": "exception", "value": ...}`,
`{"kind": "unknown"}`.

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
| `testbench` | `T/<nome>_tb.v` | `build` |
| `preprocessed_source` | `T/pp.cpp` | `build` C |
| `program_counter_map` | `T/pc_<nome>_mem.txt` | `build` |
| `source_translation` | `T/trad_cmm.txt` | `build` |
| `opcode_translation` | `T/trad_opcode.txt` | `build` |
| `compiler_log` | `T/cmm_log.txt` | `build` |
| `pre_assembler_log` | `T/app_log.txt` | `build` |
| `icarus_image` | `<trabalho>/<topo>.vvp` | `simulate*` Icarus |
| `verilated_model` | `<trabalho>/obj_dir_<topo>/V<topo>` | `simulate*` Verilator |
| `waveform` | nome do `$dumpfile` | `simulate*` |
| `simulation_output` | `Simulation/output_<n>.txt` | `simulate*` |
| `netlist` | `synth/<topo>/hierarchy.json` | `synthesize` |
| `schematic` | `synth/<topo>/<módulo>.svg` | `render_schematic` |

### 6.4 Exemplo: build com erro

Saída real de `solar build com_erro -p conta --json` (caminhos encurtados,
artefatos cortados):

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
        "program": "/opt/yanc/bin/cmmcomp",
        "args": ["-i", "conta.cmm", "-n", "conta", "-p", "/home/eu/projetos/com_erro/conta",
                 "-m", "/opt/yanc/Macros", "-t", "/home/eu/projetos/com_erro/.solar/Temp/conta", "-en"],
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

Saída real de `solar sim soma -p soma --json`, campo `simulation` (caminhos
encurtados):

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
        "program": "/usr/bin/iverilog",
        "args": ["-y", "/opt/yanc/HDL", "-s", "soma_tb", "-o", "/home/eu/projetos/soma/.solar/Temp/soma/soma_tb.vvp",
                 "/home/eu/projetos/soma/soma/Hardware/soma.v", "/home/eu/projetos/soma/.solar/Temp/soma/soma_tb.v"],
        "cwd": "/home/eu/projetos/soma/.solar/Temp/soma",
        "env": [],
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
        "program": "/usr/bin/vvp",
        "args": ["/home/eu/projetos/soma/.solar/Temp/soma/soma_tb.vvp", "-fst"],
        "cwd": "/home/eu/projetos/soma/.solar/Temp/soma",
        "env": [],
        "inherit": []
      },
      "termination": { "kind": "exited", "value": 0 },
      "stdout": "FST info: dumpfile soma_tb.vcd opened for output.\nInfo: end of program!\n/home/eu/projetos/soma/.solar/Temp/soma/soma_tb.v:61: $finish called at 985000 (1ps)\n",
      "stderr": "",
      "duration_ms": 66
    }
  ],
  "diagnostics": [
    { "tool": "vvp", "severity": "info", "message": "FST info: dumpfile soma_tb.vcd opened for output.",
      "file": null, "line": null, "column": null, "raw": "FST info: dumpfile soma_tb.vcd opened for output." },
    { "tool": "vvp", "severity": "info", "message": "$finish called at 985000 (1ps)",
      "file": "/home/eu/projetos/soma/.solar/Temp/soma/soma_tb.v", "line": 61, "column": null,
      "raw": "/home/eu/projetos/soma/.solar/Temp/soma/soma_tb.v:61: $finish called at 985000 (1ps)" }
  ],
  "artifacts": [
    { "kind": "icarus_image", "path": "/home/eu/projetos/soma/.solar/Temp/soma/soma_tb.vvp", "required": true, "fresh": true },
    { "kind": "waveform", "path": "/home/eu/projetos/soma/.solar/Temp/soma/soma_tb.vcd", "required": true, "fresh": true },
    { "kind": "simulation_output", "path": "/home/eu/projetos/soma/soma/Simulation/output_0.txt", "required": false, "fresh": true }
  ],
  "waveform": { "path": "/home/eu/projetos/soma/.solar/Temp/soma/soma_tb.vcd", "format": "fst" },
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
| `cmmcomp`, `appcomp`, `asmcomp` | `Error on line N: ...`, `Syntax error on line N. ...`, `Heads up on line N: ...`, `Warning on line N: ...`, `Error: ...`, `Info: ...`; `asmcomp`: `Error: line N of file 'x' ...` | o fonte do processador (preenchido pelo Solar), ou o arquivo de dados citado |
| `cpppp`, `cppcomp` | `<arquivo>:<linha>: error: ...`, `<ferramenta>: ...` | o `pp.cpp`: o `cpppp` não emite `#line` |
| `iverilog`, `vvp` | `<arquivo>:<linha>: error: ...`, `<arquivo>:<linha>: syntax error`, `<arquivo>: No such file or directory`, `ERROR: ...` | o que a ferramenta cita |
| `verilator` | `%Error: <arquivo>:<linha>:<coluna>: ...`, `%Warning-<CÓDIGO>: ...` (o código vai para o fim da mensagem entre colchetes); erros do `g++` no formato C | o que a ferramenta cita |
| `yosys` | `<arquivo>:<linha>: ERROR: ...`, `Warning: ...` | o que a ferramenta cita |
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
- As mensagens do YANC vêm em inglês, porque o Solar pede `-en`.

---

## 8. Erros

`SolarError` é para quando a operação não pôde rodar. `code()` devolve um
identificador estável, o mesmo que a CLI põe em `error.code` no JSON.

| Variante | `code()` | Quando | O que fazer |
|---|---|---|---|
| `BundleNotFound` | `bundle_not_found` | não há `toolchain/` com `bundle.json` ao lado do executável | reinstalar o Solar |
| `InvalidBundle` | `invalid_bundle` | manifesto ausente ou inválido, formato novo demais, outra plataforma, componente ausente ou fora do bundle, executável que é symlink para fora do bundle | reinstalar, ou montar o bundle certo |
| `UnsupportedPlatform` | `unsupported_platform` | fora de Linux x64, macOS arm64, Windows x64 | não há bundle |
| `ComponentMissing` | `component_missing` | o componente da ferramenta não foi instalado | rodar o instalador de novo e escolher o componente |
| `ToolchainIncomplete` | `toolchain_incomplete` | o componente está lá, mas falta o executável (ou o `verilator_bin`), ou um diretório do YANC | reinstalar |
| `SystemCompilerMissing` | `system_compiler_missing` | Verilator sem compilador C++, `make` e Perl do sistema | instalar (ver BUNDLE.md) ou `solar config set-compiler` |
| `InvalidProject` | `invalid_project` | diretório ou arquivo do projeto ausente ou em conflito | ver `path` e `reason` |
| `ProjectExists` | `project_exists` | `create` sobre um `.spf` existente | abrir em vez de criar |
| `ProcessorExists` | `processor_exists` | `add_processor` com nome repetido | outro nome |
| `ProcessorNotFound` | `processor_not_found` | nome inexistente | ver `available` |
| `InvalidName` | `invalid_name` | nome fora das regras de 3.4 | outro nome |
| `InvalidProjectFile` | `invalid_project_file` | `.spf` ilegível ou com campo inválido | corrigir o `.spf` |
| `InvalidSource` | `invalid_source` | fonte ausente, nome errado, `#PRNAME` ausente ou diferente | corrigir o fonte |
| `NotBuilt` | `not_built` | simular ou sintetizar processador sem `build` | rodar `build` |
| `NoTestbench` | `no_testbench` | `simulate_project` sem testbench | `set_testbench` |
| `NoTopLevel` | `no_top_level` | `check_syntax`/`synthesize(TopLevel)` sem topo | `set_top_level` |
| `InvalidNetlist` | `invalid_netlist` | netlist ilegível ou sem o módulo | ver `modules` da síntese |
| `InvalidDataFile` | `invalid_data_file` | linha de `input_<n>.txt`/`output_<n>.txt` que não é inteiro | corrigir a linha indicada |
| `ProcessExitedEarly` | `process_exited_early` | o Surfer fechou logo ao abrir | ver o fim do log (`tail`) |
| `NonUtf8Path` | `non_utf8_path` | caminho não UTF-8 | renomear |
| `PathTooLong` | `path_too_long` | caminho acima de 900 bytes | mover o projeto |
| `Spawn` | `spawn` | processo não iniciou | ver permissão e caminho |
| `Io` | `io` | leitura ou escrita do Solar falhou | ver `context` e `path` |

O enum é `#[non_exhaustive]`: todo `match` precisa de `_`.

---

## 9. Garantias e limites

Garantias:

- **Sem I/O de console.** O Core não escreve no terminal, não chama `exit`,
  não entra em pânico em erro esperado. O log sai por `tracing`, com spans
  por operação (`build`, `simulate`, `simulate_project`, `check_syntax`,
  `synthesize`, `render_schematic`); o cliente configura o subscriber.
- **Só o bundle.** Toda ferramenta sai do bundle instalado com o Solar, num
  caminho fixo por plataforma. Nada do `PATH`, nada configurável. A exceção é
  a do Verilator (compilador, `make` e Perl do sistema, em locais fixos).
- **Ambiente explícito.** Todo filho parte de um ambiente vazio. No Windows
  recebe `SystemRoot`, `windir`, `TEMP`, `TMP`, e as ferramentas do OSS CAD
  Suite recebem `PATH` com `bin;lib` do pacote. No Linux e no macOS, os
  lançadores do pacote recebem `PATH=/usr/bin:/bin`. O Verilator recebe o
  `bin/` do pacote e os diretórios do compilador, e `LC_ALL=C`; o
  surfer-aurora recebe as variáveis de display. O `StepReport` registra o que
  foi definido.
- **stdin fechado.** Todo filho recebe stdin nulo: um `$stop` num testbench
  não deixa o `vvp` esperando entrada.
- **Nunca sobrescreve código.** Criar processador ou arquivo recusa se o
  arquivo já existir. `write_input` substitui a entrada, que é estímulo.
- **Threads.** `Toolchain`, `Project`, os resultados e `SolarError` são
  `Send + Sync`; `RunningProcess` é `Send`. Uma GUI roda as operações numa
  thread de trabalho.
- **FFI.** A superfície pública só expõe tipos do Solar, da `std`, de
  `camino` (caminhos) e de `serde` (derive). Os enums são `#[non_exhaustive]`.

Limites desta versão:

- Sem progresso nem cancelamento: uma simulação longa bloqueia até o fim. O
  `stdout` é devolvido inteiro no fim.
- Sem saída incremental: cada chamada roda tudo de novo (o `obj_dir` do
  Verilator é reaproveitado pelo próprio `make`).
- No fluxo C, a linha dos erros é a do `pp.cpp`.
- Plataformas: ver 9.1.

### 9.1 Plataformas

O Solar é escrito para Linux, macOS e Windows. O que foi conferido e como:

| Verificação | Linux | Windows | macOS |
|---|---|---|---|
| compila, `clippy -D warnings` | sim | sim (MSVC e GNU, cruzado) | sim (Intel e Apple Silicon, cruzado) |
| testes de unidade | sim | sim, no Wine | não |
| build com o YANC real, regressão do YANC (66 + 10 casos), CLI | sim | sim, no Wine, com o YANC compilado para Windows | não |
| Icarus, Verilator, Yosys, `dot`, surfer-aurora do bundle | sim (bundle 2026.09.29: Icarus 14.0 devel, Verilator 5.053 devel, Yosys 0.69+156, Graphviz 2.43, surfer-aurora v0.7.0-nips.10) | não | não |

O Wine executa a API do Windows (criação de processo, caminhos `C:\`,
`.exe`, ambiente), mas não é o Windows. A verificação completa nos três
sistemas, com todas as ferramentas, é o workflow `.github/workflows/ci.yml`.

O que muda por sistema, e como o Solar trata:

| Assunto | Tratamento |
|---|---|
| extensão de executável | `.exe` no Windows para o YANC e para o modelo do Verilator (`EXE_SUFFIX`) |
| caminhos | `camino` em toda a API; `dunce` tira o prefixo `\\?\` que os compiladores C não entendem; `.spf` aceita `\` e `/` |
| limite de caminho | 259 caracteres no Windows (MAX_PATH dos `.exe` do YANC), 1000 nos outros (buffers do YANC); conferido por arquivo antes de compilar |
| nomes reservados | `CON`, `PRN`, `AUX`, `NUL`, `COM0-9`, `LPT0-9` recusados em todo sistema |
| maiúsculas | processadores que diferem só na caixa são recusados (Windows e macOS não distinguem) |
| ambiente do filho | Windows: `SystemRoot`, `windir`, `TEMP`, `TMP`. Surfer: `HOME`, `DISPLAY`, `WAYLAND_DISPLAY`, `XDG_*`, `TMPDIR` (Unix) ou `USERPROFILE`, `APPDATA`, `LOCALAPPDATA` (Windows) |
| término de processo | sinal no Unix (`Signaled`), NTSTATUS no Windows (`Exception`) |
| ferramentas do OSS CAD Suite | lançadores bash via `/bin/bash` (Linux, macOS); `.exe` com `PATH=bin;lib` (Windows) |
| Verilator | script Perl pelo Perl do sistema; Python do bundle no `make`; sem `-march=native` no macOS |
| fins de linha | CRLF aceito em toda leitura (`.spf`, fontes, saídas); no Windows o YANC grava CRLF |

---

## 10. Diferenças em relação à AURORA

| Assunto | AURORA | Solar |
|---|---|---|
| Abrir projeto | regrava o `.spf` (`lastOpened`, `exists`) | só lê |
| `clk` fracionário | trunca | recusa |
| `-` no nome do processador | aceita | recusa (o `cmmcomp` não aceita) |
| `#PRNAME` ausente ou diferente | compila e gera nomes errados | recusa antes de compilar |
| Diretório temporário | `<raiz>/.aurora/Temp` | `<raiz>/.solar/Temp` |
| Idioma do YANC | o da interface (`-pt` ou `-en`) | sempre `-en` |
| Ambiente do filho | herda o da AURORA | vazio + o mínimo |
| Classificação de arquivos | por conteúdo, automática | quem registra diz o papel |
| Testbench no `Simulation/` | copia `<nome>_tb.v` para lá | não copia; simula do temporário |
| Simulação de processador | só pelo modo projeto | `simulate` direto, além de `simulate_project` |
| Ferramentas | `components/` baixados pela AURORA | bundle versionado instalado com o Solar |
| Esquemático | netlistsvg (fork próprio), dentro do processo, ~50 skins | `show` do Yosys + `dot` do Graphviz |
| Onda do Verilator | FST | VCD (o FST do Verilator do bundle exige lz4 e zlib do sistema) |
| Layout do Surfer | gera `.surf.ron` e tabelas de tradução | recebe um layout pronto, ou nenhum |
| Teste de hardware (THTEST) | harness C++ próprio | não implementado |
