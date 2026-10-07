// O formato de um tema e as contas de cor que os temas usam. Fica separado
// de index.ts para o catálogo importar daqui sem ciclo.

export type Scheme = 'dark' | 'light';

/**
 * As cores da interface. Cada campo vira uma variável CSS, com o nome em
 * kebab-case (`bg0` → `--bg-0`, `brandFillHover` → `--brand-fill-hover`).
 * Os opcionais têm padrão por esquema, em `uiTokens`.
 */
export interface UiColors {
  /** Barras: menus, ferramentas, atividades, cabeçalho dos grupos, status. */
  bg0: string;
  /** Corpo e barra lateral. */
  bg1: string;
  /** Menus, diálogos, avisos, avisos flutuantes, listas de escolha. */
  bg2: string;
  /** Botões e controles segmentados. */
  bg3: string;
  /** Botão sob o mouse, segmento ativo, seleção de texto. */
  bg4: string;
  /** Editor, aba ativa e painel inferior. */
  bgEditor: string;
  border: string;
  borderStrong: string;
  /** Texto, do mais forte (títulos) ao mais fraco (desabilitado). */
  text0: string;
  text1: string;
  text2: string;
  text3: string;
  /** O destaque do tema em linhas e indicadores (foco, aba ativa). */
  brand: string;
  /** O destaque em texto. */
  brandText: string;
  /** O destaque como preenchimento (botão principal, caixa marcada). */
  brandFill: string;
  brandFillHover: string;
  /** O texto sobre `brandFill`. */
  brandContrast: string;
  /** Fundo translúcido do destaque. Padrão: `brandFill` a 28% (escuro) ou 10% (claro). */
  brandSoft?: string;
  /** Cores com função. Os fundos `*-soft` saem delas, a 12% ou 10%. */
  ok: string;
  warn: string;
  error: string;
  info: string;
  hover?: string;
  active?: string;
  overlay?: string;
  scrollbar?: string;
  shadowPop?: string;
  /** O botão de fechar da barra de título integrada sob o mouse, e o X
   * nele. Padrão: o vermelho do Windows e branco, em todos os temas, como
   * no sistema. */
  windowClose?: string;
  windowCloseText?: string;
}

/** Os papéis da sintaxe. Cada um vira regras de token do Monaco. */
export interface SyntaxColors {
  /** O texto sem papel (identificadores). */
  fg: string;
  comment: string;
  keyword: string;
  /** Palavras de controle de fluxo (`keyword.flow`) e saltos do assembly. */
  control: string;
  type: string;
  /** Funções da biblioteca e predefinidas. */
  func: string;
  number: string;
  string: string;
  /** Constantes, nomes de `#define`, rótulos do assembly. */
  constant: string;
  /** Variáveis, chaves de JSON, nomes de atributo. */
  variable: string;
  /** Tags de XML e Markdown. */
  tag: string;
  operator: string;
  delimiter: string;
  /** Diretivas: `#PRNAME`, `#define`, `` `timescale``. */
  directive: string;
  /** O estilo da fonte de alguns papéis (`italic`, `bold`, `italic bold` ou
   * `''`). Padrão: comentário em itálico, diretiva em negrito. */
  style?: Partial<Record<'comment' | 'keyword' | 'type' | 'func' | 'constant' | 'directive', string>>;
}

/** As cores do editor que não saem da interface nem da sintaxe. */
export interface EditorColors {
  selection: string;
  /** Padrão: `selection` com metade da opacidade. */
  selectionInactive?: string;
  /** Fundo da linha do cursor; `#00000000` para só a borda. */
  lineHighlight: string;
  lineHighlightBorder?: string;
  lineNumber: string;
  lineNumberActive: string;
  cursor: string;
  findMatch: string;
  findMatchHighlight: string;
  indentGuide: string;
  indentGuideActive: string;
  /** Padrão: `bg4` com borda `text3`. */
  bracketMatch?: string;
  bracketMatchBorder?: string;
  /** As cores dos pares de parênteses por nível (até 6, repetidas em
   * ciclo). Sem elas, todos os níveis ficam na cor dos operadores. */
  brackets?: string[];
}

/** As cores dos terminais, com os nomes do xterm.js. */
export interface TerminalColors {
  /** Padrão: `bgEditor`, o fundo do painel. */
  background?: string;
  foreground: string;
  selection: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  /** O apagado dos consoles (`\x1b[90m`): precisa ser legível sobre o fundo. */
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

/** Uma regra de token do Monaco, sem importar o Monaco aqui. */
export interface TokenRule {
  token: string;
  foreground?: string;
  fontStyle?: string;
}

export interface Theme {
  /** O que fica gravado em `settings.theme`. Não mudar depois de lançado. */
  id: string;
  /** Nome próprio, igual em todos os idiomas. */
  name: string;
  scheme: Scheme;
  /** O tema do outro esquema na mesma família, para "Alternar tema claro e escuro". */
  pair?: string;
  ui: UiColors;
  syntax: SyntaxColors;
  editor: EditorColors;
  terminal: TerminalColors;
  /** Regras de token por cima das que saem de `syntax`, para um tema que
   * pinta uma gramática do seu jeito (o Aurora Legacy, com o Dirac; o Atlas,
   * com o module do Verilog). */
  rules?: TokenRule[];
}

// Cores -------------------------------------------------------------------

function channels(hex: string): [number, number, number] {
  const value = hex.replace('#', '');
  return [0, 2, 4].map((i) => Number.parseInt(value.slice(i, i + 2), 16)) as [number, number, number];
}

function toHex(rgb: number[]): string {
  return `#${rgb.map((c) => Math.round(Math.min(255, Math.max(0, c))).toString(16).padStart(2, '0')).join('')}`.toUpperCase();
}

/** `#RRGGBB` com opacidade (0 a 1), como `#RRGGBBAA`. Serve ao CSS e ao Monaco. */
export function alpha(hex: string, opacity: number): string {
  return `${hex.slice(0, 7)}${Math.round(opacity * 255).toString(16).padStart(2, '0')}`.toUpperCase();
}

/** A mistura de duas cores `#RRGGBB`: `amount` 0 é `a`, 1 é `b`. */
export function mix(a: string, b: string, amount: number): string {
  const [ca, cb] = [channels(a), channels(b)];
  return toHex(ca.map((c, i) => c + (cb[i]! - c) * amount));
}

function luminance(hex: string): number {
  const [r, g, b] = channels(hex).map((c) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** O contraste entre duas cores `#RRGGBB`, pela fórmula do WCAG (1 a 21). */
export function contrast(a: string, b: string): number {
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (high + 0.05) / (low + 0.05);
}

/** `color` levada na direção de `toward`, em passos de 5%, até ter pelo
 * menos `minimum` de contraste com `background`. */
export function legible(color: string, toward: string, background: string, minimum: number): string {
  for (let step = 0; step <= 20; step++) {
    const candidate = mix(color, toward, step / 20);
    if (contrast(candidate, background) >= minimum) return candidate;
  }
  return toward;
}

/** A regra de token do Monaco para uma cor `#RRGGBB` (o Monaco quer sem `#`
 * e sem opacidade). */
export function tokenRule(token: string, color: string, fontStyle?: string): TokenRule {
  return { token, foreground: color.slice(1, 7), fontStyle };
}
