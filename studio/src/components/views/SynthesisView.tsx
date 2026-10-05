// As estatísticas da última síntese: o `stat` do Yosys sobre o netlist
// genérico (SynthesisStatistics do Core). Uma contagem que o Yosys não
// informou aparece como "não informado", que é diferente de zero.

import { runSynthesis } from '../../actions';
import { useT, type Key } from '../../i18n';
import type { SynthesisStatistics } from '../../ipc/lace-types';
import { useEditor } from '../../state/editor';
import { useJobs } from '../../state/jobs';
import { Button, Empty } from '../common';

const METRICS: (keyof SynthesisStatistics)[] = [
  'cells',
  'wires',
  'wire_bits',
  'public_wires',
  'public_wire_bits',
  'memories',
  'memory_bits',
  'processes',
  'modules',
];

export function SynthesisView() {
  const t = useT();
  const synthesis = useJobs((s) => s.synthesis);
  const running = useJobs((s) => s.running);
  const stats = synthesis?.statistics ?? null;

  if (!stats) {
    return (
      <div className="view-page">
        <Empty>
          <p>{t('synthesis.empty')}</p>
          <Button variant="primary" disabled={!!running} onClick={() => void runSynthesis()}>
            {t('action.synthesize')}
          </Button>
        </Empty>
      </div>
    );
  }

  const max = Math.max(1, ...stats.cell_types.map((c) => c.count));

  return (
    <div className="view-page synthesis">
      <div className="view-page__title">
        <h1>
          {t('synthesis.title')}: <span className="mono">{stats.top}</span>
        </h1>
        <Button onClick={() => useEditor.getState().openView('schematic')}>{t('action.showSchematic')}</Button>
      </div>
      <p className="muted">{t('synthesis.note')}</p>

      <div className="stat-grid">
        {METRICS.map((metric) => {
          const value = stats[metric] as number | null;
          return (
            <div key={metric} className="stat">
              <span className="stat__label">{t(`synthesis.metric.${metric}` as Key)}</span>
              <span className="stat__value">{value === null ? t('common.notReported') : value.toLocaleString()}</span>
            </div>
          );
        })}
      </div>

      <h2>{t('synthesis.cellTypes')}</h2>
      <table className="table table--bars">
        <tbody>
          {stats.cell_types.map((cell) => (
            <tr key={cell.cell_type}>
              <td className="mono">{cell.cell_type}</td>
              <td className="num">{cell.count.toLocaleString()}</td>
              <td className="bar-cell">
                <span className="bar" style={{ width: `${(cell.count / max) * 100}%` }} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
