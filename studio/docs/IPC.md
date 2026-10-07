# Referência do IPC

Os comandos que a interface chama no backend com `invoke`, um por linha das
tabelas abaixo, com o que recebem e o que devolvem. Na interface, cada um
tem uma função em `src/ipc/api.ts`; os tipos estão em `src/ipc/types.ts`
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
- "Bloqueio" quer dizer que o comando roda numa thread de bloqueio, fora da
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

`log_frontend` escreve no log do backend, com o alvo `lace_studio::ui`, o
que a interface manda: erros de JavaScript, `console.error` e todo erro
mostrado num aviso. `dev_smoke_script` alimenta o roteiro de fumaça
(DEVELOPMENT.md, seção 3).

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
| `project_open` | `path`: o `.spf` ou a pasta | `ProjectSnapshot` | |
| `project_create` | `parent`, `name` | `ProjectSnapshot`; `invalid_name` se o nome não passar na regra do Core (letras sem acento, dígitos, `_` e `-`, começando por letra) | `lace new` |
| `project_check_name` | `name` | `null` se o nome de projeto serve; senão o `IpcError` (`invalid_name`, com o motivo do Core) | `validate_project_name` |
| `project_reorder` | `path`, `position` (`{ kind: 'first' \| 'last' }` ou `{ kind: 'before' \| 'after', path }`) | `ProjectFile[]`: a lista do arquivo, na ordem nova | `lace order` (`Project::reorder_file`) |
| `processor_check_name` | `name` | `null` se o nome de processador serve; senão o `IpcError` (`invalid_name`, com o motivo: palavra do C± ou do Verilog, módulo da biblioteca SAPHO, nome longo) | `validate_processor_name` |
| `project_close` | | | |
| `project_snapshot` | | `ProjectSnapshot` | `lace status --json` |
| `project_add_verilog` | `path`, `testbench` | `AddedFile` do Core | `lace add [--tb]` |
| `project_remove_verilog` | `path` | `bool`: estava registrado | `lace remove` |
| `project_set_top` | `target`: arquivo ou nome de módulo | o arquivo de topo | `lace top` |
| `project_set_testbench` | `path` | | `lace sim <TESTBENCH>`, sem simular |
| `project_processor_defaults` | | `NewProcessor` com os padrões da AURORA | |
| `project_add_processor` | `request: NewProcessorRequest` | `Processor` do Core | `lace proc add` |
| `project_configure_processor` | `name`, `frequencyMhz?`, `clocks?`, `showArrays?` | `Processor` | `lace proc set` |
| `project_output_values` | `processor`, `port` | `number[]` | |
| `verilog_modules` | `text` | os módulos declarados | `verilog::modules_in` |
| `project_move` | `from`, `to` (o caminho final), `overwrite` | `MovedPath` do Core: `from`, `to` e os arquivos do projeto que foram junto | `lace move` |
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

`ProjectSnapshot` é o `StatusReport` da CLI com campos a mais:

| Campo | Conteúdo |
|---|---|
| `name`, `spf`, `root` | o projeto |
| `synthesizable`, `testbenches` | `ProjectFile[]`: `role`, `path`, `top_level` |
| `top_level`, `top_module`, `top_module_error` | o arquivo e o módulo de topo; o erro quando o módulo não dá para saber |
| `selected_testbench`, `testbench_module` | o testbench simulado e o módulo dele |
| `unregistered` | os `.v` e `.sv` da pasta fora do `.spf`, e os `.py` com `@cocotb.test` |
| `processors` | `ProcessorStatus[]`: os campos do `Processor` do Core, mais `built`, `inputs` e `outputs` (`{ port, path }`), `missing_inputs`, `waveform` e `generated` (o que o build gerou e existe: `verilog`, `testbench`, `assembly`, `memories[]`, `intermediates[]`, os `.txt` e `.log` da pasta temporária) |
| `waveform` | a onda da simulação do projeto, se já existe |
| `top_candidates` | os Verilog que podem ser o topo pela regra do Core (qualquer um, menos nome de testbench e testbench cocotb `.py`): registrados, o gerado de cada processador e os de fora do `.spf` |
| `issues` | `ProjectIssue[]` do Core (`Project::issues`): `kind` (`rescued_path`, `selection_not_registered`, `testbench_as_top`, `invalid_processor_name`), `path`, `detail`, `message`; o painel Problemas mostra como aviso, traduzido pelo `kind` |

`NewProcessorRequest`: `name`, `language` (`cmm` ou `cpp`) e, opcionais,
`input_ports`, `output_ports`, `nubits`, `nbmant`, `nbexpo`, `nugain`,
`ndstac`, `sdepth`. Ausente fica com o padrão do Core. O Core recusa nome e
parâmetro que o YANC não compila (`invalid_name`, `invalid_parameter`).

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
| `fs_list_files` | | todos os arquivos do projeto (até 20 000), sem pastas ocultas, `node_modules`, `target`, `obj_dir` |
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

Uma operação por vez: começar outra com uma rodando é `busy`.

### `FlowRequest`

Com o tipo no campo `flow`:

| `flow` | Campos | Equivale a |
|---|---|---|
| `build` | `processors: string[]` (vazio: todos) | `lace build [-p NOME]...` |
| `check` | `file?`, `processor?`, `lint?` | `lace check [ARQUIVO] [-p NOME] [--lint]` |
| `simulate` | `processor?`, `testbench?`, `simulator` (`icarus`, `verilator`), `timeout_s?`, `open_wave?` | `lace sim [TESTBENCH] [-p NOME] [--verilator] [--timeout S] [--open]` |
| `synthesize` | `processor?`, `schematic?`, `module?` | `lace synth [-p NOME] [--svg] [--module M]` |
| `schematic` | `netlist`, `module`, `bus_widths?` (padrão `true`) | desenha outro módulo de um netlist que já existe; sem relatório |

A composição é a da CLI (`flows.rs`): `check`, `simulate` e `synthesize`
compilam antes os processadores que têm fonte e param no primeiro build que
falhar; `build` compila todos mesmo que um falhe; cada fluxo, menos
`schematic`, grava um relatório no histórico.

### `JobMessage`

As mensagens chegam pelo `channel`, nesta ordem, com o tipo no campo `type`:

| `type` | Campos | Quando |
|---|---|---|
| `started` | `job`, `command` | logo depois de `flow_start` |
| `phase` | `phase`: `build`, `check`, `simulate`, `synthesize`, `schematic`, `wave` | uma fase começou |
| `events` | `events: Event[]` | lotes de eventos do Core (`step_started`, `output`, `step_finished`; API.md do Lace, 5.8), no máximo a cada 30 ms ou 500 eventos |
| `build` | `result: BuildResult` | um processador terminou de compilar |
| `cli_output` | `stream`, `line` | uma linha da CLI (só em `lace_install` e `lace_update`) |
| `finished` | `outcome` | fim: um `FlowOutcome` (ou o resultado da CLI) |
| `failed` | `error: IpcError` | fim: o Lace não conseguiu rodar |

Toda operação termina com exatamente um `finished` ou um `failed`.

### `FlowOutcome`

| Campo | Conteúdo |
|---|---|
| `flow`, `command` | o fluxo e o comando equivalente (`lace-studio sim -p soma`) |
| `succeeded` | tudo deu certo |
| `builds` | `BuildResult[]` dos builds feitos antes (ou do próprio build) |
| `check`, `simulation`, `synthesis`, `schematic` | o resultado de cada fase que rodou, ou `null`; com um testbench cocotb, `simulation.tests` traz os testes (`TestReport` do Core) |
| `outputs` | `PortValues[]` (`port`, `path`, `values`, `error`), na simulação de um processador que deu certo |
| `wave`, `wave_error` | a onda aberta em janela, ou por que não abriu |
| `wave_tab` | a onda que a interface abre numa aba (`wave_viewer: tab`), no lugar da janela |
| `schematic_error` | por que o esquemático não saiu depois da síntese (`module_not_found`: o módulo pedido não está no netlist); a síntese vale |
| `report`, `report_error` | o relatório gravado (`run-000042`), ou por que não foi |

O `stdout` e o `stderr` de cada passo vêm cortados em 256 KiB, guardando o
fim; o que foi cortado já chegou linha a linha pelos eventos.

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
| `lace_update_check` | | o `UpdateReport` da CLI | `lace update --check --json` |
| `lace_install` | `components: string[]`, `channel` | o identificador da operação | `lace install <componentes> --json` |
| `lace_update` | `channel` | o identificador da operação | `lace update --yes --json` |

`ToolchainInfo`: `found`, `error`, `origin` (`settings`, `environment`,
`installation`, `path`, `beside`), `root`, `bundle`, `platform`,
`components`, `not_installed`, `tools` (`name`, `component`, `path`,
`system`, `error`), `system_compiler`, `compiler_error`, `lace_cli`,
`surfer_web` (o cliente web do Surfer, `surfer-aurora/web/`, ou `null`).

`lace_update_check`, `lace_install` e `lace_update` rodam o `lace` da
instalação do bundle (`<instalação>/bin/lace`), e precisam de rede.
`lace_install` e `lace_update` são operações como as do Core: ocupam a
vaga e mandam `cli_output` e `finished` (`{ flow: 'install' | 'update',
succeeded, cancelled, exit_code, result }`). `result` é o JSON que a CLI
escreveu: o relatório do comando, ou `{ error: { code, message, hint? } }`
quando ele não rodou.

`lace_update` passa `--yes`: quem pergunta é a interface, antes de chamar,
com a versão instalada, a nova e a pasta (o `UpdateReport` de
`lace_update_check`). No `result`, `action` diz o que aconteceu:
`updated` (Linux e macOS: o instalador da release trocou o `lace` e o
bundle), `wizard_opened` (Windows: o assistente abriu e termina depois que
o `lace` sai) ou `up_to_date`. Ao contrário de `lace_install`, ela não pode
ser cancelada: `flow_cancel` marca o pedido, mas ele não chega ao processo,
porque matar o `lace` deixaria o instalador, filho dele, trocando o `bin/`
e o `toolchain/` sozinho.

## Onda numa aba (`wave_tab.rs`)

| Comando | Argumentos | Devolve |
|---|---|---|
| `wave_tab_open` | `path` | `WaveTab`: `id`, `url` (a página do cliente web para o iframe), `processors` (`WaveProcessor[]` do layout, com `outdated` quando o processador foi compilado de novo depois da simulação) |
| `wave_tab_close` | `id` | |

`wave_tab_open` registra a onda num servidor HTTP do Studio, só em
`127.0.0.1`, que sobe na primeira aba e serve, numa origem: o cliente web
(`/web/`), a onda (`/wave/<id>/<nome>`), o `.surf.ron` (`/layout/<id>`), os
tradutores e os comandos de partida (`/doc/<id>/<nome>`). A `url` abre o
cliente com `load_url` na onda e `startup_commands` que carregam os
tradutores e o estado. Erros: `no_waveform`;
`surfer_web_missing`, bundle sem cliente web; `wave_too_large`, onda acima
de 256 MB. A interface oferece a janela nos dois últimos.

`wave_tab_close` faz o servidor esquecer a aba. A vista chama ao fechar a
aba ou ao recarregar.

## Terminal (`terminal.rs`)

| Comando | Argumentos | Devolve |
|---|---|---|
| `terminal_spawn` | `cwd?` (padrão: a pasta do projeto), `cols`, `rows`, `channel: Channel<TerminalMessage>` | o identificador |
| `terminal_write` | `id`, `data` | |
| `terminal_cd` | `id`, `path` | digita no shell o `cd` para `path`, na sintaxe dele (PowerShell, `cmd`, POSIX ou fish); a interface chama ao abrir outro projeto |
| `terminal_resize` | `id`, `cols`, `rows` | |
| `terminal_kill` | `id` | |

`TerminalMessage`: `{ type: 'data', data }` com o que o shell escreveu (UTF-8),
e `{ type: 'exit', code }` quando ele termina. O shell é o do usuário
(`$SHELL`) no Linux e no macOS e o PowerShell no Windows, com a pasta `bin/`
da instalação do Lace no começo do `PATH` e `LACE_STUDIO=1`.

## Eventos

| Evento | Carga | Quando |
|---|---|---|
| `studio://fs-changed` | `{ paths: string[] }` | arquivos da pasta do projeto foram criados, gravados, apagados ou renomeados (no máximo um a cada 300 ms; sem `.lace/` e `.git/`; abrir um arquivo para ler não conta) |

## Códigos de erro do Studio

| `code` | Quando |
|---|---|
| `no_project` | o comando precisa de um projeto aberto |
| `busy` | já há uma operação rodando |
| `outside_project` | criar, renomear ou apagar fora da pasta do projeto |
| `conflict` | o arquivo mudou no disco desde que foi lido |
| `exists` | o arquivo ou a pasta de destino já existe |
| `invalid_argument` | argumento inválido (regex mal formada, componente que não existe) |

Os erros do Core chegam com o código dele (`LaceError::code`, API.md do
Lace, seção 8): por exemplo `invalid_name` (nome de projeto fora da regra),
`cannot_move`, `path_exists` e `outside_project` (de `project_move`).
| `no_waveform` | a onda pedida ainda não existe |
| `compiler_invalid` | a pasta do compilador nas preferências não tem os três programas |
| `bundle_not_found` | nenhuma instalação do Lace foi achada (mensagem com onde procurou) |
| `cli` | a CLI `lace` não foi achada, falhou ou não escreveu JSON |
| `terminal` | o pseudoterminal não abriu, ou o shell já terminou |
| `io` | leitura ou escrita do próprio Studio falhou |
| `internal` | uma thread caiu, ou JSON inválido |
