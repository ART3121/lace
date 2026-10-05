# 0005. Visual neutro, sem a paleta da AURORA

- **Status:** Aceita, emendada pela [0007](0007-azul-do-cern-nos-detalhes.md)
  (o destaque passa a ser o azul do CERN) e pela
  [0008](0008-a-marca-do-lace.md) (o logo passa a ser a marca do Lace) e
  pela [0010](0010-temas.md) (o visual neutro passa a ser o do tema padrão,
  Atlas e Atlas Branco, e as cores da AURORA voltam só no tema Aurora
  Legacy)
- **Data:** 2026-10-04

## Contexto

A AURORA tem uma identidade visual própria: fundo azulado escuro, destaque
violeta, menta, ciano e rosa de apoio, tela de abertura e fundo animado. O
Studio é outra ferramenta, e o pedido para ele foi um visual escuro neutro,
sem enfeite.

## Decisão

- Cinzas sem matiz, do fundo ao realce, nos temas escuro e claro
  (`src/styles/tokens.css`). Nenhuma cor da AURORA em lugar nenhum:
  interface, logo, tema do editor, consoles e gramáticas.
- O destaque (foco, seleção, botão principal) é um cinza claro, não uma
  cor. Cor só onde informa algo: erro, aviso, sucesso e o estado das
  operações.
- Sem gradiente, sem animação decorativa, sombra só em menus e diálogos.
- A sintaxe usa poucos tons foscos (aço, sálvia, areia, argila) sobre
  cinza.
- O logo é um L desenhado como trilha entre dois pinos, monocromático.

## Consequências

- Toda cor nova entra como token em `tokens.css`; os temas do Monaco e dos
  consoles repetem os valores e mudam junto.
- Os nomes dos botões e dos consoles continuam os da AURORA (C±, Verilog,
  Wave, PRISM): a familiaridade vem dos nomes e das teclas, não das cores.
