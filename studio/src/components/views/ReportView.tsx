// Um relatório do histórico (`lace report show`): o texto que o Lace grava
// em .lace/reports/run-NNNNNN/report.txt.

import { GitCompare, FileText } from 'lucide-react';
import { useEffect, useState } from 'react';

import { useT } from '../../i18n';
import { api, type ReportView as Report } from '../../ipc/api';
import { useEditor } from '../../state/editor';
import { showError } from '../../state/toasts';
import { Button } from '../common';

export function ReportView({ id }: { id: string }) {
  const t = useT();
  const [report, setReport] = useState<Report | null>(null);

  useEffect(() => {
    let cancelled = false;
    api.history
      .show(id)
      .then((r) => !cancelled && setReport(r))
      .catch(showError);
    return () => {
      cancelled = true;
    };
  }, [id]);

  if (!report) return <div className="view-page">{t('common.loading')}</div>;

  return (
    <div className="view-page report">
      <div className="view-page__title">
        <h1 className="mono">{report.id}</h1>
        <div className="button-row">
          <Button
            icon={<GitCompare size={14} />}
            onClick={() => useEditor.getState().openView('compare', { id: report.id, against: null })}
          >
            {t('reports.compare')}
          </Button>
          <Button icon={<FileText size={14} />} onClick={() => void useEditor.getState().openFile(report.path)}>
            {t('reports.openFile')}
          </Button>
        </div>
      </div>
      <pre className="report__text">{report.text}</pre>
    </div>
  );
}
