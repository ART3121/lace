# Lace Studio

O ambiente gráfico do [Lace](https://github.com/ART3121/lace) para projetos
Verilog e processadores SAPHO. Substitui a AURORA: cria e abre projetos
`.spf`, edita C±, C e Verilog, compila pelo YANC, verifica e simula com
Icarus Verilog e Verilator, sintetiza com Yosys, desenha o esquemático com
Graphviz, abre a onda no surfer-aurora e guarda o relatório de cada
operação.

É um aplicativo [Tauri 2](https://tauri.app): o backend em Rust usa o
`lace-core` como biblioteca, sem passar pela linha de comando, e a interface
é React com o editor Monaco. A regra de negócio continua toda no Lace; o
Studio só pede e mostra.

Estado: **0.1.0, em desenvolvimento.** O que já funciona e o que falta para
cobrir tudo o que a AURORA fazia está em
[docs/AURORA_PARITY.md](docs/AURORA_PARITY.md).

## Como é

```
┌ Arquivo Editar Exibir Projeto Fluxo Ferramentas Ajuda ───────────────────────┐
│ [Novo][Abrir][Salvar] │ Alvo [Projeto ▾] │ C± Verilog Wave Rápida Onda PRISM │ Icarus|Verilator │
├──┬──────────────────┬────────────────────────────────────────────────────────┤
│  │ EXPLORADOR       │ contador.v │ contador_tb.v │ Esquemático                 │
│E │ Fontes|Hierarquia|Arquivos                                                │
│F │ MÓDULOS          │   module contador(input clk, ...);                     │
│B │  rtl/contador.v  │                                                        │
│R │   topo: contador │                                                        │
│  │ TESTBENCHES      ├────────────────────────────────────────────────────────┤
│  │  contador_tb.v   │ C±  ASM  VERILOG  WAVE  PRISM  LACE  PROBLEMAS TERMINAL│
│T │ PROCESSADORES    │ Simulando...                                           │
│P │  soma  C± compilado│ q = 10                                               │
├──┴──────────────────┴────────────────────────────────────────────────────────┤
│ contador  Topo: contador  TB: contador_tb │ Último: simulação ok │ Ln 3, Col 1 │
└──────────────────────────────────────────────────────────────────────────────┘
```

Na barra de atividades, à esquerda: Explorador (E), Fluxo (F), Busca (B),
Relatórios (R), Ferramentas do Lace (T) e Preferências (P).

A barra de ferramentas segue a AURORA (C± F6, Verilog F7, Wave F8, Rápida
F9, PRISM F10, Parar Shift+F5); o resto segue o VS Code (Ctrl+P, Ctrl+Shift+P,
Ctrl+B, Ctrl+J, Ctrl+`). O navegador de fluxo da barra lateral é o do
Vivado: as etapas em ordem, com o estado da última execução de cada uma.

O explorador tem três modos: *Fontes* (módulos, testbenches e processadores,
com o Verilog e o testbench que o YANC gera em Módulos e Testbenches),
*Hierarquia* (a árvore de instâncias elaborada pelo Icarus, do design e de
cada testbench) e *Arquivos* (a pasta, com arrastar para mover e soltar
arquivos do sistema). O editor tem modo Vim. O visual é cinza neutro, com o
azul do CERN só nos detalhes.

## Requisitos

- O Studio vem no instalador do Lace, como o componente `studio`, marcado na
  instalação Recommended ([instalação](../README.md#instalar)); numa
  instalação sem ele, `lace install studio`. Instalado assim, ele fica no
  bundle (`toolchain/studio/`) e usa o bundle em que está. Fora do bundle
  (o `npm run tauri dev`), acha a instalação do Lace sozinho
  (`~/.local/share/lace` no Linux e no macOS, `%LOCALAPPDATA%\Programs\Lace`
  no Windows) ou pela pasta indicada nas preferências.
- No Linux, o webkit2gtk 4.1 do sistema (`libwebkit2gtk-4.1-0` no Debian e
  no Ubuntu, `webkit2gtk4.1` no Fedora); no Windows, o WebView2, que vem com
  o Windows 10 e o 11.
- Para compilar o Studio: Rust 1.90 ou mais novo, Node.js 22 e as
  dependências de sistema do Tauri. O `lace-core` vem deste mesmo
  repositório (`../crates/lace-core`). Detalhes em
  [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Rodar a partir do fonte

```sh
git clone https://github.com/ART3121/lace
cd lace/studio
npm install
npm run tauri dev                                      # abre o Studio, com recarga ao salvar
```

O Studio que vai para os usuários sai do instalador do Lace: o
`scripts/bundle.py` da raiz compila o `studio/` (`tauri build`) e o põe no
bundle. Só o Studio, sem o resto do bundle:

```sh
python3 ../scripts/bundle.py --out /tmp/bundle --only studio
```

Testes:

```sh
npm run typecheck                       # TypeScript
cd src-tauri && cargo test              # Rust, inclusive os fluxos contra o bundle instalado
```

## Documentação

| Documento | Para quê |
|---|---|
| [docs/USER_GUIDE.md](docs/USER_GUIDE.md) | usar o Studio: a janela, os fluxos Verilog e SAPHO, consoles, atalhos, preferências, problemas comuns |
| [docs/AURORA_PARITY.md](docs/AURORA_PARITY.md) | cada recurso da AURORA e onde está no Studio, ou o que falta; o plano das próximas fases |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | como o Studio é feito: backend, interface, operações, estado, segurança |
| [docs/IPC.md](docs/IPC.md) | a referência de cada comando entre a interface e o backend |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | preparar a máquina, rodar, testar, onde mudar o quê |
| [CHANGELOG.md](CHANGELOG.md) | mudanças por versão |

NIPS-CERN, Núcleo de Instrumentação e Processamento de Sinais, Faculdade de
Engenharia da UFJF. <https://nipscern.com>
