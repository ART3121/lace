# Decisões de arquitetura (ADRs)

Uma ADR (Architecture Decision Record) registra uma decisão que dá forma ao
Lace: o contexto em que foi tomada, o que se decidiu e o que isso custa.
Serve para quem chega depois entender por que o código é assim antes de
mudá-lo. O formato é o de Michael Nygard: Contexto, Decisão, Consequências.

## Quando escrever uma

Quando a decisão for cara de desfazer: muda um formato de arquivo, a forma
como as ferramentas rodam, a fronteira entre o Core e as interfaces, ou cria
uma regra que todo o código vai ter de seguir. Escolha pequena, que se
desfaz em um commit (nome de função, organização de um módulo, uma flag a
mais numa ferramenta), não precisa de ADR: basta a documentação do código e
a descrição da mudança.

## Como escrever

Copie o modelo para `NNNN-titulo-curto.md`, com o próximo número livre, e
acrescente a ADR na lista abaixo. Meia página a uma página. Todo fato sobre
o código vem com o arquivo e a função onde ele pode ser conferido.

```markdown
# NNNN. Título da decisão

- **Status:** Aceita
- **Data:** AAAA-MM-DD

## Contexto

O problema e as forças em jogo, sem a solução.

## Decisão

O que se decidiu, com os arquivos e as funções onde a decisão está no código.

## Consequências

O que fica mais fácil, o que fica mais difícil e o que passa a ser regra.
```

## Como substituir uma

Uma ADR aceita não se apaga nem se reescreve. Para mudar a decisão,
escreve-se uma ADR nova, que cita a antiga no Contexto e diz o que mudou. Na
antiga, só o status muda, para `Substituída pela [NNNN](NNNN-titulo.md)`. A
lista abaixo mostra as duas.

## Lista

| ADR | Decisão | Status |
|---|---|---|
| [0001](0001-biblioteca-com-interfaces-finas.md) | Toda regra no `lace-core`; CLI, GUI e extensão de editor são cascas | Aceita |
| [0002](0002-so-ferramentas-do-bundle.md) | Só ferramentas do bundle, com ambiente vazio; exceção do compilador do Verilator | Aceita; Windows substituído pela 0009 |
| [0003](0003-formato-spf-da-aurora.md) | O projeto é o `.spf` da AURORA, preservando o que o Lace não entende | Aceita; caminhos de fora da raiz emendados pela 0012 |
| [0004](0004-verilog-e-a-base-sapho-e-um-fluxo-dentro-dele.md) | Verilog é a base; processadores SAPHO são compilados antes e trazem a biblioteca SAPHO | Aceita |
| [0005](0005-classificacao-de-arquivos-pela-regra-da-aurora.md) | Módulo ou testbench pelo conteúdo, com a pontuação da AURORA | Aceita |
| [0006](0006-um-bundle-por-plataforma-dividido-por-ferramenta.md) | Um bundle por plataforma, com o OSS CAD Suite dividido por ferramenta | Aceita; Windows substituído pela 0009 |
| [0007](0007-cancelamento-e-saida-ao-vivo.md) | Cancelamento, prazo e saída ao vivo pelo `Control`; encerrar é encerrar a árvore de processos | Aceita |
| [0008](0008-contrato-do-json-gerado-dos-tipos.md) | O schema do `--json` sai dos tipos e é conferido por teste | Aceita |
| [0009](0009-windows-com-o-bloco-msys2-do-lace-toolchain.md) | No Windows, Icarus e Verilator (com g++, make e Perl) vêm do bloco MSYS2 do lace-toolchain; Linux e macOS acompanham o OSS CAD Suite | Aceita |
| [0010](0010-relatorio-e-historico-de-cada-operacao.md) | Cada operação grava um relatório no histórico do projeto (`lace_core::history`); `lace report` mostra e compara | Aceita |
| [0011](0011-layout-do-surfer-gerado-pelo-core.md) | O Core gera o layout do Surfer dos processadores SAPHO (variáveis, assembly, linha do C±) a partir das tabelas do YANC; o cliente web do Surfer vem no bundle | Aceita |
| [0012](0012-caminhos-fora-da-raiz-e-resgate.md) | Fora da raiz, caminho relativo quando no mesmo repositório git; absoluto de outra máquina resgatado pela cauda, como a AURORA | Aceita |
| [0013](0013-studio-no-repositorio-e-no-bundle.md) | O Lace Studio em `studio/` deste repositório, com a versão do Lace, e no bundle como o componente `studio` (instalador, `lace install studio`, atalho no menu) | Aceita |
