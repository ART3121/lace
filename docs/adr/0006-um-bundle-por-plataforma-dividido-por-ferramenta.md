# 0006. Um bundle por plataforma, dividido por ferramenta

- **Status:** Aceita; a origem do Icarus e do Verilator no Windows foi substituída pela [0009](0009-windows-com-o-bloco-msys2-do-lace-toolchain.md)
- **Data:** 2026-10-01

## Contexto

Icarus, Verilator, Yosys e o `dot` vêm do OSS CAD Suite, que compila cada
ferramenta do `master` e publica uma release por dia, por plataforma. O
pacote inteiro tem 2,5 GB no Linux: traz também nextpnr, GHDL, GTKWave,
bases de FPGA e um Python com dezenas de pacotes. O Lace usa quatro
ferramentas dele. O YANC não publica build de macOS, e o surfer-aurora só
publica o de Windows (`docs/BUNDLE.md`, seções 1 e 5). A ADR 0002 exige que
tudo saia de um bundle com versões fixas.

## Decisão

Há um bundle por plataforma: `linux-x64`, `darwin-arm64`, `windows-x64`.
`Toolchain::open` recusa um bundle montado para outra.

As versões ficam fixas em `bundle/versions.json`: o OSS CAD Suite por data
de release, com o SHA-256 do pacote de cada plataforma; YANC e surfer-aurora
por commit, compilados pelo empacotamento; o Graphviz 16.1.0 só no Windows,
onde o OSS CAD Suite não traz o `dot`. O campo `bundle` é o identificador do
bundle (hoje `2026.09.29`).

`scripts/bundle.py` é o único lugar que baixa ou compila alguma coisa; em
uso, o Lace só lê o bundle montado. O OSS CAD Suite é dividido por
ferramenta:

- `bundle/components.json` diz o que cada componente leva do pacote
  (`select`, `data`, `exclude`) e quais executáveis o Lace roda (`run`);
- para cada binário selecionado, `bundle.py` acrescenta o fecho das
  bibliotecas dinâmicas que ele carrega (`Package.closure`), lido dos
  próprios binários ELF, Mach-O e PE por `scripts/binaries.py`, que só usa a
  biblioteca padrão do Python;
- uma dependência que não está no pacote e não é do sistema faz o
  empacotamento falhar, com o nome do binário e da biblioteca.

O manifesto tem o formato 2 (`BUNDLE_SCHEMA` em `toolchain.rs`, `SCHEMA` em
`bundle.py`): o cabeçalho `bundle.json` (`schema`, `bundle`, `platform`) e
um `components/<nome>.json` por componente instalado, com versão,
diretório, origem e o SHA-256 de cada executável da lista `run`. Um
componente sem arquivo em `components/` não está instalado. O
`<saída>.contents.json`, que diz de que componente é cada arquivo, serve aos
instaladores e não é instalado.

## Consequências

- O bundle completo tem 300 MiB no Linux, contra 2,5 GB do pacote inteiro;
  no macOS e no Windows, sem o surfer-aurora, 141 MiB e 340 MiB
  (`docs/BUNDLE.md`). A instalação Recomendada no Linux tem 209 MiB
  (`docs/INSTALL.md`).
- Os instaladores deixam escolher componentes. A ferramenta de um componente
  não instalado dá `ComponentMissing` quando usada. Icarus, Verilator, Yosys
  e o `dot` dividem o diretório `oss-cad-suite/` e as bibliotecas em comum.
- `lace tools --verify` confere só os executáveis da lista `run`. Dados e
  bibliotecas não têm hash no manifesto.
- Atualizar uma ferramenta é editar `bundle/versions.json`, trocar o
  `bundle`, montar e rodar os testes (`docs/BUNDLE.md`, seção 6). O código
  do Lace só muda se o pacote mudar de estrutura; aí muda o
  `components.json` e, se o caminho do executável mudar, `Tool::location`.
- Fica de fora de propósito: os plugins do Yosys (puxariam o GHDL), o
  `site-packages` e os testes do Python, e as cópias de depuração do
  Verilator. Um recurso que precise deles começa no `components.json`.
- Montar o bundle exige, em cada plataforma, o que compila YANC e
  surfer-aurora (`gcc`, `make`, `flex`, `bison`, `cargo`; MSYS2 no
  Windows). O `.github/workflows/installers.yml` monta os três a cada push.
