# 0008. A marca do Lace no Studio

- **Status:** Aceita
- **Data:** 2026-10-04
- **Emenda:** [0005](0005-visual-neutro.md), no ponto do logo

## Contexto

A 0005 desenhou um logo provisório para o Studio: um L como trilha entre dois
pinos, cinza. Depois chegou a marca do Lace: um símbolo de oito traços
entrelaçados em octógono, um nome em letras e a composição dos dois, em
azul-marinho `#2E4374`, creme `#F4EFE6` e quase preto `#15171B`.

Em 2026-10-04 o símbolo foi trocado. A crítica foi que o octógono
entrelaçado lembrava a marca da OpenAI; das direções propostas, entrou a
torção: dois fios que entram paralelos, trocam de posição duas vezes e saem
invertidos. As cores e o nome em letras continuam os mesmos.

## Decisão

- O Studio usa a marca do Lace no lugar do L. Os arquivos ficam em
  `public/brand/`, como vieram, sem recolorir. O `lace-icon.svg` da torção
  não veio pronto: foi montado aqui com o mesmo quadrado do ícone anterior
  e os traços de `lace-mark-reverse.svg`.

  | Arquivo | O que é | Onde aparece |
  |---|---|---|
  | `lace-icon.svg` | símbolo creme sobre quadrado azul-marinho, a 78% do quadrado | favicon, ícones do app, aba Sobre |
  | `lace-mark.svg` | símbolo azul-marinho, sem fundo | barra de menus e boas-vindas no tema claro |
  | `lace-mark-reverse.svg` | símbolo creme, sem fundo | barra de menus e boas-vindas no tema escuro |
  | `lace-wordmark.svg` | o nome "Lace" em quase preto | ainda não usado |
  | `lace-lockup.svg` | símbolo e nome lado a lado, para fundo claro | ainda não usado |
  | `lace-lockup-reverse.svg` | símbolo em `#9BB0E8` e nome creme, para fundo escuro | ainda não usado |

- O componente `LaceMark` (`src/components/common.tsx`) escolhe o símbolo
  pelo tema.
- Os ícones do app (`src-tauri/icons/`) saem de `lace-icon.svg` pelo
  `tauri icon` (ver [DEVELOPMENT.md](../DEVELOPMENT.md), seção 6).

## Consequências

- As cores da marca não são tokens da interface. O azul-marinho não é da
  paleta do CERN e não entra em destaque, botão ou linha; a regra da 0007
  continua valendo para a interface.
- O nome em letras sozinho só serve em fundo claro (o texto é `#15171B`);
  no escuro, a composição tem a versão `lace-lockup-reverse.svg`.
- A torção continua legível no tamanho da barra de menus (16 px), o que o
  octógono entrelaçado não era.
