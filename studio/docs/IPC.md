# Referência do IPC

Os comandos que a interface chama no backend com `invoke`, um por linha das
tabelas abaixo, com o que recebem e o que devolvem. Na interface, cada um
tem uma função em `src/ipc/api.ts`, menos `log_frontend` e `dev_smoke_script`, chamados com `invoke` direto; os tipos estão em `src/ipc/types.ts`
(os do Studio) e `src/ipc/lace-types.ts` (os do Core, gerados).

Convenções:

- Os argumentos de primeiro nível vão em `camelCase` no JavaScript e chegam
  em `snake_case` no Rust (`expectedModifiedMs` vira `expected_modified_ms`).
  Os campos de um objeto passado inteiro (`request`, `settings`) vão em
  `snake_case`, como estão nos tipos.
- Caminhos são absolutos, do sistema (com `\` no Windows).
- Todo comando pode falhar com `IpcError { code, message }`. Os códigos do
  Core são os de `LaceError::code()` (API.md do Lace, seção 8); os do
  Studio estão em `src-tauri/src/error.rs` e na última seção daqui.
- Os comandos que tocam o disco ou rodam ferramenta rodam numa thread de bloqueio (`state::blocking`), fora da
  thread da interface; nenhum comando trava a janela.

## Aplicativo (`commands/app.rs`)

| Comando | Argumentos | Devolve | Equivale a |
|---|---|---|---|
| `app_info` | | `AppInfo`: nome, versão, sistema, plataforma do Lace, pasta das preferências, versões do Tauri e do motor de páginas, se é build de desenvolvimento | |
| `settings_get` | | `Settings` | |
| `settings_set` | `settings: Settings` | `Settings` como ficou (a lista de recentes enviada é ignorada) | |
| `recent_projects` | | `RecentStatus[]`: `spf`, `name`, `opened_at_ms`, `exists` | |
| `recent_forget` | `spf` | | |
| `wave_open` | `processor?`, `path?` | `WaveOpened`: `waveform`, `pid`, `log`, `outdated` (os processadores compilados de novo depois da simulação: PC e linha em números) | `lace wave [-p NOME] [ONDA]` |
| `log_frontend` | `level` (`error`, `warn`, `info`), `message` | | |
| `dev_smoke_script` | | o roteiro de `LACE_STUDIO_SMOKE`, só na build de desenvolvimento; senão `null` | |
| `app_exit` | `restart: boolean` | (não volta: o Studio sai) | |
| `splash_close` | | (fecha o splash, a janela com o logo que fica na frente enquanto a principal abre escondida; a interface chama logo depois de mostrar a principal) | |

`log_frontend` escreve no log do backend, com o alvo `lace_studio::ui`, o
que a interface manda: erros de JavaScript, `console.error` e todo erro
mostrado num aviso. `dev_smoke_script` alimenta o roteiro de fumaça
(DEVELOPMENT.md, seção 3).

`app_exit` sai do Studio pelo caminho de sempre (a operação rodando é
cancelada e os terminais fecham) e, com `restart`, abre de novo o
executável com que ele começou, guardado ao abrir: depois do `lace update`,
é o da versão nova. A interface grava os arquivos e a sessão antes de
chamar. Sem `restart`, só fecha: é o que o Studio faz no Windows para o
assistente de instalação trocar os arquivos dele.

`wave_open` sem `path` abre a onda da simulação do processador ou, sem
processador, a do projeto (`waveform_path` do Core). Se ela ainda não
existe: erro `no_waveform`. Espera 1,5 s para conferir que o surfer-aurora
não fechou logo (`process_exited_early`). Depois, uma thread espera o
Surfer fechar, para o processo não ficar zumbi.

`wave_open` abre em janela e aplica o layout dos processadores SAPHO
(`prepare_wave_layout` do Core), qualquer que seja `Settings.wave_viewer`.

`Settings.wave_viewer` é `tab` (o padrão) ou `window`: onde a interface
abre a onda. Com `tab` e o cliente web no bundle, a simulação com
`open_wave` não abre a janela: devolve `wave_tab` e a interface abre a aba
(seção "Onda numa aba").

`Settings.terminal_shell` é `powershell` (o padrão) ou `cmd`: o shell do
terminal no Windows. Nos outros sistemas o terminal abre `$SHELL` e o campo
não vale.

`Settings.editor` tem `font_size`, `tab_size`, `word_wrap`, `minimap`,
`auto_save` e `vim_mode`. Campo que falta no arquivo fica com o padrão.

`Settings.zen` tem `fullscreen` (padrão `true`), `center_layout` (`true`),
`show_tabs` (`false`) e `hide_line_numbers` (`false`): as opções do modo
zen. Entrar e sair do zen não passa pelo backend; a tela cheia vem do
`setFullscreen` da janela, pela permissão `core:window:allow-set-fullscreen`.

`Settings.layouts` tem `active` (o id do layout em uso; padrão `default`, o
Padrão) e `saved` (os layouts gravados pelo usuário). Cada item de `saved`
é guardado como a interface o escreveu (`src/state/layoutModel.ts`, com a
versão do formato em `v`) e o backend não o lê: um layout malformado ou de
uma versão mais nova não leva o `settings.json` para o `.bad`, e gravar
outra preferência o devolve como veio. Ao contrário dos recentes, os
layouts enviados em `settings_set` são gravados. A janela de agora (o que o
usuário mexeu depois de aplicar um layout) não passa pelo backend: fica no
`localStorage` da WebView.

## Projeto (`commands/project.rs`)

| Comando | Argumentos | Devolve | Equivale a |
|---|---|---|---|
| `project_default_parent` | | a pasta padrão para criar e abrir projetos (`Projetos` na pasta do usuário), criada se não existe | |
| `project_open` | `path`: o `.spf` ou a pasta | `ProjectSnapshot` | |
| `project_create` | `parent`, `name` | `ProjectSnapshot`; `invalid_name` se o nome não passar na regra do Core (letras sem acento, dígitos, `_` e `-`, começando por letra) | `lace new` |
| `project_check_name` | `name` | `null` se o nome de projeto serve; senão o `IpcError` (`invalid_name`, com o motivo do Core) | `validate_project_name` |
| `project_reorder` | `path`, `position` (`{ kind: 'first' \| 'last' }` ou `{ kind: 'before' \| 'after', path }`) | `ProjectFile[]`: a lista do arquivo, na ordem nova | `lace order` (`Project::reorder_file`) |
| `processor_check_name` | `name` | `null` se o nome de processador serve; senão o `IpcError` (`invalid_name`, com o motivo: palavra do Verilog ou do SystemVerilog, módulo da biblioteca SAPHO, nome reservado do Windows, nome longo, nome de uma pasta do Lace como `synth` ou `wave`) | `validate_new_processor_name` |
| `project_close` | | | |
| `project_snapshot` | | `ProjectSnapshot` | `lace status --json` |
| `project_add_verilog` | `path`, `testbench` | `AddedFile` do Core | `lace add [--tb]` |
| `project_remove_verilog` | `path` | `bool`: estava registrado | `lace remove` |
| `project_set_top` | `target`: arquivo ou nome de módulo | o arquivo de topo | `lace top` |
| `project_set_testbench` | `path` | | `lace sim <TESTBENCH>`, sem simular |
| `project_processor_defaults` | | `NewProcessor` com os padrões da AURORA | |
| `project_add_processor` | `request: NewProcessorRequest` | `Processor` do Core | `lace proc add` |
| `project_configure_processor` | `name`, `frequencyMhz?`, `clocks?`, `showArrays?` | `Processor` | `lace proc set` |
| `project_remove_processor` | `name`, `toTrash` | `{ removed, trashed }`: o que saiu (`name`, `dir` e os arquivos tirados das listas em `unregistered`) e se a pasta foi para a lixeira | `lace proc remove [--delete]` |
| `project_output_values` | `processor`, `port` | `number[]` | |
| `verilog_modules` | `text` | os módulos declarados | `verilog::modules_in` |
| `project_move` | `from`, `to` (o caminho final), `overwrite` | `MovedPath` do Core: `from`, `to` e os arquivos do projeto que foram junto | `lace move` |
| `project_convert_layout` | | `LayoutConversion` do Core: os processadores convertidos, o que mudou de lugar, o que ficou e por quê, os fontes que perderam o `#PRNAME`, os nomes que não servem e as subpastas que ficaram | `lace convert` |
| `project_hierarchy` | | `HierarchyResult` do Core | `lace hierarchy --json` |

Abrir e criar tornam o projeto o aberto: guardam o `.spf`, põem nos
recentes e começam a vigiar a pasta.

`project_move` é o `Project::move_path` do Core: move ou renomeia (o
arrastar e o F2 da árvore), com os Verilog registrados continuando no
`.spf` e o topo e o testbench escolhido acompanhando. Recusa o `.spf`,
`.lace/`, as pastas e o fonte dos processadores e uma pasta para dentro
dela mesma (`cannot_move`), caminho fora do projeto (`outside_project`) e
destino que existe (`path_exists`). Com `overwrite`, o destino que existe
vai para a lixeira e o movimento é tentado de novo.

`project_hierarchy` é o `lace_core::hierarchy`, com as opções padrão: o
design, cada testbench registrado e o testbench de cada processador
compilado, elaborados pelo Icarus. Os tipos (`HierarchyResult`,
`Elaboration`, `ModuleInstance`) estão em `src/ipc/lace-types.ts`, gerados
do `hierarchy.json` do Lace; a descrição de cada campo, na API.md do Lace,
seção 5.4.1. Não compila os processadores nem grava relatório.

`ProjectSnapshot` é o `ProjectStatus` do Core (`Project::status`), de onde a CLI monta o `StatusReport` do `lace status --json`:

| Campo | Conteúdo |
|---|---|
| `name`, `spf`, `root` | o projeto |
| `synthesizable`, `testbenches` | `ProjectFile[]`: `role`, `path`, `top_level` |
| `top_level`, `top_module`, `top_module_error` | o arquivo e o módulo de topo; o erro quando o módulo não dá para saber |
| `selected_testbench`, `testbench_module` | o testbench simulado e o módulo dele |
| `wave_selection` | a escolha de sinais da onda desse testbench (`wave/<testbench>.json`); vazia, a onda grava todos. Os botões Sinais mostram quantos itens ela tem |
| `unregistered` | os `.v` e `.sv` da pasta fora do `.spf`, e os `.py` com `@cocotb.test` |
| `processors` | `ProcessorStatus[]`: os campos do `Processor` do Core, mais `built`, `inputs` e `outputs` (`{ port, path }`), `missing_inputs`, `waveform` e `generated` (o que o build gerou e existe: `verilog`, `testbench`, `assembly`, `memories[]`, `intermediates[]`, os `.txt` e `.log` da pasta de saída `<proc>/<proc>/`, sem as saídas da simulação) |
| `waveform` | a onda da simulação do projeto, se já existe |
| `top_candidates` | os Verilog que podem ser o topo pela regra do Core (qualquer um, menos nome de testbench e testbench cocotb `.py`): registrados, o gerado de cada processador e os de fora do `.spf` |
| `issues` | `ProjectIssue[]` do Core (`Project::issues`): `kind` (`rescued_path`, `selection_not_registered`, `testbench_as_top`, `invalid_processor_name`, `duplicate_file`, `nested_project`, `legacy_layout`), `path`, `detail`, `message`; o painel Problemas mostra como aviso, traduzido pelo `kind`. Com `legacy_layout`, o Studio oferece a conversão ao abrir o projeto |

`NewProcessorRequest`: `name`, `language` (`cmm` ou `cpp`) e, opcionais,
`input_ports`, `output_ports`, `nubits`, `nbmant`, `nbexpo`, `nugain`,
`ndstac`, `sdepth`. Ausente fica com o padrão do Core; `ndstac` e `sdepth`
ausentes ficam fora do fonte, e o YANC calcula as pilhas pelo programa. O
Core recusa nome e parâmetro que o YANC não compila (`invalid_name`,
`invalid_parameter`).

## Arquivos (`commands/files.rs`)

| Comando | Argumentos | Devolve |
|---|---|---|
| `fs_read_dir` | `path` | `DirEntry[]`: `name`, `path`, `is_dir`, `hidden`, `testbench_name` (nome `tb_<nome>.v` ou `<nome>_tb.v`, que não pode ser topo); pastas primeiro |
| `fs_read_text` | `path` | `TextFile`: `content`, `modified_ms`, `binary`, `too_large` (mais de 16 MiB) |
| `fs_write_text` | `path`, `content`, `expectedModifiedMs?` | o novo `modified_ms` |
| `fs_stat` | `path` | `FileStat` (`is_dir`, `modified_ms`, `size`) ou `null` |
| `fs_create_file` | `path`, `content?` | |
| `fs_create_dir` | `path` | |
| `fs_rename` | `from`, `to` | |
| `fs_copy` | `from` (de qualquer lugar), `toDir`, `overwrite` | o caminho novo, dentro de `toDir` |
| `fs_trash` | `path` | |
| `fs_list_files` | | todos os arquivos do projeto (até 20 000), sem pastas ocultas, `node_modules`, `target`, `obj_dir` e `__pycache__` |
| `fs_search` | `query`, `regex`, `caseSensitive`, `maxResults?` (padrão 2000) | `SearchMatch[]`: `path`, `line`, `column`, `length`, `text` |

- `fs_write_text` com `expectedModifiedMs` recusa com `conflict` se o
  arquivo mudou desde a leitura. Sem ele, grava por cima.
- Criar, renomear, copiar e mandar para a lixeira só dentro da pasta do
  projeto (`outside_project`); destino que já existe é `exists`. `fs_copy`
  com `overwrite` manda o que estava no destino para a lixeira antes;
  soltar um arquivo na pasta onde ele já está não faz nada.
- A busca só olha arquivos de texto de até 2 MiB.

## Operações (`jobs.rs`)

| Comando | Argumentos | Devolve |
|---|---|---|
| `flow_start` | `request: FlowRequest`, `channel: Channel<JobMessage>` | o identificador da operação |
| `flow_cancel` | `job?` | `bool`: havia operação para cancelar |
| `flow_running` | | o identificador da operação rodando, ou `null` |

Uma operação por vez: começar outra com uma rodando é `busy`. Os fluxos
`fpga_build` e `fpga_program` são da placa FPGA, experimental: com ela
desligada, `flow_start` recusa com `experimental_disabled`, sem começar a
operação.

### `FlowRequest`

Com o tipo no campo `flow`:

| `flow` | Campos | Equivale a |
|---|---|---|
| `build` | `processors: string[]` (vazio: todos) | `lace build [-p NOME]...` |
| `check` | `file?`, `processor?`, `lint?` | `lace check [ARQUIVO] [-p NOME] [--lint]` |
| `hierarchy` | `processor?` | `lace hierarchy [-p NOME]`: a hierarquia elaborada pelo Icarus, sem compilar nem gravar relatório; a interface usa `project_hierarchy` |
| `simulate` | `processor?`, `testbench?`, `simulator` (`icarus`, `verilator`), `fast?` (a simulação rápida: sem onda, com o testbench Verilog no Verilator; `simulator` só vale para um testbench cocotb), `timeout_s?`, `open_wave?` (ignorado com `fast`) | `lace sim [TESTBENCH] [-p NOME] [--verilator] [--fast] [--timeout S] [--open]` |
| `synthesize` | `processor?`, `schematic?`, `module?`, `open_viewer?` (grava o HTML do RTL viewer ao lado do netlist e o abre no navegador; a interface não pede) | `lace synth [-p NOME] [--svg] [--module M] [--open]` |
| `schematic` | `netlist`, `module`, `bus_widths?` (padrão `true`) | desenha outro módulo de um netlist que já existe; sem relatório |
| `learn` | `root` (a pasta de exercícios, ou uma pasta dentro dela), `exercise` | `lace learn check NOME`: corrige o exercício e grava se está resolvido; sem relatório |
| `fpga_build` | nenhum | `lace fpga build`: confere o `fpga.json`, a placa e o Quartus (`no_fpga_config`, `board_not_found`, `quartus_missing` antes de compilar), compila os processadores e depois o projeto para a placa; sem relatório, como na CLI |
| `fpga_program` | `cable?` | `lace fpga program [--cable NOME]`: grava o `.sof` da última compilação (`no_bitstream`, `no_cable` antes de gravar); sem relatório |

A composição é a do Core (`lace_core::flows::run`), a mesma da CLI;
`flows.rs` só traduz o pedido e acrescenta a onda numa aba e a correção do
exercício. `simulate`, `synthesize` e `fpga_build` compilam antes os
processadores que têm fonte e param no primeiro build que falhar; `check`
não compila, verifica o que está no disco; `build` compila todos mesmo que
um falhe; só `build`, `check`, `simulate` e `synthesize` gravam um
relatório no histórico.

### `JobMessage`

As mensagens chegam pelo `channel`, nesta ordem, com o tipo no campo `type`:

| `type` | Campos | Quando |
|---|---|---|
| `started` | `job`, `command` | logo depois de `flow_start` |
| `phase` | `phase`: `build`, `check`, `hierarchy`, `simulate`, `synthesize`, `schematic`, `rtl_viewer`, `wave`, `learn`, `fpga`, `program` | uma fase começou |
| `events` | `events: Event[]` | lotes de eventos do Core (`step_started`, `output`, `step_finished`; API.md do Lace, 5.8), no máximo a cada 30 ms ou 500 eventos |
| `output_omitted` | `step`, `tool`, `lines` | linhas de `output` que ficaram de fora, no lugar delas: passam 20 000 de uma vez e depois 5 000 por segundo, em blocos, com um aviso a cada meio segundo (uma linha longa conta uma a mais a cada 1 KiB); o resultado final ainda traz o começo e o fim da saída de cada passo |
| `build` | `result: BuildResult` | um processador terminou de compilar |
| `install` | `event` | o progresso de `lace_install` e `lace_update`: um evento do instalador (`lace_installer::install::Event`, etiquetado por `event`: `start`, `chunk`, `downloading`, `progress`, `replacing`, `verifying`, `fallback`, `fallback_download`, `wizard_selection`, `installer_output`, `warning`) |
| `cli_output` | `stream`, `line` | só em `lace_install` e `lace_update`: a linha que a CLI escreveria para o evento, para o console |
| `finished` | `outcome` | fim: um `FlowOutcome` (ou o resultado de `lace_install` e `lace_update`) |
| `failed` | `error: IpcError` | fim: o Lace não conseguiu rodar |

Toda operação termina com exatamente um `finished` ou um `failed`.

### `FlowOutcome`

| Campo | Conteúdo |
|---|---|
| `flow`, `command` | o fluxo e o comando equivalente (`lace-studio sim -p soma`) |
| `status` | como terminou: o pior entre as fases que rodaram (`succeeded`, `failed`, `crashed`, `incomplete`, `timed_out`, `cancelled`) |
| `succeeded` | tudo deu certo (`status` é `succeeded`) |
| `builds` | `BuildResult[]` dos builds feitos antes (ou do próprio build) |
| `registered` | `AddedFile[]`: o `.v` e o testbench que esses builds geraram e entraram no `.spf` agora |
| `lint_missing` | com `lint`, o componente do Verilator que falta quando nenhum passo dele rodou |
| `hierarchy`, `not_built` | a hierarquia do fluxo `hierarchy` e os processadores com fonte e sem build que ficaram de fora |
| `check`, `simulation`, `synthesis`, `schematic` | o resultado de cada fase que rodou, ou `null`; com um testbench cocotb, `simulation.tests` traz os testes (`TestReport` do Core) |
| `outputs` | `PortValues[]` (`port`, `path`, `values`, `error`), na simulação de um processador que deu certo |
| `wave`, `wave_error` | a onda aberta em janela, ou por que não abriu |
| `wave_tab` | a onda que a interface abre numa aba (`wave_viewer: tab`), no lugar da janela |
| `schematic_error` | por que o esquemático não saiu depois da síntese (`module_not_found`: o módulo pedido não está no netlist); a síntese vale |
| `rtl_viewer`, `rtl_viewer_error` | o HTML do RTL viewer, com `open_viewer`, ou por que não saiu; a síntese vale |
| `report`, `report_error` | o relatório gravado (`run-000042`), ou por que não foi |
| `learn` | a correção de um exercício (fluxo `learn`): o `Grade` do lace-learn |
| `fpga` | a compilação para a placa (fluxo `fpga_build`): o `FpgaBuildResult` do Core, com os passos do Quartus, os recursos e o tempo |
| `fpga_program` | a gravação na placa (fluxo `fpga_program`): o `FpgaProgramResult` do Core |

O `stdout` e o `stderr` de cada passo vêm cortados em 256 KiB, guardando o
fim; o que foi cortado já chegou linha a linha pelos eventos.

## Recursos experimentais (`commands/experimental.rs`)

| Comando | Argumentos | Devolve | Equivale a |
|---|---|---|---|
| `experimental_list` | | `{ path, components }`: o `experimental.json` (`null` sem pasta de configuração) e cada recurso (`feature`, `enabled`, `description`), lidos do disco a cada chamada | `lace experimental --json` |
| `experimental_set` | `feature` (`"fpga"`, `"assistant"`), `enabled` | o mesmo, já com a mudança | `lace experimental enable\|disable <recurso>` |

O arquivo é o do Core, na pasta de configuração do Lace, e não o
`settings.json` do Studio: ligar aqui vale também para a CLI, e o contrário.

## Placa FPGA (`commands/fpga.rs`)

A placa FPGA é um recurso experimental: desligada, todo comando daqui
responde `experimental_disabled`.

| Comando | Argumentos | Devolve | Equivale a |
|---|---|---|---|
| `fpga_boards` | | `Board[]`: as placas, com os sinais e os pinos | `lace fpga boards --json` |
| `fpga_config` | | o `fpga.json` (`{ board, top?, connect }`, com as ligações na ordem do arquivo), ou `null` se não existe | ler o `fpga.json` |
| `fpga_config_set` | `config` | | gravar o `fpga.json` |
| `fpga_top` | `top?` | `ModuleInterface`: o módulo que vai para a placa e as portas dele | `fpga::top_interface` |
| `fpga_check` | | `Prepared`: `config`, `resolved` (as ligações bit a bit e as notas), `board_top` e `connections` (cada ligação como texto) | `lace fpga check --json` |
| `fpga_modules` | | os módulos do projeto que podem ir para a placa | `fpga::modules` |
| `fpga_status` | | `BitstreamState`: `bitstream` (o `.sof`, ou `null`), `built_at_ms` e `reasons` (por que não serve para gravar; vazio: pronto) | a conferência do `lace fpga program`, sem gravar |
| `fpga_cables` | | os cabos ligados (cerca de 1 s) | `lace fpga program --list` |

A compilação é o fluxo `fpga_build` de `flow_start`; o resultado vem em
`FlowOutcome.fpga`.

## Exercícios (`commands/learn.rs`)

| Comando | Argumentos | Devolve | Equivale a |
|---|---|---|---|
| `learn_tracks` | | `LearnTracks`: `dir` (a pasta das trilhas) e `tracks` (as trilhas dela) | de onde vêm as trilhas, antes de criar ou abrir a pasta |
| `learn_open` | `root` (a pasta de exercícios, ou uma pasta dentro dela), `lang?` | `LearnSnapshot` | abrir a pasta e pô-la em dia com a trilha, como o `lace learn` ao começar |
| `learn_init` | `dir`, `track?` (padrão `verilog`), `lang?` | `LearnSnapshot` | `lace learn init` |
| `learn_set_current` | `root`, `name`, `lang?` | `LearnSnapshot` | escolher o exercício atual |
| `learn_reset` | `root`, `name` | | `lace learn reset NOME --yes` |

O `LearnSnapshot` traz a pasta, a trilha (`track`, `title`, `welcome`,
`farewell`), os capítulos com os exercícios (`name`, `title`, `prompt`,
`hints`, `module`, `file`, `spf`, `waveform`, `solved`, `solution`), o
atual e quantos estão resolvidos. Com `lang` (`en`), os textos que têm
tradução vêm traduzidos. As trilhas vêm da preferência `learn_dir`, de
`LACE_LEARN_DIR` ou do componente `lace-learn`, nessa ordem; sem nenhum, o
erro é `learn_component_missing`. A correção é o fluxo `learn` de
`flow_start`, que traz o resultado em `FlowOutcome.learn`.

## Histórico (`commands/history.rs`)

| Comando | Argumentos | Devolve | Equivale a |
|---|---|---|---|
| `history_list` | | `RunSummary[]`, do mais novo para o mais antigo | `lace report list` |
| `history_show` | `id?` (sem: o mais novo) | `{ id, path, record, text }` | `lace report show` |
| `history_compare` | `id?`, `against?` | `RunComparison` | `lace report compare` |
| `history_plan_cleanup` | `cleanup`: `{ kind: 'all' }`, `{ kind: 'keep_latest', keep }` ou `{ kind: 'reports', ids }` | os relatórios que sairiam, do mais antigo para o mais novo, sem apagar | `history::plan_cleanup` |
| `history_clean` | `ids` (os do plano) | `{ removed, kept }` | `lace report clean` |

A limpeza segue o Core: a interface pede o plano, mostra a lista, confirma
e manda apagar exatamente aquela lista. Um relatório gravado entre as duas
chamadas não sai sem ter sido mostrado. O número de um relatório apagado
não volta.

## Bundle (`commands/toolchain.rs`)

| Comando | Argumentos | Devolve | Equivale a |
|---|---|---|---|
| `toolchain_info` | | `ToolchainInfo` (nunca falha: sem bundle, `found: false` e o erro) | `lace tools --json` |
| `toolchain_verify` | | `FileMismatch[]` (vazio: tudo confere) | `lace tools --verify` |
| `lace_update_check` | | o `UpdateReport` (`docs/schema/update.json` do Lace), com `prefix` quando o bundle é o de uma instalação do instalador | `lace update --check --json` |
| `lace_install` | `components: string[]`, `channel` | o identificador da operação | `lace install <componentes> --json` |
| `lace_update` | `channel` | o identificador da operação | `lace update --yes --json` |

`ToolchainInfo` (o `lace_core::locate::ToolchainInfo`): `found`, `error`, `origin` (`settings`, `environment`, `bundled`,
`installation`, `path`, `beside`), `root`, `bundle`, `platform`,
`components`, `not_installed`, `tools` (`name`, `component`, `path`,
`system`, `error`), `system_compiler`, `compiler_error`, `quartus`, `quartus_error`, `lace_cli`,
`surfer_web` (o cliente web do Surfer, `surfer-aurora/web/`, ou `null`).

`lace_update_check`, `lace_install` e `lace_update` são a biblioteca do
instalador (`lace_installer`, o crate do Lace), chamada em processo: a
mesma regra do `lace update` e do `lace install` da CLI e do `lace serve`,
sem rodar o `lace` como filho. Precisam de rede. `lace_install` e
`lace_update` são operações como as do Core: ocupam a vaga e mandam
`install` (cada evento do instalador), `cli_output` (a linha que a CLI
escreveria para ele, para o console) e `finished` (`{ flow: 'install' |
'update', succeeded, cancelled, exit_code, result }`). `result` é o
relatório (`InstallReport`, `UpdateReport`), ou `{ error: { code, message,
hint } }` quando a operação não rodou, com `exit_code` o que a CLI daria
(0, 2 ou 130). O `code` é o da biblioteca (`network`, `no_installation`,
`install_in_progress`, `checksum_mismatch`, `installer_failed`, ...).

A pasta da instalação é a de cima do bundle (`toolchain/..`), conferida
pela biblioteca (o Studio não está em `bin/`); com um bundle de
desenvolvimento (`LACE_TOOLCHAIN`), `lace_update_check` responde com
`prefix: null` e `lace_update` falha com `no_installation`.

`lace_update` não pergunta: quem pergunta é a interface, antes de chamar,
com a versão instalada, a nova e a pasta (o `UpdateReport` de
`lace_update_check`). No Windows pede ao assistente de instalação, quando a
atualização cai nele, para abrir o Studio de novo no fim. A interface
chama `lace_update_check` sozinha ao abrir e a cada 12 horas
(`check_updates` nas preferências), e depois de `updated` reinicia o
Studio com `app_exit`. No `result`, `action` diz o que aconteceu:
`updated` (a atualização por componentes trocou os arquivos que mudaram,
`method: "components"`, com o que mudou em `changes`; ou, no Linux e no
macOS, o instalador da release trocou o `lace` e o bundle, `method:
"installer"`), `wizard_opened` (Windows, quando a atualização por
componentes não dá: o assistente abriu e termina depois que o Studio sai)
ou `up_to_date`. Cancelar (`flow_cancel`) vale até a troca dos arquivos:
um download parado é `cancelled`, e nada muda; durante a troca o pedido é
ignorado, e uma troca interrompida por fora é desfeita na próxima
operação. A interface ainda não oferece o botão de cancelar o update.

## Onda numa aba (`wave_tab.rs`)

| Comando | Argumentos | Devolve |
|---|---|---|
| `wave_tab_open` | `path` | `WaveTab`: `id`, `url` (a página do cliente web para o iframe), `processors` (`WaveProcessor[]` do layout, com `outdated` quando o processador foi compilado de novo depois da simulação), `saved_layout` (o `wave/<testbench>.surf.ron`, ou `null` fora de projeto), `customized` (o usuário salvou o layout) e `selection` (a escolha de sinais que o layout segue) |
| `wave_tab_close` | `id` | |

`wave_tab_open` registra a onda num servidor HTTP do Studio, só em
`127.0.0.1`, que sobe na primeira aba e serve, numa origem: o cliente web
(`/web/`), a onda (`/wave/<id>/<nome>`), o `.surf.ron` (`/layout/<id>`: o
layout salvo no projeto, ou o gerado), os tradutores e os comandos de
partida (`/doc/<id>/<nome>`). A `url` abre o cliente com `load_url` na onda
e `startup_commands` que carregam os tradutores e o estado e, numa onda de
testbench do projeto, registram `state_save_url_set <origem>/save/<id>`: o
Ctrl+S da aba (que fica com o Surfer) manda o estado num `POST`, e o
servidor o grava no `wave/<testbench>.surf.ron`, num arquivo ao lado que
depois toma o lugar dele. Erros: `no_waveform`;
`surfer_web_missing`, bundle sem cliente web; `wave_too_large`, onda acima
de 256 MB. A interface oferece a janela nos dois últimos.

`wave_tab_close` faz o servidor esquecer a aba. A vista chama ao fechar a
aba ou ao recarregar.

## Sinais da onda (`commands/wave.rs`)

| Comando | Argumentos | Devolve | Equivale a |
|---|---|---|---|
| `wave_signals` | | `WaveSignals`: `testbench`, `module`, `root` (a árvore de `SignalScope`, ou `null` se a elaboração falhou), `selection`, `selection_file`, `unknown`, `status`, `steps`, `diagnostics` | `lace wave signals --json` |
| `wave_selection_set` | `signals` | a escolha como ficou; vazia apaga o `wave/<testbench>.json` | `lace wave select`, `unselect` |
| `wave_layout_reset` | `waveform` | `true` se havia layout salvo | `lace wave --reset-layout` |

`wave_signals` elabora o testbench com o Icarus, como a hierarquia: leva
menos de um segundo num projeto pequeno. A interface monta a escolha na
árvore (marcar um escopo vale por tudo dentro dele) e manda a lista
inteira a `wave_selection_set`. O retrato do projeto traz a escolha em
`wave_selection` sem elaborar nada: é o que os botões mostram.

## Terminal (`terminal.rs`)

| Comando | Argumentos | Devolve |
|---|---|---|
| `terminal_spawn` | `cwd?` (padrão: a pasta do projeto), `cols`, `rows`, `channel: Channel<TerminalMessage>` | o identificador |
| `terminal_write` | `id`, `data` | |
| `terminal_ack` | `id`, `units` | a interface escreveu no xterm `units` (o `length` das strings) do que o terminal mandou |
| `terminal_cd` | `id`, `path` | digita no shell o `cd` para `path`, na sintaxe dele (PowerShell, `cmd`, POSIX ou fish), depois das teclas que apagam o que estiver na linha; a interface chama ao abrir outro projeto |
| `terminal_resize` | `id`, `cols`, `rows` | |
| `terminal_kill` | `id` | |

`TerminalMessage`: `{ type: 'data', data }` com o que o shell escreveu (UTF-8),
e `{ type: 'exit', code }` quando ele termina. A saída tem controle de fluxo:
a interface confirma com `terminal_ack`, em lotes menores que 16 Ki unidades
UTF-16, o que o xterm já escreveu, e o backend para de ler o shell quando a
interface fica 128 Ki atrás, até o atraso cair para 16 Ki; o programa que
escreve no terminal espera. O shell é o do usuário
(`$SHELL`) no Linux e no macOS e, no Windows, o PowerShell ou o `cmd.exe`, pela preferência `terminal_shell`, com a pasta `bin/`
da instalação do Lace no começo do `PATH` e `LACE_STUDIO=1`.

## Assistente (`assistant.rs`)

O assistente é o recurso experimental `assistant`: desligado, a vista, as
ações e a página de Preferências dele somem da janela, e `assistant_start`
responde `experimental_disabled`.

| Comando | Argumentos | Devolve |
|---|---|---|
| `assistant_info` | | `{ claude, version, lace, project }`: o Claude Code achado (ou `null`), a versão dele, o `lace` que vai servir as ferramentas e o `.spf` aberto |
| `assistant_start` | `resume?` (um `session_id` para retomar), `channel: Channel<AgentEvent>` | o identificador da sessão |
| `assistant_send` | `id`, `text` | |
| `assistant_interrupt` | `id` | interrompe o turno em andamento |
| `assistant_stop` | `id` | encerra a sessão (espera o Claude Code sair por até 3 s, depois mata) |

A sessão é um processo filho do Claude Code (`crates/lace-agent`), aberto
em modo `--print` com entrada e saída em JSON por linha, o `lace mcp` como
servidor de ferramentas (o `lace` da instalação do bundle, ou a
preferência `assistant_lace`), o `.spf` aberto como projeto padrão e a
pasta do projeto como pasta de trabalho. O que ele pode fazer vem da
preferência `assistant_allow_edits`: só as ferramentas do Lace e ler
arquivos, ou também editar e criar na pasta do projeto. Quem autentica é o
Claude Code; o Studio não guarda chave.

`AgentEvent` (`lace_agent::AgentEvent`, etiquetado por `type`): `ready`
(`session_id`, `model`, `tools`, `lace_connected`; só vem depois da primeira
mensagem, porque o Claude Code não inicia nada antes dela), `text` (um
pedaço da resposta), `tool_use` (`id`, `name`, `input`), `tool_result`
(`id`, `content`, `is_error`), `done` (`is_error`, `text`, `cost_usd`,
`duration_ms`, `turns`, `session_id`, tokens), `log` (uma linha do stderr
dele) e `exited` (`code`), o último. A interface (`state/assistant.ts`)
monta a transcrição a partir deles; ao trocar de projeto, fecha a sessão,
e a próxima mensagem abre outra. Ao sair, o Studio encerra todas.

## Eventos

| Evento | Carga | Quando |
|---|---|---|
| `studio://fs-changed` | `{ paths: string[] }` | arquivos da pasta do projeto foram criados, gravados, apagados ou renomeados (no máximo um a cada 300 ms; sem `.lace/` e `.git/`; abrir um arquivo para ler não conta) |

## Códigos de erro do Studio

| `code` | Quando |
|---|---|
| `no_project` | o comando precisa de um projeto aberto |
| `spf_missing` | o `.spf` do projeto aberto foi apagado depois de aberto: os comandos de projeto e os fluxos recusam (os `fs_*` continuam, pela pasta) |
| `busy` | já há uma operação rodando |
| `outside_project` | criar, renomear ou apagar fora da pasta do projeto |
| `conflict` | o arquivo mudou no disco desde que foi lido |
| `exists` | o arquivo ou a pasta de destino já existe |
| `invalid_argument` | argumento inválido (regex mal formada, componente que não existe) |
| `terminal` | o pseudoterminal não abriu, ou o shell já terminou |
| `unsafe_path` | `terminal_cd` com um caminho que tem caractere de controle: o shell o leria como tecla, e o `cd` não é digitado |
| `terminal_busy` | `terminal_cd` com um programa em primeiro plano no terminal (Linux e macOS): a linha iria para ele, e o `cd` não é digitado |
| `cannot_replace` | `project_move` com `overwrite` e, no destino, o que o Core mantém no lugar (o `.spf`, a pasta `.lace`, a pasta, a pasta de saída ou o fonte de um processador), a pasta do projeto ou uma pasta que contém a origem: nada vai para a lixeira |
| `io` | leitura ou escrita do próprio Studio falhou |
| `surfer_web_missing` | `wave_tab_open` num bundle sem o cliente web do Surfer: a interface oferece a janela |
| `wave_too_large` | `wave_tab_open` com uma onda acima de 256 MiB (`MAX_TAB_WAVE`): a interface oferece a janela |
| `internal` | uma thread caiu, ou JSON inválido |
| `assistant` | o assistente: o Claude Code não foi achado, o `lace` das ferramentas não existe, ou a sessão já terminou |

Os erros do Core chegam com o código dele (`LaceError::code`, API.md do
Lace, seção 8): por exemplo `invalid_name` (nome de projeto fora da regra),
`cannot_move`, `path_exists` e `outside_project` (de `project_move`),
`no_waveform` (a onda pedida ainda não existe; `wave_open` e
`wave_tab_open` devolvem o mesmo código), `bundle_not_found` (nenhuma
instalação do Lace foi achada, com onde procurou) e `experimental_disabled`
(os comandos e os fluxos da placa FPGA com ela desligada, e
`assistant_start` com o assistente desligado; a interface relê os recursos
e tira o recurso da janela). Os do `lace-learn` (`learn_component_missing`),
da biblioteca do instalador (`network`, `no_installation`) e do
`lace-agent` (`claude_not_found`, `ended`) também passam como estão. A pasta
do compilador sem os três programas não é erro de comando: vem em
`ToolchainInfo.compiler_error`, com o código `compiler_invalid`.
