// A barra de ferramentas: projeto, o alvo (projeto ou um processador), os
// botões do fluxo com os nomes que a AURORA usava (C±, Verilog, Wave,
// PRISM) e o botão de parar. O simulador se escolhe no menu Fluxo e nas
// Preferências.

import { CircleStop } from 'lucide-react';

import { action, isEnabled, runAction } from '../../actions';
import { useT, t as translate } from '../../i18n';
import { useApp } from '../../state/app';
import { phaseKey, useJobs } from '../../state/jobs';
import { useProject } from '../../state/project';
import type { Key } from '../../i18n';
import { Spinner } from '../common';

function ToolButton({ id, label }: { id: string; label?: Key }) {
  const t = useT();
  const a = action(id);
  const Icon = a.icon;
  const status = useJobs((s) => {
    const running = s.running;
    if (!running) return undefined;
    return running.statusKey.split(':')[0] === id ? 'running' : undefined;
  });
  return (
    <button
      type="button"
      className="tool-btn"
      disabled={!isEnabled(a)}
      title={`${t(a.label)}${a.keys ? ` (${a.keys})` : ''}`}
      onClick={() => runAction(id)}
    >
      {status === 'running' ? <Spinner size={15} /> : Icon && <Icon size={15} />}
      {label && <span>{t(label)}</span>}
    </button>
  );
}

export function Toolbar() {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot);
  const target = useProject((s) => s.target);
  const running = useJobs((s) => s.running);
  // Redesenha quando a informação do bundle chega.
  useApp((s) => s.toolchain);

  return (
    <div className="toolbar">
      <div className="toolbar__group">
        <ToolButton id="newProject" />
        <ToolButton id="openProject" />
        <ToolButton id="save" />
      </div>
      <div className="toolbar__sep" />
      <label className="toolbar__target" title={t('toolbar.targetHint')}>
        <span>{t('toolbar.target')}</span>
        <select
          className="select select--small"
          value={target ?? ''}
          disabled={!snapshot || !!running}
          onChange={(e) => useProject.getState().setTarget(e.target.value || null)}
        >
          <option value="">{t('toolbar.targetProject')}</option>
          {snapshot?.processors.map((p) => (
            <option key={p.name} value={p.name}>
              {p.name}
            </option>
          ))}
        </select>
      </label>
      <div className="toolbar__sep" />
      <div className="toolbar__group">
        <ToolButton id="build" label="toolbar.build" />
        <ToolButton id="check" label="toolbar.check" />
        <ToolButton id="simulate" label="toolbar.simulate" />
        <ToolButton id="fastSim" label="toolbar.fastSim" />
        <ToolButton id="openWave" label="toolbar.openWave" />
        <ToolButton id="synthesize" label="toolbar.synthesize" />
      </div>
      <div className="toolbar__spacer" />
      {running && (
        <div className="toolbar__running">
          <Spinner size={13} />
          <span>{running.phase ? translate(phaseKey(running.phase, running.statusKey)) : running.command || '...'}</span>
          {running.flow !== 'update' && (
            <button type="button" className="tool-btn tool-btn--stop" title={`${t('action.cancel')} (Shift+F5)`} onClick={() => runAction('cancel')}>
              <CircleStop size={15} />
              <span>{t('toolbar.stop')}</span>
            </button>
          )}
        </div>
      )}
    </div>
  );
}
