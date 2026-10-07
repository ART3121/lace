// O projeto aberto: o retrato que o backend devolve (o `lace status`) e o
// alvo escolhido na barra (o projeto inteiro ou um processador).
//
// O retrato é relido do disco sempre que algo muda: depois de cada
// operação, de cada mudança no .spf e quando o vigia de arquivos avisa.

import { getCurrentWindow } from '@tauri-apps/api/window';
import { create } from 'zustand';

import { api } from '../ipc/api';
import type { ProjectSnapshot } from '../ipc/types';
import { useApp } from './app';
import { useEditor } from './editor';
import { useHierarchy } from './hierarchy';
import { useLayout } from './layout';
import { guarded, showError } from './toasts';

interface ProjectState {
  snapshot: ProjectSnapshot | null;
  /** Processador alvo; `null` é o projeto (testbench). */
  target: string | null;
  /** Muda quando a árvore de arquivos precisa ser relida. */
  treeVersion: number;
  /** Fechando: as abas somem uma a uma e a sessão gravada não pode mudar. */
  closing: boolean;

  open: (path: string) => Promise<boolean>;
  create: (parent: string, name: string) => Promise<boolean>;
  close: () => Promise<boolean>;
  refresh: () => Promise<void>;
  setTarget: (name: string | null) => void;
  bumpTree: () => void;
}

/** O título da janela, o do sistema (barra de tarefas, Alt+Tab) e o da barra
 * de título integrada (TitleBar.tsx). */
export function windowTitle(name: string | null): string {
  return name ? `${name} - Lace Studio` : 'Lace Studio';
}

async function setTitle(name: string | null): Promise<void> {
  try {
    await getCurrentWindow().setTitle(windowTitle(name));
  } catch {
    // Fora do Tauri (vite no navegador) não há janela.
  }
}

function loadTarget(spf: string): string | null {
  try {
    return localStorage.getItem(`lace-studio:target:${spf}`);
  } catch {
    return null;
  }
}

async function activate(snapshot: ProjectSnapshot): Promise<void> {
  const saved = loadTarget(snapshot.spf);
  const target = saved && snapshot.processors.some((p) => p.name === saved) ? saved : null;
  useProject.setState({ snapshot, target, treeVersion: useProject.getState().treeVersion + 1 });
  useHierarchy.getState().clear();
  if (useLayout.getState().explorerMode === 'hierarchy') void useHierarchy.getState().refresh();
  void setTitle(snapshot.name);
  void useApp.getState().refreshRecent();
  await useEditor.getState().restoreSession(snapshot.spf);
  if (useEditor.getState().tabs.length === 0) useEditor.getState().openView('welcome');
}

export const useProject = create<ProjectState>((set, get) => ({
  snapshot: null,
  target: null,
  treeVersion: 0,
  closing: false,

  open: async (path) => {
    if (get().snapshot && !(await get().close())) return false;
    const snapshot = await guarded(() => api.project.open(path));
    if (!snapshot) {
      void useApp.getState().refreshRecent();
      return false;
    }
    await activate(snapshot);
    return true;
  },

  create: async (parent, name) => {
    if (get().snapshot && !(await get().close())) return false;
    const snapshot = await guarded(() => api.project.create(parent, name));
    if (!snapshot) return false;
    await activate(snapshot);
    return true;
  },

  close: async () => {
    const snapshot = get().snapshot;
    if (!snapshot) return true;
    useEditor.getState().persistSession(snapshot.spf);
    // `closing` segura a gravação da sessão até o projeto sair de vez: o
    // `reset` do editor também mexe nas abas, e soltar antes dele gravava a
    // sessão vazia por cima das abas que acabaram de ser salvas.
    set({ closing: true });
    const closed = await useEditor.getState().closeAll();
    if (!closed) {
      set({ closing: false });
      return false;
    }
    useEditor.getState().reset();
    await guarded(() => api.project.close());
    set({ snapshot: null, target: null, closing: false });
    useHierarchy.getState().clear();
    void setTitle(null);
    useEditor.getState().openView('welcome');
    return true;
  },

  refresh: async () => {
    if (!get().snapshot) return;
    try {
      const snapshot = await api.project.snapshot();
      const target = get().target;
      set({
        snapshot,
        target: target && snapshot.processors.some((p) => p.name === target) ? target : null,
      });
    } catch (error) {
      // O projeto sumiu do disco ou o .spf ficou ilegível: mostra o erro e
      // mantém o último retrato, para o usuário salvar o que tem aberto.
      showError(error);
    }
  },

  setTarget: (name) => {
    set({ target: name });
    const spf = get().snapshot?.spf;
    if (!spf) return;
    try {
      if (name) localStorage.setItem(`lace-studio:target:${spf}`, name);
      else localStorage.removeItem(`lace-studio:target:${spf}`);
    } catch {
      // Sem armazenamento, o alvo só não é lembrado.
    }
  },

  bumpTree: () => set({ treeVersion: get().treeVersion + 1 }),
}));

// A sessão (abas abertas, em que grupo) é gravada a cada mudança de abas.
useEditor.subscribe((state, previous) => {
  if (state.tabs === previous.tabs && state.groups === previous.groups && state.activeGroup === previous.activeGroup) return;
  const { snapshot, closing } = useProject.getState();
  if (snapshot && !closing) state.persistSession(snapshot.spf);
});
