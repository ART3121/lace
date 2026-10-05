// Tradução da interface. `t('chave', { nome })` devolve o texto no idioma
// atual; `useT()` faz o componente se redesenhar quando o idioma muda.
//
// O idioma vem da preferência (`system`, `pt`, `en`); `system` segue o
// navegador do sistema (pt para qualquer variante de português).

import { create } from 'zustand';

import { en } from './en';
import { pt, type Key } from './pt';

export type { Key };
export type Lang = 'pt' | 'en';

const dictionaries: Record<Lang, Record<Key, string>> = { pt, en };

interface LangState {
  lang: Lang;
  setLang: (preference: 'system' | Lang) => void;
}

export function systemLang(): Lang {
  return navigator.language.toLowerCase().startsWith('pt') ? 'pt' : 'en';
}

export const useLang = create<LangState>((set) => ({
  lang: systemLang(),
  setLang: (preference) => {
    const lang = preference === 'system' ? systemLang() : preference;
    document.documentElement.lang = lang === 'pt' ? 'pt-BR' : 'en';
    set({ lang });
  },
}));

export type Params = Record<string, string | number>;

/** O texto de `key` no idioma atual, com os `{placeholders}` trocados. */
export function t(key: Key, params?: Params): string {
  const text = dictionaries[useLang.getState().lang][key] ?? pt[key] ?? key;
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}

/** `t` para componentes: redesenha quando o idioma muda. */
export function useT(): typeof t {
  useLang((s) => s.lang);
  return t;
}

/** O texto de um erro do backend: a tradução pelo código, se houver. */
export function errorText(error: { code: string; message: string }): string {
  const key = `error.${error.code}` as Key;
  return key in pt ? t(key) : error.message;
}

/** Duração legível: `12 ms`, `1.4 s`, `2 min 05 s`. */
export function formatDuration(ms: number): string {
  if (ms < 1000) return t('common.ms', { n: ms });
  if (ms < 60_000) {
    const s = (ms / 1000).toFixed(ms < 10_000 ? 2 : 1);
    return t('common.seconds', { n: useLang.getState().lang === 'pt' ? s.replace('.', ',') : s });
  }
  const minutes = Math.floor(ms / 60_000);
  const seconds = Math.round((ms % 60_000) / 1000);
  return `${minutes} min ${String(seconds).padStart(2, '0')} s`;
}
