# O bundle de ferramentas

O Lace só executa ferramentas de um bundle instalado junto com ele. Nunca do
`PATH`, nunca de um caminho configurado pelo usuário. A exceção, decidida pelo
autor, é o compilador C++ do Verilator no Linux e no macOS; no Windows ele
vem no bundle, e o `taskkill` encerra os passos cancelados (seção 4).

As duas famílias de plataforma montam o bundle de fontes diferentes:

- **Linux e macOS** acompanham o OSS CAD Suite: Icarus, Verilator, cocotb,
  Yosys e `dot` saem da mesma release datada dele, e o compilador, o `make` e o Perl
  do Verilator vêm do sistema.
- **Windows** tira Icarus e Verilator do bloco MSYS2 UCRT64 do repositório
  [lace-toolchain](https://github.com/ART3121/lace-toolchain), que traz
  também o g++, o `make` e o Perl que o Verilator usa, e o Python com o
  cocotb. O Yosys continua vindo do OSS CAD Suite, e o `dot` do Graphviz.

O motivo está no [ADR 0009](adr/0009-windows-com-o-bloco-msys2-do-lace-toolchain.md):
no Windows o OSS CAD Suite não traz compilador, `make`, Perl nem cocotb, e o
cocotb precisa de simulador, compilador e Python que casem.

## 1. O que vem no bundle

Bundle `2026.09.29`. Os pacotes, com as versões fixadas em
`bundle/versions.json`:

| Pacote | Versão | O que traz | De onde vem |
|---|---|---|---|
| YANC | v5.6 (`e1ad149`) | `cmmcomp`, `appcomp`, `asmcomp`, `cpppp`, `cppcomp`, a biblioteca SAPHO, macros e headers | compilado do fonte pelo empacotamento |
| surfer-aurora | v0.7.0-nips.10 (`d0af8a7`) | o fork do Surfer da AURORA e o cliente web (WASM) dele | o executável, compilado do fonte pelo empacotamento; o cliente web, o zip que a CI do fork publica na mesma tag, conferido pelo SHA-256 fixado |
| OSS CAD Suite | release 2026-09-29 | Linux e macOS: Icarus Verilog, Verilator, o cocotb com o Python que o roda, Yosys e o `dot` do Graphviz. Windows: só o Yosys | pacote oficial, conferido pelo SHA-256 publicado |
| msys | `ucrt64-v1` | Icarus Verilog, Verilator, o g++, o `make` e o Perl que ele usa, Python com cocotb | só no Windows: release do lace-toolchain, o zip e o manifesto conferidos pelo SHA-256 fixado |
| Graphviz | 16.1.0 | `dot` | só no Windows (o OSS CAD Suite de Windows não traz), zip oficial conferido pelo SHA-256 publicado |
| studio | a versão do Lace | o Lace Studio: o executável `lace-studio` (Linux, Windows) ou o `Lace Studio.app` (macOS) | compilado de `studio/` deste repositório pelo empacotamento (`npm ci`, `tauri build`) |

Os componentes, que o instalador deixa escolher, estão em
`bundle/components.json`:

| Componente | Pacote | Diretório |
|---|---|---|
| `yanc` | YANC | `yanc/` |
| `icarus` | OSS CAD Suite (Linux, macOS), msys (Windows) | `oss-cad-suite/` ou `msys/` |
| `verilator` | OSS CAD Suite (Linux, macOS), msys (Windows) | `oss-cad-suite/` ou `msys/` |
| `cocotb` | OSS CAD Suite (Linux, macOS), msys (Windows) | `oss-cad-suite/` ou `msys/` |
| `yosys` | OSS CAD Suite | `oss-cad-suite/` |
| `graphviz` | OSS CAD Suite (Linux, macOS), Graphviz (Windows) | `oss-cad-suite/` ou `graphviz/` |
| `surfer-aurora` | surfer-aurora | `surfer-aurora/` |
| `studio` | studio | `studio/` |

O Lace não roda o Studio: ele está no bundle para o instalador e o
`lace install` o oferecerem como os outros, e o Studio instalado usa o
bundle em que está ([ADR 0013](adr/0013-studio-no-repositorio-e-no-bundle.md)).
Liga só às bibliotecas do sistema (`closure: false`): no Linux, ao
webkit2gtk 4.1 e ao GTK 3; no Windows, ao WebView2.

O OSS CAD Suite inteiro tem 2,5 GB no Linux: traz também nextpnr, GHDL,
GTKWave, bases de FPGA e um Python com dezenas de pacotes. O empacotamento
leva de cada ferramenta só o que ela executa (os lançadores de `bin/`, os
binários de `libexec/`), os dados dela (`share/yosys`, `lib/ivl`,
`share/verilator`, a biblioteca padrão do Python para o `make` do Verilator,
as fontes do `dot`) e o fecho das bibliotecas dinâmicas que esses binários
carregam, calculado lendo os binários (ELF, Mach-O e PE, em
`scripts/binaries.py`). Dependência que não está no pacote e não é do sistema
(`/usr/lib` e `/System` no macOS, DLLs do Windows) faz o empacotamento
falhar. O bundle completo fica com 300 MiB no Linux; a divisão do macOS dá
141 MiB, sem o surfer-aurora (medidos na montagem local; o CI monta os
completos). O cocotb não entra nessas contas: acrescenta 42 MiB no Linux e
25 MiB no macOS a quem tem o Verilator, que já traz o mesmo Python, e 115 e
76 MiB a quem não tem. No Windows, o bloco MSYS2 (`ucrt64-v1`) se divide em
14 MiB para o Icarus, 630 MiB para o Verilator (quase tudo o g++ e o Perl)
e 102 MiB para o cocotb (o Python, a libstdc++ e o cocotb).

O cocotb leva o Python do pacote com a biblioteca padrão, o egg do cocotb
(as bibliotecas dele e a VPI de cada simulador), o `find_libpython` e o
pytest com o que ele importa (`pluggy`, `iniconfig`, `packaging`,
`pygments`). Do `site-packages`, só isso: os pacotes que ficam são as
exceções de `keep` em `bundle/components.json`.

O bloco MSYS2 se divide de outro jeito: o manifesto da release do
lace-toolchain (`lace-msys-<tag>.json`) lista os pacotes do MSYS2 que estão
no zip, as dependências e os arquivos de cada um. Cada componente pede os
pacotes de que precisa (`msys` em `bundle/components.json`), e o
empacotamento leva esses pacotes, as dependências deles e os arquivos que o
manifesto lista. O Icarus leva o pacote `iverilog`; o Verilator,
`verilator`, `perl`, `gcc`, `python` e a camada MSYS que o `make` usa
(`make`, `bash`, `coreutils`, `findutils`, `msys2-runtime`).

Fica de fora de propósito: os plugins do Yosys (`share/yosys/plugins`, que
puxariam o GHDL), o `site-packages` (menos o que o cocotb usa) e os testes
do Python, e as cópias de depuração do Verilator.

O OSS CAD Suite compila cada ferramenta do `master` e publica uma release por
dia. Fixar a data e o hash do pacote é o que dá a versão exata. O bloco
MSYS2 tem versões estáveis, travadas pelo lock do lace-toolchain: no
`ucrt64-v1`, Icarus 13.0, Verilator 5.050, gcc 16.2, Perl 5.44, Python 3.14
e cocotb 2.1.0. Na release 2026-09-29 do OSS CAD Suite as ferramentas se
identificam assim:

| Ferramenta | Versão reportada (Linux x64) |
|---|---|
| Yosys | 0.69+156 (git 9d0c91b23) |
| Icarus Verilog | 14.0 (devel) s20260301-500-g2e81fcccb |
| Verilator | 5.053 devel rev v5.052-240-g640607a0d |
| cocotb | 2.1.0.dev0+41564633, no Python 3.11.6 do pacote |
| dot | Graphviz 2.43 (20190912) |

## 2. Plataformas

| Plataforma | Nome no bundle | Observação |
|---|---|---|
| Linux x64 | `linux-x64` | |
| macOS Apple Silicon | `darwin-arm64` | O OSS CAD Suite avisou que vai parar de publicar o macOS Intel, que o Lace não suporta. |
| Windows 10 e 11 x64 | `windows-x64` | |

Um bundle só abre na plataforma para a qual foi montado.

## 3. Onde fica e como o Lace o usa

```
<instalação>/
  bin/lace[.exe]
  toolchain/
    bundle.json               formato, versão do bundle, plataforma
    components/<nome>.json    um por componente instalado
    yanc/                     bin/ SAPHO/ Macros/ Header/
    oss-cad-suite/            a parte do OSS CAD Suite dos componentes instalados
    msys/                     só no Windows: ucrt64/ e usr/bin/ do bloco MSYS2
    surfer-aurora/            surfer-aurora[.exe], web/ (o cliente web: index.html, surfer.js, surfer_bg.wasm)
    graphviz/                 só no Windows
```

O Lace procura `toolchain/` ao lado de `bin/` (ou ao lado do executável,
com os symlinks resolvidos).
`lace --toolchain <DIR>` usa outro bundle, para desenvolvimento; ele também
precisa ter `bundle.json`.

Cada `components/<nome>.json` tem a versão, a origem e o SHA-256 de cada
executável do componente que o Lace roda. `lace tools --verify` confere
esses hashes. Um componente sem arquivo em `components/` não está instalado.

Como cada ferramenta é executada:

| Ferramenta | Linux e macOS | Windows |
|---|---|---|
| YANC | o binário em `yanc/bin/` | `yanc/bin/*.exe` |
| Icarus | `/bin/bash oss-cad-suite/bin/<ferramenta>`: o lançador do pacote, que carrega o binário de `libexec/` com as bibliotecas do próprio pacote | o `.exe` de `msys/ucrt64/bin/`, com `PATH` nesse diretório, onde estão as DLLs |
| Yosys, `dot` | como o Icarus | o `.exe` de `oss-cad-suite/bin/` (ou `graphviz/bin/dot.exe`) com `PATH` em `bin;lib` do pacote, como o `environment.bat` |
| Verilator | Perl do sistema com o script `oss-cad-suite/bin/verilator`, que roda o `bin/verilator_bin` do pacote; o `make` e o compilador são os do sistema; o `make` gerado usa o Python do pacote (`PYTHON3=`) | o `msys/ucrt64/bin/perl.exe` com o script `msys/ucrt64/bin/verilator`, que roda o `verilator_bin.exe` ao lado; o `make` é o da camada MSYS (`msys/usr/bin/make.exe`), que roda o `verilated.mk` com o `sh` dela e compila com o `g++` de `ucrt64/bin`; o `make` gerado usa `msys/ucrt64/bin/python.exe` |
| surfer-aurora | o binário, com as variáveis de display | o `.exe` |

Todo processo parte de um ambiente vazio. Os lançadores recebem
`PATH=/usr/bin:/bin` para o `bash`, o `dirname` e o `readlink` que usam. No
Windows, o `PATH` das ferramentas do pacote termina em
`%SystemRoot%\System32`, e o `ComSpec` é repassado: o `iverilog` roda o
`ivlpp` e o `ivl` pelo `system()` da biblioteca C, que usa o `cmd.exe`.

O Lace ainda não roda o cocotb. Quem rodar com o Verilator tem de passar ao
`make` o Python do pacote (`PYTHON3=oss-cad-suite/bin/tabbypy3`, no
`MAKEFLAGS`, por exemplo): sem isso, o `verilated.mk` chama o `python3` do
sistema com o `PYTHONHOME` do pacote, herdado do lançador, e ele falha ao
carregar a biblioteca padrão. Com o Icarus não há o que passar.

O Lace recusa um bundle em que um componente, ou um executável, seja um
symlink para fora dele. E recusa rodar o Verilator se faltar o
`verilator_bin` do pacote: sem ele, o script Perl procuraria um
`verilator_bin` no `PATH`, que no Linux e no macOS inclui `/usr/bin`.

O `dot` do Linux recebe um `fonts.conf` gravado pelo Lace na pasta da
síntese, a partir do modelo que o pacote traz (como fazem os lançadores do
`xdot` e do `gtkwave`), e usa as fontes do pacote.

## 4. O que vem do sistema

| O quê | Por quê | Onde o Lace procura |
|---|---|---|
| `/bin/bash`, `dirname`, `readlink` (Linux e macOS) | os lançadores do OSS CAD Suite são scripts bash; é o jeito oficial de rodar o pacote | fazem parte do sistema base |
| `libc` (Linux e macOS) | o `lace` e o YANC são binários nativos ligados à `libc` do sistema; as ferramentas do OSS CAD Suite não, carregam as bibliotecas do pacote | sistema base |
| fontes (macOS e Windows) | os pacotes dessas plataformas não trazem fontes; o `dot` usa as do sistema | as do sistema |
| compilador C++, `make`, Perl (Linux e macOS) | exceção decidida pelo autor: o Verilator compila o modelo em C++, e o OSS CAD Suite não traz compilador. No Windows os três vêm no bundle, com o Verilator | locais fixos: `/usr/bin` (Linux); `/usr/bin` com as Command Line Tools do Xcode (macOS) |
| `taskkill.exe` (Windows) | encerrar a árvore de processos de um passo cancelado ou que passou do prazo (`taskkill /T /F`). Só encerra, não executa trabalho ([ADR 0007](adr/0007-cancelamento-e-saida-ao-vivo.md)); no Linux e no macOS, o Lace sinaliza o grupo de processos do passo e não roda programa nenhum | `%SystemRoot%\System32` (`C:\Windows\System32` sem `SystemRoot`) |

Sem o compilador, tudo funciona menos a simulação com Verilator, e
`lace tools` avisa. Para um compilador fora do local padrão, a opção global
`--compiler` ou a variável `LACE_COMPILER`, que vale para toda chamada. No
Windows ela troca o compilador do bundle pelo de um MSYS2 instalado, só
para desenvolvimento:

```
LACE_COMPILER=/opt/gcc/bin lace tools        # Linux, macOS: diretório com perl, make e g++/clang++
lace tools --compiler D:\msys64              # Windows: raiz de um MSYS2, no lugar do bundle
```

Como deixar a variável definida em cada shell: [CLI.md](CLI.md#de-onde-vêm-as-ferramentas).

Instalar o compilador:

| Sistema | Comando |
|---|---|
| Debian, Ubuntu | `sudo apt install build-essential perl` |
| Fedora | `sudo dnf install gcc-c++ make perl` |
| macOS | `xcode-select --install` |
| Windows | nada: vem com o componente verilator |

## 5. Limitações conhecidas

- **Verilator grava VCD, não FST.** No Linux e no macOS, o FST do Verilator
  compila contra lz4 e zlib, que não vêm no pacote nem fazem parte da
  exceção do compilador. O Windows segue o mesmo formato. O Icarus grava FST
  quando o `$dumpfile` termina em `.fst` (e na onda que o Lace injeta).
- **As versões de Icarus e Verilator diferem entre as plataformas.** No
  Linux e no macOS são os builds de desenvolvimento do OSS CAD Suite
  (Icarus 14 devel, Verilator 5.053 devel); no Windows, as versões estáveis
  do MSYS2 (Icarus 13.0, Verilator 5.050). Os testes rodam nas três.
- **O `dot` do OSS CAD Suite é antigo** (Graphviz 2.43, de 2019). Funciona
  para o esquemático; o do Windows é o 16.1.0.
- **O esquemático depende das fontes no macOS e no Windows.** Lá o `dot`
  mede o texto com as fontes do sistema, e o SVG pode variar de máquina para
  máquina. No Linux usa as do bundle.
- **Esquemático diferente da AURORA.** O Lace desenha com o `show` do Yosys e
  o `dot`; a AURORA, com o netlistsvg e skins próprios.
- **YANC e surfer-aurora são compilados pelo empacotamento.** O YANC não
  publica build de macOS, e o surfer-aurora publica só o executável de
  Windows e o cliente web. O cliente web, igual nas três plataformas, vem
  do zip publicado: é ele que o Lace Studio mostra numa aba, ligado a um
  `surfer-aurora server` da mesma versão (o cliente recusa um servidor com
  outra versão do leitor de ondas).

## 6. Montar um bundle

```
python3 scripts/bundle.py --out dist/toolchain                 # completo, plataforma atual
python3 scripts/bundle.py --out dist/toolchain --only yanc     # parcial, para testes de build
```

O script baixa (uma vez, para `.bundle-cache/`) e confere o SHA-256 de cada
pacote, compila YANC e surfer-aurora dos commits fixados, separa a parte de
cada componente e grava o `bundle.json`, os `components/<nome>.json` com o
hash de cada executável, e `dist/toolchain.contents.json`, o índice de que
arquivo é de que componente, que os instaladores usam
([RELEASE.md](RELEASE.md)). É o único lugar que baixa alguma coisa: em uso, o
Lace só lê o bundle.

`--platform` monta a divisão de outra plataforma: os pacotes prontos se
dividem em qualquer sistema; YANC e surfer-aurora precisam compilar para ela.

No Windows, o pacote msys precisa do SHA-256 da release do lace-toolchain em
`bundle/versions.json` (o zip em `sha256`, o manifesto em
`manifest_sha256`); sem ele, o `bundle.py` para e diz o que falta. Para
testar um bloco que ainda não foi publicado, `LACE_MSYS_DIST` aponta para o
`dist/` de um build local do lace-toolchain (com o `lace-msys-<tag>.zip` e o
`.json`), e o `bundle.py` usa ele no lugar da release.

Precisa, além de Python 3.9+ e `git`:

| Sistema | Para o YANC | Para o surfer-aurora | Para o Studio |
|---|---|---|---|
| Linux | `gcc`, `make`, `flex`, `bison` | `cargo` | `cargo`, Node.js 22, `libwebkit2gtk-4.1-dev` e o resto da [lista do Tauri](https://v2.tauri.app/start/prerequisites/) |
| macOS | Command Line Tools, `flex` e `bison` do Homebrew (`YANC_MAKE_ARGS="BISON=$(brew --prefix bison)/bin/bison FLEX=$(brew --prefix flex)/bin/flex"`) | `cargo` | `cargo`, Node.js 22 |
| Windows | shell MINGW64 do MSYS2 com `mingw-w64-x86_64-gcc`, `make`, `flex`, `bison` | `cargo` | `cargo` (MSVC), Node.js 22 |

O Studio entra na versão do `bundle/versions.json` (pacote `studio`), que
tem que ser a do `studio/package.json` e a do `tauri.conf.json`. Para montar
sem ele, `--only` com os outros componentes.

O workflow `.github/workflows/installers.yml` monta o bundle e os
instaladores nas três plataformas, instala por eles e roda os testes contra
a instalação.

### Atualizar uma versão

1. Trocar a versão e o hash em `bundle/versions.json` (em `packages`). O SHA-256 dos pacotes
   do OSS CAD Suite está no campo `digest` da API de releases do GitHub; o do
   Graphviz, no arquivo `.sha256` ao lado do zip. No Linux e no macOS, o
   bundle acompanha o OSS CAD Suite: atualizar é trocar a data dele. No
   Windows, o Icarus e o Verilator mudam com o pacote msys: publique uma
   release `ucrt64-vN` no lace-toolchain (o workflow dele monta, testa os
   quatro fluxos e publica) e troque a tag, as duas URLs e os dois hashes,
   que estão no `SHA256SUMS` da release.
2. Mudar `bundle`, o identificador do bundle.
3. No YANC, trocar também o `ref:` do passo "YANC (fonte dos testes)" em
   `.github/workflows/installers.yml` pelo mesmo `commit` de
   `bundle/versions.json`. Esse passo baixa o fonte do YANC para
   `vendor/yanc`, de onde `crates/lace-core/tests/yanc_regression.rs` lê os
   casos de teste do próprio YANC; com outro commit, o CI compara o
   compilador novo com os casos de teste antigos.
4. Montar o bundle e rodar os testes
   (`LACE_TEST_BUNDLE="$PWD/dist/toolchain" cargo test`; ver a seção 7).

Nada no código do Lace muda com a versão, a menos que um pacote mude de
estrutura (nome de diretório, forma dos lançadores). Se um caminho de
`bundle/components.json` sumir do pacote, ou um binário pedir uma biblioteca
que o pacote não tem, o `bundle.py` falha dizendo qual.

## 7. Testes

```
export LACE_TEST_BUNDLE="$PWD/dist/toolchain"
cargo test
LACE_TEST_GUI=1 cargo test --test tools surfer     # abre uma janela do surfer-aurora
```

`LACE_TEST_BUNDLE` precisa ser um caminho absoluto. O `cargo test` roda
cada teste no diretório do crate (`crates/lace-core`, `crates/lace-cli`),
e um caminho relativo como `dist/toolchain` é procurado lá: os testes do
`lace-core` falham com "LACE_TEST_BUNDLE não é um bundle válido", mesmo
com o bundle no lugar certo a partir da raiz do repositório. O fonte do YANC
para `yanc_regression` vem de `vendor/yanc`, no commit de
`bundle/versions.json`, ou de `LACE_TEST_YANC_SRC`.

Sem `LACE_TEST_BUNDLE`, os testes que dependem de ferramenta avisam e passam.
Com `CI` definido, a ausência é falha.
