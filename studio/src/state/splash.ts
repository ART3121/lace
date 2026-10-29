// O splash (public/splash.html): a janela pequena com o logo animado que
// aparece enquanto o Studio abre (janela `splash` do tauri.conf.json). Ela
// lê do localStorage, que as duas janelas dividem, o que esta janela está
// fazendo e, da abertura anterior, as cores do tema, o idioma e a versão;
// esta janela a fecha quando aparece (`reveal`, App.tsx).

import { useLang } from '../i18n';
import type { Theme } from '../themes';

const STATUS = 'lace-studio:splash:status';
const LOOK = 'lace-studio:splash:look';
const BORN = 'lace-studio:splash:born';

/** Quanto falta para o splash ter ficado `min` ms na tela, contado de quando
 * ele apareceu (ele grava a hora). Sem esse registro, ou com um de outra
 * abertura, conta desde o começo desta janela. */
export function splashRemaining(min: number): number {
  let born = Number.NaN;
  try {
    born = Number(localStorage.getItem(BORN));
  } catch {
    // Sem armazenamento, vale a conta desta janela.
  }
  const now = Date.now();
  if (Number.isFinite(born) && born > 0 && now - born < 60_000) return Math.max(0, born + min - now);
  return Math.max(0, min - performance.now());
}

/** O que aparece embaixo do logo: "Procurando o Lace", "Reabrindo contador".
 * Vai com a hora: o splash ignora o que sobrou de uma abertura anterior. */
export function splashStatus(text: string): void {
  try {
    localStorage.setItem(STATUS, JSON.stringify({ text, at: Date.now() }));
  } catch {
    // Sem armazenamento, o splash fica com o texto dele.
  }
}

/** As cores do tema, o idioma e a versão, para o splash da próxima abertura. */
export function rememberSplashLook(theme: Theme, version: string | undefined): void {
  const { ui } = theme;
  const dark = theme.scheme === 'dark';
  const look = {
    version,
    lang: useLang.getState().lang,
    colors: {
      bg: ui.bg0,
      fg: ui.text0,
      muted: ui.text2,
      track: ui.border,
      // O camaleão na cor do AuroraMark da janela.
      mark: ui.brandFill,
      pulse: dark ? ui.brandText : ui.brand,
    },
  };
  try {
    localStorage.setItem(LOOK, JSON.stringify(look));
  } catch {
    // Sem armazenamento, o próximo splash nasce nas cores do Azul.
  }
}
