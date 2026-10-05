# 0004. Verilog é a base; o SAPHO é um fluxo dentro dele

- **Status:** Aceita
- **Data:** 2026-10-01

## Contexto

A versão 0.1.0 tratava o projeto como um conjunto de processadores SAPHO. O
Lace também serve para desenvolver Verilog sem processador nenhum, e um
design pode misturar módulos do usuário com processadores SAPHO. Na 0.1.0, a
biblioteca SAPHO entrava sempre que o YANC estava instalado (`CHANGELOG.md`,
seção 0.2.0). Com ela no caminho, um módulo do usuário com o nome de um
módulo da biblioteca (`processor`, `core`) colide com ele: o Yosys lê a
biblioteca inteira e recusa a redefinição (comentário do teste
`verilog_only_synthesis_skips_sapho_library`).

## Decisão

Um projeto do Lace é um projeto Verilog: os arquivos registrados no `.spf`
(`synthesizableFiles`, `testbenchFiles`), o arquivo de topo e o testbench
simulado. Processadores SAPHO são opcionais e entram como mais uma fonte de
Verilog: o YANC gera o `Hardware/<nome>.v` de cada um.

Quando o projeto tem processadores, o YANC os compila antes de verificar,
simular ou sintetizar. O Core mantém as etapas separadas: `check`,
`simulate_project` e `synthesize` não compilam; usam o Verilog que o último
build deixou em `Hardware/`. Quem encadeia é o cliente. Na CLI, `build_first`
(`crates/lace-cli/src/commands.rs`) chama `build_processors` com
`OnFailure::Stop` sobre `Project::buildable_processors()` (ou só o
processador de `-p`) e não segue se algum falhar. `lace build` usa
`OnFailure::Continue`, para mostrar todos os erros de uma vez.

A biblioteca SAPHO (`yanc/SAPHO` do bundle) só entra quando o projeto tem
processadores. A regra está em `Toolchain::sapho_library(with_processors)`,
em `crates/lace-core/src/toolchain.rs`:

- sem processadores, devolve `None`, e nada de `-y <SAPHO>`;
- com processadores e sem o YANC instalado, `ComponentMissing("yanc")`;
- com processadores, o diretório da biblioteca.

`check`, `synthesize` e `simulate_project` passam
`!project.processors().is_empty()`; `simulate`, que simula um processador,
sempre a usa.

Os dois fluxos usam os mesmos comandos (`check`, `sim`, `synth`, `wave`) e
cabem no mesmo projeto.

## Consequências

- Um projeto só de Verilog não precisa do YANC instalado.
- Num projeto só de Verilog, um módulo do usuário pode ter o nome de um
  módulo da biblioteca sem colidir. Os testes
  `verilog_only_project_skips_sapho_library` e
  `verilog_only_synthesis_skips_sapho_library`
  (`crates/lace-core/tests/verilog_flow.rs`) conferem.
- Acrescentar o primeiro processador muda os comandos que o Lace roda: a
  biblioteca passa a entrar por `-y`, e o processador precisa compilar antes
  de qualquer outra etapa.
- `lace build` num projeto sem processadores avisa e sai com 0.
- A sequência "compilar os processadores, depois a operação" está hoje na
  CLI. Toda interface precisa repeti-la; pela ADR 0001, quando uma segunda
  interface precisar dela, o lugar é o Core.
