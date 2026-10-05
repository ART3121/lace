// A comparação de dois relatórios (`lace report compare`): estatísticas de
// síntese e tempos de simulação. Uma mudança é só descrita: tempo maior
// não é chamado de regressão, como na CLI.

import { useEffect, useState } from 'react';

import { useT, type Key } from '../../i18n';
import { api } from '../../ipc/api';
import type { MetricComparison, RunComparison } from '../../ipc/lace-types';
import { Empty } from '../common';
import { errorText } from '../../i18n';
import type { IpcError } from '../../ipc/types';

function Change({ comparison }: { comparison: MetricComparison }) {
  const t = useT();
  const percent =
    comparison.percent === null ? '' : ` (${comparison.percent > 0 ? '+' : ''}${comparison.percent.toFixed(1)}%)`;
  const delta = comparison.delta === null ? '' : `${comparison.delta > 0 ? '+' : ''}${comparison.delta}`;
  return (
    <span className={`change change--${comparison.change}`}>
      {t(`compare.change.${comparison.change}` as Key)} {delta}
      {percent}
    </span>
  );
}

function Row({ label, comparison }: { label: string; comparison: MetricComparison }) {
  const t = useT();
  const show = (v: number | null) => (v === null ? t('common.notReported') : v.toLocaleString());
  return (
    <tr>
      <td>{label}</td>
      <td className="num">{show(comparison.baseline)}</td>
      <td className="num">{show(comparison.current)}</td>
      <td>
        <Change comparison={comparison} />
      </td>
    </tr>
  );
}

export function CompareView({ id, against }: { id: string | null; against: string | null }) {
  const t = useT();
  const [comparison, setComparison] = useState<RunComparison | null>(null);
  const [error, setError] = useState<IpcError | null>(null);

  useEffect(() => {
    let cancelled = false;
    api.history
      .compare(id, against)
      .then((c) => !cancelled && setComparison(c))
      .catch((e: IpcError) => !cancelled && setError(e));
    return () => {
      cancelled = true;
    };
  }, [id, against]);

  if (error) {
    return (
      <div className="view-page">
        <Empty>
          <p>{errorText(error)}</p>
          <p className="muted">{error.message}</p>
        </Empty>
      </div>
    );
  }
  if (!comparison) return <div className="view-page">{t('common.loading')}</div>;

  const head = (
    <thead>
      <tr>
        <th />
        <th className="num">{t('compare.baseline')}</th>
        <th className="num">{t('compare.current')}</th>
        <th>{t('compare.change')}</th>
      </tr>
    </thead>
  );

  return (
    <div className="view-page compare">
      <h1>{t('compare.title', { current: comparison.current_id, baseline: comparison.baseline_id })}</h1>

      <h2>{t('compare.synthesis')}</h2>
      {comparison.synthesis ? (
        <table className="table">
          {head}
          <tbody>
            {comparison.synthesis.metrics.map((m) => (
              <Row key={m.metric} label={t(`synthesis.metric.${m.metric}` as Key)} comparison={m.comparison} />
            ))}
            {comparison.synthesis.cell_types.map((c) => (
              <Row key={c.cell_type} label={c.cell_type} comparison={c.usage} />
            ))}
          </tbody>
        </table>
      ) : (
        <p className="muted">{t('compare.notComparable')}</p>
      )}

      <h2>{t('compare.simulation')}</h2>
      {comparison.simulation ? (
        <table className="table">
          {head}
          <tbody>
            <Row label={`${t('compare.timing.compile')} (ms)`} comparison={comparison.simulation.compile} />
            <Row label={`${t('compare.timing.execution')} (ms)`} comparison={comparison.simulation.execution} />
            <Row label={`${t('compare.timing.total')} (ms)`} comparison={comparison.simulation.total} />
            <Row label={t('compare.timing.simulated')} comparison={comparison.simulation.simulated} />
          </tbody>
        </table>
      ) : (
        <p className="muted">{t('compare.notComparable')}</p>
      )}

      {comparison.warnings.length > 0 && (
        <>
          <h2>{t('compare.warnings')}</h2>
          <ul className="plain-list">
            {comparison.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
