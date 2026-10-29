// O projeto aberto: o retrato que o backend devolve (o `aurora status`) e o
// alvo escolhido na barra (o projeto inteiro ou um processador).
//
// O retrato é relido do disco sempre que algo muda: depois de cada
// operação, de cada mudança no .spf e quando o vigia de arquivos avisa.

import { getCurrentWindow } from '@tauri-apps/api/window';
import { create } from 'zustand';

import { t } from '../i18n';
import { api } from '../ipc/api';
import type { IpcError, ProjectSnapshot } from '../ipc/types';
import { baseName } from '../util/paths';
import { useApp } from './app';
import { confirm } from './dialogs';
import { useEditor } from './editor';
import { useHierarchy } from './hierarchy';
import { useLayout } from './layout';
import { guarded, onErrorCode, showError, useToasts } from './toasts';

interface ProjectState {
  snapshot: ProjectSnapshot | null;
  /** Processador alvo; `null` é o projeto (testbench). */
  target: string | null;
  /** Muda quando a árvore de arquivos precisa ser relida. */
  treeVersion: number;
  /** Fechando: as abas somem uma a uma e a sessão gravada não pode mudar. */
  closing: boolean;
  /** O `.spf` do projeto aberto foi apagado: sem ele não há operação de
   * projeto (compilar, simular, mexer nas fontes). A pasta continua aberta,
   * com as abas e a árvore de arquivos, e o Explorer oferece criar o projeto
   * de novo. Volta a `false` quando o `.spf` reaparece. */
  spfMissing: boolean;
  /** Com `spfMissing`, a pasta do projeto também não existe mais: ela foi
   * movida, renomeada ou apagada, e criar o projeto de novo no lugar dela
   * não traz nada de volta. O aviso manda abrir o projeto no lugar novo. */
  rootMissing: boolean;

  open: (path: string) => Promise<boolean>;
  create: (parent: string, name: string) => Promise<boolean>;
  close: () => Promise<boolean>;
  refresh: () => Promise<void>;
  /** Liga `spfMissing`. Só na primeira vez avisa e traz o Explorer, e então
   * devolve `true`. */
  markSpfMissing: () => boolean;
  setTarget: (name: string | null) => void;
  bumpTree: () => void;
}

/** O título da janela, o do sistema (barra de tarefas, Alt+Tab) e o da barra
 * de título integrada (TitleBar.tsx). */
export function windowTitle(name: string | null): string {
  return name ? `${name} - AURORA IDE` : 'AURORA IDE';
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
    return localStorage.getItem(`aurora-ide:target:${spf}`);
  } catch {
    return null;
  }
}

async function activate(snapshot: ProjectSnapshot): Promise<void> {
  const saved = loadTarget(snapshot.spf);
  const target = saved && snapshot.processors.some((p) => p.name === saved) ? saved : null;
  useProject.setState({
    snapshot,
    target,
    spfMissing: false,
    rootMissing: false,
    treeVersion: useProject.getState().treeVersion + 1,
  });
  useHierarchy.getState().clear();
  if (useLayout.getState().explorerMode === 'hierarchy') void useHierarchy.getState().refresh();
  void setTitle(snapshot.name);
  void useApp.getState().refreshRecent();
  await useEditor.getState().restoreSession(snapshot.spf);
  // Um projeto da estrutura de pastas do YANC 6.x não compila: a AURORA
  // pergunta ao abrir se pode converter, e o Studio também.
  if (legacyProcessors(snapshot).length > 0) void convertLayout();
}

/** Os processadores ainda na estrutura de pastas do YANC 6.x. */
function legacyProcessors(snapshot: ProjectSnapshot): string[] {
  return snapshot.issues
    .filter((issue) => issue.kind === 'legacy_layout' && issue.detail)
    .map((issue) => issue.detail!);
}

/** Converte o projeto da estrutura de pastas do YANC 6.x para a do 7.1
 * (`aurora convert`), perguntando antes, e mostra o que mudou de lugar. */
export async function convertLayout(): Promise<void> {
  const snapshot = useProject.getState().snapshot;
  if (!snapshot) return;
  const legacy = legacyProcessors(snapshot);
  if (legacy.length === 0) return;
  const answer = await confirm({
    title: t('dialog.convertLayout.title'),
    message: t('dialog.convertLayout.message', { processors: legacy.join(', ') }),
    buttons: [
      { label: t('dialog.convertLayout.convert'), value: 'convert', primary: true },
      { label: t('dialog.convertLayout.later'), value: 'later' },
    ],
  });
  if (answer !== 'convert') return;
  const report = await guarded(() => api.project.convertLayout());
  if (!report) return;
  await useProject.getState().refresh();
  useProject.getState().bumpTree();
  const root = snapshot.root.replace(/\\/g, '/');
  const shown = (path: string) => {
    const p = path.replace(/\\/g, '/');
    return p.startsWith(`${root}/`) ? p.slice(root.length + 1) : p;
  };
  const lines = [t('dialog.convertLayout.moved', { count: report.moved.length })];
  for (const s of report.skipped) {
    lines.push(t('dialog.convertLayout.skipped', { path: shown(s.from), reason: s.reason }));
  }
  for (const line of report.name_lines.filter((l) => l.old_name !== l.processor)) {
    lines.push(t('dialog.convertLayout.renamed', { processor: line.processor, old: line.old_name }));
  }
  for (const name of report.invalid_names) {
    lines.push(t('dialog.convertLayout.invalid', { processor: name }));
  }
  for (const dir of report.remaining_dirs) {
    lines.push(t('dialog.convertLayout.remaining', { path: shown(dir) }));
  }
  await confirm({
    title: t('dialog.convertLayout.doneTitle'),
    message: lines.join('\n'),
    buttons: [{ label: t('dialog.convertLayout.ok'), value: 'ok', primary: true }],
  });
}

export const useProject = create<ProjectState>((set, get) => ({
  snapshot: null,
  target: null,
  treeVersion: 0,
  closing: false,
  spfMissing: false,
  rootMissing: false,

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
    set({ snapshot: null, target: null, closing: false, spfMissing: false, rootMissing: false });
    useHierarchy.getState().clear();
    void setTitle(null);
    return true;
  },

  refresh: async () => {
    if (!get().snapshot) return;
    try {
      const snapshot = await api.project.snapshot();
      const target = get().target;
      // O .spf voltou (restaurado por fora, ou recriado pela CLI).
      if (get().spfMissing) {
        useToasts.getState().push({ kind: 'success', title: t('project.spfBack') });
      }
      set({
        snapshot,
        spfMissing: false,
        rootMissing: false,
        target: target && snapshot.processors.some((p) => p.name === target) ? target : null,
      });
    } catch (error) {
      // O .spf foi apagado: o aviso do projeto, uma vez só, e não um erro a
      // cada arquivo que o vigia vê mudar.
      if ((error as IpcError)?.code === 'spf_missing') {
        get().markSpfMissing();
        return;
      }
      // O .spf ficou ilegível: mostra o erro e mantém o último retrato, para
      // o usuário salvar o que tem aberto.
      showError(error);
    }
  },

  markSpfMissing: () => {
    const snapshot = get().snapshot;
    if (!snapshot || get().spfMissing) return false;
    set({ spfMissing: true });
    // Sem o .spf e sem a pasta, o projeto foi movido, renomeado ou apagado
    // inteiro: o aviso é outro, e não oferece criar o projeto no lugar vazio.
    void api.fs
      .stat(snapshot.root)
      .catch(() => null)
      .then((stat) => {
        if (get().snapshot !== snapshot || !get().spfMissing) return;
        const rootMissing = !stat?.is_dir;
        set({ rootMissing });
        useToasts.getState().push(
          rootMissing
            ? {
                kind: 'warning',
                title: t('project.rootMissing.title', { name: snapshot.name }),
                detail: t('project.rootMissing.detail', { path: snapshot.root }),
              }
            : {
                kind: 'warning',
                title: t('project.spfMissing.title', { file: baseName(snapshot.spf) }),
                detail: t('project.spfMissing.detail'),
              },
          15000,
        );
        // O aviso com o botão (criar o projeto, ou abrir no lugar novo) fica
        // no Explorer.
        useLayout.getState().revealView('explorer');
      });
    return true;
  },

  setTarget: (name) => {
    set({ target: name });
    const spf = get().snapshot?.spf;
    if (!spf) return;
    try {
      if (name) localStorage.setItem(`aurora-ide:target:${spf}`, name);
      else localStorage.removeItem(`aurora-ide:target:${spf}`);
    } catch {
      // Sem armazenamento, o alvo só não é lembrado.
    }
  },

  bumpTree: () => set({ treeVersion: get().treeVersion + 1 }),
}));

// Uma operação que falha porque o .spf sumiu liga o aviso do projeto. Da
// primeira vez ele mesmo explica, e o erro genérico não aparece; depois, quem
// tentou uma operação recebe o erro.
onErrorCode('spf_missing', () => useProject.getState().markSpfMissing());

// A sessão (abas abertas, em que grupo) é gravada a cada mudança de abas.
useEditor.subscribe((state, previous) => {
  if (state.tabs === previous.tabs && state.groups === previous.groups && state.activeGroup === previous.activeGroup) return;
  const { snapshot, closing } = useProject.getState();
  if (snapshot && !closing) state.persistSession(snapshot.spf);
});
