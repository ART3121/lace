# 0003. Um editor Monaco, um modelo por arquivo

- **Status:** Aceita, emendada pela [0009](0009-editor-dividido-em-grupos.md)
- **Data:** 2026-10-03

## Contexto

A AURORA usava o Monaco, com gramáticas próprias de C± e do assembly do
SAPHO. Uma IDE abre muitos arquivos; criar um editor por aba gasta memória e
deixa a troca de aba lenta.

## Decisão

- O Monaco, empacotado pelo Vite com os workers locais (nada vem de CDN:
  o Studio funciona sem rede).
- Uma instância só do editor, que fica montada e só se esconde atrás de uma
  vista. Cada arquivo aberto é um modelo identificado pelo caminho; trocar
  de aba troca o modelo e restaura cursor e rolagem.
- As gramáticas de C± e do assembly foram portadas da AURORA e conferidas
  contra os léxicos do YANC (`CMMComp.l`, `ASMComp.l`, `isa.tsv`). Onde a
  AURORA divergia do YANC, vale o YANC, com um comentário no código.
- Gravar envia o horário de modificação lido ao abrir; o backend recusa se
  o arquivo mudou por fora (`conflict`).

## Consequências

- O pacote da interface passa de 4 MB, quase tudo Monaco. Num aplicativo de
  desktop isso só pesa no tamanho do instalador.
- Diagnósticos de arquivos que ainda não estão abertos ficam guardados e
  viram marcadores quando o arquivo abrir.
- Dividir o editor (a AURORA tinha até 3 painéis) vai precisar de mais de
  uma instância, cada uma com o seu modelo ativo: a decisão de "uma
  instância" muda nesse ponto (Fase 2).
