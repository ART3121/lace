# `solar`: a linha de comando

A CLI é uma casca fina sobre o `solar-core` (ver [API.md](API.md)): cada
comando abre o projeto, chama uma função do Core e formata o resultado. O que
a CLI faz, uma GUI faz chamando as mesmas funções.

```
cargo install --path crates/solar-cli     # instala o binário `solar`
solar --help                              # visão geral
solar <comando> --help                    # detalhes de um comando
```

## Primeiro uso

```
solar tools                       # confere o bundle instalado
solar new demo
cd demo
solar proc add soma
# edite soma/Software/soma.cmm
solar input soma 0 5 -7 21
solar sim -p soma
solar output soma 0
solar synth -p soma --svg
solar sim -p soma --open          # abre a onda no Surfer
```

## Opções globais

Valem em qualquer comando, antes ou depois dele.

| Opção | Padrão | Efeito |
|---|---|---|
| `-C, --project <CAMINHO>` | `.` | o `.spf` ou o diretório do projeto |
| `--json` | não | um objeto JSON no stdout, nada mais |
| `-v`, `-vv` | não | `-v`: mensagens informativas, saída da simulação (`$display`), módulos da síntese e log `info` no stderr; `-vv`: log de depuração |

Bundle (seção "Bundle" do `--help`):

| Opção | Variável | Efeito |
|---|---|---|
| `--toolchain <DIR>` | `SOLAR_TOOLCHAIN` | outro bundle no lugar do instalado (desenvolvimento) |
| `--config <ARQUIVO>` | `SOLAR_CONFIG` | arquivo de configuração |

## De onde vêm as ferramentas

Do bundle instalado com o Solar (`toolchain/` ao lado de `bin/`), e só dele.
`solar tools` mostra o bundle, as versões e onde está cada ferramenta;
`solar tools --verify` confere os hashes. Como instalar: [INSTALL.md](INSTALL.md). O que o bundle traz e como montá-lo:
[BUNDLE.md](BUNDLE.md).

O arquivo de configuração guarda uma coisa só: onde está o compilador do
sistema para o Verilator, quando ele não está no local padrão. Fica em
`$XDG_CONFIG_HOME/solar/config.json` (`~/.config/solar/config.json`) no Linux
e no macOS, e em `%APPDATA%\solar\config.json` no Windows.

```json
{ "compiler_dir": "D:/msys64" }
```

## Comandos

### Projeto

| Comando | Faz | Função do Core |
|---|---|---|
| `solar new <NOME> [--dir DIR]` | cria `<DIR>/<NOME>/<NOME>.spf` | `Project::create` |
| `solar status` | processadores (linguagem, frequência, clocks, se está compilado), arquivos, topo e testbench | `Project::open`, `Processor::is_built` |

### Processadores: `solar proc`

| Comando | Faz | Função do Core |
|---|---|---|
| `proc add <NOME> [--lang cmm\|cpp] [--inputs N] [--outputs N] [--nubits N] [--nbmant N] [--nbexpo N] [--nugain N] [--ndstac N] [--sdepth N]` | cria diretórios, fonte-modelo e entrada no `.spf` | `Project::add_processor` |
| `proc set <NOME> [--freq MHZ] [--clocks N] [--show-arrays true\|false]` | grava os parâmetros de simulação no `.spf`; valem a partir do próximo build | `Project::configure_processor` |
| `proc list` | lista os processadores | `Project::processors` |

As opções `--nubits` a `--sdepth` só têm efeito em C±. Os padrões são os da
AURORA (23, 16, 6, 128, 5, 5).

### Arquivos: `solar file`

| Comando | Faz | Função do Core |
|---|---|---|
| `file add <CAMINHO> [--testbench] [--create]` | registra (com `--create`, cria vazio; recusa se existir) | `Project::add_file` |
| `file remove <CAMINHO> [--testbench]` | tira do `.spf`, não apaga do disco | `Project::remove_file` |
| `file top <CAMINHO>` | marca o módulo de topo | `Project::set_top_level` |
| `file testbench <CAMINHO>` | escolhe o testbench da simulação do projeto | `Project::set_testbench` |
| `file list` | lista sintetizáveis e testbenches | `Project::files` |

Caminhos relativos são relativos à raiz do projeto.

### Estímulos e resultados

| Comando | Faz | Função do Core |
|---|---|---|
| `solar input <PROC> <PORTA> [VALORES...]` | grava `Simulation/input_<PORTA>.txt`, um valor por linha (aceita negativos) | `Processor::write_input_values` |
| `solar input <PROC> <PORTA> --from <ARQUIVO>` | lê os valores de um arquivo (um inteiro por linha; recusa linha inválida sem gravar nada) | `read_data_file`, `Processor::write_input_values` |
| `solar output <PROC> <PORTA>` | mostra `Simulation/output_<PORTA>.txt`; com `--json`, `{"path", "values": [...]}` (linha inválida é erro) | `Processor::read_output`, `read_output_values` |

### Compilar, simular, sintetizar

| Comando | Faz | Função do Core |
|---|---|---|
| `solar build [-p NOME]... [--freq MHZ] [--clocks N] [--show-arrays]` | compila os processadores pedidos (sem `-p`, todos). `--freq`/`--clocks` valem só nesta execução | `build_processors` (`Continue`) |
| `solar sim -p NOME [opções]` | compila o processador e simula com o testbench gerado | `build_processors` (`Stop`), `simulate` |
| `solar sim [opções]` | compila todo processador que tem fonte e simula o testbench do projeto | `buildable_processors`, `build_processors` (`Stop`), `simulate_project` |
| `solar check` | `iverilog -t null` a partir do módulo de topo | `check_syntax` |
| `solar synth -p NOME [--svg] [--module M] [--no-widths]` | compila o processador, sintetiza e, com `--svg`, desenha (Yosys `show` + `dot`) | `build`, `synthesize`, `render_schematic` |
| `solar synth [--svg] [--module M] [--no-widths]` | sintetiza o módulo de topo do projeto | `synthesize`, `render_schematic` |

Opções de `sim`:

| Opção | Efeito |
|---|---|
| `--simulator icarus\|verilator` | padrão `icarus` |
| `--vcd` | onda em VCD em vez de FST (Icarus; o Verilator sempre grava VCD) |
| `--jobs N` | paralelismo da compilação do Verilator (padrão: todos os núcleos) |
| `--no-build` | não compila antes |
| `--open` | abre a onda no surfer-aurora ao terminar |
| `--freq`, `--clocks` | só para o build desta execução |

`synth -p` também aceita `--no-build`. Para ver os nomes dos módulos que
`--module` aceita, rode `solar synth -v`.

`sim` e `synth` param no primeiro build que falhar, com código 1, sem simular
nem sintetizar. `build` compila todos os pedidos mesmo que um falhe, para
mostrar todos os erros de uma vez.

### Onda

| Comando | Faz | Função do Core |
|---|---|---|
| `solar wave <ONDA> [--view ARQUIVO] [--wait]` | abre no Surfer. `--view` passa um `.sucl` (`-c`) ou `.surf.ron` (`-s`). Sem `--wait`, retorna logo | `open_waveform` |

Se o Surfer fechar em menos de 1,5 s (sem display, onda inválida), o comando
falha com código 2 (`process_exited_early`) e mostra o fim do log do Surfer
(`RunningProcess::ensure_started`).

### Bundle e configuração

| Comando | Faz |
|---|---|
| `solar tools [--verify]` | o bundle (identificador, plataforma, componentes instalados com versão e origem, e os não instalados), cada ferramenta (`ok`, `--` se o componente dela não foi instalado, `!!` se falta o executável), o compilador do sistema. Com `--verify`, confere o SHA-256 de cada executável e sai com 1 se algum não conferir |
| `solar config path` | onde fica o arquivo de configuração |
| `solar config show` | o conteúdo |
| `solar config set-compiler <DIR>` | declara o compilador do sistema para o Verilator (diretório com `perl`, `make` e `g++`/`clang++`, ou a raiz do MSYS2 no Windows); recusa se não achar os três |
| `solar config unset-compiler` | volta aos locais padrão |
| `solar completions <bash\|zsh\|fish\|elvish\|powershell>` | script de autocompletar |

Para instalar o autocompletar no bash:

```
solar completions bash > ~/.local/share/bash-completion/completions/solar
```

## Saída

### Texto

```
build conta: falhou: cmmcomp saiu com código 1 (C±, 100 MHz, 2000 clocks)
  /home/eu/projetos/com_erro/conta/Software/conta.cmm:16: erro: c'mon dude, declare the variable 'total' properly!
  -> conta/Hardware/conta.v (não gerado)
```

- Uma linha de título por operação: `ok`, `falhou`, `quebrou` (a ferramenta
  morreu por sinal ou exceção) ou `incompleto` (faltou artefato).
- Um diagnóstico por linha, no formato `arquivo:linha[:coluna]: gravidade:
  mensagem`, que editores e terminais transformam em link. Mensagens `info`
  só aparecem com `-v`.
- `->` lista os artefatos obrigatórios; em falha, só os que não foram gerados.
- Cores só num terminal, e nunca com `NO_COLOR` definido.

### JSON

Com `--json`, o stdout tem exatamente um objeto JSON e nada mais. Os
resultados de operação são os tipos do Core serializados (formato em
[API.md, seção 6](API.md#6-resultados)).

| Comando | Objeto |
|---|---|
| `build` | `{"project": <spf>, "results": [BuildResult...]}` |
| `sim` | `{"builds": [BuildResult...], "simulation": SimulationResult \| null, "surfer_pid": N \| null}` |
| `check` | `CheckResult` |
| `synth` | `{"builds": [...], "synthesis": SynthesisResult \| null, "schematic": SchematicResult \| null}` |
| `status` | `{"name", "spf", "root", "processors": [...], "synthesizable", "testbench", "top_level", "selected_testbench"}` |
| `proc list` | `[Processor...]`, cada um com `"built": bool` |
| `proc set` | o `Processor` atualizado |
| `file list` | `{"synthesizable", "testbench", "top_level", "selected_testbench"}` |
| `output` | `{"path", "values": [...]}` |
| `tools` | `{"root", "bundle", "platform", "components", "not_installed", "tools": {"<nome>": {"path", "system"} \| {"error": {"code", "message"}}}, "system_compiler", "verify"}` |
| `wave` | `{"pid", "log"}` |
| `config show` | `{"path", "config"}` |
| `new`, `proc add`, `file add/remove/top/testbench`, `input`, `config set-compiler/unset-compiler` | `{"message", "path"}` |
| qualquer erro | `{"error": {"code", "message"}}` |

`error.code` é o `SolarError::code()` do Core (tabela em
[API.md, seção 8](API.md#8-erros)), ou `"cli"` para erros da própria linha de
comando (arquivo de configuração inválido).

A exceção são os erros de argumento (opção desconhecida, valor inválido): o
`clap` os detecta antes de o comando começar e escreve a mensagem no stderr,
em texto, com código 2.

### Códigos de saída

| Código | Significado |
|---|---|
| 0 | deu certo |
| 1 | a operação rodou e falhou: erro de compilação, Verilog que não elabora, simulação que falhou |
| 2 | o Solar não conseguiu rodar: bundle, projeto, arquivo, opção inválida |

### Log

O log do Core sai no stderr: avisos por padrão, `info` com `-v`, `debug` com
`-vv`. `RUST_LOG` tem precedência (por exemplo `RUST_LOG=solar_core=debug`).
