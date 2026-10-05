# 0009. Windows com o bloco MSYS2 do lace-toolchain

- **Status:** Aceita
- **Data:** 2026-10-03

## Contexto

A ADR 0002 deixou o compilador C++, o `make` e o Perl do Verilator fora do
bundle, vindos do sistema; no Windows, de um MSYS2 em `C:\msys64` que o
usuário instalava à parte. A ADR 0006 tirou Icarus, Verilator e Yosys do
OSS CAD Suite nas três plataformas.

No Windows isso não fecha. O OSS CAD Suite de Windows traz o Verilator, mas
não traz compilador, `make`, Perl nem cocotb. O laboratório precisa do
cocotb, e o cocotb carrega uma VPI por simulador: a do Icarus tem de casar
com o `vvp` que roda, e a do Verilator é compilada com o mesmo `g++` que
compila o modelo. O pacote do cocotb para Windows no PyPI não traz a VPI do
Verilator. A AURORA resolveu isso com um bloco MSYS2 próprio
(nipscernlab/aurora-toolchain), em MINGW64, ambiente que o MSYS2 descontinuou
em 15/03/2026, e travando só três pacotes.

No Linux e no macOS o OSS CAD Suite traz Icarus, Verilator, Yosys e o
Python com o cocotb, e o sistema tem compilador, `make` e Perl.

## Decisão

As famílias de plataforma montam o bundle de fontes diferentes.

- **Linux e macOS** acompanham o OSS CAD Suite, como na ADR 0006, com o
  compilador do sistema, como na ADR 0002.
- **Windows** tira Icarus e Verilator do pacote `msys` de
  `bundle/versions.json`: a release do repositório lace-toolchain, um bloco
  MSYS2 UCRT64 com Icarus, Verilator, o `g++`, o `make` e o Perl que ele
  usa, e Python com o cocotb compilado com a VPI do Verilator. O
  lace-toolchain trava cada pacote por versão e SHA-256, guarda os pacotes
  e testa quatro fluxos (Icarus, Verilator, cocotb com Icarus, cocotb com
  Verilator) antes de publicar. O Yosys continua vindo do OSS CAD Suite, e o
  `dot` do Graphviz.

No código:

- `bundle/components.json` escolhe o pacote por plataforma (`package`) e,
  no `msys`, os pacotes do MSYS2 de cada componente (`msys`);
  `scripts/bundle.py` (`pkg_msys`, `select_msys`) leva esses pacotes, as
  dependências e os arquivos que o manifesto da release
  (`lace-msys-<tag>.json`) lista para cada um.
- `Tool::location` aponta Icarus e Verilator para `ucrt64/bin` no Windows.
- `Toolchain::open` monta o `SystemCompiler` do Windows a partir do
  componente verilator (`SystemCompiler::in_bundle`), com `bundled`
  verdadeiro, e `SystemCompiler::detect` não procura mais o MSYS2 do
  usuário. `--compiler` (`SystemCompiler::in_msys2`) continua trocando o
  compilador, para desenvolvimento.
- `lace update` compara o Icarus e o Verilator do Windows com as releases
  do lace-toolchain (`package_of`, em `crates/lace-cli/src/update.rs`).

## Consequências

- No Windows o Verilator funciona sem instalar nada além do Lace, e o
  instalador deixa de avisar sobre o MSYS2. No Linux e no macOS a exceção
  da ADR 0002 continua.
- As versões de Icarus e Verilator diferem entre as famílias: builds de
  desenvolvimento do OSS CAD Suite no Linux e no macOS, versões estáveis do
  MSYS2 no Windows. Os testes do Lace rodam nas três.
- Atualizar o Icarus ou o Verilator do Windows passa por uma release do
  lace-toolchain e pela troca da tag e dos hashes do pacote `msys`
  (`docs/BUNDLE.md`, seção 6). Sem os hashes, o `bundle.py` não monta o
  bundle de Windows; `LACE_MSYS_DIST` usa um build local do lace-toolchain
  para testar antes de publicar.
- O cocotb já vem no bloco de Windows, mas o Lace ainda não tem fluxo de
  cocotb, e o componente `cocotb` só existe no Linux e no macOS, com o
  cocotb do OSS CAD Suite.
- O bundle de Windows passa a redistribuir software GPL do MSYS2 (gcc,
  make, bash, coreutils). A licença e o pacote-fonte de cada um estão no
  manifesto da release do lace-toolchain.
