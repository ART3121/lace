# 0010. Temas, com o Atlas de padrão

- **Status:** Aceita
- **Data:** 2026-10-04
- **Emenda:** [0005](0005-visual-neutro.md) e [0007](0007-azul-do-cern-nos-detalhes.md),
  que passam a descrever o tema padrão, e não a interface inteira

## Contexto

O Studio tinha um tema escuro e um claro, os dois neutros (0005), com o
azul do CERN nos detalhes (0007). As cores estavam em três lugares que
precisavam mudar juntos: `styles/tokens.css`, o tema do Monaco em
`editor/monaco.ts` e o dos consoles em `console/consoles.ts`.

O pedido foi ter temas como as IDEs costumam ter: o atual como padrão, com
o nome de Atlas, a versão clara como Atlas Branco, o Dark Modern, um Aurora
Legacy com as cores da AURORA e os temas conhecidos, como Dracula e Gruvbox.

## Decisão

- Um tema é um objeto (`Theme`, em `src/themes/model.ts`) com quatro
  partes: as cores da interface (`ui`, que viram as variáveis CSS), os
  papéis da sintaxe (`syntax`: palavra-chave, tipo, string, número,
  comentário, diretiva e outros), as cores do editor (`editor`) e as dos
  terminais (`terminal`). Desse objeto saem as variáveis CSS
  (`applyThemeCss`), o tema do Monaco (`setMonacoTheme`) e o do xterm.js
  (`terminalTheme`). Nenhuma cor de tema fica fora de `src/themes/`.
- As gramáticas de C± e do assembly pintam os seus tokens próprios pelos
  papéis da sintaxe (`cmmTokenRules(syntax)`), então todo tema cobre C± e
  asm sem regra própria. Um tema pode acrescentar regras (`rules`), como o
  Aurora Legacy faz com a notação de Dirac.
- O catálogo (`src/themes/catalog.ts`) tem 15 temas: Atlas (padrão), Atlas
  Branco, Aurora Legacy, Dark Modern, Light Modern, Dracula, Gruvbox Dark,
  Gruvbox Light, Monokai, Nord, One Dark Pro, Solarized Dark, Solarized
  Light, Catppuccin Mocha e Tokyo Night. A preferência `system` fica com o
  Atlas ou o Atlas Branco, conforme o sistema.
- Atlas e Atlas Branco são o visual das ADRs 0005 e 0007, com os mesmos
  valores de antes.
- O Aurora Legacy é o único lugar com a paleta da AURORA, tirada do
  repositório dela (`css/base/brand_tokens.css`, `theme_variables.css`, o
  tema `cmm-dark` do Monaco e o do terminal). O resto da interface continua
  sem nada da AURORA: logo, telas e nomes não mudam com o tema.
- Os outros temas usam a paleta oficial de cada um, conferida na fonte
  (a tabela está no começo de `catalog.ts`). A interface do Studio tem menos
  camadas que a do VS Code, então os fundos foram escolhidos entre as cores
  do tema, e um tom que faltava sai de uma mistura de duas cores do tema.
- O contraste baixo que alguns temas têm de propósito (o comentário do
  Nord, os tons do Solarized) fica como veio. A exceção é o "apagado" dos
  consoles (ANSI 90), em que as ferramentas escrevem diagnóstico:
  `terminalTheme` o clareia até 3:1 sobre o fundo.
- Escolher: Preferências > Aparência (um cartão por tema, com uma prévia
  desenhada nas cores dele) ou Exibir > Selecionar tema na paleta. Alternar
  tema claro e escuro leva ao par do tema (Atlas e Atlas Branco, Dark e
  Light Modern, Gruvbox, Solarized) ou, sem par, ao Atlas ou Atlas Branco.
- `settings.theme` guarda o id do tema. O padrão passa de `system` para
  `atlas`. Os valores antigos `dark` e `light` viram `atlas` e
  `atlas-light` ao ler o `settings.json`; um id desconhecido vale o Atlas.

## Consequências

- Um tema novo é um objeto a mais em `catalog.ts`; nada mais muda. Um id,
  depois de lançado, não muda, porque fica gravado nas preferências.
- Um componente novo usa as variáveis CSS (`--bg-0`, `--text-1`,
  `--brand`...), nunca cor literal; uma variável nova entra em `UiColors` e
  em todos os temas (o TypeScript cobra).
- O destaque (`--brand*`) só é o azul do CERN no Atlas e no Atlas Branco;
  nos outros temas é o destaque do próprio tema.
- `tokens.css` ficou só com fontes, tamanhos, raios, espaços e alturas.
  Os tokens `--accent*`, que nenhuma regra usava, saíram.
