# 0002. Só ferramentas do bundle, com ambiente vazio

- **Status:** Aceita; a exceção no Windows foi substituída pela [0009](0009-windows-com-o-bloco-msys2-do-lace-toolchain.md)
- **Data:** 2026-10-01

## Contexto

O laboratório usa Linux, macOS e Windows. Se cada máquina rodar o Icarus, o
Yosys ou o Verilator que estiver no `PATH`, a versão muda de máquina para
máquina, e com ela as mensagens, os avisos e o resultado da simulação. Um
erro de ambiente passa a parecer erro do projeto. A AURORA herda o próprio
ambiente nos processos filhos e baixa as ferramentas em `components/`
(`docs/API.md`, seção 10).

## Decisão

Toda ferramenta sai do bundle versionado instalado com o Lace, e só dele.

- O caminho de cada executável dentro do componente é fixo por plataforma,
  no código (`Tool::location`, em `crates/lace-core/src/toolchain.rs`). O
  diretório do componente vem do manifesto. Nada vem do `PATH`, nada é
  configurável.
- `Toolchain::tool` confere o arquivo no disco a cada uso e recusa um
  symlink que sai do bundle (`InvalidBundle`).
- `process.rs` é o único módulo do Core que cria processos. A função
  `command` chama `env_clear()` e fecha o stdin (`Stdio::null()`). O filho
  recebe só:
  - no Windows, `SystemRoot`, `windir`, `ComSpec`, `TEMP` e `TMP`
    (`INHERITED_ENV`);
  - o que cada ferramenta exige, montado em `Toolchain::invocation`: no
    Linux e no macOS, os lançadores bash do OSS CAD Suite rodam pelo
    `/bin/bash` com `PATH=/usr/bin:/bin`; no Windows, o `.exe` roda com
    `PATH` em `bin;lib` do pacote e `System32` no fim;
  - para o surfer-aurora, as variáveis de display e de usuário (`GUI_ENV`).
- Todo `StepReport` guarda programa, argumentos, diretório de trabalho,
  variáveis definidas e nomes das herdadas.

A exceção declarada é a do Verilator, que compila o modelo em C++; o OSS CAD
Suite não traz compilador. O compilador C++, o `make` e o Perl vêm do
sistema (`SystemCompiler`), procurados em locais fixos por
`SystemCompiler::detect`: `/usr/bin` no Linux; `/usr/bin` com as Command
Line Tools ou o Xcode instalados no macOS; o MSYS2 em `C:\msys64` ou
`C:\tools\msys64` no Windows. Outro diretório só por declaração explícita
(`--compiler <DIR>` ou a variável `LACE_COMPILER`, que chegam ao Core por
`Toolchain::with_system_compiler`), nunca pelo `PATH`. Mesmo assim, o
Verilator usa o `verilator_bin` do bundle (o Lace recusa rodar sem ele,
porque o script Perl o procuraria no `PATH`) e o `make` usa o Python do
bundle (`PYTHON3=`, `bundled_python`).

## Consequências

- O mesmo bundle dá as mesmas versões em toda máquina.
  `lace tools --verify` confere o SHA-256 de cada executável contra o
  manifesto (`Toolchain::verify`).
- Qualquer passo pode ser reproduzido à mão a partir do `StepReport`:
  ambiente vazio, mais `env` e `inherit`.
- O usuário não pode apontar para o próprio Icarus. Trocar a versão de uma
  ferramenta é montar outro bundle (ADR 0006).
- No desenvolvimento, `--toolchain <DIR>` ou `LACE_TOOLCHAIN` apontam para
  outro bundle, que também precisa ter `bundle.json`.
- Sobra o que depende do sistema: as fontes do `dot` no macOS e no Windows,
  e o Verilator grava só VCD, porque o FST dele exige lz4 e zlib, fora da
  exceção (`docs/BUNDLE.md`, seção 5).
- Código novo que roda um programa passa por `Toolchain::invocation` e
  `process::run`. `std::process::Command` em outro módulo do Core contraria
  esta decisão.
- Para encerrar a árvore de processos de um passo cancelado, o Windows usa o
  `taskkill.exe` do `System32` (ADR 0007). É o único programa do sistema
  que o Lace roda fora da exceção do Verilator, e ele não executa trabalho.
