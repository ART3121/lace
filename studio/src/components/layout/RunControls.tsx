// O Executar dos modelos `titlebar` e `editor` da barra de ferramentas
// (layoutModel.ts, TOOLBAR_STYLES): o Alvo, o botão Executar com a seta do
// menu e o Parar.
//
// Nos dois lugares, na barra de título (ou na barra de menus, sem ela) e no
// canto das abas do editor, o clique no Executar roda a etapa escolhida por
// último no menu da seta (de início, Simular). No editor, com o fonte C± ou
// C++ de um processador na frente, o botão sugere compilar esse processador;
// com qualquer outro arquivo, fica a etapa escolhida.
//
// O menu tem todas as etapas do fluxo, com os atalhos; a etapa escolhida ali
// passa a ser a do botão. As de ver (a onda, os sinais, a placa) rodam dali
// sem mudar o botão. Cada um dos três controles pode ser escondido pelo menu
// de contexto deles ou por Preferências > Layout da janela.

import { ChevronDown, CircleStop, Play, type LucideIcon } from 'lucide-react';
import { useState, type MouseEvent as ReactMouseEvent } from 'react';

import { action, isAvailable, isEnabled, runAction, runBuild, toolbarItemAvailable } from '../../actions';
import { useT, t as translate } from '../../i18n';
import type { Key } from '../../i18n';
import type { ProjectSnapshot } from '../../ipc/types';
import { useApp } from '../../state/app';
import { useExperimental } from '../../state/experimental';
import { useJobs } from '../../state/jobs';
import { useLayout } from '../../state/layout';
import { useProject } from '../../state/project';
import { extension, isInside, samePath } from '../../util/paths';
import { openContextMenu, Spinner, useContextMenu, type MenuItem } from '../common';
import { actionItem, toolbarMenu } from './layoutMenus';

/** As etapas que o botão Executar pode rodar, com o nome curto dele. */
const STEPS: Record<string, Key> = {
  build: 'run.build',
  check: 'run.check',
  simulate: 'run.simulate',
  fastSim: 'run.fastSim',
  fullFlow: 'run.fullFlow',
  synthesize: 'run.synthesize',
};

/** O menu da seta, em blocos: compilar, simular, ver a onda, hardware. */
const MENU = ['build', 'check', '-', 'simulate', 'fastSim', 'fullFlow', '-', 'openWave', 'waveSignals', '-', 'synthesize', 'showBoard'];

const STEP_KEY = 'lace-studio:runStep';

function savedStep(): string {
  const saved = localStorage.getItem(STEP_KEY);
  return saved && saved in STEPS ? saved : 'simulate';
}

/** O que o botão roda: a etapa escolhida, ou compilar o processador do fonte
 * aberto. */
interface Run {
  label: string;
  title: string;
  icon: LucideIcon;
  enabled: boolean;
  run: () => void;
}

function stepRun(id: string): Run {
  const a = action(id);
  return {
    label: translate(STEPS[id]),
    title: `${translate(a.label)}${a.keys ? ` (${a.keys})` : ''}`,
    icon: a.icon ?? Play,
    enabled: isEnabled(a),
    run: () => runAction(id),
  };
}

/** Os fontes de um processador que sugerem compilá-lo. */
const PROCESSOR_SOURCES = new Set(['cmm', 'cpp']);

/** Compilar o processador do fonte C± ou C++ aberto, ou `null` em qualquer
 * outro arquivo. */
function sourceRun(path: string | null, snapshot: ProjectSnapshot | null): Run | null {
  if (!path || !snapshot || !PROCESSOR_SOURCES.has(extension(path))) return null;
  const p = snapshot.processors.find((candidate) => samePath(path, candidate.source) || isInside(path, candidate.dir));
  if (!p) return null;
  const build = action('build');
  const label = translate('flow.buildOne', { name: p.name });
  return { label, title: label, icon: build.icon ?? Play, enabled: isEnabled(build), run: () => void runBuild(p.name) };
}

/** `path`: no editor, o arquivo da aba da frente do grupo (ou `null` numa
 * vista). */
export function RunControls({ variant, path = null }: { variant: 'titlebar' | 'editor'; path?: string | null }) {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot);
  // Sem o .spf, as etapas ficam desabilitadas: redesenha quando ele some ou
  // volta (`isEnabled` lê o estado na hora de desenhar).
  const spfMissing = useProject((s) => s.spfMissing);
  const target = useProject((s) => s.target);
  const running = useJobs((s) => s.running);
  const hidden = useLayout((s) => s.live.hidden.toolbar);
  // Redesenha quando a informação do bundle chega, e quando um recurso
  // experimental liga ou desliga: a placa só aparece no menu com ele ligado.
  useApp((s) => s.toolchain);
  useExperimental((s) => s.enabled);
  const [step, setStep] = useState(savedStep);
  const shown = (id: string) => !hidden.includes(id) && toolbarItemAvailable(id);

  const choose = (id: string) => {
    setStep(id);
    localStorage.setItem(STEP_KEY, id);
  };

  const openMenu = (event: ReactMouseEvent<HTMLButtonElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const items: MenuItem[] = [];
    for (const id of MENU) {
      if (id === '-') {
        if (items.length && !items[items.length - 1].separator) items.push({ separator: true });
        continue;
      }
      const a = action(id);
      if (!isAvailable(a)) continue;
      if (!(id in STEPS)) {
        items.push(actionItem(id));
        continue;
      }
      items.push({
        label: translate(a.menuLabel ?? a.label),
        keys: a.keys,
        checked: id === step,
        disabled: !isEnabled(a),
        run: () => {
          choose(id);
          runAction(id);
        },
      });
    }
    if (items[items.length - 1]?.separator) items.pop();
    useContextMenu.getState().show(rect.left, rect.bottom + 2, items);
  };

  const main = (variant === 'editor' && sourceRun(path, snapshot)) || stepRun(step);
  const Icon = main.icon;
  const stoppable = !!running && running.flow !== 'update';

  return (
    <div
      className={`run run--${variant}`}
      onContextMenu={(e) => {
        e.stopPropagation();
        openContextMenu(e, toolbarMenu());
      }}
    >
      {shown('target') && (
        <select
          className="select select--small run__target"
          aria-label={t('toolbar.target')}
          title={`${t('toolbar.target')}: ${t('toolbar.targetHint')}`}
          value={target ?? ''}
          disabled={!snapshot || spfMissing || !!running}
          onChange={(e) => useProject.getState().setTarget(e.target.value || null)}
        >
          <option value="">{t('run.targetProject')}</option>
          {snapshot?.processors.map((p) => (
            <option key={p.name} value={p.name}>
              {p.name}
            </option>
          ))}
        </select>
      )}
      {shown('run') && (
        <div className="run__split">
          <button type="button" className="run__main" disabled={!main.enabled} title={main.title} onClick={main.run}>
            {running ? <Spinner size={13} /> : <Icon size={14} />}
            <span>{main.label}</span>
          </button>
          <button type="button" className="run__more" aria-label={t('run.more')} title={t('run.more')} onClick={openMenu}>
            <ChevronDown size={14} />
          </button>
        </div>
      )}
      {shown('stop') && (
        <button
          type="button"
          className="run__stop"
          disabled={!stoppable}
          aria-label={t('action.cancel')}
          title={`${t('action.cancel')} (Shift+F5)`}
          onClick={() => runAction('cancel')}
        >
          <CircleStop size={14} />
        </button>
      )}
    </div>
  );
}
