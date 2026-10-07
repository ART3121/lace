// O navegador de fluxo, como o "Flow Navigator" do Vivado: as etapas do
// projeto em ordem, cada uma com o estado da última execução.

import type { ReactNode } from 'react';

import { action, fastSimulator, isEnabled, openWave, runAction, runBuild, runSimulation } from '../../actions';
import { useT, type Key } from '../../i18n';
import { useApp } from '../../state/app';
import { openDialog } from '../../state/dialogs';
import { useEditor } from '../../state/editor';
import { useJobs, type ActionStatus } from '../../state/jobs';
import { useProject } from '../../state/project';
import { Empty, Kbd, Section, StatusDot } from '../common';

function Item({
  label,
  keys,
  status,
  disabled,
  onClick,
  hint,
}: {
  label: string;
  keys?: string;
  status?: ActionStatus;
  disabled?: boolean;
  onClick: () => void;
  hint?: ReactNode;
}) {
  return (
    <button type="button" className="flow-item" disabled={disabled} onClick={onClick}>
      <StatusDot status={status} />
      <span className="flow-item__label">
        {label}
        {hint && <span className="flow-item__hint">{hint}</span>}
      </span>
      {keys && <Kbd keys={keys} />}
    </button>
  );
}

function ActionItem({ id, statusKey, hint }: { id: string; statusKey?: string; hint?: ReactNode }) {
  const t = useT();
  const a = action(id);
  const status = useJobs((s) => s.statusByKey[statusKey ?? id]);
  useJobs((s) => s.running);
  useProject((s) => s.snapshot);
  return (
    <Item label={t(a.label)} keys={a.keys} status={status} disabled={!isEnabled(a)} onClick={() => runAction(id)} hint={hint} />
  );
}

export function FlowNavigator() {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot);
  const target = useProject((s) => s.target);
  const running = useJobs((s) => s.running);
  const statusByKey = useJobs((s) => s.statusByKey);
  const simulator = useApp((s) => s.settings?.simulator ?? 'icarus');

  if (!snapshot) {
    return (
      <div className="sidebar__body">
        <Empty>{t('explorer.noProject')}</Empty>
      </div>
    );
  }

  const busy = !!running;
  const top = snapshot.top_module ?? (snapshot.top_level ? '?' : null);

  return (
    <div className="sidebar__body flow-nav">
      <Section title={t('flow.project')}>
        <Item label={t('action.addVerilog')} onClick={() => runAction('addVerilog')} />
        <Item label={t('action.newVerilog')} onClick={() => openDialog({ kind: 'newVerilog', testbench: false })} />
        <Item label={t('action.newTestbench')} onClick={() => openDialog({ kind: 'newVerilog', testbench: true })} />
        <Item
          label={t('action.newCocotb')}
          onClick={() => openDialog({ kind: 'newVerilog', testbench: true, cocotb: true })}
        />
        <Item label={t('action.newProcessor')} keys="Ctrl+Alt+P" onClick={() => openDialog({ kind: 'newProcessor' })} />
        <Item
          label={t('action.chooseTop')}
          hint={top ?? t('flow.noTop')}
          onClick={() => openDialog({ kind: 'chooseTop' })}
        />
        <Item
          label={t('action.chooseTestbench')}
          hint={snapshot.testbench_module ?? t('flow.noTestbench')}
          onClick={() => openDialog({ kind: 'chooseTestbench' })}
        />
      </Section>

      {snapshot.processors.length > 0 && (
        <Section title={t('flow.processors')}>
          <ActionItem id="build" statusKey={target ? `build:${target}` : 'build'} />
          {snapshot.processors.map((p) => (
            <Item
              key={p.name}
              label={t('flow.buildOne', { name: p.name })}
              status={statusByKey[`build:${p.name}`]}
              disabled={busy}
              onClick={() => void runBuild(p.name)}
            />
          ))}
        </Section>
      )}

      <Section title={t('flow.verification')}>
        <ActionItem id="check" statusKey={target ? `check:${target}` : 'check'} />
        <ActionItem id="lint" statusKey={target ? `lint:${target}` : 'lint'} />
      </Section>

      <Section title={`${t('flow.simulation')} (${simulator === 'icarus' ? 'Icarus' : 'Verilator'})`}>
        <ActionItem id="simulate" statusKey={target ? `simulate:${target}` : 'simulate'} />
        {/* A Rápida tem simulador próprio: o Verilator, menos com cocotb. */}
        <ActionItem
          id="fastSim"
          statusKey={target ? `fastSim:${target}` : 'fastSim'}
          hint={fastSimulator() === 'icarus' ? 'Icarus' : 'Verilator'}
        />
        <Item label={t('action.openWave')} keys="Ctrl+F8" onClick={() => void openWave()} />
        {snapshot.processors.map((p) => (
          <Item
            key={p.name}
            label={t('flow.simulateOne', { name: p.name })}
            status={statusByKey[`simulate:${p.name}`]}
            disabled={busy}
            onClick={() => void runSimulation(false, p.name)}
          />
        ))}
      </Section>

      <Section title={t('flow.synthesis')}>
        <ActionItem id="synthesize" statusKey={target ? `synthesize:${target}` : 'synthesize'} />
        <Item label={t('action.showSchematic')} onClick={() => useEditor.getState().openView('schematic')} />
        <Item label={t('action.showStatistics')} onClick={() => useEditor.getState().openView('synthesis')} />
      </Section>

      <Section title={t('flow.reports')}>
        {(['lastReport', 'compareReports'] as const).map((id) => (
          <Item key={id} label={t(action(id).label as Key)} onClick={() => runAction(id)} />
        ))}
      </Section>
    </div>
  );
}
