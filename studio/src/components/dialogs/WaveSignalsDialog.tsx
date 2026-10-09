// A escolha dos sinais da onda do testbench do projeto: a Wave Configuration
// da AURORA. A árvore vem da elaboração do testbench (wave_signals, no Core
// `wave_signals.rs`); marcar um escopo vale por tudo o que há dentro dele, e
// desmarcar um sinal dentro de um escopo marcado deixa os outros. Gravar
// escreve `wave/<testbench>.json`: a próxima simulação grava só a escolha, e
// o layout mostra só ela. Nada marcado: a onda grava todos os sinais.
//
// Abre pelo botão Sinais da barra de ferramentas, pelo da aba da onda, pelo
// Navegador de fluxo, pelo menu Fluxo e pelo menu do testbench simulado.

import { ChevronDown, ChevronRight, Cpu } from 'lucide-react';
import { useEffect, useMemo, useRef, useState, type ReactElement } from 'react';

import { runSimulation } from '../../actions';
import { useT } from '../../i18n';
import { api } from '../../ipc/api';
import type { SignalScope, WaveSignal, WaveSignals } from '../../ipc/lace-types';
import type { IpcError } from '../../ipc/types';
import { useApp } from '../../state/app';
import { useDialogs } from '../../state/dialogs';
import { useJobs } from '../../state/jobs';
import { useProject } from '../../state/project';
import { showError, useToasts } from '../../state/toasts';
import { Badge, Button, Checkbox, Spinner } from '../common';
import { Dialog } from './Dialogs';

/** `item` cobre `path`: é ele, ou um escopo acima dele (a regra do Core). */
function covers(item: string, path: string): boolean {
  return path === item || path.startsWith(`${item}.`);
}

/** A escolha sem repetições e sem o que um escopo dela já cobre. */
function normalize(items: string[]): string[] {
  const set = [...new Set(items.map((s) => s.trim()).filter(Boolean))].sort();
  return set.filter((s) => !set.some((other) => other !== s && covers(other, s)));
}

function findScope(scope: SignalScope, path: string): SignalScope | null {
  if (scope.path === path) return scope;
  for (const child of scope.scopes) {
    if (covers(child.path, path)) return findScope(child, path);
  }
  return null;
}

/** O que há dentro de `scope`, menos `path` (e o que leva até ele). */
function expandWithout(scope: SignalScope, path: string, out: string[]) {
  for (const signal of scope.signals) if (signal.path !== path) out.push(signal.path);
  for (const child of scope.scopes) {
    if (child.path === path) continue;
    if (covers(child.path, path)) expandWithout(child, path, out);
    else out.push(child.path);
  }
}

/** A escolha sem `path`: o escopo marcado que o contém dá lugar ao resto dele
 * (`without` do Core); um escopo só em parte marcado perde o que tinha. */
function without(root: SignalScope, selection: string[], path: string): string[] {
  if (selection.includes(path)) return selection.filter((s) => s !== path);
  const ancestor = selection.find((item) => covers(item, path));
  if (!ancestor) return selection.filter((item) => !covers(path, item));
  const out = selection.filter((s) => s !== ancestor);
  const node = findScope(root, ancestor);
  if (node) expandWithout(node, path, out);
  return normalize(out);
}

type Mark = 'on' | 'partial' | 'off';

function markOf(path: string, selection: string[]): Mark {
  if (selection.some((item) => covers(item, path))) return 'on';
  if (selection.some((item) => covers(path, item))) return 'partial';
  return 'off';
}

/** Uma caixa de três estados: marcada, em parte (algo dentro) e não. */
function TriCheck({ mark, label, onToggle }: { mark: Mark; label: string; onToggle: () => void }) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = mark === 'partial';
  }, [mark]);
  return (
    <input
      ref={ref}
      type="checkbox"
      className="wave-signals__check"
      aria-label={label}
      checked={mark === 'on'}
      onChange={onToggle}
    />
  );
}

/** O filtro do nome: texto (sem maiúsculas) ou expressão regular. `null`
 * sem filtro; `false` com uma expressão inválida. */
function matcher(text: string, regex: boolean): ((name: string) => boolean) | null | false {
  if (!text.trim()) return null;
  if (!regex) {
    const needle = text.trim().toLowerCase();
    return (name) => name.toLowerCase().includes(needle);
  }
  try {
    const re = new RegExp(text, 'i');
    return (name) => re.test(name);
  } catch {
    return false;
  }
}

export function WaveSignalsDialog() {
  const t = useT();
  const simulator = useApp((s) => s.settings?.simulator);
  const busy = useJobs((s) => s.running !== null);
  const [tree, setTree] = useState<WaveSignals | null>(null);
  const [error, setError] = useState<IpcError | null>(null);
  const [selection, setSelection] = useState<string[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState('');
  const [regex, setRegex] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let alive = true;
    api.wave
      .signals()
      .then((result) => {
        if (!alive) return;
        setTree(result);
        setSelection(result.selection);
        if (result.root) setExpanded(new Set([result.root.path]));
      })
      .catch((e: IpcError) => alive && setError(e));
    return () => {
      alive = false;
    };
  }, []);

  const test = useMemo(() => matcher(filter, regex), [filter, regex]);
  const root = tree?.root ?? null;
  const unknown = tree?.unknown ?? [];

  const toggle = (path: string) => {
    if (!root) return;
    setSelection((current) =>
      markOf(path, current) === 'on' ? without(root, current, path) : normalize([...current, path]),
    );
  };
  const toggleOpen = (path: string) =>
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  // Grava e, com `simulate`, roda a Wave (F8) do projeto, onde a escolha
  // vale, qualquer que seja o alvo da barra.
  const save = async (simulate: boolean) => {
    if (!tree || saving) return;
    setSaving(true);
    try {
      const chosen = selection.filter((s) => !unknown.includes(s));
      const saved = await api.wave.setSelection(chosen);
      await useProject.getState().refresh();
      useDialogs.getState().close();
      if (simulate) {
        void runSimulation(useApp.getState().settings?.open_wave_after_sim ?? true, null);
      } else {
        useToasts
          .getState()
          .push({ kind: 'success', title: t(saved.length ? 'waveSignals.saved' : 'waveSignals.savedAll') });
      }
    } catch (e) {
      showError(e);
      setSaving(false);
    }
  };

  const signalRow = (signal: WaveSignal, depth: number) => (
    <div key={signal.path} className="wave-signals__row" style={{ paddingLeft: depth * 16 + 22 }}>
      <TriCheck mark={markOf(signal.path, selection)} label={signal.path} onToggle={() => toggle(signal.path)} />
      <span className="mono">{signal.name}</span>
      {signal.width > 1 && <span className="tree__detail mono">[{signal.width}]</span>}
      {signal.direction && <span className="wave-signals__dir">{signal.direction}</span>}
    </div>
  );

  const scopeRows = (scope: SignalScope, depth: number): ReactElement | null => {
    const filtering = typeof test === 'function';
    const signals = filtering ? scope.signals.filter((s) => (test as (n: string) => boolean)(s.name)) : scope.signals;
    const children = scope.scopes.map((child) => scopeRows(child, depth + 1)).filter((c) => c !== null);
    const named = filtering && (test as (n: string) => boolean)(scope.name);
    if (filtering && !named && signals.length === 0 && children.length === 0) return null;
    const open = filtering || expanded.has(scope.path);
    return (
      <div key={scope.path}>
        <div className="wave-signals__row" style={{ paddingLeft: depth * 16 }}>
          <button
            type="button"
            className="wave-signals__chevron"
            aria-label={t(open ? 'waveSignals.collapse' : 'waveSignals.expand')}
            onClick={() => toggleOpen(scope.path)}
            disabled={filtering}
          >
            {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </button>
          <TriCheck mark={markOf(scope.path, selection)} label={scope.path} onToggle={() => toggle(scope.path)} />
          <span className="mono">{scope.name}</span>
          {scope.module && scope.module !== scope.name && <span className="tree__detail">{scope.module}</span>}
          {scope.processor && (
            <Badge tone="info" title={t('waveSignals.processor')}>
              <Cpu size={11} /> {t('waveSignals.processor')}
            </Badge>
          )}
        </div>
        {open && (
          <>
            {(filtering ? signals : scope.signals).map((s) => signalRow(s, depth))}
            {children}
          </>
        )}
      </div>
    );
  };

  const rows = root ? scopeRows(root, 0) : null;
  const count = selection.filter((s) => !unknown.includes(s)).length;

  return (
    <Dialog
      title={t('waveSignals.title')}
      wide
      onSubmit={() => void save(true)}
      footer={
        <>
          <Button onClick={() => useDialogs.getState().close()}>{t('common.cancel')}</Button>
          <Button disabled={!root || saving} onClick={() => void save(false)}>
            {t('waveSignals.save')}
          </Button>
          <Button variant="primary" type="submit" disabled={!root || saving || busy}>
            {t('waveSignals.saveAndSimulate')}
          </Button>
        </>
      }
    >
      {error ? (
        <p className="text-error">{error.message}</p>
      ) : !tree ? (
        <p className="wave-signals__loading">
          <Spinner /> {t('waveSignals.loading')}
        </p>
      ) : (
        <div className="wave-signals">
          <p>
            <strong>{t('waveSignals.testbench', { module: tree.module })}</strong>
            {' · '}
            {count === 0 ? t('waveSignals.all') : t('waveSignals.some', { count })}
          </p>
          {simulator === 'verilator' && count > 0 && <p className="muted">{t('waveSignals.verilator')}</p>}
          {unknown.length > 0 && <p className="wave-signals__warning">{t('waveSignals.unknown', { items: unknown.join(', ') })}</p>}
          {!root ? (
            <div className="wave-signals__failed">
              <p>{t('waveSignals.failed')}</p>
              {tree.diagnostics.map((d, i) => (
                <p key={i} className="mono text-error">
                  {d.message}
                </p>
              ))}
            </div>
          ) : (
            <>
              <div className="wave-signals__tools">
                <input
                  className={`input${test === false ? ' is-invalid' : ''}`}
                  placeholder={t('waveSignals.filter')}
                  value={filter}
                  autoFocus
                  onChange={(e) => setFilter(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && e.preventDefault()}
                />
                <Checkbox checked={regex} onChange={setRegex} label={t('waveSignals.regex')} />
                <span className="wave-signals__spacer" />
                <Button small onClick={() => setSelection(root.signals.map((s) => s.path))}>
                  {t('waveSignals.onlyTestbench')}
                </Button>
                <Button small onClick={() => setSelection([])}>
                  {t('waveSignals.clear')}
                </Button>
              </div>
              <div className="wave-signals__tree">{rows ?? <p className="muted">{t('waveSignals.noMatch')}</p>}</div>
            </>
          )}
        </div>
      )}
    </Dialog>
  );
}
