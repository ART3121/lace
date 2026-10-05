# 0008. O contrato do `--json` é gerado dos tipos e conferido por teste

- **Status:** Aceita
- **Data:** 2026-10-01

## Contexto

Quem lê o `--json` da CLI é outro programa: uma extensão de editor, um
script, o CI. Ele quebra quando um campo muda de nome ou de tipo. Até a
versão 0.1.0, o formato estava descrito só à mão, numa tabela de
`docs/CLI.md`, e a CLI montava parte dos objetos com `serde_json::json!`. Em
2026-10-01 a tabela estava errada em vários comandos, sem que nada
acusasse.

As alternativas eram duas: escrever o JSON Schema à mão e validar a saída
contra ele, ou gerar o schema dos próprios tipos. Escrito à mão, o schema
dos resultados do Core (passos, diagnósticos, artefatos) seria um segundo
lugar para manter igual ao código, que é o que já tinha falhado.

## Decisão

O schema sai dos tipos, com o `schemars`.

- Os resultados do Core derivam `JsonSchema` ao lado de `Serialize`. O
  `schemars` lê os mesmos atributos `#[serde(...)]`, e o comentário `///`
  de cada campo vira a `description` do schema.
- Na CLI, cada comando escreve um tipo de `crates/lace-cli/src/report.rs`.
  `Output::json` só aceita o trait `Report`, que só a macro `reports!`
  implementa, e a macro registra o schema do tipo. Um comando não tem como
  escrever JSON sem schema: não compila.
- Os schemas ficam versionados em `docs/schema/<comando>.json`, gerados no
  modo de serialização (`SchemaSettings::for_serialize`): um campo que sai
  sempre, mesmo `null`, é obrigatório.
- Dois testes. `schema_files_match_the_types` (unitário, em `report.rs`)
  falha quando os arquivos não são o que os tipos geram; com
  `LACE_UPDATE_SCHEMA=1`, regrava os arquivos. `crates/lace-cli/tests/schema.rs`
  roda os comandos de verdade e valida a saída contra os arquivos com o
  `jsonschema`.

## Consequências

- Mudar um resultado do Core ou um relatório da CLI muda `docs/schema/`, e
  o diff aparece na revisão. É ali que se decide se a mudança quebra quem lê
  o `--json`.
- Uma extensão em TypeScript pode gerar os tipos dela a partir de
  `docs/schema/` em vez de copiá-los à mão.
- O `schemars` não conhece o `camino`: todo campo de caminho leva
  `#[schemars(with = "String")]` (ou `Option<String>`, `Vec<String>`).
  Esquecer um não compila.
- O schema não fecha o objeto (`additionalProperties` fica livre): um campo
  a mais na saída não reprova a validação. O que pega um campo novo é o
  teste de arquivos, porque ele muda o schema gerado.
- O `jsonschema` é dependência só dos testes da CLI; o `schemars`, do Core e
  da CLI.
