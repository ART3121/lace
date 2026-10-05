# 0001. Toda regra no lace-core; as interfaces são cascas

- **Status:** Aceita
- **Data:** 2026-10-01

## Contexto

Na AURORA, a orquestração das ferramentas (que programa rodar, com que
argumentos, em que pasta, como ler a saída) está no código da própria IDE:
os comentários dos módulos do Core citam de onde cada regra veio
(`js/compilation/builders/*.ts`, `js/compilation/icarus_da_onda.ts`,
`main/ipc/project.js`). Para usar o mesmo fluxo de outra interface seria
preciso reescrever essa lógica. O Lace existe para tirá-la de dentro da
IDE. Hoje há uma interface, a CLI `lace`; estão previstas uma GUI e uma
extensão de VS Code.

## Decisão

Toda regra de negócio fica na biblioteca `crates/lace-core`. Uma interface
abre o projeto, chama uma função do Core e mostra o resultado, e nada além.

- A CLI (`crates/lace-cli`) é a primeira casca: cada comando em
  `commands.rs` chama o Core e passa o resultado para `output.rs`.
- Uma GUI em Rust usa a biblioteca direto. Os tipos que ela manda entre
  threads são `Send + Sync`, conferido em
  `crates/lace-core/tests/api_contract.rs`.
- Interfaces em outra linguagem, como a extensão de VS Code, chamam
  `lace ... --json`. O JSON é a serialização (`Serialize`) dos próprios
  tipos do Core, e não um formato à parte.

Não há protocolo RPC próprio. Se um dia for preciso um servidor, ele será um
protocolo padrão por cima do Core: LSP para editores, MCP para agentes.

A API separa dois tipos de falha, e toda interface mantém a separação:

- `Err(LaceError)`: o Lace não conseguiu rodar (bundle, projeto, I/O,
  processo que não iniciou). É problema do ambiente. Cada variante tem um
  código estável, `LaceError::code()` (`error.rs`).
- `Ok(resultado)` com `Status` diferente de `Succeeded`: as ferramentas
  rodaram e recusaram a entrada. É feedback sobre o código do usuário, com
  arquivo e linha nos `diagnostics` do resultado.

Na CLI, isso vira o código de saída, em `main.rs`: 0 deu certo; 1
(`EXIT_FAILED`) a operação rodou e falhou; 2 (`EXIT_ERROR`) o Lace não
conseguiu rodar. Argumento inválido também sai com 2, pelo `clap`.

## Consequências

- O Core não escreve no console: `lib.rs` tem
  `#![deny(clippy::print_stdout, clippy::print_stderr)]`. O log sai por
  `tracing`, e quem configura a saída é a interface (`init_tracing`, em
  `crates/lace-cli/src/main.rs`).
- As mensagens de erro do Core não citam comandos da CLI. A dica com o
  comando que resolve fica na função `hint` de
  `crates/lace-cli/src/output.rs`. O teste
  `new_errors_have_stable_codes_and_neutral_messages`
  (`crates/lace-core/tests/verilog_flow.rs`) confere.
- Regra posta na CLI é regra que a GUI vai ter de copiar. Lógica nova em
  `commands.rs` precisa de justificativa na revisão.
- O JSON da CLI é contrato: mudar um campo de um tipo público do Core muda a
  saída de `--json` e quebra quem a lê.
- Um `code()` existente nunca muda; a documentação de `LaceError::code`
  promete isso.
