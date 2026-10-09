// A placa FPGA do projeto, em três etapas, cada uma dizendo se está em dia:
//
//   1. Ligações: cada porta do topo e o sinal da placa que vai nela (o
//      `fpga.json`), conferidas a cada mudança e gravadas sozinhas;
//   2. Compilação: o Quartus gera o arquivo de gravação (`lace fpga build`);
//   3. Gravação: o arquivo vai para a placa pelo cabo (`lace fpga program`).
//
// O botão principal compila e grava. A saída do Quartus vai para o console
// Placa; os erros, para Problemas. A gravação só aceita o arquivo que
// descreve o projeto de agora (o Core confere), e a etapa 2 diz por que um
// arquivo ficou velho.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import {
  Cable,
  CircleCheck,
  CircleDashed,
  CircleX,
  FileCode,
  Hammer,
  LoaderCircle,
  Plus,
  RefreshCw,
  TriangleAlert,
  Trash2,
  Upload,
} from 'lucide-react';

import { runFpgaBuild, runFpgaBuildAndProgram, runFpgaProgram } from '../../actions';
import { formatDuration, useT, type Key } from '../../i18n';
import { api } from '../../ipc/api';
import type { Board, BoardSignal, FpgaBuildResult, ResourceUsage } from '../../ipc/lace-types';
import type { BitstreamState, FpgaConfig, FpgaPrepared, IpcError, ModuleInterface } from '../../ipc/types';
import { useApp } from '../../state/app';
import { useEditor } from '../../state/editor';
import { useJobs } from '../../state/jobs';
import { useProject } from '../../state/project';
import { Button, Empty, IconButton } from '../common';

type Port = ModuleInterface['ports'][number];
type Kind = 'none' | 'signal' | '0' | '1';

/** Uma ligação da tabela: a porta (com faixa, se o `fpga.json` tiver) e o
 * que vai nela. */
interface Row {
  id: number;
  port: string;
  kind: Kind;
  signal: string;
  bits: string;
  invert: boolean;
}

type StepState = 'todo' | 'ok' | 'warn' | 'error' | 'running';

const SOURCE = /^(!?)([A-Za-z_][A-Za-z0-9_$]*)(?:\[(\d+(?::\d+)?)\])?$/;
const PROBLEM = /^\s*"([^"]+)":\s*"[^"]*":\s*(.*)$/;
/** Quanto esperar depois da última mudança para gravar o `fpga.json`. */
const SAVE_DELAY_MS = 450;

let nextId = 1;

function basePort(port: string): string {
  return port.split('[')[0].trim();
}

function parseSource(text: string): Pick<Row, 'kind' | 'signal' | 'bits' | 'invert'> {
  const value = text.trim();
  if (value === '0' || value === '1') return { kind: value, signal: '', bits: '', invert: false };
  const m = SOURCE.exec(value);
  if (!m) return { kind: 'signal', signal: value, bits: '', invert: false };
  return { kind: 'signal', signal: m[2], bits: m[3] ?? '', invert: m[1] === '!' };
}

function sourceOf(row: Row): string | null {
  if (row.kind === '0' || row.kind === '1') return row.kind;
  if (row.kind !== 'signal' || !row.signal) return null;
  const bits = row.bits.trim();
  return `${row.invert ? '!' : ''}${row.signal}${bits ? `[${bits}]` : ''}`;
}

function emptyRow(port: string): Row {
  return { id: nextId++, port, kind: 'none', signal: '', bits: '', invert: false };
}

/** As linhas: uma por porta do topo, na ordem dele, com as ligações do
 * `fpga.json`; as ligações de portas que o topo não tem vêm no fim, para o
 * problema aparecer. */
function rowsFrom(config: FpgaConfig | null, ports: Port[]): Row[] {
  const links = Object.entries(config?.connect ?? {});
  const taken = new Set<number>();
  const rows: Row[] = [];
  for (const port of ports) {
    const mine = links.map((link, i) => [link, i] as const).filter(([[p]]) => basePort(p) === port.name);
    if (mine.length === 0) rows.push(emptyRow(port.name));
    for (const [[p, s], i] of mine) {
      taken.add(i);
      rows.push({ id: nextId++, port: p, ...parseSource(s) });
    }
  }
  links.forEach(([p, s], i) => {
    if (!taken.has(i)) rows.push({ id: nextId++, port: p, ...parseSource(s) });
  });
  return rows;
}

function width(signal: BoardSignal): number {
  return signal.pins.length;
}

function withRange(name: string, bits: number): string {
  return bits > 1 ? `${name}[${bits - 1}:0]` : name;
}

/** Liga as portas soltas: pelo nome igual ao de um sinal da placa (um topo
 * escrito para ela); o clock e o reset de um processador SAPHO (`clk` no
 * oscilador, `rst` no primeiro botão, invertido); e as entradas e saídas de
 * vários bits nas chaves e nos LEDs que sobrarem. */
function autoConnect(rows: Row[], ports: Port[], board: Board): { rows: Row[]; count: number } {
  const used = new Set(rows.filter((r) => r.kind === 'signal').map((r) => r.signal));
  const take = (s: BoardSignal | undefined) => (s && !used.has(s.name) ? (used.add(s.name), s) : undefined);
  const inputs = board.signals.filter((s) => s.direction === 'input');
  const outputs = board.signals.filter((s) => s.direction === 'output');
  const clock = inputs.find((s) => s.clock_mhz != null);
  const button = inputs.find((s) => s.active_low && s.clock_mhz == null);
  const widest = (list: BoardSignal[]) =>
    list.filter((s) => !s.active_low && s.clock_mhz == null && width(s) > 1 && !used.has(s.name)).sort((a, b) => width(b) - width(a))[0];
  let count = 0;
  let spare: BoardSignal | undefined;
  let spareBit = 0;
  const connect = (row: Row, signal: string, bits = '', invert = false): Row => {
    count++;
    return { ...row, kind: 'signal', signal, bits, invert };
  };
  const next = rows.map((row) => {
    if (row.kind !== 'none') return row;
    const port = ports.find((p) => p.name === basePort(row.port));
    if (!port || port.direction === 'inout') return row;
    const side = port.direction === 'input' ? inputs : outputs;
    const name = port.name.toLowerCase();
    const same = take(side.find((s) => s.name.toLowerCase() === name));
    if (same) return connect(row, same.name);
    if (port.direction === 'input' && port.width === 1) {
      if (/^(clk|clock|clk_i|i_clk|sys_clk)$/.test(name) && clock) return connect(row, clock.name);
      if (/^(rst|reset|rst_i|i_rst)$/.test(name) && button) return connect(row, button.name, width(button) > 1 ? '0' : '', true);
      if (/^(rst_n|reset_n|resetn|rstn|nreset)$/.test(name) && button) return connect(row, button.name, width(button) > 1 ? '0' : '');
      return row;
    }
    if (port.width > 1) {
      const target = take(widest(side));
      if (target) return connect(row, target.name);
      return row;
    }
    // Saídas de um bit (o `out_en` do SAPHO): um bit por porta, nos LEDs que
    // sobraram.
    if (port.direction === 'output') {
      spare = spare ?? take(widest(outputs));
      if (spare && spareBit < width(spare)) return connect(row, spare.name, String(spareBit++));
    }
    return row;
  });
  return { rows: next, count };
}

/** Os problemas das ligações por porta, e os que não são de uma porta. */
function splitProblems(error: IpcError | null): { byPort: Map<string, string[]>; general: string[] } {
  const byPort = new Map<string, string[]>();
  const general: string[] = [];
  if (!error) return { byPort, general };
  const lines = error.message.split('\n').slice(1);
  for (const line of lines) {
    const m = PROBLEM.exec(line);
    if (m) byPort.set(m[1], [...(byPort.get(m[1]) ?? []), m[2]]);
    else if (line.trim()) general.push(line.trim());
  }
  if (lines.length === 0) general.push(error.message);
  return { byPort, general };
}

function percent(used: number, available: number | null | undefined): number | null {
  if (!available) return null;
  return (used * 100) / available;
}

function percentText(value: number | null): string {
  if (value === null) return '';
  if (value > 0 && value < 1) return '<1%';
  return `${Math.round(value)}%`;
}

function timeOf(ms: number | null): string {
  if (!ms) return '';
  const date = new Date(ms);
  const today = new Date().toDateString() === date.toDateString();
  return today ? date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : date.toLocaleString();
}

export function BoardView() {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot);
  const quartus = useApp((s) => s.toolchain?.quartus ?? null);
  const running = useJobs((s) => s.running);
  const last = useJobs((s) => s.last);
  const build = useJobs((s) => s.outcomes.fpga_build ?? null);
  const programmed = useJobs((s) => s.outcomes.fpga_program?.fpga_program ?? null);

  const [boards, setBoards] = useState<Board[]>([]);
  const [modules, setModules] = useState<string[]>([]);
  const [hasConfig, setHasConfig] = useState(false);
  const [boardId, setBoardId] = useState('');
  const [top, setTop] = useState('');
  const [rows, setRows] = useState<Row[]>([]);
  const [ports, setPorts] = useState<ModuleInterface | null>(null);
  const [portsError, setPortsError] = useState<IpcError | null>(null);
  const [check, setCheck] = useState<FpgaPrepared | null>(null);
  const [checkError, setCheckError] = useState<IpcError | null>(null);
  const [status, setStatus] = useState<BitstreamState | null>(null);
  const [cables, setCables] = useState<string[] | null>(null);
  const [saving, setSaving] = useState<'idle' | 'pending' | 'saved' | 'error'>('idle');
  const [saveError, setSaveError] = useState<IpcError | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const saveTimer = useRef<number | null>(null);
  const loaded = useRef(false);

  const board = boards.find((b) => b.id === boardId) ?? null;

  const refreshCheck = useCallback(async () => {
    try {
      setCheck(await api.fpga.check());
      setCheckError(null);
    } catch (error) {
      setCheck(null);
      setCheckError(error as IpcError);
    }
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await api.fpga.status());
    } catch {
      setStatus(null);
    }
  }, []);

  const refreshCables = useCallback(async () => {
    setCables(null);
    try {
      setCables(await api.fpga.cables());
    } catch {
      setCables([]);
    }
  }, []);

  /** Lê tudo do disco: as placas, o `fpga.json`, os módulos e as portas do
   * topo, e monta a tabela. */
  const load = useCallback(async () => {
    const [list, config, mods] = await Promise.all([
      api.fpga.boards().catch(() => [] as Board[]),
      api.fpga.config().catch(() => null),
      api.fpga.modules().catch(() => [] as string[]),
    ]);
    setBoards(list);
    setModules(mods);
    setHasConfig(!!config);
    setBoardId(config?.board ?? list[0]?.id ?? '');
    setTop(config?.top ?? '');
    let interfacePorts: Port[] = [];
    try {
      const found = await api.fpga.top(config?.top ?? null);
      setPorts(found);
      setPortsError(null);
      interfacePorts = found.ports;
    } catch (error) {
      setPorts(null);
      setPortsError(error as IpcError);
    }
    setRows(rowsFrom(config, interfacePorts));
    loaded.current = true;
    if (config) {
      void refreshCheck();
      void refreshStatus();
    } else {
      setCheck(null);
      setCheckError(null);
      setStatus(null);
    }
  }, [refreshCheck, refreshStatus]);

  useEffect(() => {
    if (!snapshot) return;
    loaded.current = false;
    void load();
    void refreshCables();
  }, [snapshot?.spf, load, refreshCables]);

  // Depois de uma operação: compilar muda o arquivo de gravação e, com os
  // processadores, as portas do topo.
  useEffect(() => {
    if (!last || !loaded.current) return;
    if (['fpga_build', 'fpga_program', 'build'].includes(last.flow)) {
      void refreshStatus();
      void refreshCheck();
      if (last.flow !== 'fpga_program' && !ports) void load();
    }
  }, [last]);

  /** Grava o `fpga.json` logo depois da última mudança e confere. */
  const persist = useCallback(
    (nextBoard: string, nextTop: string, nextRows: Row[]) => {
      if (saveTimer.current) window.clearTimeout(saveTimer.current);
      setSaving('pending');
      saveTimer.current = window.setTimeout(async () => {
        saveTimer.current = null;
        try {
          await api.fpga.setConfig({ board: nextBoard, top: nextTop.trim() || null, connect: connectOf(nextRows) });
          setHasConfig(true);
          setSaving('saved');
          setSaveError(null);
          await Promise.all([refreshCheck(), refreshStatus()]);
        } catch (error) {
          setSaving('error');
          setSaveError(error as IpcError);
        }
      }, SAVE_DELAY_MS);
    },
    [refreshCheck, refreshStatus],
  );

  useEffect(
    () => () => {
      if (saveTimer.current) window.clearTimeout(saveTimer.current);
    },
    [],
  );

  const changeRows = (next: Row[]) => {
    setRows(next);
    setNotice(null);
    persist(boardId, top, next);
  };

  /** As ligações da tabela como o `fpga.json` as guarda. */
  const connectOf = (list: Row[]): Record<string, string> => {
    const connect: Record<string, string> = {};
    for (const row of list) {
      const source = sourceOf(row);
      if (source && row.port.trim()) connect[row.port.trim()] = source;
    }
    return connect;
  };

  // Trocar a placa ou o topo nunca apaga ligações: um sinal que a outra
  // placa não tem aparece como problema na linha, e voltar restaura.
  const changeBoard = (id: string) => {
    setBoardId(id);
    persist(id, top, rows);
  };

  const changeTop = async (name: string) => {
    setTop(name);
    try {
      const found = await api.fpga.top(name || null);
      setPorts(found);
      setPortsError(null);
      // As ligações das portas que o novo topo também tem continuam.
      const next = rowsFrom({ board: boardId, connect: connectOf(rows) }, found.ports);
      setRows(next);
      persist(boardId, name, next);
    } catch (error) {
      setPorts(null);
      setPortsError(error as IpcError);
    }
  };

  const problems = useMemo(() => splitProblems(checkError), [checkError]);

  if (!snapshot) {
    return (
      <div className="view-page">
        <Empty>{t('explorer.noProject')}</Empty>
      </div>
    );
  }

  const busy = !!running;
  const buildRunning = running?.flow === 'fpga_build';
  const programRunning = running?.flow === 'fpga_program';
  const pending = saving === 'pending';
  const connectedCount = rows.filter((r) => r.kind !== 'none').length;
  const linksOk = hasConfig && !pending && !checkError && !!check;
  const ready = !!status?.bitstream && status.reasons.length === 0;
  const lastBuild: FpgaBuildResult | null = build?.fpga ?? null;
  const buildFailed = !!build && !build.succeeded;

  const linkState: StepState = !hasConfig ? 'todo' : pending ? 'running' : checkError ? 'error' : check && check.resolved.notes.length ? 'warn' : check ? 'ok' : 'todo';
  const buildState: StepState = buildRunning ? 'running' : buildFailed && !ready ? 'error' : ready ? 'ok' : status?.bitstream ? 'warn' : 'todo';
  const programState: StepState = programRunning
    ? 'running'
    : programmed && programmed.status !== 'succeeded'
      ? 'error'
      : programmed?.status === 'succeeded' && ready
        ? 'ok'
        : cables && cables.length === 0
          ? 'warn'
          : 'todo';

  const canBuild = !!quartus && !busy && linksOk;
  const canProgram = !!quartus && !busy && ready && linksOk && !!cables && cables.length > 0;

  return (
    <div className="view-page board-page">
      <div className="view-page__title">
        <h1>{t('board.title')}</h1>
        <div className="button-row">
          <Button
            variant="primary"
            icon={<Upload size={14} />}
            disabled={!canBuild}
            title={canBuild ? undefined : t('board.buildBlocked')}
            onClick={() => void runFpgaBuildAndProgram()}
          >
            {t('action.fpgaBuildProgram')}
          </Button>
        </div>
      </div>

      <div className="board-head">
        <label className="board-head__field">
          <span>{t('board.board')}</span>
          <select className="select" value={boardId} disabled={busy} onChange={(e) => changeBoard(e.target.value)}>
            {boards.map((b) => (
              <option key={b.id} value={b.id}>
                {b.name}
              </option>
            ))}
          </select>
        </label>
        {board && (
          <span className="muted">
            {board.device.family} {board.device.part} · {board.jtag.cable === 'usb-blasterII' ? 'USB-Blaster II' : 'USB-Blaster'}
          </span>
        )}
        <label className="board-head__field">
          <span>{t('board.top')}</span>
          <select className="select" value={top} disabled={busy} onChange={(e) => void changeTop(e.target.value)}>
            <option value="">{ports && !top ? t('board.topAuto', { name: ports.name }) : t('board.topAutoUnknown')}</option>
            {modules.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        </label>
      </div>

      {!quartus && (
        <p className="notice notice--warn">
          {t('board.noQuartus')}{' '}
          <button type="button" className="link" onClick={() => useEditor.getState().openView('settings')}>
            {t('action.settings')}
          </button>
        </p>
      )}

      <table className="board-status">
        <tbody>
        <StepCard title={t('board.step.links')} state={linkState}>
          {!hasConfig ? (
            <p>{t('board.links.todo')}</p>
          ) : pending ? (
            <p>{t('board.links.saving')}</p>
          ) : checkError ? (
            <p>{t('board.links.problems', { count: problems.byPort.size + problems.general.length })}</p>
          ) : check ? (
            <p>{t('board.links.ok', { count: connectedCount })}</p>
          ) : (
            <p>{t('board.links.checking')}</p>
          )}
        </StepCard>

        <StepCard
          title={t('board.step.build')}
          state={buildState}
          action={
            <Button small icon={<Hammer size={13} />} disabled={!canBuild} onClick={() => void runFpgaBuild()}>
              {t('board.build')}
            </Button>
          }
        >
          {buildRunning ? (
            <p>{t('board.build.running')}</p>
          ) : ready ? (
            <p>{t('board.build.ready', { time: timeOf(status?.built_at_ms ?? null) })}</p>
          ) : status?.bitstream ? (
            <>
              <p>{t('board.build.stale')}</p>
              <ul className="board-reasons">
                {status.reasons.slice(0, 3).map((r) => (
                  <li key={r}>{r}</li>
                ))}
              </ul>
            </>
          ) : buildFailed ? (
            <p>{t('board.build.failed')}</p>
          ) : (
            <p>{t('board.build.todo')}</p>
          )}
        </StepCard>

        <StepCard
          title={t('board.step.program')}
          state={programState}
          action={
            <>
              <IconButton label={t('board.cables.refresh')} disabled={busy || cables === null} onClick={() => void refreshCables()}>
                <RefreshCw size={13} />
              </IconButton>
              <Button small icon={<Upload size={13} />} disabled={!canProgram} onClick={() => void runFpgaProgram()}>
                {t('board.program')}
              </Button>
            </>
          }
        >
          {programRunning ? (
            <p>{t('board.program.running')}</p>
          ) : cables === null ? (
            <p>{t('board.cables.looking')}</p>
          ) : cables.length === 0 ? (
            <p>{t('board.cables.none')}</p>
          ) : programmed?.status === 'succeeded' && ready ? (
            <p>{t('board.program.done', { cable: programmed.cable })}</p>
          ) : programmed && programmed.status !== 'succeeded' ? (
            <p>{t('board.program.failed')}</p>
          ) : (
            <p>{t('board.cables.found', { cable: cables[0] })}</p>
          )}
        </StepCard>
        </tbody>
      </table>

      <section className="board-section">
        <div className="board-section__title">
          <h2>{t('board.connections')}</h2>
          <span className={`board-save board-save--${saving}`}>
            {saving === 'pending' ? t('board.saving') : saving === 'saved' ? t('board.saved') : saving === 'error' ? t('board.saveFailed') : ''}
          </span>
          <div className="button-row">
            <Button
              small
              icon={<Cable size={13} />}
              disabled={busy || !board || !ports}
              onClick={() => {
                if (!board || !ports) return;
                const result = autoConnect(rows, ports.ports, board);
                if (result.count) changeRows(result.rows);
                setNotice(result.count ? t('board.auto.done', { count: result.count }) : t('board.auto.none'));
              }}
            >
              {t('board.auto')}
            </Button>
            <Button
              small
              icon={<FileCode size={13} />}
              disabled={!hasConfig}
              onClick={() => void useEditor.getState().openFile(`${snapshot.root}/fpga.json`)}
            >
              {t('board.openJson')}
            </Button>
          </div>
        </div>
        {notice && <p className="muted">{notice}</p>}
        {saveError && <p className="notice notice--error">{saveError.message}</p>}

        {portsError ? (
          <div className="notice notice--warn">
            <p>{t('board.ports.missing')}</p>
            <p className="muted">{portsError.message}</p>
            <Button small disabled={busy} onClick={() => void useJobs.getState().run({ flow: 'build' }).then(() => load())}>
              {t('board.ports.build')}
            </Button>
          </div>
        ) : (
          <ConnectionTable
            rows={rows}
            ports={ports?.ports ?? []}
            board={board}
            problems={problems.byPort}
            disabled={busy}
            onChange={changeRows}
          />
        )}
        {problems.general.length > 0 && (
          <ul className="board-problems">
            {problems.general.map((p) => (
              <li key={p}>{p}</li>
            ))}
          </ul>
        )}
        {check && !checkError && !pending && <CheckSummary check={check} />}
      </section>

      {lastBuild && <BuildSummary result={lastBuild} />}

      {board && (
        <details className="board-section">
          <summary>{t('board.signals', { board: board.name })}</summary>
          <SignalTable board={board} />
        </details>
      )}
    </div>
  );
}

/** Uma linha do quadro de estado: a etapa, o estado em texto (só o ícone
 * leva cor) e os botões dela. */
function StepCard({
  title,
  state,
  action,
  children,
}: {
  title: string;
  state: StepState;
  action?: ReactNode;
  children: ReactNode;
}) {
  const icon =
    state === 'ok' ? (
      <CircleCheck size={14} />
    ) : state === 'error' ? (
      <CircleX size={14} />
    ) : state === 'warn' ? (
      <TriangleAlert size={14} />
    ) : state === 'running' ? (
      <LoaderCircle size={14} className="spin" />
    ) : (
      <CircleDashed size={14} />
    );
  return (
    <tr className={`board-status__row board-status__row--${state}`}>
      <th scope="row">{title}</th>
      <td className="board-status__icon">{icon}</td>
      <td className="board-status__text">{children}</td>
      <td className="board-status__actions">{action}</td>
    </tr>
  );
}

type Group = 'clocks' | 'buttons' | 'switches' | 'leds' | 'displays';

const GROUPS: Group[] = ['clocks', 'buttons', 'switches', 'leds', 'displays'];

/** O grupo de um sinal na lista: pelo clock, pela direção e por ser ativo
 * em nível baixo (os botões e os segmentos dos displays). */
function groupOf(signal: BoardSignal): Group {
  if (signal.clock_mhz != null) return 'clocks';
  if (signal.direction === 'input') return signal.active_low ? 'buttons' : 'switches';
  return signal.active_low ? 'displays' : 'leds';
}

/** Os bits efetivos de uma ligação: os escritos, ou os de baixo que cabem
 * na porta. */
function effectiveBits(row: Row, signal: BoardSignal | undefined, port: Port | undefined): string {
  if (!signal || width(signal) === 1) return '';
  if (row.bits.trim()) return row.bits.trim();
  const n = Math.min(port?.width ?? width(signal), width(signal));
  return n === 1 ? '0' : `${n - 1}:0`;
}

/** A ligação de uma porta como etiqueta (`~KEY[0]`, `SW[15:0]`); um clique
 * abre o painel com os sinais da placa, os bits e o inverter. */
function SignalPicker({
  row,
  port,
  board,
  disabled,
  onChange,
}: {
  row: Row;
  port: Port | undefined;
  board: Board | null;
  disabled: boolean;
  onChange: (change: Partial<Row>) => void;
}) {
  const t = useT();
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const input = port ? port.direction !== 'output' : true;
  const signal = board?.signals.find((s) => s.name === row.signal);

  useLayoutEffect(() => {
    if (!open || !button.current || !panel.current) return;
    const anchor = button.current.getBoundingClientRect();
    const rect = panel.current.getBoundingClientRect();
    const x = Math.max(8, Math.min(anchor.left, window.innerWidth - rect.width - 8));
    const below = anchor.bottom + 4;
    const y = below + rect.height > window.innerHeight - 8 ? Math.max(8, anchor.top - rect.height - 4) : below;
    setPosition({ x, y });
  }, [open, row.signal]);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      const target = e.target as Node;
      if (!panel.current?.contains(target) && !button.current?.contains(target)) setOpen(false);
    };
    const escape = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    window.addEventListener('mousedown', close);
    window.addEventListener('keydown', escape);
    return () => {
      window.removeEventListener('mousedown', close);
      window.removeEventListener('keydown', escape);
    };
  }, [open]);

  const bits = effectiveBits(row, signal, port);
  const label =
    row.kind === '0' || row.kind === '1'
      ? t(row.kind === '0' ? 'board.signal.zero' : 'board.signal.one')
      : row.kind === 'signal'
        ? `${row.invert ? '~' : ''}${row.signal}${bits ? `[${bits}]` : ''}`
        : t('board.signal.none');
  const candidates = board?.signals.filter((s) => s.direction === (input ? 'input' : 'output')) ?? [];
  const mode = !row.bits.trim() ? 'auto' : row.bits.includes(':') ? 'range' : 'one';
  const [msb, lsb] = row.bits.includes(':') ? row.bits.split(':') : [row.bits, row.bits];

  return (
    <>
      <button
        ref={button}
        type="button"
        className={`board-pick${row.kind === 'none' ? ' board-pick--empty' : ''}`}
        disabled={disabled}
        onClick={() => setOpen(!open)}
      >
        <span className="mono">{label}</span>
        {row.kind === 'signal' && row.invert && <span className="board-pick__tag">{t('board.inverted')}</span>}
      </button>
      {open && (
        <div ref={panel} className="menu menu--floating board-panel" style={{ left: position.x, top: position.y }}>
          <div className="board-panel__fixed">
            <button type="button" className={`board-chip${row.kind === 'none' ? ' is-on' : ''}`} onClick={() => onChange({ kind: 'none', signal: '', bits: '', invert: false })}>
              {t('board.signal.none')}
            </button>
            {input && (
              <>
                <button type="button" className={`board-chip${row.kind === '0' ? ' is-on' : ''}`} onClick={() => onChange({ kind: '0', signal: '', bits: '', invert: false })}>
                  {t('board.signal.zero')}
                </button>
                <button type="button" className={`board-chip${row.kind === '1' ? ' is-on' : ''}`} onClick={() => onChange({ kind: '1', signal: '', bits: '', invert: false })}>
                  {t('board.signal.one')}
                </button>
              </>
            )}
          </div>
          {GROUPS.map((group) => {
            const list = candidates.filter((s) => groupOf(s) === group);
            if (list.length === 0) return null;
            return (
              <div key={group} className="board-panel__group">
                <span className="board-panel__label">{t(`board.group.${group}` as Key)}</span>
                <div className="board-panel__chips">
                  {list.map((s) => (
                    <button
                      key={s.name}
                      type="button"
                      title={s.description}
                      className={`board-chip mono${row.kind === 'signal' && row.signal === s.name ? ' is-on' : ''}`}
                      onClick={() => onChange({ kind: 'signal', signal: s.name, bits: '', invert: false })}
                    >
                      {withRange(s.name, width(s))}
                      {s.clock_mhz ? <span className="muted"> {s.clock_mhz} MHz</span> : null}
                    </button>
                  ))}
                </div>
              </div>
            );
          })}
          {row.kind === 'signal' && signal && (
            <div className="board-panel__options">
              {width(signal) > 1 && (
                <div className="board-panel__bits">
                  <span className="board-panel__label">{t('board.bits')}</span>
                  <label>
                    <input type="radio" checked={mode === 'auto'} onChange={() => onChange({ bits: '' })} />
                    {t('board.bits.auto', { bits: `${signal.name}[${effectiveBits({ ...row, bits: '' }, signal, port)}]` })}
                  </label>
                  <label>
                    <input type="radio" checked={mode === 'one'} onChange={() => onChange({ bits: '0' })} />
                    {t('board.bits.one')}
                    <input
                      type="number"
                      className="input input--small board-panel__num"
                      min={0}
                      max={width(signal) - 1}
                      disabled={mode !== 'one'}
                      value={mode === 'one' ? row.bits : ''}
                      onChange={(e) => onChange({ bits: e.target.value })}
                    />
                  </label>
                  <label>
                    <input type="radio" checked={mode === 'range'} onChange={() => onChange({ bits: `${Math.min(port?.width ?? 1, width(signal)) - 1}:0` })} />
                    {t('board.bits.range')}
                    <input
                      type="number"
                      className="input input--small board-panel__num"
                      min={0}
                      max={width(signal) - 1}
                      disabled={mode !== 'range'}
                      value={mode === 'range' ? msb : ''}
                      onChange={(e) => onChange({ bits: `${e.target.value}:${lsb}` })}
                    />
                    :
                    <input
                      type="number"
                      className="input input--small board-panel__num"
                      min={0}
                      max={width(signal) - 1}
                      disabled={mode !== 'range'}
                      value={mode === 'range' ? lsb : ''}
                      onChange={(e) => onChange({ bits: `${msb}:${e.target.value}` })}
                    />
                  </label>
                </div>
              )}
              <label className="board-panel__invert">
                <input type="checkbox" checked={row.invert} onChange={(e) => onChange({ invert: e.target.checked })} />
                {t('board.invert')}
                <span className="muted">{signal.active_low ? t('board.invert.activeLow', { name: signal.name }) : t('board.invert.hint')}</span>
              </label>
            </div>
          )}
          <div className="board-panel__foot">
            <Button small onClick={() => setOpen(false)}>
              {t('board.done')}
            </Button>
          </div>
        </div>
      )}
    </>
  );
}

function ConnectionTable({
  rows,
  ports,
  board,
  problems,
  disabled,
  onChange,
}: {
  rows: Row[];
  ports: Port[];
  board: Board | null;
  problems: Map<string, string[]>;
  disabled: boolean;
  onChange: (rows: Row[]) => void;
}) {
  const t = useT();
  if (rows.length === 0) return <p className="muted">{t('board.ports.none')}</p>;
  const update = (id: number, change: Partial<Row>) => onChange(rows.map((r) => (r.id === id ? { ...r, ...change } : r)));
  return (
    <table className="table board-links">
      <thead>
        <tr>
          <th>{t('board.port')}</th>
          <th>{t('board.signal')}</th>
          <th aria-label={t('common.remove')} />
        </tr>
      </thead>
      <tbody>
        {rows.map((row, index) => {
          const port = ports.find((p) => p.name === basePort(row.port));
          const rowProblems = problems.get(row.port) ?? [];
          const extra = rows.findIndex((r) => basePort(r.port) === basePort(row.port)) !== index;
          return (
            <tr key={row.id} className={rowProblems.length ? 'board-links__row--error' : undefined}>
              <td>
                <span className="mono">{row.port}</span>
                {port && (
                  <span className="board-links__meta">
                    {port.direction === 'input' ? t('board.dir.in') : port.direction === 'output' ? t('board.dir.out') : t('board.dir.inout')}
                    {' · '}
                    {t('board.bitsCount', { count: port.width })}
                  </span>
                )}
                {!port && <span className="board-links__meta text-error">{t('board.ports.unknown')}</span>}
              </td>
              <td>
                <SignalPicker row={row} port={port} board={board} disabled={disabled} onChange={(change) => update(row.id, change)} />
                {rowProblems.map((p) => (
                  <span key={p} className="board-links__problem">
                    {p}
                  </span>
                ))}
              </td>
              <td className="board-links__actions">
                {extra || !port ? (
                  <IconButton label={t('common.remove')} disabled={disabled} onClick={() => onChange(rows.filter((r) => r.id !== row.id))}>
                    <Trash2 size={14} />
                  </IconButton>
                ) : port.width > 1 && row.kind === 'signal' ? (
                  <IconButton
                    label={t('board.addRange')}
                    className="board-links__add"
                    disabled={disabled}
                    onClick={() => {
                      const at = rows.findIndex((r) => r.id === row.id);
                      onChange([...rows.slice(0, at + 1), emptyRow(`${port.name}[${port.width - 1}:0]`), ...rows.slice(at + 1)]);
                    }}
                  >
                    <Plus size={14} />
                  </IconButton>
                ) : null}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function CheckSummary({ check }: { check: FpgaPrepared }) {
  const t = useT();
  const r = check.resolved;
  return (
    <div className="board-check">
      <h3>{t('board.result')}</h3>
      <table className="table board-check__table">
        <tbody>
          {check.connections.map((c) => (
            <tr key={c.target}>
              <td className="mono">{c.target}</td>
              <td className="muted">&larr;</td>
              <td className="mono">{c.source}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {r.clocks.map((c) => (
        <p key={`${c.signal}-${c.port}`} className="muted">
          {t('board.clock', { signal: c.signal, mhz: c.mhz, port: c.port })}
        </p>
      ))}
      {r.notes.map((note) => (
        <p key={note} className="text-warn">
          {t('console.note', { note })}
        </p>
      ))}
      <details>
        <summary>{t('board.showTop')}</summary>
        <pre className="mono board-code">{check.board_top}</pre>
      </details>
    </div>
  );
}

const MAIN_RESOURCES = /logic elements|ALMs|registers|pins|memory bits|Multiplier|DSP|PLLs/i;

function BuildSummary({ result }: { result: FpgaBuildResult }) {
  const t = useT();
  const shown: ResourceUsage[] = result.resources.filter(
    (r) => !r.detail && MAIN_RESOURCES.test(r.name) && !/virtual/i.test(r.name),
  );
  return (
    <section className="board-section">
      <div className="board-section__title">
        <h2>{t('board.lastBuild')}</h2>
        <span className="muted">
          {t(`console.status.${result.status}` as Key)} · {formatDuration(result.duration_ms)} · Quartus {result.quartus.version ?? ''}
        </span>
      </div>
      {shown.length > 0 && (
        <div className="stat-grid">
          {shown.map((r) => {
            const p = percent(r.used, r.available);
            return (
              <div key={r.name} className="stat">
                <span className="stat__label">{r.name}</span>
                <span className="stat__value">
                  {r.used.toLocaleString()}
                  {r.available ? <span className="muted"> / {r.available.toLocaleString()}</span> : null}
                </span>
                {p !== null && (
                  <span className="board-meter" title={percentText(p)}>
                    <span style={{ width: `${Math.max(p, p > 0 ? 2 : 0)}%` }} />
                  </span>
                )}
                {p !== null && <span className="muted">{percentText(p)}</span>}
              </div>
            );
          })}
        </div>
      )}
      {result.timing && (
        <>
          <p className={result.timing.met ? 'text-ok' : 'text-warn'}>
            {result.timing.met ? t('board.timingMet') : t('console.fpgaTimingNotMet')}
          </p>
          <table className="table">
            <thead>
              <tr>
                <th>{t('board.clockName')}</th>
                <th className="num">{t('board.target')}</th>
                <th className="num">Fmax</th>
                <th className="num">{t('board.setupSlack')}</th>
                <th className="num">{t('board.holdSlack')}</th>
              </tr>
            </thead>
            <tbody>
              {result.timing.clocks.map((c) => (
                <tr key={c.clock}>
                  <td className="mono">{c.clock}</td>
                  <td className="num">{c.target_mhz == null ? '-' : `${c.target_mhz} MHz`}</td>
                  <td className="num">{c.fmax_mhz == null ? '-' : `${c.fmax_mhz} MHz`}</td>
                  <td className={`num${(c.setup_slack_ns ?? 0) < 0 ? ' text-error' : ''}`}>
                    {c.setup_slack_ns == null ? '-' : `${c.setup_slack_ns} ns`}
                  </td>
                  <td className={`num${(c.hold_slack_ns ?? 0) < 0 ? ' text-error' : ''}`}>
                    {c.hold_slack_ns == null ? '-' : `${c.hold_slack_ns} ns`}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      {!result.timing && result.status === 'succeeded' && <p className="muted">{t('board.noTiming')}</p>}
      {result.status !== 'succeeded' && <p className="text-error">{t('board.build.failed')}</p>}
    </section>
  );
}

function SignalTable({ board }: { board: Board }) {
  const t = useT();
  return (
    <table className="table">
      <thead>
        <tr>
          <th>{t('board.signal')}</th>
          <th>{t('board.direction')}</th>
          <th>{t('board.pins')}</th>
          <th>{t('board.standard')}</th>
        </tr>
      </thead>
      <tbody>
        {board.signals.map((s) => (
          <tr key={s.name}>
            <td className="mono" title={s.description}>
              {withRange(s.name, width(s))}
              {s.active_low && <span className="badge">{t('board.activeLow')}</span>}
              {s.clock_mhz ? <span className="badge">{s.clock_mhz} MHz</span> : null}
            </td>
            <td>{s.direction === 'input' ? t('board.dir.in') : t('board.dir.out')}</td>
            <td className="mono board-pins">{s.pins.map((p) => p.replace('PIN_', '')).join(' ')}</td>
            <td>{Array.isArray(s.io_standard) ? [...new Set(s.io_standard)].join(', ') : s.io_standard}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

