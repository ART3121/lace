# Mudanças

## 0.1.0 (2026-09-29)

Primeira versão. Substitui a orquestração da AURORA para o SAPHO.

- `solar-core`: API para criar projetos e processadores (formato `.spf` da
  AURORA), compilar com o YANC (C± e C), simular com Icarus Verilog e
  Verilator, verificar sintaxe e sintetizar com Yosys, desenhar o esquemático
  com o `show` do Yosys e o Graphviz, e abrir ondas no surfer-aurora.
- `solar`: a linha de comando sobre o `solar-core`, com saída em texto ou
  JSON.
- Bundle de ferramentas `2026.09.29`: OSS CAD Suite de 2026-09-29, YANC v5.6,
  surfer-aurora v0.7.0-nips.10 e, no Windows, Graphviz 16.1.0. O Solar só
  executa ferramentas desse bundle; a exceção declarada é o compilador C++, o
  `make` e o Perl do sistema, para o Verilator.
- O OSS CAD Suite é dividido por ferramenta: o bundle completo de Linux tem
  300 MiB, contra os 2,5 GB do pacote inteiro.
- Instaladores: assistente (Inno Setup) no Windows, instalação guiada no
  terminal no Linux e no macOS. Tipo Recomendada por padrão; Avançada para
  escolher os componentes.

Plataformas: Linux x64, macOS Apple Silicon, Windows 10 e 11 x64.
