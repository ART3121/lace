// O estado do aplicativo: versão, preferências, bundle do Lace e recentes.
// As preferências moram no backend (settings.json); aqui fica a cópia que a
// interface usa, e cada mudança é gravada lá.

import { create } from 'zustand';

import { useLang } from '../i18n';
import { api } from '../ipc/api';
import type { AppInfo, RecentStatus, Settings, ToolchainInfo } from '../ipc/types';
import { applyThemeCss, DEFAULT_THEME, resolveTheme, type Theme } from '../themes';
import { showError } from './toasts';

interface AppState {
  info: AppInfo | null;
  settings: Settings | null;
  toolchain: ToolchainInfo | null;
  recent: RecentStatus[];
  /** O tema em uso, já resolvido: o "Do sistema" vira o Atlas ou o Atlas Branco. */
  theme: Theme;

  init: () => Promise<Settings | null>;
  updateSettings: (change: (current: Settings) => Settings) => Promise<void>;
  refreshToolchain: () => Promise<void>;
  refreshRecent: () => Promise<void>;
}

const darkQuery = window.matchMedia('(prefers-color-scheme: dark)');

function themeFor(preference: string | undefined): Theme {
  return resolveTheme(preference, darkQuery.matches);
}

// O padrão vai para a página já ao carregar o módulo, antes do primeiro
// quadro; o tema das preferências chega em `init`.
const initialTheme = themeFor(DEFAULT_THEME);
applyThemeCss(initialTheme);

export const useApp = create<AppState>((set, get) => ({
  info: null,
  settings: null,
  toolchain: null,
  recent: [],
  theme: initialTheme,

  init: async () => {
    try {
      const [info, settings] = await Promise.all([api.app.info(), api.app.settings()]);
      const theme = themeFor(settings.theme);
      applyThemeCss(theme);
      useLang.getState().setLang(settings.language);
      set({ info, settings, theme });
      void get().refreshToolchain();
      void get().refreshRecent();
      return settings;
    } catch (error) {
      showError(error);
      return null;
    }
  },

  updateSettings: async (change) => {
    const current = get().settings;
    if (!current) return;
    const next = change(current);
    // Aplica na hora e grava depois; um erro ao gravar volta ao anterior.
    const theme = themeFor(next.theme);
    set({ settings: next, theme });
    applyThemeCss(theme);
    useLang.getState().setLang(next.language);
    try {
      const saved = await api.app.saveSettings(next);
      set({ settings: saved });
      const toolchainChanged =
        current.toolchain_dir !== next.toolchain_dir ||
        current.compiler_dir !== next.compiler_dir ||
        current.quartus_dir !== next.quartus_dir;
      if (toolchainChanged) void get().refreshToolchain();
    } catch (error) {
      const theme = themeFor(current.theme);
      set({ settings: current, theme });
      applyThemeCss(theme);
      useLang.getState().setLang(current.language);
      showError(error);
    }
  },

  refreshToolchain: async () => {
    try {
      set({ toolchain: await api.toolchain.info() });
    } catch (error) {
      showError(error);
    }
  },

  refreshRecent: async () => {
    try {
      set({ recent: await api.app.recent() });
    } catch (error) {
      showError(error);
    }
  },
}));

// Com "Do sistema", o tema acompanha a troca de esquema do sistema.
darkQuery.addEventListener('change', () => {
  const theme = themeFor(useApp.getState().settings?.theme ?? DEFAULT_THEME);
  applyThemeCss(theme);
  useApp.setState({ theme });
});
