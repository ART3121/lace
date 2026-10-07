// A página das Preferências que está aberta (Geral, Aparência, Layout da
// janela...). Fica no localStorage, por conveniência: Ctrl+, volta à última
// vista. "Personalizar layout..." abre direto na do layout.

import { create } from 'zustand';

export const SETTINGS_PAGES = ['general', 'appearance', 'layout', 'editor', 'simulation', 'toolchain'] as const;
export type SettingsPage = (typeof SETTINGS_PAGES)[number];

const STORAGE_KEY = 'lace-studio:settings-page';

function load(): SettingsPage {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    return (SETTINGS_PAGES as readonly string[]).includes(saved ?? '') ? (saved as SettingsPage) : 'general';
  } catch {
    return 'general';
  }
}

export const useSettingsPage = create<{ page: SettingsPage; setPage: (page: SettingsPage) => void }>((set) => ({
  page: load(),
  setPage: (page) => {
    set({ page });
    try {
      localStorage.setItem(STORAGE_KEY, page);
    } catch {
      // Sem armazenamento, a página só não é lembrada.
    }
  },
}));
