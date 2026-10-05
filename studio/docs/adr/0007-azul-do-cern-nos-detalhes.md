# 0007. O azul do CERN nos detalhes

- **Status:** Aceita, emendada pela [0010](0010-temas.md) (o azul do CERN é
  o destaque do Atlas e do Atlas Branco; os outros temas têm o seu)
- **Data:** 2026-10-04
- **Emenda:** [0005](0005-visual-neutro.md), no ponto do destaque cinza

## Contexto

A 0005 deixou o destaque (foco, seleção, botão principal) em cinza claro.
O pedido seguinte foi pôr o azul do CERN em detalhes da interface: o Studio
é do NIPS-CERN, da colaboração ATLAS, e o azul da organização identifica a
ferramenta sem trazer de volta a paleta da AURORA.

O azul vem da identidade visual do CERN
(design-guidelines.web.cern.ch/guidelines/colours): Pantone 286 C, CMYK
100 75 0 0, RGB 0 51 160, `#0033A0`, com os tons da paleta oficial
`#385CB4`, `#6B84C5`, `#9BAEDB` e `#CDD6ED`. Sobre o fundo escuro do Studio,
o `#0033A0` numa linha de 1 ou 2 px quase some.

## Decisão

- O resto continua como na 0005: cinzas sem matiz, cor com função para erro,
  aviso e sucesso, nada da AURORA.
- O azul entra só em detalhes: o contorno de foco, a linha da aba ativa e da
  aba do painel, o indicador da barra de atividades e da linha selecionada
  da árvore, o botão principal, a caixa marcada, o texto da aba de modo
  ativa, as marcas de topo e simulado, a seleção do editor e dos consoles, o
  destino de arrastar, o modo do Vim e o indicador de operação rodando.
- Só os valores da paleta oficial. No tema escuro, linhas e textos usam os
  tons claros (`#6B84C5`, `#9BAEDB`), e o `#0033A0` fica para preenchimentos
  com texto branco (botão principal, caixa marcada) e, com transparência,
  para a seleção. No tema claro, o `#0033A0` serve para tudo.
- Os tokens são `--cern-blue*` (os valores) e `--brand*` (o uso), em
  `src/styles/tokens.css`. Os temas do Monaco e dos consoles repetem os
  mesmos valores.

## Consequências

- Uma cor nova de destaque precisa sair dessa paleta; um azul que não é dela
  não entra.
- Fundos grandes continuam cinza: barra de status, barras e painéis não
  ficam azuis.
