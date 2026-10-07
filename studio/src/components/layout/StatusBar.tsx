// A barra de status: o projeto (topo, testbench, alvo) à esquerda, a
// operação rodando ou o último resultado no meio, e à direita os
// problemas, o cursor, a linguagem, o simulador e o bundle.
//
// Cada item pode ser escondido pelo layout (o menu de contexto da barra, ou
// Preferências > Layout). A espera de um atalho de duas etapas e a linha do
// Vim aparecem sempre: sem elas, o teclado parece não responder.

import { CircleAlert, LayoutTemplate, TriangleAlert } from 'lucide-react';
import { useEffect, useState } from 'react';

import { useChord } from '../../actions';
import { languageLabel } from '../../editor/monaco';
import { setVimStatusNode } from '../../editor/vim';
import { formatDuration, useT, type Key } from '../../i18n';
import { useApp } from '../../state/app';
import { useEditor } from '../../state/editor';
import { useJobs } from '../../state/jobs';
import { useLayout } from '../../state/layout';
import { useProject } from '../../state/project';
import { openDialog } from '../../state/dialogs';
import { useLayoutStatus } from '../../state/savedLayouts';
import { baseName } from '../../util/paths';
import { Kbd, openContextMenu, Spinner } from '../common';
import { layoutItems, statusBarMenu } from './layoutMenus';

export function Elapsed({ since }: { since: number }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(timer);
  }, []);
  return <span className="mono">{formatDuration(Math.max(0, now - since))}</span>;
}

/** O layout em uso, com um ponto quando a janela está diferente da foto
 * dele. O clique troca de layout; o botão direito salva ou restaura. */
function LayoutIndicator() {
  const t = useT();
  const { layout, dirty } = useLayoutStatus();
  return (
    <button
      type="button"
      className="statusbar__item statusbar__button statusbar__layout"
      title={t(dirty ? 'layout.indicator.modified' : 'layout.indicator', { name: layout.name })}
      onClick={() => openDialog({ kind: 'layout' })}
      onContextMenu={(e) => {
        e.stopPropagation();
        openContextMenu(e, layoutItems());
      }}
    >
      <LayoutTemplate size={12} />
      {layout.name}
      {dirty && <span className="statusbar__dirty" aria-hidden />}
    </button>
  );
}

export function StatusBar() {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot);
  const target = useProject((s) => s.target);
  const running = useJobs((s) => s.running);
  const last = useJobs((s) => s.last);
  const problems = useJobs((s) => s.problems);
  const cursor = useEditor((s) => s.cursor);
  const languageId = useEditor((s) => s.languageId);
  const activeIsFile = useEditor((s) => s.tabs.find((tab) => tab.id === s.activeId)?.kind === 'file');
  const simulator = useApp((s) => s.settings?.simulator ?? 'icarus');
  const toolchain = useApp((s) => s.toolchain);
  const vim = useApp((s) => s.settings?.editor.vim_mode ?? false);
  const pending = useChord((s) => s.pending);
  const hidden = useLayout((s) => s.live.hidden.statusbar);
  const shown = (id: string) => !hidden.includes(id);

  const errors = problems.filter((p) => p.severity === 'error').length;
  const warnings = problems.filter((p) => p.severity === 'warning').length;

  return (
    <footer className="statusbar" onContextMenu={(e) => openContextMenu(e, statusBarMenu())}>
      <div className="statusbar__left">
        {snapshot ? (
          <>
            {shown('project') && <span className="statusbar__item statusbar__project">{snapshot.name}</span>}
            {shown('top') && (
              <span className="statusbar__item" title={snapshot.top_level ?? undefined}>
                {snapshot.top_module ? t('status.top', { name: snapshot.top_module }) : t('status.noTop')}
              </span>
            )}
            {shown('testbench') && (
              <span className="statusbar__item" title={snapshot.selected_testbench ?? undefined}>
                {snapshot.selected_testbench
                  ? t('status.testbench', { name: snapshot.testbench_module ?? baseName(snapshot.selected_testbench) })
                  : t('status.noTestbench')}
              </span>
            )}
            {target && shown('target') && (
              <span className="statusbar__item">
                {t('toolbar.target')}: {target}
              </span>
            )}
          </>
        ) : (
          shown('project') && <span className="statusbar__item">{t('status.noProject')}</span>
        )}
      </div>

      <div className="statusbar__center">
        {pending ? (
          <span className="statusbar__item">
            <Kbd keys={pending} />
            {t('status.chordPending')}
          </span>
        ) : !shown('operation') ? null : running ? (
          <span className="statusbar__item">
            <Spinner size={12} />
            {running.phase ? t(`console.phase.${running.phase}` as Key) : t('common.running')}
            <Elapsed since={running.startedAt} />
          </span>
        ) : last ? (
          <span className={`statusbar__item ${last.succeeded ? 'text-ok' : 'text-error'}`}>
            {last.succeeded
              ? t('status.lastOk', { flow: t(`flowName.${last.flow}` as Key), time: formatDuration(last.durationMs) })
              : t('status.lastFailed', {
                  flow: t(`flowName.${last.flow}` as Key),
                  status: last.status === 'error' ? t('console.status.failed') : t(`console.status.${last.status}` as Key),
                })}
          </span>
        ) : (
          <span className="statusbar__item muted">{t('status.idle')}</span>
        )}
      </div>

      <div className="statusbar__right">
        {shown('problems') && (
          <button type="button" className="statusbar__item statusbar__button" onClick={() => useLayout.getState().revealView('problems', { explicit: true })}>
            <CircleAlert size={12} /> {errors}
            <TriangleAlert size={12} /> {warnings}
          </button>
        )}
        {/* O modo do Vim e a linha de comando (`:w`, `/busca`), escritos pelo monaco-vim. */}
        <span ref={setVimStatusNode} className={`statusbar__item statusbar__vim${vim && activeIsFile ? '' : ' is-hidden'}`} />
        {activeIsFile && cursor && shown('cursor') && (
          <span className="statusbar__item">{t('status.cursor', { line: cursor.line, col: cursor.column })}</span>
        )}
        {activeIsFile && languageId && shown('language') && <span className="statusbar__item">{languageLabel(languageId)}</span>}
        {shown('simulator') && <span className="statusbar__item">{simulator === 'icarus' ? 'Icarus' : 'Verilator'}</span>}
        {shown('bundle') && (
          <button
            type="button"
            className={`statusbar__item statusbar__button${toolchain && !toolchain.found ? ' text-warn' : ''}`}
            onClick={() => useEditor.getState().openView('toolchain')}
          >
            {toolchain?.found ? t('status.bundle', { bundle: toolchain.bundle ?? '' }) : toolchain ? t('status.toolchainMissing') : ''}
          </button>
        )}
        {shown('layout') && <LayoutIndicator />}
      </div>
    </footer>
  );
}
