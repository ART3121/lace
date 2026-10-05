// A hierarquia do design, elaborada pelo Icarus (`lace_core::hierarchy`).
//
// Atualiza sozinha depois de cada verificação, simulação, build ou síntese
// que passa (jobs.ts), e quando a vista abre sem nada. Quando um .v muda
// depois da última elaboração, fica marcada como desatualizada até a
// próxima.

import { create } from 'zustand';

import { api } from '../ipc/api';
import type { HierarchyResult, IpcError } from '../ipc/types';

interface HierarchyState {
  data: HierarchyResult | null;
  /** Quando a última elaboração terminou (ms desde 1970). */
  at: number;
  loading: boolean;
  error: IpcError | null;
  /** Um arquivo Verilog mudou depois da elaboração. */
  stale: boolean;
  refresh: () => Promise<void>;
  markStale: () => void;
  clear: () => void;
}

let again = false;

export const useHierarchy = create<HierarchyState>((set, get) => ({
  data: null,
  at: 0,
  loading: false,
  error: null,
  stale: false,

  refresh: async () => {
    // Um pedido durante outro roda de novo no fim, uma vez.
    if (get().loading) {
      again = true;
      return;
    }
    set({ loading: true });
    try {
      const data = await api.project.hierarchy();
      set({ data, at: Date.now(), error: null, stale: false });
    } catch (error) {
      set({ error: error as IpcError });
    } finally {
      set({ loading: false });
    }
    if (again) {
      again = false;
      await get().refresh();
    }
  },

  markStale: () => {
    if (get().data) set({ stale: true });
  },

  clear: () => set({ data: null, error: null, stale: false }),
}));
