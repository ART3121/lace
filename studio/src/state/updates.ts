// A atualização do Lace pelo Studio: a busca da release nova (`lace update
// --check`) ao abrir e a cada 12 horas, a oferta, a atualização (`lace
// update --yes`, em jobs.ts) e o reinício do Studio na versão nova.
//
// A busca automática não mostra erro: sem rede, ela só tenta de novo na
// próxima vez. Uma versão nova é oferecida uma vez por sessão, num diálogo
// se nada estiver aberto nem rodando, senão num aviso; o indicador da barra
// de status fica até a atualização.
//
// Atualizado, o Studio grava os arquivos abertos e a sessão e reinicia
// (`app_exit`, em commands/app.rs). Com o assistente de instalação do
// Windows, que só troca os arquivos com o Studio fechado, ele fecha, e o
// assistente o abre de novo quando termina.

import { invoke } from '@tauri-apps/api/core';
import { create } from 'zustand';

import { t } from '../i18n';
import { api } from '../ipc/api';
import type { UpdateReport } from '../ipc/lace-types';
import { useApp } from './app';
import { confirm, useDialogs } from './dialogs';
import { useEditor } from './editor';
import { useJobs } from './jobs';
import { useProject } from './project';
import { showError, useToasts } from './toasts';

/** De quanto em quanto tempo procurar com o Studio aberto. */
const INTERVAL_MS = 12 * 60 * 60 * 1000;
/** Quanto esperar depois de abrir: a busca não disputa com a abertura do
 * último projeto. */
const FIRST_CHECK_MS = 5000;
/** Quanto o aviso de reinício fica à vista antes de a janela fechar. */
const RESTART_DELAY_MS = 1500;
/** A versão para a qual o Studio reiniciou, que a abertura seguinte avisa. */
const UPDATED_KEY = 'lace.updatedTo';

/** A atualização terminou e o Studio ainda não saiu, porque algum arquivo
 * não foi gravado: `restart` reinicia, `quit` fecha para o assistente. */
export interface PendingExit {
  mode: 'restart' | 'quit';
  version: string;
}

interface UpdatesState {
  /** O último `lace update --check`, da busca automática ou do botão. */
  report: UpdateReport | null;
  checking: boolean;
  pending: PendingExit | null;
  /** Procura. `auto`: a busca automática, sem erro à vista e com a oferta da
   * versão nova. */
  check: (auto?: boolean) => Promise<UpdateReport | null>;
}

/** As versões já oferecidas nesta sessão. */
const offered = new Set<string>();

/** Há Lace mais novo, e esta instalação veio do instalador. */
export function canUpdate(report: UpdateReport | null): report is UpdateReport {
  return !!report && report.lace.newer && !!report.prefix;
}

export const useUpdates = create<UpdatesState>((set, get) => ({
  report: null,
  checking: false,
  pending: null,

  check: async (auto = false) => {
    if (get().checking) return get().report;
    set({ checking: true });
    let report: UpdateReport | null = null;
    try {
      report = (await api.toolchain.updateCheck()) as UpdateReport;
    } catch (error) {
      if (!auto) showError(error);
      else {
        const message = `Automatic update check: ${(error as { message?: string })?.message ?? String(error)}`;
        void invoke('log_frontend', { level: 'info', message }).catch(() => undefined);
      }
    }
    set(report ? { checking: false, report } : { checking: false });
    if (auto && canUpdate(report) && !offered.has(report.lace.latest)) {
      offered.add(report.lace.latest);
      if (!useDialogs.getState().dialog && !useJobs.getState().running) void offerUpdate();
      else useToasts.getState().push({ kind: 'info', title: t('update.availableToast', { version: report.lace.latest }) }, 10000);
    }
    return report;
  },
}));

/** Oferece a atualização do último `lace update --check` e, com o sim,
 * atualiza e reinicia. */
export async function offerUpdate(): Promise<void> {
  const report = useUpdates.getState().report;
  if (!canUpdate(report)) return;
  const windows = useApp.getState().info?.os.startsWith('windows') ?? false;
  const answer = await confirm({
    title: t('dialog.update.title', { version: report.lace.latest }),
    message: t(windows ? 'dialog.update.messageWindows' : 'dialog.update.message', {
      installed: report.lace.installed,
      latest: report.lace.latest,
      prefix: report.prefix ?? '',
    }),
    buttons: [
      { label: t('dialog.update.confirm'), value: 'update', primary: true },
      { label: t('dialog.update.later'), value: 'later' },
    ],
  });
  if (answer !== 'update') return;
  const result = await useJobs.getState().update();
  if (!result) return;
  if (result.action === 'updated') {
    await leave({ mode: 'restart', version: result.lace.latest });
  } else if (result.action === 'wizard_opened') {
    useUpdates.setState({ pending: { mode: 'quit', version: result.lace.latest } });
    const close = await confirm({
      title: t('dialog.updateWizard.title'),
      message: t('dialog.updateWizard.message', { version: result.lace.latest }),
      buttons: [{ label: t('dialog.updateWizard.close'), value: 'close', primary: true }],
    });
    if (close === 'close') await leave({ mode: 'quit', version: result.lace.latest });
  } else {
    // Nada mais novo, afinal: o indicador sai.
    useUpdates.setState({ report: result });
  }
}

/** Conclui a saída que ficou para depois (o indicador da barra de status). */
export function finishPending(): void {
  const pending = useUpdates.getState().pending;
  if (pending) void leave(pending);
}

/** Grava a sessão e os arquivos abertos e sai do Studio, reiniciando ou não.
 * Um arquivo que não grava (um conflito com o disco que o usuário cancelou)
 * segura a saída, que fica no indicador da barra de status. */
async function leave(exit: PendingExit): Promise<void> {
  useUpdates.setState({ pending: exit });
  const snapshot = useProject.getState().snapshot;
  if (snapshot) useEditor.getState().persistSession(snapshot.spf);
  if (!(await useEditor.getState().saveAll())) {
    const key = exit.mode === 'restart' ? 'update.restartBlocked' : 'update.quitBlocked';
    useToasts.getState().push({ kind: 'warning', title: t(key) }, 15000);
    return;
  }
  if (exit.mode === 'restart') {
    try {
      localStorage.setItem(UPDATED_KEY, exit.version);
    } catch {
      // Sem armazenamento, só não há o aviso na volta.
    }
    useToasts.getState().push({ kind: 'info', title: t('update.restarting', { version: exit.version }) });
    await new Promise((resolve) => window.setTimeout(resolve, RESTART_DELAY_MS));
  }
  try {
    await api.app.exit(exit.mode === 'restart');
  } catch (error) {
    showError(error);
  }
}

/** Ao abrir: avisa se o Studio acabou de reiniciar numa versão nova e começa
 * a busca automática, se a preferência deixa. Devolve o que a desliga. */
export function startUpdateChecks(): () => void {
  let updatedTo: string | null = null;
  try {
    updatedTo = localStorage.getItem(UPDATED_KEY);
    localStorage.removeItem(UPDATED_KEY);
  } catch {
    // Sem armazenamento, sem o aviso.
  }
  if (updatedTo) useToasts.getState().push({ kind: 'success', title: t('update.done', { version: updatedTo }) }, 6000);

  const auto = () => {
    if (useApp.getState().settings?.check_updates === false) return;
    void useUpdates.getState().check(true);
  };
  const first = window.setTimeout(auto, FIRST_CHECK_MS);
  const every = window.setInterval(auto, INTERVAL_MS);
  return () => {
    window.clearTimeout(first);
    window.clearInterval(every);
  };
}
