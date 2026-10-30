// Os temas do Studio. Cada tema é um objeto só, com as cores da interface,
// da sintaxe, do editor e dos terminais, e dele saem as três coisas que
// mudam com o tema:
//
// | O quê                    | Quem aplica                     |
// |--------------------------|---------------------------------|
// | variáveis CSS (`--bg-0`) | `applyThemeCss`, aqui           |
// | tema do Monaco           | `setMonacoTheme`, editor/monaco |
// | tema dos xterm           | `terminalTheme`, console/consoles |
//
// Nenhuma cor de tema fica fora deste módulo: os componentes usam as
// variáveis, e as medidas (fontes, espaços, raios) continuam em
// styles/tokens.css. Os temas estão em catalog.ts.
//
// A preferência é uma paleta e um modo. A paleta (`settings.theme`) é o id do
// tema escuro dela, com o claro no `pair`; o modo (`settings.theme_mode`) é
// escuro, claro ou o do sistema. Uma paleta só escura (o Dracula, o Aurora
// Legacy) fica escura em qualquer modo.

import { THEMES } from './catalog';
import { alpha, legible, mix, type Scheme, type Theme, type UiColors } from './model';

export * from './model';

export { THEMES };

/** A paleta padrão. O settings.rs leva a ela quem tinha um tema que saiu: o
 * Atlas, o padrão do Lace Studio, o "escuro" de antes dele e os temas de fora
 * que não ficaram. */
export const DEFAULT_THEME = 'boreal';
/** O tema claro da paleta padrão. */
export const DEFAULT_LIGHT_THEME = 'boreal-claro';

/** O modo: um esquema fixo, ou o do sistema. */
export type ThemeMode = Scheme | 'system';

const BY_ID = new Map(THEMES.map((theme) => [theme.id, theme]));

/** As paletas, na ordem do catálogo: o tema escuro de cada uma, ou o único
 * de uma paleta só escura. */
export const PALETTES: Theme[] = THEMES.filter((theme) => theme.scheme === 'dark' || !theme.pair);

export function themeById(id: string): Theme | undefined {
  return BY_ID.get(id);
}

/** O nome da paleta: o `family` do tema escuro, ou o nome dele. */
export function paletteName(palette: Theme): string {
  return palette.family ?? palette.name;
}

/** A paleta de um id: o tema escuro dela. Um id desconhecido (de uma versão
 * mais nova, ou de um tema que saiu) cai na paleta padrão. */
export function paletteOf(id: string | undefined): Theme {
  const theme = (id && BY_ID.get(id)) || BY_ID.get(DEFAULT_THEME)!;
  return theme.scheme === 'light' && theme.pair ? (BY_ID.get(theme.pair) ?? theme) : theme;
}

/** O esquema de um modo; o "Do sistema" segue `systemDark`, e um modo
 * desconhecido vale o escuro. */
export function schemeOf(mode: string | undefined, systemDark: boolean): Scheme {
  if (mode === 'light') return 'light';
  if (mode === 'system') return systemDark ? 'dark' : 'light';
  return 'dark';
}

/** O tema de uma paleta num esquema; a paleta só escura fica escura. */
export function paletteTheme(palette: Theme, scheme: Scheme): Theme {
  if (palette.scheme === scheme || !palette.pair) return palette;
  return BY_ID.get(palette.pair) ?? palette;
}

/** O tema da preferência: a paleta no esquema do modo. */
export function resolveTheme(palette: string | undefined, mode: string | undefined, systemDark: boolean): Theme {
  return paletteTheme(paletteOf(palette), schemeOf(mode, systemDark));
}

/** O modo para onde "Alternar tema claro e escuro" leva: o contrário do
 * esquema em uso. */
export function toggledMode(theme: Theme): ThemeMode {
  return theme.scheme === 'dark' ? 'light' : 'dark';
}

// Interface ---------------------------------------------------------------

/** As variáveis CSS do tema, com os padrões preenchidos. */
export function uiTokens(theme: Theme): Record<string, string> {
  const { ui } = theme;
  const dark = theme.scheme === 'dark';
  const soft = dark ? 0.12 : 0.1;
  const complete: Required<UiColors> & Record<string, string> = {
    ...ui,
    brandSoft: ui.brandSoft ?? alpha(ui.brandFill, dark ? 0.28 : 0.1),
    okSoft: alpha(ui.ok, soft),
    warnSoft: alpha(ui.warn, soft),
    errorSoft: alpha(ui.error, soft),
    infoSoft: alpha(ui.info, soft),
    hover: ui.hover ?? (dark ? '#FFFFFF0A' : '#0000000A'),
    active: ui.active ?? (dark ? '#FFFFFF12' : '#00000012'),
    overlay: ui.overlay ?? (dark ? '#0000008C' : '#00000040'),
    scrollbar: ui.scrollbar ?? (dark ? '#FFFFFF1F' : '#00000026'),
    shadowPop: ui.shadowPop ?? (dark ? '0 8px 24px #00000073' : '0 8px 24px #0000001F'),
    windowClose: ui.windowClose ?? '#C42B1C',
    windowCloseText: ui.windowCloseText ?? '#FFFFFF',
  };
  const tokens: Record<string, string> = {};
  for (const [key, value] of Object.entries({ ...complete, ...schematicColors(theme) })) {
    tokens[key.replace(/([A-Z]|\d+)/g, '-$1').toLowerCase()] = value;
  }
  return tokens;
}

/** As famílias de célula do RTLViewer e o tom de cada uma no tema. */
function families(theme: Theme): Record<string, string> {
  const { ui, terminal, syntax } = theme;
  return {
    arith: terminal.yellow,
    logic: terminal.blue,
    compare: terminal.magenta,
    mux: terminal.cyan,
    reg: terminal.green,
    mem: mix(terminal.red, terminal.yellow, 0.35),
    module: ui.brand,
    other: ui.text2,
    port: ui.text1,
    const: syntax.number,
  };
}

/**
 * As cores do RTLViewer (`--sch-*`). Cada família de célula pega um tom da
 * paleta dos terminais, que todo tema define com seis matizes distintos, e
 * o preenchimento é a mistura dele com o fundo do editor: opaco, para os
 * fios que entram no símbolo ficarem por baixo. Um tom fraco demais sobre o
 * fundo vai na direção do texto até ter contraste 3:1.
 */
function schematicColors(theme: Theme): Record<string, string> {
  const { ui } = theme;
  const dark = theme.scheme === 'dark';
  const canvas = ui.bgEditor;
  const colors: Record<string, string> = {
    schCanvas: canvas,
    schGrid: mix(canvas, ui.text3, dark ? 0.4 : 0.3),
    schWire: mix(ui.text2, canvas, dark ? 0.05 : 0.15),
    schText: ui.text0,
    schMuted: ui.text2,
    schHot: legible(ui.brandText, ui.text0, canvas, 4.5),
    schSelect: ui.brand,
  };
  for (const [family, color] of Object.entries(families(theme))) {
    const ink = legible(color, ui.text0, canvas, 3);
    const name = family[0]!.toUpperCase() + family.slice(1);
    colors[`sch${name}`] = ink;
    colors[`sch${name}Fill`] = mix(canvas, ink, dark ? 0.16 : 0.09);
  }
  return colors;
}

let styleElement: HTMLStyleElement | null = null;
let appliedTheme: Theme | null = null;

/**
 * Põe as variáveis do tema na página, num `<style>` próprio, e marca o
 * `<html>` com o esquema (`data-theme`, que o logo e o `color-scheme` usam)
 * e o tema (`data-theme-id`).
 */
export function applyThemeCss(theme: Theme): void {
  if (theme === appliedTheme) return;
  appliedTheme = theme;
  if (!styleElement) {
    styleElement = document.createElement('style');
    styleElement.id = 'aurora-theme';
    document.head.appendChild(styleElement);
  }
  const lines = Object.entries(uiTokens(theme)).map(([name, value]) => `  --${name}: ${value};`);
  styleElement.textContent = `:root {\n  color-scheme: ${theme.scheme};\n${lines.join('\n')}\n}\n`;
  const root = document.documentElement;
  root.dataset.theme = theme.scheme;
  root.dataset.themeId = theme.id;
}
