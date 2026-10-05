// O histórico de relatórios: a versão que a lista observa e a limpeza
// (`lace report clean`), que a barra lateral, o menu e a paleta usam.
//
// A limpeza segue o Core: `planCleanup` escolhe sem apagar, a interface
// mostra e confirma, e `clean` apaga exatamente a lista mostrada.

import { create } from 'zustand';

import { t } from '../i18n';
import { api } from '../ipc/api';
import { confirm } from './dialogs';
import { useEditor } from './editor';
import { guarded, useToasts } from './toasts';

interface ReportsState {
  /** Muda quando relatórios entram ou saem; a lista relê. */
  version: number;
  bump: () => void;
}

export const useReports = create<ReportsState>((set, get) => ({
  version: 0,
  bump: () => set({ version: get().version + 1 }),
}));

/** Apaga os relatórios `ids` (os que `planCleanup` escolheu), fecha as abas
 * deles e avisa quantos saíram. */
export async function removeReports(ids: string[]): Promise<boolean> {
  if (ids.length === 0) return false;
  const result = await guarded(() => api.history.clean(ids));
  if (!result) return false;
  const gone = new Set(result.removed);
  const editor = useEditor.getState();
  for (const tab of editor.tabs) {
    const id = tab.data?.id;
    const against = tab.data?.against;
    if ((tab.kind === 'report' && id && gone.has(id)) || (tab.kind === 'compare' && ((id && gone.has(id)) || (against && gone.has(against))))) {
      await editor.closeTab(tab.id);
    }
  }
  useToasts.getState().push({
    kind: 'success',
    title: t('reports.removed', { count: result.removed.length }),
    detail: t('reports.kept', { count: result.kept }),
  });
  useReports.getState().bump();
  return true;
}

/** Apaga um relatório, depois de perguntar. */
export async function removeReport(id: string): Promise<void> {
  const answer = await confirm({
    title: t('reports.deleteTitle'),
    message: t('reports.deleteOne', { id }),
    buttons: [
      { label: t('common.delete'), value: 'yes', danger: true },
      { label: t('common.cancel'), value: 'no' },
    ],
  });
  if (answer !== 'yes') return;
  const ids = await guarded(() => api.history.planCleanup({ kind: 'reports', ids: [id] }));
  if (ids) await removeReports(ids);
}
