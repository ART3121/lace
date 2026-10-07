// As vistas que nasceram no painel: os consoles de cada etapa, os problemas
// da última operação e o terminal de shell. Elas ficam em qualquer região
// (Region.tsx); os consoles e o terminal são elementos que vivem fora do
// React (console/), e a vista só os põe dentro dela enquanto está montada.

import { Eraser, Plus } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';

import { attachConsole, clearConsole, detachConsole, fitConsole } from '../../console/consoles';
import { attachShell, detachShell, fitShell, restartShell } from '../../console/shell';
import { useT, type Key } from '../../i18n';
import type { Diagnostic, ProjectIssue } from '../../ipc/lace-types';
import { useEditor } from '../../state/editor';
import { useJobs } from '../../state/jobs';
import { useLayout, type ConsoleChannel } from '../../state/layout';
import { useProject } from '../../state/project';
import { relativeUp } from '../../util/paths';
import { Empty, IconButton } from '../common';
import { ViewActions } from '../layout/ViewActions';

/** Chama `fit` quando o elemento muda de tamanho. */
function useResize(ref: React.RefObject<HTMLElement | null>, fit: () => void) {
  useEffect(() => {
    if (!ref.current) return;
    const observer = new ResizeObserver(() => fit());
    observer.observe(ref.current);
    return () => observer.disconnect();
  }, [ref, fit]);
}

export function ConsoleView({ channel }: { channel: ConsoleChannel }) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const fit = useMemo(() => () => fitConsole(channel), [channel]);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    attachConsole(channel, node);
    return () => detachConsole(channel, node);
  }, [channel]);
  useResize(ref, fit);
  return (
    <>
      <ViewActions>
        <IconButton label={t('panel.clear')} onClick={() => clearConsole(channel)}>
          <Eraser size={14} />
        </IconButton>
      </ViewActions>
      <div ref={ref} className="console" />
    </>
  );
}

export function ShellView() {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    // O foco vai para o terminal só quando o usuário o pediu (a aba, Ctrl+`);
    // aparecer porque um layout foi aplicado não tira o foco do editor.
    attachShell(node, { focus: useLayout.getState().takeFocus('terminal') });
    return () => detachShell(node);
  }, []);
  useResize(ref, fitShell);
  return (
    <>
      <ViewActions>
        <IconButton label={t('panel.newTerminal')} onClick={() => void restartShell()}>
          <Plus size={15} />
        </IconButton>
      </ViewActions>
      <div ref={ref} className="console" />
    </>
  );
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

/** Erros e avisos da última operação mais os avisos do `.spf`: o número da
 * aba Problemas e do ícone dela. */
export function useProblemCount(): { errors: number; warnings: number } {
  const problems = useJobs((s) => s.problems);
  const issues = useProject((s) => s.snapshot?.issues.length ?? 0);
  return useMemo(
    () => ({
      errors: problems.filter((p) => p.severity === 'error').length,
      warnings: problems.filter((p) => p.severity === 'warning').length + issues,
    }),
    [problems, issues],
  );
}

export function ProblemsView() {
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
