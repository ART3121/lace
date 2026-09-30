# O bundle de ferramentas

O Solar só executa ferramentas de um bundle instalado junto com ele. Nunca do
`PATH`, nunca de um caminho configurado pelo usuário. A exceção, decidida pelo
autor, é o compilador C++ do Verilator (seção 4).

## 1. O que vem no bundle

Bundle `2026.09.29`. Os pacotes, com as versões fixadas em
`bundle/versions.json`:

| Pacote | Versão | O que traz | De onde vem |
|---|---|---|---|
| YANC | v5.6 (`e1ad149`) | `cmmcomp`, `appcomp`, `asmcomp`, `cpppp`, `cppcomp`, a biblioteca SAPHO, macros e headers | compilado do fonte pelo empacotamento |
| surfer-aurora | v0.7.0-nips.10 (`d0af8a7`) | o fork do Surfer da AURORA | compilado do fonte pelo empacotamento |
| OSS CAD Suite | release 2026-09-29 | Icarus Verilog, Verilator, Yosys e, fora do Windows, o `dot` do Graphviz | pacote oficial, conferido pelo SHA-256 publicado |
| Graphviz | 16.1.0 | `dot` | só no Windows (o OSS CAD Suite de Windows não traz), zip oficial conferido pelo SHA-256 publicado |

Os componentes, que o instalador deixa escolher, estão em
`bundle/components.json`:

| Componente | Pacote | Diretório |
|---|---|---|
| `yanc` | YANC | `yanc/` |
| `icarus` | OSS CAD Suite | `oss-cad-suite/` |
| `verilator` | OSS CAD Suite | `oss-cad-suite/` |
| `yosys` | OSS CAD Suite | `oss-cad-suite/` |
| `graphviz` | OSS CAD Suite (Linux, macOS), Graphviz (Windows) | `oss-cad-suite/` ou `graphviz/` |
| `surfer-aurora` | surfer-aurora | `surfer-aurora/` |

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
141 MiB e a do Windows 340 MiB, os dois sem o surfer-aurora (medidos na
montagem local; o CI monta os completos).

Fica de fora de propósito: os plugins do Yosys (`share/yosys/plugins`, que
puxariam o GHDL), o `site-packages` e os testes do Python, e as cópias de
depuração do Verilator.

O OSS CAD Suite compila cada ferramenta do `master` e publica uma release por
dia. Fixar a data e o hash do pacote é o que dá a versão exata. Na release
2026-09-29 as ferramentas se identificam assim:

| Ferramenta | Versão reportada (Linux x64) |
|---|---|
| Yosys | 0.69+156 (git 9d0c91b23) |
| Icarus Verilog | 14.0 (devel) s20260301-500-g2e81fcccb |
| Verilator | 5.053 devel rev v5.052-240-g640607a0d |
| dot | Graphviz 2.43 (20190912) |

## 2. Plataformas

| Plataforma | Nome no bundle | Observação |
|---|---|---|
| Linux x64 | `linux-x64` | |
| macOS Apple Silicon | `darwin-arm64` | O OSS CAD Suite avisou que vai parar de publicar o macOS Intel, que o Solar não suporta. |
| Windows 10 e 11 x64 | `windows-x64` | |

Um bundle só abre na plataforma para a qual foi montado.

## 3. Onde fica e como o Solar o usa

```
<instalação>/
  bin/solar[.exe]
  toolchain/
    bundle.json               formato, versão do bundle, plataforma
    components/<nome>.json    um por componente instalado
    yanc/                     bin/ SAPHO/ Macros/ Header/
    oss-cad-suite/            a parte do OSS CAD Suite dos componentes instalados
    surfer-aurora/            surfer-aurora[.exe]
    graphviz/                 só no Windows
```

O Solar procura `toolchain/` ao lado de `bin/` (ou ao lado do executável,
com os symlinks resolvidos).
`solar --toolchain <DIR>` usa outro bundle, para desenvolvimento; ele também
precisa ter `bundle.json`.

Cada `components/<nome>.json` tem a versão, a origem e o SHA-256 de cada
executável do componente que o Solar roda. `solar tools --verify` confere
esses hashes. Um componente sem arquivo em `components/` não está instalado.

Como cada ferramenta é executada:

| Ferramenta | Linux e macOS | Windows |
|---|---|---|
| YANC | o binário em `yanc/bin/` | `yanc/bin/*.exe` |
| Icarus, Yosys, `dot` | `/bin/bash oss-cad-suite/bin/<ferramenta>`: o lançador do pacote, que carrega o binário de `libexec/` com as bibliotecas do próprio pacote | o `.exe` de `oss-cad-suite/bin/` (ou `graphviz/bin/dot.exe`) com `PATH` em `bin;lib` do pacote, como o `environment.bat` |
| Verilator | Perl do sistema com o script `oss-cad-suite/bin/verilator`, que roda o `bin/verilator_bin` do pacote; o `make` gerado usa o Python do pacote (`PYTHON3=`) | igual, com o Perl do MSYS2, o `bin/verilator_bin.exe` e o `lib/python3.exe` do pacote |
| surfer-aurora | o binário, com as variáveis de display | o `.exe` |

Todo processo parte de um ambiente vazio. Os lançadores recebem
`PATH=/usr/bin:/bin` para o `bash`, o `dirname` e o `readlink` que usam. No
Windows, o `PATH` das ferramentas do pacote termina em
`%SystemRoot%\System32`, e o `ComSpec` é repassado: o `iverilog` roda o
`ivlpp` e o `ivl` pelo `system()` da biblioteca C, que usa o `cmd.exe`.

O Solar recusa um bundle em que um componente, ou um executável, seja um
symlink para fora dele. E recusa rodar o Verilator se faltar o
`bin/verilator_bin` do pacote: sem ele, o script Perl procuraria um
`verilator_bin` no `PATH`, que no Linux e no macOS inclui `/usr/bin`.

O `dot` do Linux recebe um `fonts.conf` gravado pelo Solar na pasta da
síntese, a partir do modelo que o pacote traz (como fazem os lançadores do
`xdot` e do `gtkwave`), e usa as fontes do pacote.

## 4. O que vem do sistema

| O quê | Por quê | Onde o Solar procura |
|---|---|---|
| `/bin/bash`, `dirname`, `readlink` (Linux e macOS) | os lançadores do OSS CAD Suite são scripts bash; é o jeito oficial de rodar o pacote | fazem parte do sistema base |
| `libc` (Linux e macOS) | o `solar` e o YANC são binários nativos ligados à `libc` do sistema; as ferramentas do OSS CAD Suite não, carregam as bibliotecas do pacote | sistema base |
| fontes (macOS e Windows) | os pacotes dessas plataformas não trazem fontes; o `dot` usa as do sistema | as do sistema |
| compilador C++, `make`, Perl | exceção decidida pelo autor: o Verilator compila o modelo em C++, e o OSS CAD Suite não traz compilador | locais fixos: `/usr/bin` (Linux); `/usr/bin` com as Command Line Tools do Xcode (macOS); MSYS2 em `C:\msys64` ou `C:\tools\msys64`, com `ucrt64` ou `mingw64` (Windows) |

Sem o compilador, tudo funciona menos a simulação com Verilator, e
`solar tools` avisa. Para um compilador fora do local padrão:

```
solar config set-compiler /opt/gcc/bin        # Linux, macOS: diretório com perl, make e g++/clang++
solar config set-compiler D:\msys64           # Windows: raiz do MSYS2
```

Instalar o compilador:

| Sistema | Comando |
|---|---|
| Debian, Ubuntu | `sudo apt install build-essential perl` |
| Fedora | `sudo dnf install gcc-c++ make perl` |
| macOS | `xcode-select --install` |
| Windows | MSYS2, e no shell dele: `pacman -S make perl mingw-w64-ucrt-x86_64-gcc` |

## 5. Limitações conhecidas

- **Verilator grava VCD, não FST.** O FST do Verilator 5.053 compila contra
  lz4 e zlib, que não vêm no pacote nem fazem parte da exceção do compilador.
  O Icarus continua gravando FST.
- **O `dot` do OSS CAD Suite é antigo** (Graphviz 2.43, de 2019). Funciona
  para o esquemático; o do Windows é o 16.1.0.
- **O esquemático depende das fontes no macOS e no Windows.** Lá o `dot`
  mede o texto com as fontes do sistema, e o SVG pode variar de máquina para
  máquina. No Linux usa as do bundle.
- **Esquemático diferente da AURORA.** O Solar desenha com o `show` do Yosys e
  o `dot`; a AURORA, com o netlistsvg e skins próprios.
- **YANC e surfer-aurora são compilados pelo empacotamento.** O YANC não
  publica build de macOS e o surfer-aurora só publica o de Windows.

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
Solar só lê o bundle.

`--platform` monta a divisão de outra plataforma: os pacotes prontos se
dividem em qualquer sistema; YANC e surfer-aurora precisam compilar para ela.

Precisa, além de Python 3.9+ e `git`:

| Sistema | Para o YANC | Para o surfer-aurora |
|---|---|---|
| Linux | `gcc`, `make`, `flex`, `bison` | `cargo` |
| macOS | Command Line Tools, `flex` e `bison` do Homebrew (`YANC_MAKE_ARGS="BISON=$(brew --prefix bison)/bin/bison FLEX=$(brew --prefix flex)/bin/flex"`) | `cargo` |
| Windows | shell MINGW64 do MSYS2 com `mingw-w64-x86_64-gcc`, `make`, `flex`, `bison` | `cargo` |

O workflow `.github/workflows/installers.yml` monta o bundle e os
instaladores nas três plataformas, instala por eles e roda os testes contra
a instalação.

### Atualizar uma versão

1. Trocar a versão e o hash em `bundle/versions.json` (em `packages`). O SHA-256 dos pacotes
   do OSS CAD Suite está no campo `digest` da API de releases do GitHub; o do
   Graphviz, no arquivo `.sha256` ao lado do zip.
2. Mudar `bundle`, o identificador do bundle.
3. Montar o bundle e rodar os testes (`SOLAR_TEST_BUNDLE=dist/toolchain cargo test`).

Nada no código do Solar muda com a versão, a menos que um pacote mude de
estrutura (nome de diretório, forma dos lançadores). Se um caminho de
`bundle/components.json` sumir do pacote, ou um binário pedir uma biblioteca
que o pacote não tem, o `bundle.py` falha dizendo qual.

## 7. Testes

```
export SOLAR_TEST_BUNDLE=dist/toolchain
cargo test
SOLAR_TEST_GUI=1 cargo test --test tools surfer     # abre uma janela do surfer-aurora
```

Sem `SOLAR_TEST_BUNDLE`, os testes que dependem de ferramenta avisam e passam.
Com `CI` definido, a ausência é falha.
