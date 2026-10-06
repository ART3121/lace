# Contribuir

1. Prepare a máquina e rode o Studio: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).
2. Entenda onde cada coisa mora: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
3. Antes de mandar: `npm run typecheck`, `cd src-tauri && cargo clippy` sem
   aviso, `cargo test`, e abra o Studio para passar pelo que mudou.
4. Mudou um comando do backend: atualize [docs/IPC.md](docs/IPC.md).
   Mudou o estado de um recurso da AURORA: atualize
   [docs/AURORA_PARITY.md](docs/AURORA_PARITY.md). Mudou algo visível:
   [CHANGELOG.md](CHANGELOG.md).
5. Regra de negócio (que ferramenta rodar, com que argumentos, como ler a
   saída) vai para o Lace, não para cá.

Dúvidas sobre o SAPHO, o C± e o YANC: o manual em
<https://nipscern.com/library/sapho> e o repositório do YANC.
