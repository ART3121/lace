// O histórico de relatórios do projeto (`lace report list`), do mais novo
// para o mais antigo, com a limpeza (`lace report clean`): o botão da
// lixeira abre o diálogo de limpar, e o menu de cada relatório o apaga.

import { GitCompare, RefreshCw, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';

import { useT } from '../../i18n';
import { api } from '../../ipc/api';
import type { RunSummary } from '../../ipc/lace-types';
import { openDialog } from '../../state/dialogs';
import { useEditor } from '../../state/editor';
import { useJobs } from '../../state/jobs';
import { useProject } from '../../state/project';
import { removeReport, useReports } from '../../state/reports';
import { showError } from '../../state/toasts';
import { Empty, IconButton, openContextMenu, StatusDot } from '../common';
import { ViewActions } from '../layout/ViewActions';

function when(timestamp: string | null): string {
  if (!timestamp) return '';
  const date = new Date(timestamp);
  return Number.isNaN(date.getTime()) ? timestamp : date.toLocaleString();
}

export function ReportsPanel() {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot);
  const lastRun = useJobs((s) => s.last?.finishedAt);
  const [reports, setReports] = useState<RunSummary[]>([]);
  const version = useReports((s) => s.version);

  useEffect(() => {
    if (!snapshot) return;
    let cancelled = false;
    api.history
      .list()
      .then((list) => !cancelled && setReports(list))
      .catch((error) => {
        if (!cancelled) setReports([]);
        if ((error as { code?: string }).code !== 'no_reports') showError(error);
      });
    return () => {
      cancelled = true;
    };
  }, [snapshot?.spf, lastRun, version]);

  if (!snapshot) {
    return (
      <div className="sidebar__body">
        <Empty>{t('explorer.noProject')}</Empty>
      </div>
    );
  }

  return (
    <>
      <ViewActions>
        <IconButton label={t('action.cleanReports')} disabled={reports.length === 0} onClick={() => openDialog({ kind: 'cleanReports' })}>
          <Trash2 size={14} />
        </IconButton>
        <IconButton label={t('common.refresh')} onClick={() => useReports.getState().bump()}>
          <RefreshCw size={14} />
        </IconButton>
      </ViewActions>
      <div className="sidebar__toolbar">
        <span className="sidebar__subtitle">{t('reports.count', { count: reports.length })}</span>
      </div>
      <div className="sidebar__body">
        {reports.length === 0 && <Empty>{t('reports.empty')}</Empty>}
        <ul className="report-list">
          {reports.map((report) => (
            <li key={report.id}>
              <button
                type="button"
                className="report-list__item"
                disabled={!report.readable}
                onClick={() => useEditor.getState().openView('report', { id: report.id })}
                onContextMenu={(e) =>
                  openContextMenu(e, [
                    { label: t('common.open'), disabled: !report.readable, run: () => useEditor.getState().openView('report', { id: report.id }) },
                    { label: t('reports.compare'), run: () => useEditor.getState().openView('compare', { id: report.id, against: null }) },
                    { label: t('reports.copyId'), run: () => void navigator.clipboard.writeText(report.id) },
                    { separator: true },
                    { label: t('common.delete'), danger: true, run: () => void removeReport(report.id) },
                  ])
                }
              >
                <StatusDot status={report.status === 'succeeded' ? 'ok' : report.status ? 'failed' : undefined} />
                <span className="report-list__main">
                  <span className="report-list__command" title={report.command ?? ''}>
                    {report.command ?? t('reports.unreadable')}
                  </span>
                  <span className="report-list__meta">
                    {report.id} · {when(report.timestamp)}
                    {report.synthesis && ` · ${t('reports.synthesis')}`}
                    {report.simulation !== 'unavailable' && ` · ${t('reports.simulation')}`}
                  </span>
                </span>
              </button>
              <IconButton
                label={t('reports.compare')}
                className="report-list__compare"
                onClick={() => useEditor.getState().openView('compare', { id: report.id, against: null })}
              >
                <GitCompare size={13} />
              </IconButton>
            </li>
          ))}
        </ul>
      </div>
    </>
  );
}
