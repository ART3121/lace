# 0003. O projeto é o `.spf` da AURORA

- **Status:** Aceita; caminhos de fora da raiz emendados pela [0012](0012-caminhos-fora-da-raiz-e-resgate.md)
- **Data:** 2026-10-01

## Contexto

Os projetos do laboratório já existem como arquivos `.spf` da AURORA. O
Lace substitui a orquestração da AURORA, não os projetos: durante a
transição, a mesma pessoa abre o mesmo projeto nos dois programas. Um
formato próprio exigiria migração e faria a AURORA deixar de abrir o que o
Lace criou.

## Decisão

O arquivo de projeto do Lace é o `.spf` da AURORA (JSON), lido e gravado
como a AURORA faz, preservando o que o Lace não entende. O código está em
`crates/lace-core/src/spf.rs` e `project.rs`.

- **Leitura tolerante**, como `js/project/spf_parse.ts` da AURORA
  (`spf::parse`): JSON estrito primeiro; se falhar, tira comentários `//` e
  `/* */`, BOM e vírgula sobrando antes de `}` ou `]`, e tenta de novo. Sem a
  seção `structure`, não é projeto (`InvalidProjectFile`).
- **O documento inteiro fica em memória** (`Project::document`, um
  `serde_json::Value`). O Lace muda só os campos que entende e regrava o
  resto como estava: `commandOverrides`, `folders`, `metadata.lastOpened` e
  qualquer outro. A ordem das chaves se mantém pela feature
  `preserve_order` do `serde_json`, ligada no `Cargo.toml` do workspace.
- **Gravação como a da AURORA** (`spf::write`, `Project::save`): atômica
  (arquivo temporário e `rename`), com indentação de 2 espaços, atualizando
  `lastModified`, `projectPath` e `basePath` com a raiz atual.
- **A raiz é o diretório do `.spf`.** O `basePath` gravado é ignorado na
  leitura, para que um projeto copiado de outra máquina abra.
- **Abrir só lê.** A AURORA regrava o arquivo ao abrir; o Lace não.
- **Um campo só do Lace:** `structure.topLevelModule`, gravado por
  `Project::set_top` quando o topo é escolhido pelo nome do módulo. A AURORA
  o ignora.
- **O mesmo layout de pastas:** `<processador>/Software`, `Hardware`,
  `Simulation`. Os intermediários ficam em `.lace/Temp`, e não em
  `.aurora/Temp`.

## Consequências

- Lace e AURORA abrem o mesmo projeto. O teste
  `preserves_unknown_fields_on_save` (`project.rs`) confere que um campo
  desconhecido sobrevive a uma gravação.
- O Lace herda o formato da AURORA: as listas `synthesizableFiles` e
  `testbenchFiles` com `{ "name", "path", "isTopLevel" }` (`files.rs`) e
  processadores que podem ser uma string (formato antigo) ou um objeto.
- Campo novo no `.spf` precisa ser um que a AURORA ignore, como o
  `topLevelModule`.
- Onde a AURORA adivinha, o Lace recusa: `clk` fracionário é
  `InvalidProjectFile` (a AURORA trunca) e `-` em nome de processador é
  `InvalidName` (o `cmmcomp` não aceita). Um projeto que abre na AURORA pode
  ser recusado pelo Lace nesses casos.
- Se a AURORA mudar o formato, o Lace precisa acompanhar. As referências
  estão nos comentários de `spf.rs` e `files.rs` (`main/ipc/project.js`,
  `js/project/spf_parse.ts`, `js/project/file_mode.js`, `spf_store.ts`).
- Um `Project` aberto não recarrega o arquivo. Se a AURORA gravar o `.spf`
  enquanto o Lace o tem aberto, a próxima gravação do Lace sobrescreve a
  dela. A interface deve abrir de novo antes de alterar.
