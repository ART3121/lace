// Os exercícios do `lace learn`: a pasta de exercícios aberta (lembrada
// entre as sessões), a última correção de cada exercício e as dicas à
// mostra.
//
// Cada exercício é um projeto Lace. Escolher um torna-o o atual e abre o
// projeto dele, o arquivo do aluno à esquerda e o enunciado à direita, sem
// corrigir. Cada gravação do arquivo (no Studio ou fora dele, pelo vigia de
// arquivos) corrige, como o modo watch do `lace learn`. A correção é a
// operação `learn` (flows.rs): grava antes os arquivos abertos e põe os
// erros no editor e na aba do enunciado.
//
// Com um exercício aberto, o Studio está numa sessão de exercícios: nada é
// gravado sozinho ao trocar de aba (MonacoHost.tsx), o painel fica
// escondido até o aluno chamá-lo, e a correção não revela console nenhum
// (jobs.ts). Ao sair da sessão (outro projeto, a pasta fechada), o painel
// volta como estava.

import { create } from 'zustand';

import { t, useLang } from '../i18n';
import { api } from '../ipc/api';
import type { Grade } from '../ipc/lace-types';
import type { IpcError, LearnExercise, LearnSnapshot } from '../ipc/types';
import { samePath } from '../util/paths';
import { confirm } from './dialogs';
import { fileTabId, useEditor, viewTabId } from './editor';
import { useJobs } from './jobs';
import { useLayout } from './layout';
import { learnSession } from './learnSession';
import { useProject } from './project';
import { guarded, showError, useToasts } from './toasts';
import { openWaveTab, useWaveReloads, waveInTab } from './waves';

const ROOT_KEY = 'lace-studio:learn-root';
const AUTO_KEY = 'lace-studio:learn-auto';
/** Se o painel estava à mostra antes da sessão de exercícios, para voltar
 * a ele ao sair (também depois de fechar e abrir o Studio no meio dela). */
const PANEL_KEY = 'lace-studio:learn-panel';

function stored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function store(key: string, value: string | null): void {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    // Sem armazenamento, só não é lembrado.
  }
}

interface LearnState {
  /** A pasta de exercícios aberta, ou a lembrada antes de abrir. */
  root: string | null;
  snapshot: LearnSnapshot | null;
  loading: boolean;
  /** Por que a pasta não abriu (componente que falta, pasta que sumiu). */
  error: IpcError | null;
  /** A última correção de cada exercício, nesta sessão. */
  grades: Record<string, Grade>;
  /** Quantas dicas de cada exercício estão à mostra. */
  hints: Record<string, number>;
  /** Corrigir a cada gravação do arquivo do exercício. */
  auto: boolean;

  /** Abre a pasta lembrada, se houver. */
  load: () => Promise<void>;
  /** Abre a pasta de exercícios que contém `root`. */
  openRoot: (root: string) => Promise<boolean>;
  /** Cria a pasta de exercícios em `dir` e abre o primeiro exercício. */
  create: (dir: string) => Promise<boolean>;
  /** Fecha a pasta (o projeto aberto continua). */
  forget: () => void;
  /** Torna atual, abre e corrige. */
  select: (name: string) => Promise<void>;
  /** Corrige o exercício (o atual, sem nome). */
  check: (name?: string) => Promise<Grade | null>;
  /** O próximo por resolver. */
  next: () => Promise<void>;
  showHint: (name: string) => void;
  reset: (name: string) => Promise<void>;
  openWave: (name?: string) => void;
  setAuto: (auto: boolean) => void;
  /** O vigia de arquivos avisou: o arquivo do exercício atual mudou? */
  onDiskChange: (paths: string[]) => void;
  /** Entra na sessão de exercícios: esconde o painel, guardando como
   * estava. */
  enterSession: () => void;
  /** Sai da sessão: o painel volta como estava antes dela. */
  leaveSession: () => void;
}

/** O exercício pelo nome. */
export function findExercise(snapshot: LearnSnapshot | null, name: string | null | undefined): LearnExercise | null {
  if (!snapshot || !name) return null;
  for (const chapter of snapshot.chapters) {
    const exercise = chapter.exercises.find((e) => e.name === name);
    if (exercise) return exercise;
  }
  return null;
}

/** O exercício atual. */
export function currentExercise(snapshot: LearnSnapshot | null): LearnExercise | null {
  return findExercise(snapshot, snapshot?.current);
}

/** Numa sessão de exercícios: o projeto aberto é um exercício da pasta. */
export function inLearnSession(): boolean {
  return openExercise() !== null;
}

/** O exercício cujo projeto está aberto, se for um da pasta. */
export function openExercise(): LearnExercise | null {
  const spf = useProject.getState().snapshot?.spf;
  const snapshot = useLearn.getState().snapshot;
  if (!spf || !snapshot) return null;
  for (const chapter of snapshot.chapters) {
    const exercise = chapter.exercises.find((e) => samePath(e.spf, spf));
    if (exercise) return exercise;
  }
  return null;
}

const lang = () => useLang.getState().lang;

/** Para não corrigir duas vezes a mesma gravação (o editor grava e o vigia
 * avisa logo depois). */
let diskTimer = 0;
/** A data de modificação do arquivo de cada exercício na última correção:
 * o aviso do vigia por uma gravação que já foi corrigida (a do próprio
 * "Corrigir", que grava antes de rodar) não pede outra. */
const checkedStamp: Record<string, number> = {};

async function stamp(path: string): Promise<number | null> {
  try {
    return (await api.fs.stat(path))?.modified_ms ?? null;
  } catch {
    return null;
  }
}

export const useLearn = create<LearnState>((set, get) => ({
  root: stored(ROOT_KEY),
  snapshot: null,
  loading: false,
  error: null,
  grades: {},
  hints: {},
  auto: stored(AUTO_KEY) !== 'off',

  load: async () => {
    const root = get().root;
    if (!root || get().snapshot) return;
    await get().openRoot(root);
  },

  openRoot: async (root) => {
    set({ loading: true, error: null });
    try {
      const snapshot = await api.learn.open(root, lang());
      store(ROOT_KEY, snapshot.root);
      set({ root: snapshot.root, snapshot, loading: false });
      // O Studio reabriu o projeto de um exercício antes de a pasta carregar.
      if (inLearnSession()) get().enterSession();
      return true;
    } catch (error) {
      set({ loading: false, error: error as IpcError, snapshot: null });
      return false;
    }
  },

  create: async (dir) => {
    set({ loading: true, error: null });
    try {
      const snapshot = await api.learn.init(dir, null, lang());
      store(ROOT_KEY, snapshot.root);
      set({ root: snapshot.root, snapshot, loading: false, grades: {}, hints: {} });
      await get().select(snapshot.current);
      return true;
    } catch (error) {
      // A vista mostra o erro; quem chama decide se abre a pasta que já
      // existe (`learn_workspace_exists`).
      set({ loading: false, error: error as IpcError });
      return false;
    }
  },

  forget: () => {
    get().leaveSession();
    store(ROOT_KEY, null);
    set({ root: null, snapshot: null, error: null, grades: {}, hints: {} });
  },

  select: async (name) => {
    const root = get().snapshot?.root;
    if (!root) return;
    const snapshot = await guarded(() => api.learn.setCurrent(root, name, lang()));
    if (!snapshot) return;
    // Cada visita começa sem a correção anterior: o resultado aparece quando
    // o aluno grava (ou pede).
    const grades = { ...get().grades };
    delete grades[name];
    set({ snapshot, grades });
    const exercise = findExercise(snapshot, name);
    if (!exercise) return;
    const project = useProject.getState();
    if (!samePath(project.snapshot?.spf, exercise.spf) && !(await project.open(exercise.spf))) return;
    const editor = useEditor.getState();
    // O código à esquerda e o enunciado à direita.
    await editor.openFile(exercise.file);
    editor.openView('learn');
    const view = viewTabId('learn');
    const left = useEditor.getState().groups[0];
    if (left?.tabIds.includes(view)) useEditor.getState().split(view, 'right');
    const first = useEditor.getState().groups[0];
    if (first) useEditor.getState().activate(fileTabId(exercise.file), first.id);
    const welcome = viewTabId('welcome');
    if (useEditor.getState().tabs.some((tab) => tab.id === welcome)) await useEditor.getState().closeTab(welcome);
    get().enterSession();
  },

  check: async (name) => {
    const snapshot = get().snapshot;
    const exercise = name ? findExercise(snapshot, name) : currentExercise(snapshot);
    if (!snapshot || !exercise) return null;
    if (useJobs.getState().running) return null;
    // A correção roda no projeto aberto: o do exercício. Pedida com outro
    // projeto aberto, abre o exercício antes (e não corrige se não abriu).
    if (!samePath(useProject.getState().snapshot?.spf, exercise.spf)) {
      await get().select(exercise.name);
      if (!samePath(useProject.getState().snapshot?.spf, exercise.spf)) return null;
    }
    // Grava antes, para saber o que esta correção viu; a operação grava de
    // novo, sem nada a gravar.
    if (!(await useEditor.getState().saveAll())) return null;
    const seen = await stamp(exercise.file);
    if (seen !== null) checkedStamp[exercise.name] = seen;
    const outcome = await useJobs.getState().run({ flow: 'learn', root: snapshot.root, exercise: exercise.name }, `learn:${exercise.name}`);
    const grade = outcome?.learn ?? null;
    if (grade) {
      set({ grades: { ...get().grades, [exercise.name]: grade } });
      // A onda aberta numa aba lê de novo a desta correção.
      if (grade.waveform) useWaveReloads.getState().bump(grade.waveform);
      // Resolvido ou não: a lista e a solução liberada mudam.
      const fresh = await api.learn.open(snapshot.root, lang()).catch(() => null);
      if (fresh) set({ snapshot: fresh });
    }
    return grade;
  },

  next: async () => {
    const snapshot = get().snapshot;
    if (!snapshot) return;
    const all = snapshot.chapters.flatMap((c) => c.exercises);
    const at = all.findIndex((e) => e.name === snapshot.current);
    const after = [...all.slice(at + 1), ...all.slice(0, Math.max(at, 0) + 1)];
    const pending = after.find((e) => !e.solved);
    if (pending) await get().select(pending.name);
  },

  showHint: (name) => {
    const exercise = findExercise(get().snapshot, name);
    if (!exercise) return;
    const shown = Math.min((get().hints[name] ?? 0) + 1, exercise.hints.length);
    set({ hints: { ...get().hints, [name]: shown } });
  },

  reset: async (name) => {
    const snapshot = get().snapshot;
    const exercise = findExercise(snapshot, name);
    if (!snapshot || !exercise) return;
    const answer = await confirm({
      title: t('learn.resetTitle'),
      message: t('learn.resetMessage', { file: exercise.module + '.v' }),
      buttons: [
        { label: t('common.cancel'), value: 'cancel' },
        { label: t('learn.reset'), value: 'reset', danger: true },
      ],
    });
    if (answer !== 'reset') return;
    try {
      await api.learn.reset(snapshot.root, name);
      // O editor relê o arquivo pelo vigia; a aba aberta sem mudanças
      // recarrega sozinha.
      set({ hints: { ...get().hints, [name]: 0 } });
    } catch (error) {
      showError(error);
    }
  },

  openWave: (name) => {
    const snapshot = get().snapshot;
    const exercise = name ? findExercise(snapshot, name) : currentExercise(snapshot);
    if (!exercise) return;
    // Na aba ou na janela, a onda abre com o layout da correção (wave_tab.rs
    // e flows.rs acham o `wave.sucl` ao lado dela).
    if (waveInTab()) {
      openWaveTab(exercise.waveform, true);
      return;
    }
    void guarded(() => api.app.openWave(null, exercise.waveform)).then((opened) => {
      if (opened) useToasts.getState().push({ kind: 'info', title: t('console.waveOpened', { pid: opened.pid, path: opened.waveform }) });
    });
  },

  setAuto: (auto) => {
    store(AUTO_KEY, auto ? null : 'off');
    set({ auto });
  },

  onDiskChange: (paths) => {
    const { auto, snapshot } = get();
    const exercise = currentExercise(snapshot);
    if (!auto || !exercise || !samePath(useProject.getState().snapshot?.spf, exercise.spf)) return;
    if (!paths.some((p) => samePath(p, exercise.file))) return;
    window.clearTimeout(diskTimer);
    const attempt = async () => {
      // Com uma operação rodando, espera ela acabar: a gravação não se perde.
      if (useJobs.getState().running) {
        diskTimer = window.setTimeout(() => void attempt(), 300);
        return;
      }
      const now = await stamp(exercise.file);
      if (now !== null && now === checkedStamp[exercise.name]) return;
      void get().check(exercise.name);
    };
    diskTimer = window.setTimeout(() => void attempt(), 300);
  },

  enterSession: () => {
    learnSession.set(true);
    if (stored(PANEL_KEY) !== null) return;
    const layout = useLayout.getState();
    store(PANEL_KEY, layout.live.regions.panel.visible ? 'shown' : 'hidden');
    layout.setRegionVisible('panel', false);
  },

  leaveSession: () => {
    learnSession.set(false);
    const before = stored(PANEL_KEY);
    if (before === null) return;
    store(PANEL_KEY, null);
    useLayout.getState().setRegionVisible('panel', before === 'shown');
  },
}));

// Abrir outro projeto (ou fechar o projeto) sai da sessão de exercícios;
// abrir o projeto de um exercício por fora da vista (recentes, Abrir
// projeto) entra nela.
useProject.subscribe((state, previous) => {
  if (state.snapshot?.spf === previous.snapshot?.spf) return;
  if (inLearnSession()) useLearn.getState().enterSession();
  else useLearn.getState().leaveSession();
});
