// O catálogo de temas, na ordem em que aparecem nas Preferências.
//
// Os da AURORA IDE são o Boreal, o Austral, o Nebulosa, o Alvorada e o
// Lunar, cada um com o seu claro: os mesmos cinzas e uma cor de destaque por
// paleta, o azul da aurora boreal, o verde da austral, o violeta das
// nebulosas, o âmbar do amanhecer e o prata da lua. O verde e o violeta são
// os da Aurora antiga, separados. O Aurora Legacy traz a paleta inteira da
// AURORA Electron. O VS Code (o Dark e o Light Modern) e o Dracula seguem a
// paleta oficial, conferida na fonte em 2026-10-04:
//
// | Tema                 | Fonte                                                        |
// |----------------------|--------------------------------------------------------------|
// | Aurora Legacy        | ../Zelda-main: css/base/brand_tokens.css, theme_variables.css, js/editor/monaco_editor.js, js/terminal/shell_terminal.js |
// | Dark/Light Modern    | microsoft/vscode, extensions/theme-defaults (dark_modern, light_modern, dark_plus, light_plus) e terminalColorRegistry.ts |
// | Dracula              | dracula/visual-studio-code, src/dracula.yml                  |
//
// A sintaxe e os terminais usam as cores do tema como vieram; o contraste
// baixo que um tema tem de propósito (o comentário do Dracula) fica. A exceção é o apagado dos consoles, que terminalTheme
// (console/consoles.ts) clareia até 3:1. A interface do Studio tem menos
// camadas que a do VS Code, então os fundos foram escolhidos entre as cores
// do próprio tema, e onde faltava um tom (texto secundário, borda forte) ele
// sai de `mix` entre duas cores do tema.

import { alpha, mix, type Scheme, type Theme } from './model';

// AURORA IDE ---------------------------------------------------------------

/**
 * Os cinzas dos temas da AURORA IDE, comuns às três paletas: quase neutros,
 * com um nada de azul (matiz 260 e croma 0,006 no OKLCH). As barras e a barra
 * lateral ficam num tom só e o editor um tom acima; no claro, editor e menus
 * em branco sobre barras quase brancas. Todo texto tem pelo menos 4,5:1 com o
 * fundo do editor, o das barras e o dos menus. Os terminais têm os seis
 * matizes com a mesma luminosidade, com o vermelho e o verde iguais ao erro e
 * ao ok da interface.
 */
const AURORA_DARK = {
  bars: '#101214',
  editor: '#141619',
  raised: '#1C1E21',
  control: '#252729',
  controlHover: '#2F3033',
  border: '#232528',
  borderStrong: '#36383B',
  text0: '#EAEDF1',
  text1: '#BBBEC2',
  text2: '#909296',
  text3: '#595B5E',
  code: '#CFD1D5',
  lineHighlight: '#181A1C',
  lineNumber: '#6D6F72',
  indentGuide: '#1C1E21',
  ok: '#6DC88F',
  warn: '#F2985F',
  error: '#F2716A',
  info: '#79B8ED',
  ansi: {
    black: '#101214',
    red: '#F2716A',
    green: '#6DC88F',
    yellow: '#E2C162',
    blue: '#75AEF5',
    magenta: '#D58ECF',
    cyan: '#26C0CF',
    white: '#CFD1D5',
    brightBlack: '#909296',
    brightRed: '#FFB4AD',
    brightGreen: '#A0DCAC',
    brightYellow: '#F4D88C',
    brightBlue: '#A3CEFF',
    brightMagenta: '#EDB6E7',
    brightCyan: '#7FDCE6',
    brightWhite: '#F4F5F7',
  },
};

const AURORA_LIGHT: typeof AURORA_DARK = {
  bars: '#F5F7F9',
  editor: '#FFFFFF',
  raised: '#FFFFFF',
  control: '#EDEEF1',
  controlHover: '#E4E6EA',
  border: '#DEE0E3',
  borderStrong: '#C2C4C8',
  text0: '#191B1D',
  text1: '#46484B',
  text2: '#67696C',
  text3: '#9C9EA2',
  code: '#2C2E31',
  lineHighlight: '#F5F7F9',
  lineNumber: '#8A8C90',
  indentGuide: '#EDEEF1',
  ok: '#1E7546',
  warn: '#B05315',
  error: '#BE2F2C',
  info: '#2266A4',
  ansi: {
    black: '#191B1D',
    red: '#BE2F2C',
    green: '#1E7546',
    yellow: '#816500',
    blue: '#2863AB',
    magenta: '#8B4486',
    cyan: '#007686',
    white: '#46484B',
    brightBlack: '#67696C',
    brightRed: '#BB5752',
    brightGreen: '#348F4F',
    brightYellow: '#9A7D22',
    brightBlue: '#407BC5',
    brightMagenta: '#A45B9F',
    brightCyan: '#008F9F',
    brightWhite: '#191B1D',
  },
};

/** A cor de uma paleta da AURORA IDE num esquema. */
interface AuroraAccent {
  /** Linhas e indicadores (foco, aba ativa): 3:1 com as barras e o editor. */
  line: string;
  /** O destaque em texto, nas palavras reservadas e no cursor. */
  text: string;
  /** O preenchimento do botão principal, que é também a cor do logo. */
  fill: string;
  fillHover: string;
  /** O texto sobre `fill`. */
  onFill: string;
  /** Números, textos e constantes: a segunda cor do código. */
  literal: string;
  /** A opacidade de `fill` na seleção. */
  selection: number;
  /** As palavras reservadas e os tipos no código, quando não vão no `text`:
   * o Lunar tem a interface no prata do texto, e o código ganha cor própria. */
  keyword?: string;
  type?: string;
}

/**
 * Um tema da AURORA IDE. A sintaxe tem duas cores além dos cinzas: o
 * destaque nas palavras reservadas e nos tipos, e `literal` em números,
 * textos e constantes. Funções em `text0`, o resto do código em `code`,
 * operadores e comentários em `text2`. A busca usa `literal`, para não se
 * confundir com a seleção, que usa o destaque. `bold` põe as palavras
 * reservadas em negrito, para o Lunar, cuja interface não tem cor de
 * destaque.
 */
function aurora(id: string, name: string, scheme: Scheme, pair: string, accent: AuroraAccent, bold = false): Theme {
  const dark = scheme === 'dark';
  const base = dark ? AURORA_DARK : AURORA_LIGHT;
  const selection = alpha(accent.fill, accent.selection);
  return {
    id,
    name,
    scheme,
    pair,
    ui: {
      bg0: base.bars,
      bg1: base.bars,
      bg2: base.raised,
      bg3: base.control,
      bg4: base.controlHover,
      bgEditor: base.editor,
      border: base.border,
      borderStrong: base.borderStrong,
      text0: base.text0,
      text1: base.text1,
      text2: base.text2,
      text3: base.text3,
      brand: accent.line,
      brandText: accent.text,
      brandFill: accent.fill,
      brandFillHover: accent.fillHover,
      brandContrast: accent.onFill,
      ok: base.ok,
      warn: base.warn,
      error: base.error,
      info: base.info,
    },
    syntax: {
      fg: base.code,
      comment: base.text2,
      keyword: accent.keyword ?? accent.text,
      control: accent.keyword ?? accent.text,
      type: accent.type ?? accent.text,
      func: base.text0,
      number: accent.literal,
      string: accent.literal,
      constant: accent.literal,
      variable: base.code,
      tag: accent.type ?? accent.text,
      operator: base.text2,
      delimiter: base.text2,
      directive: base.text1,
      style: bold ? { keyword: 'bold', type: 'bold', directive: '' } : { directive: '' },
    },
    editor: {
      selection,
      lineHighlight: base.lineHighlight,
      lineNumber: base.lineNumber,
      lineNumberActive: base.text0,
      cursor: accent.text,
      findMatch: alpha(accent.literal, dark ? 0.38 : 0.3),
      findMatchHighlight: alpha(accent.literal, dark ? 0.18 : 0.14),
      indentGuide: base.indentGuide,
      indentGuideActive: base.borderStrong,
    },
    terminal: { foreground: base.code, selection, ...base.ansi },
  };
}

/** O padrão: o azul de destaque, com areia nos números e textos. O botão
 * principal leva texto escuro, que tem 6,8:1 sobre o azul. */
const boreal = aurora('boreal', 'Boreal', 'dark', 'boreal-claro', {
  line: '#59A0F9',
  text: '#73AEF8',
  fill: '#59A0F9',
  fillHover: '#70ADFA',
  onFill: '#141619',
  literal: '#E2BD93',
  selection: 0.3,
});

const borealClaro = aurora('boreal-claro', 'Boreal Claro', 'light', 'boreal', {
  line: '#236ADE',
  text: '#2062CE',
  fill: '#236ADE',
  fillHover: '#205FC8',
  onFill: '#FFFFFF',
  literal: '#86572D',
  selection: 0.18,
});

/** O verde da aurora austral, o do "ok" da Aurora antiga, com o lilás dela
 * nos números e textos. O botão leva texto escuro, que tem 9,9:1. */
const austral = aurora('austral', 'Austral', 'dark', 'austral-claro', {
  line: '#5DD4A8',
  text: '#68DEB1',
  fill: '#5DD4A8',
  fillHover: '#69E0B3',
  onFill: '#141619',
  literal: '#AEA7ED',
  selection: 0.24,
});

/** No claro, o verde do botão (#2AA578, com texto escuro) não chega a 3:1
 * com as barras: as linhas usam um verde mais escuro, e o texto, um
 * verde-garrafa. Os números e textos vão no violeta do Nebulosa Clara. */
const australClaro = aurora('austral-claro', 'Austral Claro', 'light', 'austral', {
  line: '#1F8A63',
  text: '#17704F',
  fill: '#2AA578',
  fillHover: '#13986C',
  onFill: '#191B1D',
  literal: '#5B50B8',
  selection: 0.18,
});

/** O violeta da Aurora antiga (#8E83E8, o destaque dela), com a menta dela
 * nos números e textos. O botão leva texto escuro, que tem 5,7:1. */
const nebulosa = aurora('nebulosa', 'Nebulosa', 'dark', 'nebulosa-clara', {
  line: '#8E83E8',
  text: '#A89EF0',
  fill: '#8E83E8',
  fillHover: '#988EF4',
  onFill: '#141619',
  literal: '#75D4AE',
  selection: 0.3,
});

/** No claro, o violeta do botão da Aurora antiga (#6E63C8), que segura o
 * texto branco (4,9:1) e as linhas (4,6:1 com as barras). */
const nebulosaClara = aurora('nebulosa-clara', 'Nebulosa Clara', 'light', 'nebulosa', {
  line: '#6E63C8',
  text: '#5B50B8',
  fill: '#6E63C8',
  fillHover: '#6458BC',
  onFill: '#FFFFFF',
  literal: '#1E7A5A',
  selection: 0.16,
});

/** O âmbar, a cor do amanhecer, com azul-claro nos números e textos. */
const alvorada = aurora('alvorada', 'Alvorada', 'dark', 'alvorada-clara', {
  line: '#F5B33B',
  text: '#F5BD56',
  fill: '#F5B33B',
  fillHover: '#F6BE56',
  onFill: '#141619',
  literal: '#8EC6E7',
  selection: 0.26,
});

/** No claro, o âmbar do botão e do logo (#E38F00) fica em 2,4:1 com as
 * barras: as linhas usam um âmbar mais escuro, e o texto, um castanho. */
const alvoradaClara = aurora('alvorada-clara', 'Alvorada Clara', 'light', 'alvorada', {
  line: '#CB7400',
  text: '#97570D',
  fill: '#E38F00',
  fillHover: '#CC8100',
  onFill: '#191B1D',
  literal: '#1F6A96',
  selection: 0.22,
});

/** A interface sem cor de destaque, no prata do próprio texto. O código tem
 * cores de luar: as palavras reservadas em negrito num azul (#99C2ED,
 * 9,8:1), os tipos num lilás prateado (10,2:1) e os números e textos num
 * dourado (11:1). */
const lunar = aurora(
  'lunar',
  'Lunar',
  'dark',
  'lunar-claro',
  {
    line: '#EAEDF1',
    text: '#EAEDF1',
    fill: '#EAEDF1',
    fillHover: '#D3D6DA',
    onFill: '#141619',
    literal: '#EFC37E',
    selection: 0.18,
    keyword: '#99C2ED',
    type: '#C6BDE8',
  },
  true,
);

const lunarClaro = aurora(
  'lunar-claro',
  'Lunar Claro',
  'light',
  'lunar',
  {
    line: '#191B1D',
    text: '#191B1D',
    fill: '#191B1D',
    fillHover: '#36383B',
    onFill: '#FFFFFF',
    literal: '#935A11',
    selection: 0.12,
    keyword: '#285994',
    type: '#5F4E8D',
  },
  true,
);

/**
 * A paleta da AURORA: céu noturno (#0A0D14), violeta #8E83E8 de destaque e
 * menta, ciano, roxo e rosa de apoio. A sintaxe é a do tema `cmm-dark` da
 * AURORA, levada aos tokens das gramáticas do Studio.
 */
const auroraLegacy: Theme = {
  id: 'aurora-legacy',
  name: 'Aurora Legacy',
  scheme: 'dark',
  ui: {
    bg0: '#0F131C',
    bg1: '#0F131C',
    bg2: '#151A25',
    bg3: '#1B2130',
    bg4: '#2A3040',
    bgEditor: '#0A0D14',
    border: '#1F2532',
    borderStrong: '#2A3040',
    text0: '#E8ECF3',
    text1: '#9CA1AE',
    text2: '#6A6F7C',
    text3: '#3F434E',
    brand: '#8E83E8',
    brandText: '#A89EF0',
    brandFill: '#6E63C8',
    brandFillHover: '#8E83E8',
    brandContrast: '#FFFFFF',
    brandSoft: '#8E83E82E',
    ok: '#5DD4A8',
    warn: '#E8B86C',
    error: '#E26C6C',
    info: '#5BB8E8',
    hover: '#FFFFFF09',
    overlay: '#06080CB8',
    shadowPop: '0 6px 18px #00000073',
  },
  syntax: {
    fg: '#E8ECF3',
    comment: '#6A6F7C',
    keyword: '#8E83E8',
    control: '#E8B86C',
    type: '#8E83E8',
    func: '#5BB8E8',
    number: '#5FE0B0',
    string: '#E68FB8',
    constant: '#E8B86C',
    variable: '#4FD3C2',
    tag: '#8E83E8',
    operator: '#9CA1AE',
    delimiter: '#9CA1AE',
    directive: '#B98AE0',
    style: { keyword: 'bold', type: 'bold', func: 'bold', constant: 'bold', directive: '' },
  },
  rules: [
    { token: 'dirac.bracket.cmm', foreground: 'A89EF0', fontStyle: 'bold' },
    { token: 'dirac.bar.cmm', foreground: 'A89EF0', fontStyle: 'bold' },
    { token: 'dirac.special.cmm', foreground: 'B98AE0', fontStyle: 'bold' },
    { token: 'number.imaginary.cmm', foreground: 'BD93F9', fontStyle: 'bold' },
    { token: 'operator.shift.arithmetic.cmm', foreground: 'A89EF0', fontStyle: 'bold' },
    { token: 'label.asm', foreground: 'A89EF0', fontStyle: 'italic' },
  ],
  editor: {
    selection: '#8E83E830',
    selectionInactive: '#8E83E820',
    lineHighlight: '#0F131C',
    lineNumber: '#3F434E',
    lineNumberActive: '#A89EF0',
    cursor: '#8E83E8',
    findMatch: '#8E83E866',
    findMatchHighlight: '#8E83E833',
    indentGuide: '#161A23',
    indentGuideActive: '#8E83E8',
    bracketMatch: '#8E83E833',
    bracketMatchBorder: '#8E83E8',
    brackets: ['#5FE0B0', '#5BB8E8', '#8E83E8', '#B98AE0', '#E68FB8', '#4FD3C2'],
  },
  terminal: {
    foreground: '#E8ECF3',
    selection: '#8E83E859',
    black: '#11131C',
    red: '#E26C6C',
    green: '#5FE0B0',
    yellow: '#E8C97D',
    blue: '#5BB8E8',
    magenta: '#B58AE8',
    cyan: '#4FD3C2',
    white: '#C5C9D6',
    brightBlack: '#5A6072',
    brightRed: '#FF8A8A',
    brightGreen: '#7FF0C6',
    brightYellow: '#FFE0A0',
    brightBlue: '#8CD0FF',
    brightMagenta: '#D0B0FF',
    brightCyan: '#7FEFE0',
    brightWhite: '#FFFFFF',
  },
};

// Os padrões do VS Code ---------------------------------------------------

const VSCODE_DARK_ANSI = {
  black: '#000000',
  red: '#CD3131',
  green: '#0DBC79',
  yellow: '#E5E510',
  blue: '#2472C8',
  magenta: '#BC3FBC',
  cyan: '#11A8CD',
  white: '#E5E5E5',
  brightBlack: '#666666',
  brightRed: '#F14C4C',
  brightGreen: '#23D18B',
  brightYellow: '#F5F543',
  brightBlue: '#3B8EEA',
  brightMagenta: '#D670D6',
  brightCyan: '#29B8DB',
  brightWhite: '#E5E5E5',
};

/** O tema padrão do VS Code desde 2023, com a sintaxe do Dark+. */
const darkModern: Theme = {
  id: 'dark-modern',
  name: 'Dark Modern',
  family: 'VS Code',
  scheme: 'dark',
  pair: 'light-modern',
  ui: {
    bg0: '#181818',
    bg1: '#181818',
    bg2: '#202020',
    bg3: '#313131',
    bg4: '#3C3C3C',
    bgEditor: '#1F1F1F',
    border: '#2B2B2B',
    borderStrong: '#3C3C3C',
    text0: '#FFFFFF',
    text1: '#CCCCCC',
    text2: '#9D9D9D',
    text3: '#6E7681',
    brand: '#0078D4',
    brandText: '#4DAAFC',
    brandFill: '#0078D4',
    brandFillHover: '#026EC1',
    brandContrast: '#FFFFFF',
    ok: '#2EA043',
    warn: '#CCA700',
    error: '#F85149',
    info: '#3794FF',
  },
  syntax: {
    fg: '#CCCCCC',
    comment: '#6A9955',
    keyword: '#569CD6',
    control: '#C586C0',
    type: '#4EC9B0',
    func: '#DCDCAA',
    number: '#B5CEA8',
    string: '#CE9178',
    constant: '#4FC1FF',
    variable: '#9CDCFE',
    tag: '#569CD6',
    operator: '#D4D4D4',
    delimiter: '#D4D4D4',
    directive: '#569CD6',
    style: { comment: '', directive: '' },
  },
  editor: {
    selection: '#264F78',
    selectionInactive: '#3A3D41',
    lineHighlight: '#00000000',
    lineHighlightBorder: '#282828',
    lineNumber: '#6E7681',
    lineNumberActive: '#CCCCCC',
    cursor: '#AEAFAD',
    findMatch: '#9E6A03',
    findMatchHighlight: '#EA5C0055',
    indentGuide: '#404040',
    indentGuideActive: '#707070',
    bracketMatch: '#0064001A',
    bracketMatchBorder: '#888888',
    brackets: ['#FFD700', '#DA70D6', '#179FFF'],
  },
  terminal: { foreground: '#CCCCCC', selection: '#264F78', ...VSCODE_DARK_ANSI },
};

/** O par claro do Dark Modern, com a sintaxe do Light+. */
const lightModern: Theme = {
  id: 'light-modern',
  name: 'Light Modern',
  scheme: 'light',
  pair: 'dark-modern',
  ui: {
    bg0: '#F8F8F8',
    bg1: '#F8F8F8',
    bg2: '#FFFFFF',
    bg3: '#E5E5E5',
    bg4: '#CCCCCC',
    bgEditor: '#FFFFFF',
    border: '#E5E5E5',
    borderStrong: '#CECECE',
    text0: '#1F1F1F',
    text1: '#3B3B3B',
    text2: '#616161',
    text3: '#8B949E',
    brand: '#005FB8',
    brandText: '#005FB8',
    brandFill: '#005FB8',
    brandFillHover: '#0258A8',
    brandContrast: '#FFFFFF',
    ok: '#2EA043',
    warn: '#BF8803',
    error: '#E51400',
    info: '#1A85FF',
  },
  syntax: {
    fg: '#3B3B3B',
    comment: '#008000',
    keyword: '#0000FF',
    control: '#AF00DB',
    type: '#267F99',
    func: '#795E26',
    number: '#098658',
    string: '#A31515',
    constant: '#0070C1',
    variable: '#0451A5',
    tag: '#800000',
    operator: '#3B3B3B',
    delimiter: '#3B3B3B',
    directive: '#0000FF',
    style: { comment: '', directive: '' },
  },
  editor: {
    selection: '#ADD6FF',
    selectionInactive: '#E5EBF1',
    lineHighlight: '#00000000',
    lineHighlightBorder: '#EEEEEE',
    lineNumber: '#6E7681',
    lineNumberActive: '#171184',
    cursor: '#000000',
    findMatch: '#A8AC94',
    findMatchHighlight: '#EA5C0055',
    indentGuide: '#D3D3D3',
    indentGuideActive: '#939393',
    bracketMatch: '#0064001A',
    bracketMatchBorder: '#B9B9B9',
    brackets: ['#0431FA', '#319331', '#7B3814'],
  },
  terminal: {
    foreground: '#3B3B3B',
    selection: '#ADD6FF',
    black: '#000000',
    red: '#CD3131',
    green: '#107C10',
    yellow: '#949800',
    blue: '#0451A5',
    magenta: '#BC05BC',
    cyan: '#0598BC',
    white: '#555555',
    brightBlack: '#666666',
    brightRed: '#F14C4C',
    brightGreen: '#14CE14',
    brightYellow: '#B5BA00',
    brightBlue: '#3B8EEA',
    brightMagenta: '#D670D6',
    brightCyan: '#29B8DB',
    brightWhite: '#A5A5A5',
  },
};

// Dracula -----------------------------------------------------------------

const DRACULA = {
  bg: '#282A36',
  fg: '#F8F8F2',
  selection: '#44475A',
  comment: '#6272A4',
  cyan: '#8BE9FD',
  green: '#50FA7B',
  orange: '#FFB86C',
  pink: '#FF79C6',
  purple: '#BD93F9',
  red: '#FF5555',
  yellow: '#F1FA8C',
  bgLighter: '#424450',
  bgLight: '#343746',
  bgDark: '#21222C',
  bgDarker: '#191A21',
};

const dracula: Theme = {
  id: 'dracula',
  name: 'Dracula',
  scheme: 'dark',
  ui: {
    bg0: DRACULA.bgDarker,
    bg1: DRACULA.bgDark,
    bg2: DRACULA.bgDark,
    bg3: DRACULA.bgLight,
    bg4: DRACULA.selection,
    bgEditor: DRACULA.bg,
    border: DRACULA.bgDarker,
    borderStrong: DRACULA.selection,
    text0: DRACULA.fg,
    text1: mix(DRACULA.fg, DRACULA.bg, 0.2),
    text2: mix(DRACULA.fg, DRACULA.bg, 0.45),
    text3: DRACULA.comment,
    brand: DRACULA.purple,
    brandText: DRACULA.purple,
    brandFill: DRACULA.purple,
    brandFillHover: mix(DRACULA.purple, DRACULA.fg, 0.25),
    brandContrast: DRACULA.bg,
    brandSoft: '#BD93F933',
    ok: DRACULA.green,
    warn: DRACULA.orange,
    error: DRACULA.red,
    info: DRACULA.cyan,
  },
  syntax: {
    fg: DRACULA.fg,
    comment: DRACULA.comment,
    keyword: DRACULA.pink,
    control: DRACULA.pink,
    type: DRACULA.cyan,
    func: DRACULA.green,
    number: DRACULA.purple,
    string: DRACULA.yellow,
    constant: DRACULA.purple,
    variable: DRACULA.cyan,
    tag: DRACULA.pink,
    operator: DRACULA.pink,
    delimiter: DRACULA.fg,
    directive: DRACULA.pink,
    style: { type: 'italic', directive: '' },
  },
  editor: {
    selection: DRACULA.selection,
    lineHighlight: '#00000000',
    lineHighlightBorder: DRACULA.selection,
    lineNumber: DRACULA.comment,
    lineNumberActive: DRACULA.fg,
    cursor: DRACULA.fg,
    findMatch: '#FFB86C80',
    findMatchHighlight: '#FFFFFF40',
    indentGuide: '#FFFFFF1A',
    indentGuideActive: '#FFFFFF45',
    brackets: [DRACULA.fg, DRACULA.pink, DRACULA.cyan, DRACULA.green, DRACULA.purple, DRACULA.orange],
  },
  terminal: {
    foreground: DRACULA.fg,
    selection: DRACULA.selection,
    black: '#21222C',
    red: '#FF5555',
    green: '#50FA7B',
    yellow: '#F1FA8C',
    blue: '#BD93F9',
    magenta: '#FF79C6',
    cyan: '#8BE9FD',
    white: '#F8F8F2',
    brightBlack: '#6272A4',
    brightRed: '#FF6E6E',
    brightGreen: '#69FF94',
    brightYellow: '#FFFFA5',
    brightBlue: '#D6ACFF',
    brightMagenta: '#FF92DF',
    brightCyan: '#A4FFFF',
    brightWhite: '#FFFFFF',
  },
};

export const THEMES: Theme[] = [
  boreal,
  borealClaro,
  austral,
  australClaro,
  nebulosa,
  nebulosaClara,
  alvorada,
  alvoradaClara,
  lunar,
  lunarClaro,
  auroraLegacy,
  darkModern,
  lightModern,
  dracula,
];
