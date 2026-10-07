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

import { THEMES } from './catalog';
import { alpha, legible, mix, type Theme, type UiColors } from './model';

export * from './model';

export { THEMES };

/** O tema padrão, também o de quem tinha "escuro" gravado. */
export const DEFAULT_THEME = 'atlas';
/** O claro do "Do sistema", também o de quem tinha "claro" gravado. */
export const DEFAULT_LIGHT_THEME = 'atlas-light';
/** A preferência que segue o esquema do sistema. */
export const SYSTEM_THEME = 'system';

const BY_ID = new Map(THEMES.map((theme) => [theme.id, theme]));

export function themeById(id: string): Theme | undefined {
  return BY_ID.get(id);
}

/**
 * O tema de uma preferência. `system` fica com o Atlas ou o Atlas Branco,
 * conforme o sistema; um id desconhecido (de uma versão mais nova, ou de um
 * tema que saiu) cai no padrão.
 */
export function resolveTheme(preference: string | undefined, systemDark: boolean): Theme {
  if (!preference || preference === SYSTEM_THEME) {
    return BY_ID.get(systemDark ? DEFAULT_THEME : DEFAULT_LIGHT_THEME)!;
  }
  return BY_ID.get(preference) ?? BY_ID.get(DEFAULT_THEME)!;
}

/** O tema para onde "Alternar tema claro e escuro" leva. */
export function toggledTheme(theme: Theme): string {
  return theme.pair ?? (theme.scheme === 'dark' ? DEFAULT_LIGHT_THEME : DEFAULT_THEME);
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
  };
  const tokens: Record<string, string> = {};
  for (const [key, value] of Object.entries({ ...complete, ...schematicColors(theme) })) {
    tokens[key.replace(/([A-Z]|\d+)/g, '-$1').toLowerCase()] = value;
  }
  return tokens;
}

/** As famílias de célula do PRISM e o tom de cada uma no tema. */
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
 * As cores do PRISM (`--sch-*`). Cada família de célula pega um tom da
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
    styleElement.id = 'lace-theme';
    document.head.appendChild(styleElement);
  }
  const lines = Object.entries(uiTokens(theme)).map(([name, value]) => `  --${name}: ${value};`);
  styleElement.textContent = `:root {\n  color-scheme: ${theme.scheme};\n${lines.join('\n')}\n}\n`;
  const root = document.documentElement;
  root.dataset.theme = theme.scheme;
  root.dataset.themeId = theme.id;
}
