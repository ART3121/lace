# Contribuir com o Lace

Guia para quem vai mexer no código do Lace pela primeira vez: como preparar
a máquina, onde mora cada coisa, como fazer as mudanças mais comuns e o que
conferir antes de mandar. O que o Lace faz para o usuário está no
[README](README.md). Por que ele é como é está nas
[decisões de arquitetura](docs/adr/README.md); leia-as antes de uma
mudança grande, porque as regras deste guia saem delas.

A referência da API está em [docs/API.md](docs/API.md) e na documentação do
código (`cargo doc -p lace-core --no-deps --open`). A da linha de comando,
em [docs/CLI.md](docs/CLI.md). O bundle de ferramentas, em
[docs/BUNDLE.md](docs/BUNDLE.md).

## Preparar a máquina

### Rust

Instale o Rust pelo [rustup](https://rustup.rs), com o clippy e o rustfmt:

```sh
rustup component add clippy rustfmt
```

Use o Rust estável atual, que é o que o CI usa
(`dtolnay/rust-toolchain@stable`, em `.github/workflows/ci.yml`). O mínimo é
o 1.88 (`rust-version` no `Cargo.toml`, edição 2024): o código usa let chains
(`if let ... && let ...`, por exemplo em `diagnostics.rs`), que versões
anteriores recusam.

### Compilar e rodar os testes

```sh
cargo build
cargo test
```

Num clone recém-feito, `cargo test` passa sem nenhuma ferramenta instalada.
Os testes que precisam de YANC, Icarus, Verilator, Yosys, `dot` ou
surfer-aurora não acham o bundle, escrevem `PULADO: ...` e retornam sem
testar nada (`crates/lace-core/tests/common/mod.rs` e `env_or_skip` em
`crates/lace-cli/tests/cli.rs`). Um teste pulado aparece como `ok`; o aviso
só aparece com `cargo test -- --nocapture`. Um `cargo test` verde sem bundle
não diz nada sobre o fluxo das ferramentas.

Não defina a variável `CI` na sua máquina. Com ela definida, a falta do
bundle ou de uma ferramenta é falha, não teste pulado. O GitHub Actions
define `CI`, e por isso lá nada é pulado.

### O bundle, para os testes que rodam ferramentas

Os testes acham o bundle pela variável `LACE_TEST_BUNDLE`, que precisa ser
um caminho absoluto: o `cargo test` roda cada teste no diretório do crate,
então um caminho relativo aponta para o lugar errado. Há dois jeitos de ter
um bundle.

**Usar o de uma instalação do Lace.** Instale pelo [README](README.md) e
aponte para a pasta `toolchain/` da instalação. Não compila nada; é o que o
CI faz (`.github/workflows/installers.yml`).

```sh
export LACE_TEST_BUNDLE="$HOME/.local/share/lace/toolchain"            # Linux, macOS
```

```powershell
$env:LACE_TEST_BUNDLE = "$env:LOCALAPPDATA\Programs\Lace\toolchain"     # Windows
```

Confira com `lace tools` que o identificador do bundle instalado é o mesmo
do campo `bundle` de `bundle/versions.json`; se não for, os testes rodam
contra outras versões. A instalação Recommended não traz o Verilator, e os
testes dele são pulados.

**Montar o bundle.** Precisa de Python 3.9 ou mais novo, `git`, e do que
compila o YANC e o surfer-aurora (`gcc`, `make`, `flex`, `bison`, `cargo`; a
tabela por sistema está em [docs/BUNDLE.md](docs/BUNDLE.md), seção 6).

```sh
python3 scripts/bundle.py --out dist/toolchain               # completo
python3 scripts/bundle.py --out dist/toolchain --only yanc   # só o YANC: basta para os testes de build
export LACE_TEST_BUNDLE="$PWD/dist/toolchain"
```

Com o bundle definido, há mais duas dependências:

- **No Linux e no macOS, os testes do Verilator precisam do compilador do
  sistema** (C++, `make` e Perl; [docs/BUNDLE.md](docs/BUNDLE.md), seção 4).
  Com o Verilator no bundle e sem o compilador, eles falham. No Windows o
  compilador vem no bundle.
- **`yanc_regression` precisa do fonte do YANC** no commit do bundle (o
  `commit` de `yanc` em `bundle/versions.json`), em `vendor/yanc` ou no
  caminho de `LACE_TEST_YANC_SRC`. Sem ele, o teste falha ao ler a pasta
  dos casos de teste.

```sh
git clone https://github.com/nipscernlab/yanc vendor/yanc
git -C vendor/yanc checkout <commit de bundle/versions.json>
```

Depois, o mesmo comando que o CI roda contra a instalação:

```sh
cargo test --workspace -- --test-threads=2
```

### Variáveis dos testes

| Variável | Efeito |
|---|---|
| `LACE_TEST_BUNDLE` | o bundle dos testes que rodam ferramentas; sem ela, eles são pulados |
| `LACE_TEST_YANC_SRC` | o fonte do YANC para `yanc_regression` (padrão: `vendor/yanc`) |
| `LACE_TEST_GUI=1` | com o bundle, roda o teste que abre a janela do surfer-aurora: `cargo test --test tools surfer` |
| `CI` | com qualquer valor, bundle ou ferramenta ausente é falha |

### Rodar a CLI a partir do código

O `lace` procura o bundle ao lado do executável, e `target/debug/` não tem
um. Passe o bundle com `--toolchain` ou com a variável `LACE_TOOLCHAIN`:

```sh
cargo run -p lace-cli -- --toolchain "$LACE_TEST_BUNDLE" tools
cargo run -p lace-cli -- --toolchain "$LACE_TEST_BUNDLE" -C examples/contador check
```

`new`, `add`, `remove`, `top`, `status` e `proc` funcionam sem
bundle.

## Mapa do código

O workspace tem três crates. `lace-core` é a biblioteca, com toda a regra
de negócio. `lace-cli` é o binário `lace`, uma casca fina sobre o Core.
`lace-installer` é o instalador em terminal (Linux, macOS) e o empacotador
`lace-pack`. A divisão está na [ADR 0001](docs/adr/0001-biblioteca-com-interfaces-finas.md).

### `crates/lace-core/src/`

| Arquivo | O que mora lá |
|---|---|
| `lib.rs` | a lista de módulos, os `pub use` que formam a API pública, a documentação do crate (fluxo típico, garantias) e os atributos que valem para o crate todo |
| `toolchain.rs` | o bundle: `Toolchain` (`open`, `locate`, `tool`, `verify`, `sapho_library`), `Tool` e o caminho fixo de cada executável (`Tool::location`), `Platform`, o manifesto (`BundleManifest`, `BundleComponent`), `SystemCompiler` e o ambiente de cada ferramenta (`Toolchain::invocation`) |
| `process.rs` | o único módulo que cria processos: `Invocation`, `run` (lê a saída linha a linha, confere cancelamento e prazo, encerra o grupo de processos no Unix e o Job Object no Windows), `Watch`, `spawn`, `RunningProcess`, `Termination`; o ambiente vazio (`INHERITED_ENV`, `GUI_ENV`); `ProcessJob` e `hide_console`, que o Studio também usa |
| `control.rs` | `Control`, `CancelToken`, `Event` e `Stream`: o que a interface passa a cada operação para cancelá-la e acompanhar a saída ([Cancelamento e saída ao vivo](#cancelamento-e-saída-ao-vivo)) |
| `pipeline.rs` | o que todo resultado tem: `Step`, `Status`, `StepReport`, `Artifact`, `ArtifactKind`; por dentro, `PlannedStep`, `Runner` (roda os passos em sequência, emite os eventos do `Control` e para no primeiro que falha ou no cancelamento) e `ArtifactTracker` (o `fresh` dos artefatos) |
| `diagnostics.rs` | `Diagnostic` e `Severity`; `parse` traduz a saída de cada ferramenta em diagnósticos; `is_message` diz, linha a linha, se uma linha é mensagem da ferramenta (o campo `diagnostic` do `Event::Output`) |
| `error.rs` | `LaceError`, com a mensagem de cada variante e o `code()` estável |
| `project.rs` | `Project` (`open`, `create`, `add_processor`, `configure_processor`, `buildable_processors`, `save`), `Processor`, `NewProcessor` (o fonte-modelo de um processador), `Language`, validação de nomes (`validate_project_name`, pública) |
| `spf.rs` | leitura tolerante e gravação atômica do `.spf` da AURORA, e as entradas de processador |
| `files.rs` | os arquivos Verilog do projeto no `.spf`: `add_verilog`, `remove_verilog`, `set_top`, `top_module`, `testbench_module`, `unregistered_verilog`, `add_file`, `set_top_level`, `set_testbench`; mover e renomear com o `.spf` em dia (`move_path`, `MovedPath`); os arquivos de entrada e saída da simulação (`read_data_file`, `Processor::write_input`, `read_output_values`) |
| `verilog.rs` | módulo público `lace_core::verilog`: `classify`, `modules_in`, `read_interfaces`, `module_template`, `testbench_template` |
| `build.rs` | `build` e `build_processors` (YANC): `BuildOptions`, `BuildResult`, `OnFailure`; os comandos de cada passo em `Plan::steps` |
| `source.rs` | lê o `#PRNAME` (C±) ou o `#pragma yanc prname` (C) do fonte antes de compilar |
| `simulate.rs` | `simulate` (um processador), `simulate_project` (o testbench do projeto), `waveform_path`, `missing_inputs`; os comandos do Icarus e do Verilator em `execute`; a injeção de `$dumpvars` |
| `synth.rs` | `check` (Icarus e, com `lint`, Verilator), `synthesize` (Yosys), `render_schematic` (`show` do Yosys e `dot`); `CheckOptions`, `DesignTarget`; os arquivos do design (`project_sources`, `processor_verilog`), que `hierarchy.rs` também usa |
| `hierarchy.rs` | `hierarchy`: a árvore de instâncias do design e de cada testbench, elaborada pelo Icarus e lida do `.vvp` (`parse_vvp`); `HierarchyOptions`, `HierarchyResult`, `Elaboration`, `ModuleInstance` |
| `stats.rs` | `SynthesisStatistics`: lê o `stat -json` do Yosys que a síntese grava (`SynthesisMetric`, `CellUsage`) |
| `history.rs` | módulo público `lace_core::history`: o relatório de cada operação e o histórico em `.lace/reports/` (`record`, `list`, `load`, `latest`, `report_text`), a comparação (`compare`, `compare_reports`, `previous_comparable`) e o texto do `report.txt` (`render`) |
| `wave.rs` | `open_waveform` (surfer-aurora) e `ViewerOptions` |
| `wave_layout.rs` | `wave_layout` e `prepare_wave_layout`: o layout do Surfer dos processadores SAPHO (o `.surf.ron` e os tradutores do assembly, da linha do C± e dos complexos), lido do cabeçalho do VCD e das tabelas do YANC (ADR 0011) |
| `paths.rs` | `YANC_PATH_LIMIT`, `canonicalize` sem o prefixo `\\?\` do Windows, normalização de caminhos |

### `crates/lace-cli/src/`

| Arquivo | O que mora lá |
|---|---|
| `main.rs` | a definição da linha de comando com o `clap` (`Cli`, `Command`, `ProcCommand`, as structs `*Args`), o texto de ajuda, `main` com os códigos de saída (`EXIT_FAILED`, `EXIT_ERROR`, `EXIT_CANCELLED`), `cancel_on_signals` (Ctrl+C e SIGTERM viram cancelamento), `init_tracing` e o teste `cli_definition_is_valid` |
| `commands.rs` | uma função por comando, chamada por `run`; converte os caminhos digitados em absolutos e chama o Core com o `Control`; `build_first` compila os processadores antes de `check`, `sim` e `synth`; `record` grava cada operação no histórico, e `report` atende `lace report` |
| `output.rs` | `Output`: a saída em texto (cores só em terminal, `NO_COLOR`), em JSON (`Output::json`) e em eventos (`--events`); `Output::control` monta o `Control` de cada modo, e `live_text` mostra o testbench enquanto roda; a formatação dos diagnósticos, das tabelas de `lace report list` e `compare`, e `hint`, a dica de comando para alguns erros |
| `report.rs` | um tipo por saída do `--json` (`SimReport`, `StatusReport`, ...), o trait `Report` que `Output::json` exige, a macro `reports!` que registra cada um, e o teste que confere `docs/schema/` ([Contrato do `--json`](#contrato-do---json)) |
| `installation.rs` | a instalação de onde este `lace` roda: a pasta (`prefix`), o recibo `install.json` (`Receipt`) e o desinstalador do Windows |
| `install.rs` | `lace install`: a lista (a TUI de `lace-installer`) ou os nomes, e de onde vêm os pedaços dos aplicativos (`--from`, ou a release, por `ReleasePayload`); quem extrai é `lace_installer::add` |
| `update.rs` | `lace update`: as versões instaladas, as da última release e as upstream (atrás do trait `Sources`, para testar sem rede), e a reinstalação pela release nova |
| `release.rs` | as releases no GitHub: a última versão, baixar um arquivo conferindo o `SHA256SUMS`, `LACE_REPO` e `LACE_RELEASE_URL` |
| `uninstall.rs` | `lace uninstall`: acha a instalação ao redor do executável e roda o desinstalador que o instalador deixou (`uninstall.sh`, ou o do Inno no Windows) |
| `settings.rs` | de onde vem o bundle (`ToolchainArgs::resolve`: `--toolchain`, `LACE_TOOLCHAIN` ou ao lado do executável), o compilador declarado (`--compiler`, `LACE_COMPILER`, `compiler_in`) e `absolute` |

### O resto do repositório

| Caminho | O que é |
|---|---|
| `crates/lace-core/tests/` | `api_contract.rs` (garantias da API), `build.rs` (YANC, com snapshots em `snapshots/`), `control.rs` (cancelamento, prazo e eventos com o Icarus), `tools.rs` (simulação, síntese, esquemático, Surfer), `verilog_flow.rs` (o fluxo Verilog, mover arquivos e a hierarquia; a primeira metade roda sem ferramenta), `yanc_regression.rs` (os casos de teste do YANC), `common/mod.rs` (como achar o bundle) |
| `crates/lace-cli/tests/cli.rs` | a CLI de ponta a ponta: códigos de saída, texto, JSON, eventos e Ctrl+C |
| `crates/lace-cli/tests/schema.rs` | a saída `--json` de cada comando, validada contra `docs/schema/` |
| `docs/schema/` | o JSON Schema de cada comando, gerado dos tipos; não edite à mão |
| `crates/lace-installer/` | o instalador em terminal e o `lace-pack`; `tests/install.rs` empacota um bundle pequeno e instala de verdade |
| `examples/` | os projetos `soma`, `com_erro` e `contador`, usados pelos testes |
| `bundle/versions.json` | a versão e o hash de cada pacote do bundle |
| `bundle/components.json` | o que cada componente leva de cada pacote |
| `scripts/bundle.py`, `scripts/binaries.py` | a montagem do bundle e a leitura de dependências de binários |
| `installer/windows/lace.iss` | o instalador de Windows (Inno Setup) |
| `install.sh`, `install.ps1` | a instalação em um comando; servidos da `main`, valem assim que chegam nela |
| `.github/workflows/` | `ci.yml` (verificação a cada push), `installers.yml` (bundle, instaladores e testes contra a instalação), `release.yml` (rascunho de release numa tag) |
| `docs/` | API, CLI, bundle, instalação, release e as ADRs |

### Como uma operação anda

O caminho de `lace check`, que é o de toda operação que roda ferramentas:

1. `main.rs`: o `clap` lê os argumentos em `Cli`, e `commands::run` chama
   `check`.
2. `commands.rs`, função `check`: abre o `Project`, resolve o `Toolchain`
   (`settings.rs`), compila os processadores (`build_first`), monta
   `CheckOptions` e chama `lace_core::check` com o `Control` que `main`
   criou (`Output::control`).
3. `synth.rs`, função `check`: monta cada comando com
   `Toolchain::invocation` e o entrega a um `Runner`.
4. `pipeline.rs`, `Runner::run`: confere o cancelamento, avisa
   `StepStarted`, executa por `process::run` (ambiente vazio, stdin fechado,
   grupo de processos próprio), que avisa cada linha (`Event::Output`) e
   encerra o processo se o cancelamento chegar; interpreta stdout e stderr
   com `diagnostics::parse`, guarda o `StepReport`, avisa `StepFinished` e
   para no primeiro passo que falha.
5. De volta à CLI, `Output::check` escreve o texto e `Output::json` o
   `CheckReport`; `main` transforma `Ok(true)`, `Ok(false)` e `Err` em 0, 1 e
   2, ou em 130 se o Ctrl+C chegou.

## Onde mudar o quê

Cada receita diz os arquivos, as funções e o teste a rodar. Os nomes de
teste são os de hoje; procure-os com `grep -rn "fn <nome>" crates`.

### Acrescentar uma flag a uma ferramenta

Exemplo: uma opção nova do `iverilog` no `lace check`.

1. Em `crates/lace-core/src/synth.rs`, função `check`, a closure
   `iverilog` monta o comando base de todos os passos de verificação:
   `toolchain.invocation(Tool::Iverilog, project.root())`, depois `-g2012`
   se houver `.sv`, depois `.arg("-tnull").arg("-Wall")`. Acrescente a flag
   ali com `.arg("...")`. Caminho vai com `.path_arg(...)`, que o põe no
   formato do sistema.
2. As flags de outras etapas ficam em `Plan::steps` (`build.rs`, YANC), em
   `execute` e `verilator_cflags` (`simulate.rs`, Icarus e Verilator) e em
   `synthesize` e `render_schematic` (`synth.rs`, Yosys e `dot`).
3. Teste: `verilog_flow_from_scratch`, em
   `crates/lace-core/tests/verilog_flow.rs`, confere as flags de cada passo
   de `check` com `has_arg` (`for flag in ["-tnull", "-Wall"]`). Acrescente
   a nova. Ele precisa do Icarus:

   ```sh
   cargo test -p lace-core --test verilog_flow
   ```

4. Documente em [docs/API.md](docs/API.md), seção 5.4 (a tabela de
   comandos do `check`), e em [docs/CLI.md](docs/CLI.md), "O que `check`
   verifica".

Uma variável de ambiente nova para a ferramenta entra por
`Invocation::env` (veja `Toolchain::invocation`, em `toolchain.rs`), nunca
herdada do ambiente do usuário ([ADR 0002](docs/adr/0002-so-ferramentas-do-bundle.md)).

### Acrescentar um comando ou uma opção à CLI

1. `crates/lace-cli/src/main.rs`: uma opção nova é um campo na struct de
   argumentos do comando (por exemplo `CheckArgs`), com `#[arg(long)]` e um
   comentário `///`, que vira o texto da ajuda e por isso é em inglês. Um comando novo
   é uma variante de `enum Command`.
2. `crates/lace-cli/src/commands.rs`: o braço do `match` em `run` e a
   função do comando, que passa o valor ao Core (no `check`, um campo de
   `CheckOptions`). A função devolve `Ok(false)` quando a operação rodou e
   falhou (código 1) e `Err` quando não conseguiu rodar (código 2).
3. `crates/lace-cli/src/output.rs`: o texto, num método de `Output`. O
   JSON é um tipo de `crates/lace-cli/src/report.rs`, escrito por
   `out.json(&relatorio)`: um comando novo ganha um tipo novo, registrado na
   macro `reports!` com o nome do arquivo de schema; uma opção que muda o
   JSON muda o tipo. Gere o schema de novo (veja
   [Contrato do `--json`](#contrato-do---json)). Com `--json`, o comando
   escreve um único objeto JSON no stdout e nada mais.
4. Se a opção pede uma regra nova (decidir, validar, escolher arquivo), a
   regra vai para o Core, e a CLI só repassa
   ([ADR 0001](docs/adr/0001-biblioteca-com-interfaces-finas.md)).
5. Testes:
   - `cli_definition_is_valid`, em `main.rs`, confere a definição do
     `clap` (nomes repetidos, conflitos): `cargo test -p lace-cli --bins`;
   - um teste em `crates/lace-cli/tests/cli.rs`, com os auxiliares de lá:
     `new_project`, `lace_in(&cfg, &root)` para rodar dentro do projeto,
     `json(&mut cmd, código)` e `stdout(&mut cmd, código)`. Se precisar de
     ferramenta, comece com `env_or_skip("LACE_TEST_BUNDLE")` e passe o
     bundle por `LACE_TOOLCHAIN`, como `build_ok_is_exit_code_0` faz.

   ```sh
   cargo test -p lace-cli
   ```

6. Documente em [docs/CLI.md](docs/CLI.md): a tabela do comando e, se o JSON
   mudou, a linha do comando na seção JSON.

### Interpretar uma mensagem nova de uma ferramenta

Quando uma ferramenta escreve algo que o Lace mostra como `unknown`, ou com
a gravidade errada:

1. `crates/lace-core/src/diagnostics.rs`: `parse_line` escolhe o
   interpretador pela ferramenta: `parse_yanc_phrase` (`cmmcomp`, `appcomp`,
   `asmcomp`), `parse_c_style` e `parse_tool_prefixed` (`cpppp`, `cppcomp`),
   `parse_c_style` e `parse_icarus` (`iverilog`, `vvp`), `parse_verilator`,
   `parse_yosys`, `parse_graphviz`. Acrescente o caso na função da
   ferramenta.
2. Mantenha as regras do módulo: o texto original fica em `raw`; linha do
   stderr que não casa vira `Severity::Unknown` e nunca é descartada; linha
   do stdout só vira diagnóstico nas ferramentas de `stdout_is_messages`.
3. Teste: um teste novo no `mod tests` do próprio `diagnostics.rs`, com a
   linha copiada da saída real da ferramenta (o `stderr` do passo, em
   `--json`). O auxiliar `one(Tool::..., "linha")` interpreta uma linha de
   stderr; `parse(Tool::..., stdout, stderr, None)` interpreta várias. Não
   precisa de bundle:

   ```sh
   cargo test -p lace-core --lib diagnostics
   ```

4. Documente o formato novo na tabela de [docs/API.md](docs/API.md),
   seção 7.

### Mudar o modelo de módulo ou de testbench

Os arquivos que `lace add` cria saem de `crates/lace-core/src/verilog.rs`:
`module_template(nome)` e `testbench_template(testbench, dut)`, com os
auxiliares `is_clock`, `reset_polarity` e `range`. Quem os chama é
`Project::add_verilog`, em `files.rs`; o módulo testado vem de
`module_under_test`.

1. O modelo precisa continuar classificado como o que é
   ([ADR 0005](docs/adr/0005-classificacao-de-arquivos-pela-regra-da-aurora.md)):
   o módulo-modelo precisa ter portas (um módulo sem portas soma 3 pontos de
   testbench), e o testbench-modelo, `$dumpfile` e `$finish`.
2. Testes sem ferramenta: `templates_are_classified_as_intended` e
   `testbench_template_instantiates_every_port` em `verilog.rs`;
   `module_template_is_a_synthesizable_module`,
   `testbench_template_without_dut` e `testbench_template_drives_every_port`
   em `crates/lace-core/tests/verilog_flow.rs`. Com Icarus e Yosys:
   `testbench_from_yosys_ports_elaborates` e `verilog_flow_from_scratch`,
   que conferem que o modelo elabora e simula.

   ```sh
   cargo test -p lace-core --lib verilog
   cargo test -p lace-core --test verilog_flow
   ```

3. Documente em [docs/CLI.md](docs/CLI.md), "Arquivo novo", e em
   [docs/API.md](docs/API.md), seção 4.2.

Mudar os pesos de `classify` não é esta receita: é mudar a ADR 0005.

### Atualizar a versão de uma ferramenta do bundle

1. Em `bundle/versions.json`, dentro de `packages`, troque a versão e o
   SHA-256 (OSS CAD Suite e Graphviz, por plataforma) ou o commit (YANC e
   surfer-aurora). O SHA-256 do OSS CAD Suite está no campo `digest` da API
   de releases do GitHub; o do Graphviz, no `.sha256` ao lado do zip.
2. Troque `bundle`, o identificador do bundle.
3. YANC: troque também o `ref:` do passo "YANC (fonte dos testes)" em
   `.github/workflows/installers.yml`, que baixa o fonte para
   `yanc_regression`, e atualize o seu `vendor/yanc`.
4. Monte e confira:

   ```sh
   python3 scripts/bundle.py --out dist/toolchain
   cargo run -p lace-cli -- --toolchain "$PWD/dist/toolchain" tools --verify
   LACE_TEST_BUNDLE="$PWD/dist/toolchain" cargo test --workspace -- --test-threads=2
   ```

   Se a saída do YANC mudou, os snapshots de `crates/lace-core/tests/build.rs`
   falham: o `insta` grava um `.snap.new`. Compare com o `.snap` em
   `crates/lace-core/tests/snapshots/` e, se a diferença for a esperada,
   troque o `.snap` pelo novo.
5. Se o pacote mudou de estrutura, o `bundle.py` falha dizendo o caminho ou
   a biblioteca que falta. Ajuste `bundle/components.json` (`select`,
   `data`, `run`) e, se o executável mudou de lugar, `Tool::location` em
   `crates/lace-core/src/toolchain.rs`.
6. Documente em [docs/BUNDLE.md](docs/BUNDLE.md), seção 1 (as tabelas de
   versões) e no `CHANGELOG.md`. O identificador antigo aparece em mais
   lugares (`lib.rs`, `docs/API.md` seção 9.1, `docs/RELEASE.md`):
   `grep -rn "<identificador antigo>" --exclude-dir=target --exclude-dir=vendor .`

O `installers.yml` monta o bundle nas três plataformas a cada push; é ali
que se confirma que a versão nova funciona no macOS e no Windows.

### Acrescentar um erro novo

Antes, decida se é mesmo um erro: `Err(LaceError)` é para quando o Lace
não consegue rodar. Se a ferramenta rodou e recusou o código do usuário, o
lugar é um diagnóstico no resultado
([ADR 0001](docs/adr/0001-biblioteca-com-interfaces-finas.md)).

1. `crates/lace-core/src/error.rs`: a variante em `LaceError`, com
   documentação na variante e em cada campo (o crate não compila sem ela),
   e a mensagem em `#[error("...")]`, em inglês, começando com maiúscula e
   sem nome de comando da CLI.
2. No mesmo arquivo, o braço em `LaceError::code()`, com um código novo em
   `snake_case`. Um código existente nunca muda.
3. Se um comando resolve o erro, a dica vai na função `hint` de
   `crates/lace-cli/src/output.rs`; no JSON ela sai em `error.hint`.
4. Testes: o código e a mensagem neutra, como em
   `new_errors_have_stable_codes_and_neutral_messages`
   (`crates/lace-core/tests/verilog_flow.rs`); a dica, como em
   `error_hints_name_the_command` (`crates/lace-cli/tests/cli.rs`).
5. Documente na tabela de [docs/API.md](docs/API.md), seção 8, e, se houver
   dica, em [docs/CLI.md](docs/CLI.md), "Erros e dicas".

`LaceError` é `#[non_exhaustive]`: acrescentar uma variante não quebra
quem usa a biblioteca.

## Regras do código

Algumas regras o compilador e o clippy cobram; as outras, a revisão.

- **O Core não escreve no console.** `lib.rs` tem
  `#![deny(clippy::print_stdout, clippy::print_stderr)]`: um `println!` ou
  `eprintln!` no `lace-core` reprova o `cargo clippy`. O log sai por
  `tracing` (`tracing::info!`, `tracing::debug!`). O Core também não chama
  `exit` e não entra em pânico em erro esperado: devolve `LaceError`.
- **Sem `unsafe`.** `lace-core` e `lace-installer` têm
  `#![forbid(unsafe_code)]`. A CLI não tem o atributo, e também não usa
  `unsafe`.
- **Todo item público do Core é documentado.** `#![deny(missing_docs)]` em
  `lib.rs`, e o CI roda o `cargo doc` com `-D warnings`, que também reprova
  link quebrado na documentação. Os exemplos da documentação rodam como
  teste; marque com `no_run` o que precisa do bundle, como os de `lib.rs`.
- **Só ferramentas do bundle.** Programa externo roda por
  `Toolchain::invocation` e `process::run`. Nada de
  `std::process::Command` fora de `process.rs`, nada do `PATH`, nenhuma
  variável herdada sem estar declarada
  ([ADR 0002](docs/adr/0002-so-ferramentas-do-bundle.md)).
- **Regra de negócio no Core.** A CLI abre, chama e mostra. As mensagens do
  Core não citam comandos; os comandos aparecem só em `hint`, na CLI.
- **O Core nunca sobrescreve código do usuário.** Criar processador ou
  arquivo recusa se o arquivo já existe; só artefatos gerados (`Hardware/`,
  `.asm`, `.lace/Temp/`) são reescritos.
- **Tipos públicos.** Os enums públicos são `#[non_exhaustive]`, e as
  structs que podem ganhar campos também (`CheckOptions` é construída com
  `CheckOptions::default()` e campo a campo). Os resultados implementam
  `Serialize` e `JsonSchema`, e esse é o JSON da CLI; campo de caminho leva
  `#[schemars(with = "String")]`.
- **Toda espera por ferramenta passa pelo `Runner`.** É ele que respeita o
  `Control`: cancelamento, prazo e eventos. Uma operação nova que roda
  ferramenta recebe `control: &Control` como último argumento e o entrega a
  `Runner::new`.
- **Caminhos** são `camino::Utf8Path` e `Utf8PathBuf` em toda a API.
- **Português e inglês.** Comentários, documentação e `CHANGELOG.md` em
  português. O que o `lace` e o instalador mostram é em inglês e começa com
  maiúscula: mensagens, erros (`Error:`), avisos (`Warning:`), dicas, texto
  de ajuda (os `///` que o `clap` transforma em ajuda), as telas do
  instalador e o `message` e o `hint` do JSON. A exceção é a gravidade dos
  diagnósticos (`arquivo:linha: error: ...`), em minúscula como no gcc, que
  é o que os editores reconhecem. Identificadores em inglês:
  funções, tipos, variáveis, nomes de teste, códigos de erro e campos do
  JSON (`classify`, `TESTBENCH_THRESHOLD`, `no_top_level`, `failed_step`,
  `classification_follows_the_aurora_scores`). As mensagens das asserções
  dos testes são em português. As mensagens do YANC chegam em inglês porque
  o Lace sempre pede `-en`.
- **Três plataformas.** O código compila para Linux, macOS e Windows; o que
  muda por sistema vai em `cfg(windows)` (veja `INHERITED_ENV` em
  `process.rs` e `YANC_PATH_LIMIT` em `paths.rs`). O CI compila e testa nos
  três.
- **Teste que precisa de ferramenta** pede o bundle com
  `common::toolchain_with(&[Tool::...])` (ou `common::toolchain()`) e
  retorna se vier `None`. Assim ele é pulado fora do CI e cobrado no CI.
- **Formatação** é a padrão do `rustfmt`; o repositório não tem
  `rustfmt.toml`.

## Cancelamento e saída ao vivo

Toda operação que executa ferramentas (`build`, `build_processors`,
`check`, `hierarchy`, `simulate`, `simulate_project`, `synthesize`,
`render_schematic`)
recebe um `Control` como último argumento. `Control::default()` roda até o
fim e não avisa nada; é o que os testes usam quando não testam isso. A
decisão e o porquê estão na
[ADR 0007](docs/adr/0007-cancelamento-e-saida-ao-vivo.md).

Uma interface usa assim (o exemplo completo está na documentação de
`control.rs`):

```rust
let cancel = CancelToken::new();
let control = Control::new()
    .with_cancel(cancel.clone())                  // o botão de parar fica com o clone
    .on_event(move |event| { let _ = tx.send(event.clone()); });
// numa thread: simulate_project(&toolchain, &project, &options, &control)
```

- **Cancelar** (`cancel.cancel()`, de qualquer thread) encerra o processo
  que roda, com tudo o que ele iniciou: no Unix o grupo de processos, no
  Windows o Job Object do passo.
  Os passos seguintes não começam. A operação volta como `Ok`, com
  `Status::Cancelled` e os passos que chegaram a rodar.
- **Prazo** é só o da simulação, `SimulationOptions::timeout`. Passou dele:
  `Status::TimedOut`, com o que o testbench escreveu até ali.
- **Eventos**: `StepStarted` (com o comando), `Output` (uma linha, com
  `stream` e `diagnostic`) e `StepFinished` (com o `Termination` e a
  duração). `diagnostic` é `true` nas linhas que o Lace reconhece como
  mensagem da ferramenta e que voltam em `diagnostics`; `false` é saída do
  programa, como o `$display` do testbench. O receptor roda na thread da
  operação: precisa ser rápido.
- **Custo**: com receptor, o `vvp` roda com `-i` (sem buffer no stdout),
  senão o `$display` só sairia no fim. Sem receptor, fica o buffer.

Na CLI, `Output::control` monta o `Control` de cada modo, e
`cancel_on_signals` liga o Ctrl+C e o SIGTERM ao `CancelToken`.

Para testar: `cancel_stops_the_process_and_what_it_started`,
`timeout_stops_the_process_and_keeps_its_output` e
`lines_arrive_while_the_process_runs` em `process.rs` rodam sem bundle;
`crates/lace-core/tests/control.rs` e os testes de `--timeout`, `--events`
e Ctrl+C em `crates/lace-cli/tests/cli.rs` precisam do Icarus.

```sh
cargo test -p lace-core --lib process
cargo test -p lace-core --test control
```

## Contrato do `--json`

O que cada comando escreve com `--json` tem um JSON Schema em
`docs/schema/<comando>.json` (`events.json` para as linhas do `--events`,
`error.json` para o código de saída 2). Os arquivos são gerados dos tipos;
não edite à mão. A decisão está na
[ADR 0008](docs/adr/0008-contrato-do-json-gerado-dos-tipos.md).

- A CLI só escreve JSON por `Output::json`, que só aceita tipos de
  `crates/lace-cli/src/report.rs` registrados na macro `reports!`.
- Os resultados do Core derivam `JsonSchema`; o comentário `///` de cada
  campo vira a descrição no schema, então ele é a documentação que o autor
  de uma extensão vai ler.
- `schema_files_match_the_types` (em `report.rs`, roda com
  `cargo test --bins`) falha quando `docs/schema/` não é o que os tipos
  geram. `crates/lace-cli/tests/schema.rs` roda os comandos e valida a
  saída contra os arquivos.

Quando uma mudança altera o JSON (um campo novo num resultado do Core, um
relatório novo), o teste falha e diz o que fazer:

```sh
LACE_UPDATE_SCHEMA=1 cargo test -p lace-cli schema
git diff docs/schema/
```

Leia o diff: ele é a mudança que quem lê o `--json` vai ver. Renomear ou
tirar um campo quebra essas pessoas e vai no `CHANGELOG.md`; acrescentar
um, em geral, não quebra.

## Antes de mandar a mudança

Rode o que o CI roda (`.github/workflows/ci.yml`):

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo test --workspace --lib --bins
cargo test --workspace --doc
```

No PowerShell, o `cargo doc` é
`$env:RUSTDOCFLAGS = "-D warnings"; cargo doc --no-deps`.

Depois confira:

- [ ] Rodou `cargo test --workspace` com `LACE_TEST_BUNDLE` definido, se a
      mudança toca qualquer fluxo de ferramenta (build, check, sim, synth,
      wave, bundle). Sem o bundle, esses testes não testaram nada.
- [ ] O comportamento novo tem teste, e o teste falha sem a mudança.
- [ ] Nenhum `.snap.new` sobrou na árvore.
- [ ] A documentação acompanha: [docs/API.md](docs/API.md) para o Core,
      [docs/CLI.md](docs/CLI.md) para a CLI, [docs/BUNDLE.md](docs/BUNDLE.md)
      para o bundle e a documentação do código para todo item público.
- [ ] Se o usuário percebe a mudança, há uma linha no `CHANGELOG.md`, na
      seção da versão não publicada.
- [ ] Se a mudança contraria uma ADR, há uma ADR nova que a substitui
      ([docs/adr/README.md](docs/adr/README.md)).
- [ ] Você leu o diff inteiro (`git diff`) antes de mandar.

Toda mudança precisa ser explicável por quem a submete: o que ela faz, por
que é assim e como foi testada. Vale também quando um assistente de IA
escreveu o código. Quem submete responde por cada linha; se não sabe
explicar uma, ela não está pronta. Instruções locais para assistentes de IA
ficam em `AGENTS*.md` na raiz, que o `.gitignore` deixa fora do
repositório.

O CI roda a cada push e em todo pull request: os comandos acima nas três
plataformas e, pelo `installers.yml`, a montagem do bundle e dos
instaladores, a instalação e todos os testes contra ela.
