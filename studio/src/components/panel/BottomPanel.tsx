// O painel inferior: os consoles de cada etapa, os problemas da última
// operação e o terminal de shell. A aba de um console com saída nova que
// não está à vista ganha um ponto.

import { ChevronDown, Eraser, Maximize2, Minimize2, Plus } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';

import { attachConsole, clearConsole, detachConsole, fitConsole } from '../../console/consoles';
import { attachShell, detachShell, fitShell, restartShell } from '../../console/shell';
import { useT, type Key } from '../../i18n';
import type { Diagnostic, ProjectIssue } from '../../ipc/lace-types';
import { useEditor } from '../../state/editor';
import { useJobs } from '../../state/jobs';
import { useLayout, type ConsoleChannel, type PanelTab } from '../../state/layout';
import { useProject } from '../../state/project';
import { relativeUp } from '../../util/paths';
import { Empty, IconButton } from '../common';

const TABS: PanelTab[] = ['cmm', 'asm', 'verilog', 'wave', 'prism', 'problems', 'terminal'];
const CONSOLES = new Set<PanelTab>(['cmm', 'asm', 'verilog', 'wave', 'prism']);

/** Chama `fit` quando o elemento muda de tamanho. */
function useResize(ref: React.RefObject<HTMLElement | null>, fit: () => void) {
  useEffect(() => {
    if (!ref.current) return;
    const observer = new ResizeObserver(() => fit());
    observer.observe(ref.current);
    return () => observer.disconnect();
  }, [ref, fit]);
}

function ConsoleView({ channel }: { channel: ConsoleChannel }) {
  const ref = useRef<HTMLDivElement>(null);
  const fit = useMemo(() => () => fitConsole(channel), [channel]);
  useEffect(() => {
    if (!ref.current) return;
    attachConsole(channel, ref.current);
    return () => detachConsole(channel);
  }, [channel]);
  useResize(ref, fit);
  return <div ref={ref} className="console" />;
}

function ShellView() {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    attachShell(node);
    return () => detachShell(node);
  }, []);
  useResize(ref, fitShell);
  return <div ref={ref} className="console" />;
}

/** Uma linha do painel: um diagnóstico da última operação, ou um aviso do
 * `.spf` (`Project::issues`), que vale enquanto o projeto está aberto. */
type Row = Pick<Diagnostic, 'severity' | 'message' | 'file' | 'line' | 'column'> & { tool: string };

const NO_ISSUES: ProjectIssue[] = [];

/** Os avisos do `.spf` como linhas do painel, traduzidos pelo tipo. */
function useIssueRows(): Row[] {
  const t = useT();
  const issues = useProject((s) => s.snapshot?.issues ?? NO_ISSUES);
  const root = useProject((s) => s.snapshot?.root);
  return useMemo(
    () =>
      issues.map((issue) => ({
        severity: 'warning',
        message: t(`issue.${issue.kind}` as Key, {
          detail: issue.detail ?? '',
          path: issue.path ? relativeUp(issue.path, root) : '',
        }),
        file: issue.path,
        line: null,
        column: null,
        tool: '.spf',
      })),
    [issues, root, t],
  );
}

function ProblemsView() {
  const t = useT();
  const diagnostics = useJobs((s) => s.problems);
  const issues = useIssueRows();
  const problems: Row[] = useMemo(() => [...issues, ...diagnostics], [issues, diagnostics]);
  const root = useProject((s) => s.snapshot?.root);
  const [filter, setFilter] = useState('');
  const [showInfo, setShowInfo] = useState(false);

  const visible = problems.filter(
    (p) =>
      (showInfo || p.severity === 'error' || p.severity === 'warning') &&
      (!filter || `${p.message} ${p.file ?? ''} ${p.tool}`.toLowerCase().includes(filter.toLowerCase())),
  );

  const open = (p: Row) => {
    if (p.file) void useEditor.getState().openFile(p.file, { line: p.line ?? 1, column: p.column ?? 1 });
  };

  return (
    <div className="problems">
      <div className="problems__toolbar">
        <input className="input input--small" placeholder={t('panel.problemsFilter')} value={filter} onChange={(e) => setFilter(e.target.value)} />
        <label className="check check--small">
          <input type="checkbox" checked={showInfo} onChange={(e) => setShowInfo(e.target.checked)} />
          <span className="check__box" aria-hidden />
          <span>{t('panel.severity.info')}</span>
        </label>
      </div>
      {visible.length === 0 ? (
        <Empty>{t('panel.noProblems')}</Empty>
      ) : (
        <table className="table problems__table">
          <tbody>
            {visible.map((p, index) => (
              <tr key={index} className={p.file ? 'is-link' : ''} onClick={() => open(p)}>
                <td className={`sev sev--${p.severity}`}>{t(`panel.severity.${p.severity}` as Key)}</td>
                <td className="problems__message">{p.message}</td>
                <td className="mono problems__where">
                  {p.file ? `${relativeUp(p.file, root)}${p.line ? `:${p.line}` : ''}${p.column ? `:${p.column}` : ''}` : ''}
                </td>
                <td className="muted">{p.tool}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

export function BottomPanel() {
  const t = useT();
  const tab = useLayout((s) => s.panelTab);
  const unread = useLayout((s) => s.unread);
  const maximized = useLayout((s) => s.panelMaximized);
  const problems = useJobs((s) => s.problems);
  const issues = useProject((s) => s.snapshot?.issues.length ?? 0);
  const errors = problems.filter((p) => p.severity === 'error').length;
  const warnings = problems.filter((p) => p.severity === 'warning').length + issues;

  return (
    <div className="panel">
      <div className="panel__header">
        <div className="panel__tabs" role="tablist">
          {TABS.map((id) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={tab === id}
              className={`panel__tab${tab === id ? ' is-active' : ''}`}
              onClick={() => useLayout.getState().showPanel(id)}
            >
              {t(`panel.${id}` as Key)}
              {id === 'problems' && errors + warnings > 0 && (
                <span className={`count${errors ? ' count--error' : ' count--warn'}`}>{errors + warnings}</span>
              )}
              {unread[id] && <span className="panel__unread" aria-hidden />}
            </button>
          ))}
        </div>
        <div className="panel__actions">
          {CONSOLES.has(tab) && (
            <IconButton label={t('panel.clear')} onClick={() => clearConsole(tab as ConsoleChannel)}>
              <Eraser size={14} />
            </IconButton>
          )}
          {tab === 'terminal' && (
            <IconButton label={t('panel.newTerminal')} onClick={() => void restartShell()}>
              <Plus size={15} />
            </IconButton>
          )}
          <IconButton
            label={t('panel.maximize')}
            onClick={() => useLayout.getState().setPanelMaximized(!maximized)}
          >
            {maximized ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
          </IconButton>
          <IconButton label={t('panel.hide')} onClick={() => useLayout.getState().togglePanel()}>
            <ChevronDown size={15} />
          </IconButton>
        </div>
      </div>
      <div className="panel__body">
        {CONSOLES.has(tab) && <ConsoleView key={tab} channel={tab as ConsoleChannel} />}
        {tab === 'problems' && <ProblemsView />}
        {tab === 'terminal' && <ShellView />}
      </div>
    </div>
  );
}
