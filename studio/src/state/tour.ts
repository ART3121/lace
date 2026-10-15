// O tour pela interface (components/tour/Tour.tsx): a janela escurece, e
// uma parte de cada vez fica acesa, com um cartão que diz o que ela faz. A
// parte acesa continua viva: o clique nela funciona, e alguns passos pedem
// uma tarefa (abrir o menu Fluxo, trocar de vista, esconder e mostrar o
// painel) e avançam sozinhos quando ela é feita.
//
// Abre sozinho num Studio recém-instalado (`tour_done` das preferências,
// settings.rs), com o tema, o idioma e o layout padrão; volta por Ajuda >
// Tour pela interface e pela tela inicial.

import { create } from 'zustand';

import { useApp } from './app';
import { useDialogs } from './dialogs';
import { useLayout } from './layout';

interface TourState {
  active: boolean;
  step: number;
  /** Começa do primeiro passo: fecha o diálogo aberto e sai do zen, que
   * esconderia as partes que o tour mostra. */
  start: () => void;
  setStep: (step: number) => void;
  finish: () => void;
}

/** Grava que o tour já apareceu: ele não abre mais sozinho. */
function markDone() {
  if (useApp.getState().settings?.tour_done !== false) return;
  void useApp.getState().updateSettings((settings) => ({ ...settings, tour_done: true }));
}

export const useTour = create<TourState>((set) => ({
  active: false,
  step: 0,
  start: () => {
    useDialogs.getState().close();
    useLayout.getState().exitZen();
    markDone();
    set({ active: true, step: 0 });
  },
  setStep: (step) => set({ step }),
  finish: () => set({ active: false, step: 0 }),
}));
