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
import { alpha, type Theme, type UiColors } from './model';

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
    schematicBg: ui.schematicBg ?? (dark ? '#F4F4F4' : '#FFFFFF'),
  };
  const tokens: Record<string, string> = {};
  for (const [key, value] of Object.entries(complete)) {
    tokens[key.replace(/([A-Z]|\d+)/g, '-$1').toLowerCase()] = value;
  }
  return tokens;
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
