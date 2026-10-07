// A barra de ferramentas: projeto, o alvo (projeto ou um processador), os
// botões do fluxo com os nomes que a AURORA usava (C±, Verilog, Wave,
// PRISM) e o botão de parar. O simulador se escolhe no menu Fluxo e nas
// Preferências.
//
// Cada item pode ser escondido pelo layout (o menu de contexto da barra, ou
// Preferências > Layout); um grupo sem nenhum botão some com o separador.

import { CircleStop } from 'lucide-react';
import type { ReactNode } from 'react';

import { action, isEnabled, runAction } from '../../actions';
import { useT, t as translate } from '../../i18n';
import { useApp } from '../../state/app';
import { useJobs } from '../../state/jobs';
import { useLayout } from '../../state/layout';
import { TOOLBAR_ITEMS } from '../../state/layoutModel';
import { useProject } from '../../state/project';
import type { Key } from '../../i18n';
import { openContextMenu, Spinner } from '../common';
import { toolbarMenu } from './layoutMenus';

function ToolButton({ id, label }: { id: string; label?: Key }) {
  const t = useT();
  const a = action(id);
  const Icon = a.icon;
  const status = useJobs((s) => {
    const running = s.running;
    if (!running) return undefined;
    return running.statusKey.split(':')[0] === id || (id === 'fastSim' && running.statusKey.startsWith('simulate'))
      ? 'running'
      : undefined;
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

const FILE_BUTTONS = TOOLBAR_ITEMS.filter((item) => item.group === 'file');
const FLOW_BUTTONS = TOOLBAR_ITEMS.filter((item) => item.group === 'flow');

export function Toolbar() {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot);
  const target = useProject((s) => s.target);
  const running = useJobs((s) => s.running);
  const hidden = useLayout((s) => s.live.hidden.toolbar);
  // Redesenha quando a informação do bundle chega.
  useApp((s) => s.toolchain);
  const shown = (id: string) => !hidden.includes(id);

  const segments: ReactNode[] = [];
  const file = FILE_BUTTONS.filter((item) => shown(item.id));
  if (file.length) {
    segments.push(
      <div key="file" className="toolbar__group">
        {file.map((item) => (
          <ToolButton key={item.id} id={item.id} />
        ))}
      </div>,
    );
  }
  if (shown('target')) {
    segments.push(
      <label key="target" className="toolbar__target" title={t('toolbar.targetHint')}>
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
      </label>,
    );
  }
  const flow = FLOW_BUTTONS.filter((item) => shown(item.id));
  if (flow.length) {
    segments.push(
      <div key="flow" className="toolbar__group">
        {flow.map((item) => (
          <ToolButton key={item.id} id={item.id} label={item.label} />
        ))}
      </div>,
    );
  }

  return (
    <div className="toolbar" onContextMenu={(e) => openContextMenu(e, toolbarMenu())}>
      {segments.flatMap((segment, index) => (index === 0 ? [segment] : [<div key={`sep-${index}`} className="toolbar__sep" />, segment]))}
      <div className="toolbar__spacer" />
      {running && shown('running') && (
        <div className="toolbar__running">
          <Spinner size={13} />
          <span>{running.phase ? translate(`console.phase.${running.phase}` as Key) : running.command || '...'}</span>
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
