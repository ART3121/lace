# Mudanças

## 0.2.0 (não publicada)

A 0.1.0 saiu com o nome Solar; esta é a mesma linha com o nome Lace (o
executável `lace`, a pasta `~/.local/share/lace`, `LACE_*` no lugar de
`SOLAR_*`). O Lace não troca o Solar: quem tem o Solar 0.1.0 o desinstala e
instala o Lace (INSTALL.md, "Quem tem o Solar 0.1.0").

Lace Studio e instaladores:

- O Lace Studio, o ambiente gráfico, passa a morar neste repositório
  (`studio/`), com a versão do Lace, e a sair no instalador como o
  componente `studio`, marcado na instalação Recommended. Fica no bundle
  (`toolchain/studio/`), usa o bundle em que está e ganha atalho no menu de
  aplicativos: o `lace-studio.desktop` no Linux, o `Lace Studio.app` em
  `~/Applications` no macOS, o menu Iniciar (e, se marcado, a área de
  trabalho) no Windows. `lace install studio` o instala depois, e o
  `uninstall.sh` tira o atalho (ADR 0013, `lace_installer::desktop`).
- Os instaladores avisam quando falta o WebView do Studio: o webkit2gtk 4.1
  no Linux, o WebView2 no Windows.
- O cocotb também é componente no Windows, do bloco MSYS2 do lace-toolchain
  (o Python e a VPI do Verilator compilados lá). Com isso, todo aplicativo
  do bundle é selecionável nas três plataformas; só a linha de comando é
  sempre instalada.
- O bloco de Windows (`msys`) vem do lace-toolchain `ucrt64-v1`.
- No Windows, o `lace.exe`, o instalador, o Studio e o surfer-aurora levam
  o runtime do Visual C++ dentro (`+crt-static`, `.cargo/config.toml`): não
  pedem mais o `VCRUNTIME140.dll`. O Lace cria a pasta `tmp/` do MSYS antes
  do Verilator, cujo `make` avisava `could not find /tmp` a cada chamada.
- Mínimos documentados (INSTALL.md, Requisitos): no Linux, a glibc 2.35
  (Ubuntu 22.04, Debian 12, Fedora 36), porque o CI compila no
  `ubuntu-22.04` (antes, `ubuntu-24.04`, que exigia a 2.39); no macOS, o 13,
  com `MACOSX_DEPLOYMENT_TARGET=13.0` no build.
- O `release.yml` publica os pedaços do bundle na raiz da release (antes eles
  ficavam em `apps/` e o passo das somas falhava) e confere que o Studio
  tem a versão do Lace.

O Lace passa a cobrir o desenvolvimento em Verilog e o de processadores
SAPHO num fluxo só. Um projeto é Verilog; os processadores, quando existem,
são compilados pelo YANC antes de verificar, simular ou sintetizar. Um
projeto só de Verilog não precisa do YANC: a biblioteca SAPHO só entra
quando o projeto tem processadores (antes, entrava sempre que o YANC estava
instalado).

Comandos e opções novos:

- `lace add <arquivo.v>... [--tb]` registra arquivos Verilog e decide pelo
  conteúdo, com a regra da AURORA, se cada um é módulo ou testbench. O
  arquivo que não existe é criado a partir de um modelo, e o testbench-modelo
  já instancia o módulo testado. O primeiro módulo vira o topo; o primeiro
  testbench, o simulado.
- `lace remove <arquivo.v>...` tira do projeto, sem apagar do disco.
- `lace top [arquivo|módulo]` mostra o topo ou o escolhe, por arquivo ou
  por nome de módulo. Qualquer Verilog pode ser o topo, inclusive o
  `Hardware/<proc>.v` que o build de um processador gerou; a exceção é o
  nome de testbench (`tb_<nome>.v`, `<nome>_tb.v`, `tb.v`,
  `verilog::is_testbench_name`), recusado com `invalid_name`. `lace add`
  não escolhe sozinho um arquivo com nome assim como topo.
- `lace check <arquivo>` verifica só os módulos de um arquivo;
  `lace check --lint` acrescenta o `verilator --lint-only -Wall`.
- `lace sim <testbench>` escolhe o testbench e simula.
- `lace wave`, sem argumento, abre a onda da simulação do projeto;
  `lace wave -p <proc>`, a do processador.
- `lace wave` e `lace sim --open` abrem a onda de um processador SAPHO
  com o layout da AURORA: as variáveis do programa, a instrução de assembly
  e a linha do C± de cada ciclo, em grupos (I/O, instruções, variáveis,
  flags). As tabelas do YANC (`trad_opcode.txt`, `trad_cmm.txt`) viram
  tradutores do Surfer em `.lace/Temp/surfer/`, sem tocar na configuração
  do usuário. `--no-layout` abre a onda crua. No Core, `wave_layout` e
  `prepare_wave_layout` (ADR 0011).
- O bundle traz o cliente web do surfer-aurora (`surfer-aurora/web/`, da
  mesma tag do executável), para interfaces que mostram a onda numa página;
  `Toolchain::surfer_web_dir` o acha.
- `lace status` mostra o módulo de topo e os `.v` da pasta que não estão
  registrados.
- `lace move <origem>... <destino>` move ou renomeia arquivos e pastas do
  projeto, com os argumentos do `mv`, e mantém o `.spf` em dia: os arquivos
  registrados continuam registrados no lugar novo, na mesma posição, e o
  topo, o testbench escolhido e o módulo de topo acompanham. O `.spf`, a
  `.lace` e as pastas e o fonte dos processadores ficam no lugar; o destino
  que existe não é sobrescrito (`Project::move_path`, erros novos
  `outside_project`, `cannot_move` e `path_exists`).
- `lace hierarchy [-p <proc>]` mostra a árvore de instâncias do design e de
  cada testbench (os registrados e o de cada processador compilado), como o
  Icarus as elabora: parâmetros resolvidos, blocos `generate` expandidos e
  as unidades da biblioteca SAPHO que o processador usa. Uma elaboração que
  falha não esconde as outras. Não compila nem grava relatório
  (`lace_core::hierarchy`, `docs/schema/hierarchy.json`).

Nome de projeto:

- `lace new` (e `Project::create`) só aceita letras sem acento, dígitos,
  `_` e `-`, começando por letra: o nome vira a pasta, o `.spf` e parte do
  caminho de tudo que as ferramentas recebem. Antes aceitava espaço e
  acento. Projetos que já existem, como os da AURORA, continuam abrindo.
  `validate_project_name` é pública, para uma interface avisar enquanto o
  usuário digita.

Comandos e opções que saíram:

| Saiu | No lugar |
|---|---|
| `lace file add` (`--testbench`, `--create`) | `lace add` (`--tb`); o arquivo que não existe é criado |
| `lace file remove` | `lace remove` |
| `lace file top` | `lace top` |
| `lace file testbench` | `lace sim <testbench>` |
| `lace file list`, `lace proc list` | `lace status` |
| `lace input` | escrever `<proc>/Simulation/input_<n>.txt`, um valor por linha |
| `lace output` | `lace sim -p` mostra as saídas; o arquivo continua em `<proc>/Simulation/output_<n>.txt` |
| `lace config` (`show`, `path`, `set-compiler`, `unset-compiler`) e o arquivo de configuração | `--compiler <DIR>` ou a variável `LACE_COMPILER`, em qualquer comando |
| `--config`, `LACE_CONFIG` | nenhum: não há mais arquivo de configuração |
| `proc set --show-arrays` | `proc set --arrays` |
| `build --freq`, `--clocks`, `--show-arrays`; `sim --freq`, `--clocks` | `proc set --freq`, `--clocks`, `--arrays`, que gravam no `.spf` |
| `sim --simulator verilator` | `sim --verilator` |
| `sim --vcd` | a extensão do `$dumpfile` do testbench |
| `sim --jobs` | nenhum: o Verilator usa todos os núcleos |
| `sim --no-build`, `synth --no-build` | nenhum: os processadores são sempre compilados antes |
| `synth --no-widths` | nenhum: o esquemático sempre traz as larguras |
| `wave --view`, `wave --wait` | nenhum |

`lace completions` continua, mas não aparece mais na ajuda.

De qualquer pasta do projeto:

- Todo comando funciona de qualquer pasta do projeto, não só da raiz: o Lace
  procura o `.spf` na pasta atual e nas de cima (`Project::discover`). Fora
  de um projeto, o erro é `project_not_found`.
- Dentro da pasta de um processador (`soma/`, `soma/Software/`,
  `soma/Simulation/`), `build`, `check`, `sim`, `wave`, `synth` e
  `proc set` sem nome valem para ele; no resto do projeto, para o projeto
  inteiro (`proc set` pede o nome). `lace status` marca o processador da
  pasta, e o JSON dele ganha `here`.
- `lace check -p <proc>` verifica só o Verilog do processador e o testbench
  gerado pelo YANC (`CheckOptions::processor`).

Saída em texto:

- Num terminal, uma barra mostra que cada passo está rodando (`Simulating
  (vvp)`, `Compiling the model (verilator)`), com o tempo decorrido. É
  indeterminada, porque os simuladores não informam quanto falta, e só
  aparece depois de 0,3 s.
- O fim de cada operação diz o que ela fez e como terminou (`finished in
  0.09 s`, `failed after 2 ms: cmmcomp exited with code 1`), lista cada
  passo (`done`, `failed`, `not run`) e mostra os artefatos com o papel e o
  caminho: `Generated`, ou, numa falha, `Written before it stopped` e `Not
  generated`. Quando um build falha antes, `check`, `sim` e `synth` dizem que
  a fase seguinte não rodou. Depois da simulação, a dica de abrir a onda.

Testbench dos processadores:

- O build copia o testbench que o YANC gera na pasta temporária para
  `<processador>/Simulation/<processador>_tb.v`, como a AURORA, e a
  simulação roda essa cópia. Antes ele ficava só em
  `.lace/Temp/<processador>/`, e parecia que o YANC não o gerava.

Relatórios:

- Cada `lace build`, `check`, `sim` e `synth` grava um relatório no
  histórico do projeto (`.lace/reports/run-NNNNNN/`): status, comando,
  máquina, ferramentas do bundle com versão e caminho, projeto, tempo de
  cada fase e de cada ferramenta, estatísticas de síntese, artefatos e, numa
  falha, o erro e o fim da saída do passo. É a função de relatório do
  Alpha-Solar, trazida para o Core
  ([ADR 0010](docs/adr/0010-relatorio-e-historico-de-cada-operacao.md)).
- `lace report` mostra o mais novo; `lace report show <ID>`, um deles;
  `lace report list [--limit N]`, todos; `lace report compare [ID]
  [--against ID] [--summary]` compara as estatísticas de síntese e os tempos
  de simulação de dois, com avisos quando o contexto difere.
- `lace report clean` apaga relatórios: todos, todos menos os `N` mais novos
  (`--keep N`) ou os pedidos pelo identificador. Apagar em massa pergunta
  antes; sem terminal, exige `--yes`. O número de um relatório apagado não
  volta.
- A síntese grava o `stat -json` do Yosys, e o `SynthesisResult` traz as
  estatísticas em `statistics`.
- Todo resultado de operação ganha `duration_ms`, do começo ao fim; o JSON
  de `build`, `check`, `sim` e `synth` ganha `report`, o relatório gravado.

Bundle de Windows:

- No Windows, o Icarus e o Verilator passam a vir do bloco MSYS2 UCRT64 do
  repositório lace-toolchain (o pacote `msys` de `bundle/versions.json`),
  com o g++, o `make` e o Perl que o Verilator usa e o Python com o cocotb.
  O Verilator funciona sem instalar o MSYS2, e o Lace não procura mais o
  MSYS2 em `C:\msys64`; `--compiler` continua trocando o compilador, para
  desenvolvimento. O Yosys continua vindo do OSS CAD Suite. No Linux e no
  macOS nada muda: tudo do OSS CAD Suite, com o compilador do sistema
  ([ADR 0009](docs/adr/0009-windows-com-o-bloco-msys2-do-lace-toolchain.md)).
- `lace tools` diz se o compilador do Verilator é do bundle ou do sistema;
  no `--json`, `system_compiler.bundled`.
- `lace update` compara o Icarus e o Verilator do Windows com as releases
  do lace-toolchain.

Bundle de Linux e macOS:

- Componente novo, `cocotb`, fora da Recommended: o cocotb do OSS CAD Suite
  com o Python que o roda e o pytest, para testbenches em Python no Icarus
  (exigido) e no Verilator. Acrescenta 115 MiB no Linux e 76 MiB no macOS
  (42 e 25 MiB com o Verilator, que traz o mesmo Python). O Lace ainda não
  tem fluxo de cocotb, e no Windows o componente ainda não existe.
- O empacotamento segue o `$ORIGIN` do RUNPATH das bibliotecas do Linux, e
  aceita exceções ao `exclude` (`keep` em `bundle/components.json`).

Instalar aplicativos do bundle:

- `lace install` instala aplicativos do bundle (Verilator, Yosys, ...) na
  instalação, sem reinstalar o Lace. Sem nomes, abre no terminal a lista,
  com os instalados travados; `lace install verilator` instala direto. Só
  os pedaços dos aplicativos novos são baixados e extraídos em `toolchain/`,
  e uma falha no meio desfaz o que entrou.
- A release passa a publicar o bundle em pedaços
  (`lace-<versão>-<plataforma>-<pedaço>`, com o índice), conferidos pelo
  `SHA256SUMS`: é de lá que o `lace install` baixa. `--from` usa um
  instalador no disco; `LACE_RELEASE_URL`, um espelho.
- `lace-pack tui --assets <DIR>` grava esses pedaços com os nomes da
  release.

Atualizar:

- `lace update` compara o Lace, o bundle e cada aplicativo instalado com
  a última release e com a última versão upstream, e marca o que é mais
  novo; `--check` só mostra. Com um Lace mais novo, pergunta (`--yes` não
  pergunta) e reinstala pelo instalador da release, conferido pelo
  `SHA256SUMS`, com os mesmos aplicativos, a mesma pasta e o mesmo atalho; no
  Windows, abre o assistente. Ferramenta mais nova upstream não é instalada
  direto: chega num bundle novo, numa release nova do Lace.

Desinstalar:

- `lace uninstall` remove a instalação de onde ele roda, com o bundle
  inteiro: no Linux e no macOS roda o `uninstall.sh` da pasta; no Windows
  abre o desinstalador. Pergunta antes; `--yes` não pergunta. Apaga também o
  arquivo de configuração da 0.1.0.
- O `uninstall.sh` apaga também as sobras de uma instalação interrompida
  (`.instalando-*`, `.antigo-*`), que antes deixavam a pasta para trás.

Cancelamento, prazo e saída ao vivo:

- O Ctrl+C (e o SIGTERM, e no Unix o SIGHUP) cancela a operação: o Lace
  encerra a ferramenta que roda, com tudo o que ela iniciou, mostra o que
  chegou a rodar e sai com o código 130. Um segundo Ctrl+C sai na hora.
  Antes, o sinal matava só o Lace, e um `vvp` de testbench sem `$finish`
  continuava rodando sozinho.
- `lace sim --timeout <SEGUNDOS>` encerra a simulação depois do prazo
  (código de saída 1, status `timed_out`), com a dica do `$finish`.
- Em texto, o `$display` do testbench sai enquanto a simulação roda, e não
  mais só no fim. Com `-v`, saem o comando e toda a saída de cada passo,
  também enquanto rodam.
- `--events`, opção global: o stdout tem um objeto JSON por linha, um evento
  por passo que começa, por linha que as ferramentas escrevem e por passo
  que termina, e na última linha `{"event": "result", "result": ...}` com o
  mesmo objeto do `--json`.

Contrato do `--json`:

- O JSON Schema da saída de cada comando está em `docs/schema/`, gerado dos
  tipos e conferido por teste contra a saída real.
- `lace proc add` escreve o processador criado, o mesmo objeto de
  `lace proc set`; antes, `{message, path}`.
- `lace sim` escreve sempre `outputs` e `surfer_pid`, também quando um
  processador não compila.
- Em `lace tools`, as ferramentas de `tools` saem em ordem alfabética.

Comportamento:

- `check` elabora todas as raízes do design, sem `-s`: um módulo que ninguém
  instancia também é verificado, e o projeto não precisa ter topo. Depois,
  elabora cada testbench registrado com o design. Antes, só elaborava a
  partir do topo e deixava os testbenches de fora.
- `check` e `synth` compilam antes os processadores, como o `sim` já fazia.
- `sim` sempre mostra a saída do testbench (`$display`, `$monitor`); antes,
  só com `-v`.
- `sim` falha quando o testbench chama `$error` ou `$fatal`. O `vvp` sai com
  0 nesse caso, e antes isso passava como sucesso. O `vvp` roda com `-n`:
  `$stop` termina a simulação.
- `sim -p` mostra os valores de cada porta de saída e diz qual arquivo de
  entrada criar quando falta um.
- Num testbench sem `$dumpfile`, o Lace injeta `$dumpvars(0, <tb>)`: a onda
  traz todos os sinais, inclusive os do módulo testado. Antes, o nível 1 só
  trazia os do testbench.
- O formato da onda segue a extensão do `$dumpfile`. Antes, o Icarus gravava
  FST mesmo num arquivo `.vcd`; agora a onda de um processador
  (`<nome>_tb.vcd`) é VCD, e a injetada é `<tb>.fst` no Icarus e `<tb>.vcd`
  no Verilator.
- O módulo do testbench vem do conteúdo do arquivo, não do nome.
- Caminhos na linha de comando são relativos ao diretório atual do shell.
  Antes, os de `lace file` eram relativos à raiz do projeto, e um caminho
  relativo em `lace wave` não abria.
- `build` num projeto sem processadores avisa e sai com 0; antes, era erro.
- O log do Surfer sai de perto da onda: vai para `.lace/Temp/surfer/` do
  projeto (`<onda>.log`) ou, com a onda fora de projeto, para a pasta de
  cache do usuário. Antes, `<onda>.surfer.log` ficava ao lado da onda, na
  raiz do projeto no caso da onda injetada.
- Os erros de projeto sem topo, sem testbench ou vazio vêm com o comando que
  resolve.

Saída em inglês:

- O que o `lace`, o instalador, o `install.sh` e o `install.ps1` mostram
  passa a ser em inglês, e toda mensagem começa com maiúscula. `erro:` e
  `aviso:` viram `Error:` e `Warning:`; os erros de argumento do `clap` saem
  do mesmo jeito. Nos diagnósticos, a gravidade continua em minúscula, como
  no gcc (`arquivo:linha: error: ...`), porque é o que os editores
  reconhecem.
- As confirmações de `lace update` e `lace uninstall` pedem `[y/N]` e
  aceitam `y` ou `yes`.
- No `--json` e no `--events`, os textos de `message` e `hint` mudaram
  junto; os `code`s e as chaves continuam os mesmos. As mensagens de
  `LaceError` no `lace-core` também passam a ser em inglês.
- `lace --help` ficou mais curto e em inglês, junta as opções globais num
  grupo só e não mostra mais o `--toolchain`, que é de desenvolvimento.
- Os tipos de instalação se chamam Recommended e Advanced. No assistente do
  Windows, `/TYPE=recomendada` e `/TYPE=avancada` continuam valendo.

`lace-core`:

- `Control`, `CancelToken`, `Event` e `Stream` (ADR 0007). Toda operação que
  executa ferramentas recebe `&Control` como último argumento: `build`,
  `build_processors` (antes do `on_result`), `check`, `simulate`,
  `simulate_project`, `synthesize` e `render_schematic`. Com
  `&Control::default()`, nada muda.
- `Status::Cancelled` e `Status::TimedOut`; `Termination::Cancelled` e
  `Termination::TimedOut`. `SimulationOptions::timeout`.
- Cada passo roda num grupo de processos próprio (Unix); cancelar encerra o
  grupo, ou a árvore com o `taskkill` no Windows.
- Com quem receba os eventos, o `vvp` roda com `-i` (stdout sem buffer). O
  Verilator compila o modelo com `--autoflush`: como a linha de comando
  mudou, a primeira simulação depois da atualização pode compilar o modelo
  de novo.
- Os resultados derivam `schemars::JsonSchema` (ADR 0008).
- `check(toolchain, project, &CheckOptions)` substitui `check_syntax`;
  `CheckResult` ganhou `targets`, e `Step` ganhou `Lint`.
- `SimulationOptions` perdeu o campo `fst`; `waveform_path` diz onde a
  simulação grava a onda.
- Módulo `lace_core::verilog`: `classify`, `modules_in`, `read_interfaces`,
  `module_template`, `testbench_template`.
- `Project::add_verilog`, `remove_verilog`, `set_top`, `top_module`,
  `testbench_module`, `unregistered_verilog`, e o tipo `AddedFile`.
- `LaceError::EmptyProject` (`empty_project`) e `ModuleNotFound`
  (`module_not_found`). As mensagens de `NoTopLevel` e `NoTestbench` não
  citam mais comandos.

Correções do teste de fogo (2026-10-04):

- Processador num caminho com acento ("Área de Trabalho") simulava "com
  sucesso" sem programa nem entradas, porque o `vvp` não abre nome de
  arquivo com acento. Agora `sim` recusa antes com `non_ascii_path`, e o
  aviso do `vvp` ("contains non-printable characters") vira erro.
- O esquemático falhava em qualquer projeto com acento no caminho: o
  `write_json` do Yosys estraga os bytes fora do ASCII, e o Lace agora os
  conserta no `hierarchy.json` depois da síntese.
- O esquemático de um módulo com muitas ligações deixava o `dot` rodando por
  minutos e consumindo centenas de MB. Agora `synth --svg` recusa acima de
  120 ligações (`schematic_too_large`, `--no-schematic-limit` desliga) e o
  `dot` tem prazo de 60 s (`SchematicOptions::max_connections` e `timeout`).
- Duas gravações simultâneas do `.spf` (o Studio e a CLI, dois `lace add`)
  perdiam alterações. Cada mudança agora trava `.lace/spf.lock`, relê o
  `.spf` e grava com um temporário de nome único.
- A simulação do projeto sobrescrevia, na raiz, um arquivo do usuário com o
  mesmo nome de um arquivo de dados do testbench, e com `../` chegava a
  escrever fora da pasta do projeto. Agora não sai da raiz nem sobrescreve o
  que não foi copiado pelo próprio Lace (`.lace/Temp/data_copies.txt`); o
  que não foi copiado vira aviso.
- `synth` de arquivo `.sv` falhava: o Yosys agora lê com `-sv`.
- Um testbench sem `$dumpfile` que chega ao `$finish` no tempo 0 era
  reprovado por falta da onda injetada; a onda injetada não é mais exigida.
  O dump entra no `endmodule` do módulo do testbench (antes, no último do
  arquivo, inclusive dentro de comentário), e os diagnósticos apontam para o
  testbench do usuário, não para a cópia em `.lace/Temp`.
- Uma saída de processador com `x` (divisão por zero) derrubava `sim -p` com
  código 2, sem resumo nem relatório. Agora a porta sai como "unreadable",
  com o motivo em `error` no JSON, e o resto continua.
- `include` e `$readmem` relativos: o Icarus procura o `include` na pasta do
  arquivo e depois na raiz (`-grelative-include -I <raiz>`), o Yosys recebe
  `-I <raiz>` e roda com CWD na raiz, como a simulação. O mesmo projeto passa
  a valer em `check`, `sim`, `hierarchy` e `synth`.
- A versão mínima do Rust sobe para 1.89 (`File::lock` da biblioteca padrão).

Correções médias do teste de fogo (2026-10-04):

- Caminhos no `.spf` (ADR 0012, emenda a 0003). Um arquivo de fora da pasta
  do projeto é gravado com `..` quando está no mesmo repositório git (o
  `rtl/` do HITS), e absoluto quando não. Um absoluto de outra máquina
  (`C:\Users\...`) é procurado pela cauda dentro da pasta, como a AURORA, e
  a próxima gravação conserta o `.spf`.
- `Project::issues()` (e `issues` no `lace status --json`): avisos do `.spf`
  que não impedem de abrir, como caminho achado pela cauda, `topLevelFile`
  fora da lista e processador com nome que não compila.
- O `.spf` com tipo errado nos campos de arquivos (`synthesizableFiles`
  objeto, `topLevelFile: 42`), com `structure` que não é objeto, ou com
  processador de nome vazio, com barra ou repetido agora é
  `invalid_project_file`, com o campo; antes era ignorado e apagado na
  gravação seguinte, ou tomava a raiz como pasta do processador.
- `topLevelFile` ou `testbenchFile` fora da lista, ou topo com nome de
  testbench, é ignorado por todas as operações (antes `status`, `top`,
  `synth` e `check` discordavam).
- Só `.v` e `.sv` entram nas listas: `lace top README.md` e
  `lace sim notas.txt` recusam sem gravar nada. `lace add` com vários
  arquivos confere todos antes de registrar o primeiro
  (`Project::check_add_verilog`). Um arquivo só de `` `define `` não vira
  topo.
- `lace top MÓDULO` com o módulo em dois arquivos é `ambiguous_module`, com
  os arquivos; a lista de `module_not_found` não repete nomes.
- `proc add` recusa nome que não compila (palavras do C± e do Verilog,
  módulos da biblioteca SAPHO, mais de 64 caracteres;
  `validate_processor_name`) e parâmetro fora do que o YANC compila
  (`invalid_parameter`, `NewProcessor::validate`). `--nubits` e os outros
  de C± com `--lang cpp` recusam; só `--nbmant` ou `--nbexpo` ajusta o
  `--nubits`. `proc set` recusa frequência acima de 500000 MHz e clocks
  acima de 2147483647 com o motivo (antes o erro vinha depois, do `asmcomp`
  ou do Icarus). Um processador do `.spf` com nome que não compila (`x-y`)
  abre com aviso, e o `build` recusa com `invalid_name` antes do YANC.
- A verificação acha as raízes num passe por arquivo (1500 arquivos: de
  1 s para milissegundos), e o título diz quantas são e nomeia cinco. Com
  recursão parametrizada, `check` e `hierarchy` passam as raízes com `-s`
  (antes: "No top level modules").
- Testbench com dois módulos e nenhum com o nome do arquivo usa o único que
  o outro não instancia, em `check`, `sim` e `hierarchy`.
- Macro indefinida no ponto de uso é erro no `check` e na simulação, com a
  explicação da ordem dos arquivos; antes passava com a largura errada.
- O parser do Yosys reconhece `ERROR:` sem espaço e o aviso de latch
  (antes, `unknown` sem arquivo). O erro `Error in function f: where's the
  return...` do `cmmcomp` vira erro com o fonte.
- `lace hierarchy` marca cada passo pelo próprio resultado (um testbench que
  elaborou não aparece mais como `failed`).
- `check --lint` sem o Verilator roda a verificação e avisa que o lint não
  rodou; a CLI sugere `lace install <componente>` para todo
  `component_missing` (o Core não cita mais o instalador).
- `synth --svg --module X` com X fora da árvore do topo grava o relatório e
  sai com `module_not_found` e os módulos sintetizados (antes
  `invalid_netlist`, sem relatório). `schematic_too_large` também grava.
- Simulação: `$dumpfile(ONDA)` com `localparam`, `parameter` ou `` `define ``
  usa o nome deles; com uma expressão, o Lace não injeta outro dump (antes o
  `-fst` dele gravava FST no `.vcd` do usuário). `$dumpfile` sem `$dumpvars`
  dá aviso.
- Simulação de processador: uma entrada que não é inteiro recusa
  (`invalid_data_file`); arquivo vazio ou valor que não cabe em `#NUBITS`
  dá aviso. Programa que não chega ao fim nos clocks dá aviso para subir os
  clocks.
- Build e simulação travam o processador (`.lace/Temp/<nome>.lock`) e a
  simulação do projeto trava o projeto: duas ao mesmo tempo, do Studio e da
  CLI, não se quebram mais; a segunda recusa com `operation_in_progress`.
- A saída guardada de cada passo tem teto: 2 MiB do começo e 2 MiB do fim
  (um testbench que imprime sem parar crescia sem limite na memória).
- O histórico usa nomes únicos por operação também dentro do mesmo processo.
- Layout da onda: os rótulos de I/O usam o número da porta (`in_sim_2` é
  `input 2`); tabelas mais novas que a onda não são aplicadas
  (`WaveProcessor::outdated`, e a CLI avisa para simular de novo); onda sem
  processador SAPHO ganha o grupo Top-level com os sinais do testbench; a
  varredura dos complexos lê em bytes, e o teto de 16384 valores está
  documentado.

Correções baixas do teste de fogo (2026-10-04):

- O mesmo arquivo repetido numa lista do `.spf` (também por outro caminho)
  ou nas duas conta uma vez, com aviso (`duplicate_file`); a próxima
  gravação tira as entradas que sobram. O `isMarkedTestbench` legado da
  AURORA escolhe o testbench, como nela.
- `Project::reorder_file` e `lace order`: muda a posição de um arquivo na
  lista (a ordem dos compiladores, que decide quem vê cada `` `define ``).
- `lace new`: nome até 64 caracteres (com 248, a pasta ficava vazia para
  trás); nome que só difere na caixa de outro da mesma pasta recusa; projeto
  dentro da pasta de outro avisa (`nested_project`), e o de fora não lista
  mais os arquivos do de dentro como não registrados.
- `lace move` não sobrescreve um link quebrado no destino, renomeia só a
  caixa num sistema que não distingue maiúsculas, e uma origem que não
  existe é `invalid_project` (antes, `io`).
- A CLI não repete a causa de um erro de I/O ("No such file or directory"
  duas vezes).
- Texto: síntese que falhou não diz mais "(0 modules)"; o prazo aparece
  como `stopped at the 3 s limit`; a continuação `: Padding ...` do Icarus
  entra no aviso anterior; `*** These modules were missing` vira um resumo
  só; na tabela de ferramentas do relatório, o passo que estourou o prazo
  aparece como `TIMEOUT`, e não `FAIL`.
- Simulação: `$display("ERROR: ...")` do testbench fica como saída dele e
  não reprova (só `$error` e `$fatal`); a linha `Time: ... Scope: ...` do
  `$error` entra na mensagem; o programa que lê mais valores do que um
  `input_<n>.txt` tem gera aviso com o arquivo e a linha; VCD acima de
  100 MB sugere FST.
- Síntese: o `check` do Yosys aponta laço combinacional e drivers em
  conflito; módulo vazio (caixa-preta) sai da lista de módulos do
  esquemático; um módulo que se instancia sem condição falha antes de rodar
  o Yosys (que rodava sem fim), e uma queda do Yosys com recursão vem
  explicada.
- `lace report compare`: a referência automática prefere uma execução que
  terminou bem; relatórios de um projeto movido continuam se comparando;
  dois sem parte em comum dão `not_comparable`.
- Layout da onda: processador em C rotulado `C`; array com mais de 9999
  elementos num grupo só, com o índice certo; função com `_v_` no nome
  desfeita pelo `cmm_log.txt`; escopo escapado com `.` num componente só;
  `delta_*` real sem formato de bits; uma pasta de layout por onda (duas
  `pa_tb.vcd` não dividem mais a pasta).

O que o teste de fogo achou e ainda não foi corrigido está em
`docs/PENDENCIAS.md`.

## 0.1.0 (2026-09-29)

Primeira versão. Substitui a orquestração da AURORA para o SAPHO.

- `lace-core`: API para criar projetos e processadores (formato `.spf` da
  AURORA), compilar com o YANC (C± e C), simular com Icarus Verilog e
  Verilator, verificar sintaxe e sintetizar com Yosys, desenhar o esquemático
  com o `show` do Yosys e o Graphviz, e abrir ondas no surfer-aurora.
- `lace`: a linha de comando sobre o `lace-core`, com saída em texto ou
  JSON.
- Bundle de ferramentas `2026.09.29`: OSS CAD Suite de 2026-09-29, YANC v5.6,
  surfer-aurora v0.7.0-nips.10 e, no Windows, Graphviz 16.1.0. O Lace só
  executa ferramentas desse bundle; a exceção declarada é o compilador C++, o
  `make` e o Perl do sistema, para o Verilator.
- O OSS CAD Suite é dividido por ferramenta: o bundle completo de Linux tem
  300 MiB, contra os 2,5 GB do pacote inteiro.
- Instaladores: assistente (Inno Setup) no Windows, instalação guiada no
  terminal no Linux e no macOS. Tipo Recomendada por padrão; Avançada para
  escolher os componentes.

Plataformas: Linux x64, macOS Apple Silicon, Windows 10 e 11 x64.
