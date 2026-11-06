// A placa virtual: a placa desenhada a partir do JSON dela (BoardLayout), com
// o estado que o `vboard.run` manda. Em cima, o monitor (VGA) e, ao lado, a
// serial e o teclado; embaixo, a placa: os displays, os LEDs alinhados sobre
// as chaves (como na DE2-115) e, à direita, os LEDs que sobram sobre os
// botões. Cada elemento mostra a porta do topo ligada a ele; no modo de
// editar as ligações, um clique escolhe a porta (`fpga.connect`). A sessão
// vive em state/vboard.ts.

import { Cable, Keyboard, MonitorPlay, Pause, Play, Power, TableProperties } from 'lucide-react';
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from 'react';

import { errorText, useT } from '../../i18n';
import type { LayoutPeripheral, LayoutSignal, PortChoice } from '../../ipc/aurora-types';
import { api } from '../../ipc/api';
import { useEditor } from '../../state/editor';
import { useProject } from '../../state/project';
import { useVboard } from '../../state/vboard';
import { Button, Checkbox } from '../common';

type Port = NonNullable<ReturnType<typeof useVboard.getState>['ports']>[number];

/** Os bits de `count`, do mais alto ao mais baixo, como na placa. */
function bitsDown(count: number): number[] {
  return Array.from({ length: count }, (_, i) => count - 1 - i);
}

function signalOf(item: LayoutPeripheral, name: string): LayoutSignal | undefined {
  return item.signals.find((s) => s.name === name);
}

/** O alvo de uma escolha de porta: um bit de um sinal, ou o sinal inteiro. */
interface Target {
  signal: string;
  bit: number | null;
  /** Quantos bits o alvo tem (1 num bit; a largura do sinal inteiro). */
  span: number;
  direction: 'input' | 'output';
  activeLow: boolean;
  current: string | null;
  anchor: DOMRect;
}

export function VirtualBoardView() {
  const t = useT();
  const spf = useProject((s) => s.snapshot?.spf ?? null);
  const status = useVboard((s) => s.status);
  const layout = useVboard((s) => s.layout);
  const layoutError = useVboard((s) => s.layoutError);
  const board = useVboard((s) => s.board);
  const diagnostics = useVboard((s) => s.diagnostics);
  const buildLine = useVboard((s) => s.buildLine);
  const ended = useVboard((s) => s.ended);
  const timePs = useVboard((s) => s.timePs);
  const speed = useVboard((s) => s.speed);
  const paused = useVboard((s) => s.paused);
  const realtime = useVboard((s) => s.realtime);
  const output = useVboard((s) => s.output);
  const editing = useVboard((s) => s.editing);
  const [boards, setBoards] = useState<{ id: string; name: string }[]>([]);
  const [target, setTarget] = useState<Target | null>(null);

  useEffect(() => {
    if (spf && useVboard.getState().status === 'idle') void useVboard.getState().loadLayout();
  }, [spf]);
  useEffect(() => {
    if (layoutError?.code !== 'no_fpga_config') return;
    void api.fpga
      .boards()
      .then((list) => setBoards(list.map((b) => ({ id: b.id, name: b.name }))))
      .catch(() => setBoards([]));
  }, [layoutError?.code]);

  const running = status === 'running';
  const busy = status === 'building' || status === 'stopping';
  const { start, stop, togglePause, setRealtime, chooseBoard, setEditing } = useVboard.getState();
  // Mudar as ligações pede compilar de novo: só com a placa desligada.
  const canEdit = status === 'idle' && !!layout;

  const statusText =
    status === 'building'
      ? t('vboard.building')
      : status === 'stopping'
        ? t('vboard.stopping')
        : running
          ? t(paused ? 'vboard.paused' : 'vboard.running', {
              ms: (timePs / 1e9).toLocaleString(undefined, { maximumFractionDigits: 1 }),
              speed: speed.toLocaleString(undefined, { maximumFractionDigits: speed < 0.1 ? 3 : 2 }),
            })
          : ended
            ? t(`vboard.ended.${ended}` as 'vboard.ended.crashed')
            : t('vboard.off');

  /** Abre a escolha da porta para um elemento, no modo de edição. */
  const pick = (event: ReactMouseEvent, info: Omit<Target, 'anchor'>) => {
    if (!editing) return false;
    event.preventDefault();
    setTarget({ ...info, anchor: (event.currentTarget as HTMLElement).getBoundingClientRect() });
    return true;
  };

  const peripherals = layout?.peripherals ?? [];
  // O monitor, a serial e o teclado só aparecem inteiros quando o topo os
  // usa; os outros ficam numa linha, para não ocupar a página vazios.
  const allMedia = peripherals.filter((p) => ['vga', 'uart', 'ps2'].includes(p.peripheral.kind));
  const media = allMedia.filter((p) => p.connected);
  const unused = allMedia.filter((p) => !p.connected);
  const video = media.filter((p) => p.peripheral.kind === 'vga');
  const side = media.filter((p) => p.peripheral.kind !== 'vga');
  const mediaName = (item: LayoutPeripheral) =>
    item.peripheral.kind === 'uart' ? t('vboard.kind.uart', { name: item.id }) : t(`vboard.kind.${item.peripheral.kind}` as 'vboard.kind.vga');

  return (
    <div className={`view-page vboard-page${editing ? ' is-editing' : ''}`}>
      <div className="vboard-header">
        <div className="vboard-header__title">
          <MonitorPlay size={20} />
          <h1>{t('vboard.title')}</h1>
          {layout && <span className="vboard-chip">{layout.board_name}</span>}
        </div>
        <div className="button-row">
          <Button
            icon={<Cable size={14} />}
            variant={editing ? 'primary' : 'default'}
            disabled={!canEdit && !editing}
            title={canEdit ? undefined : t('vboard.editOff')}
            onClick={() => setEditing(!editing)}
          >
            {editing ? t('vboard.editDone') : t('vboard.edit')}
          </Button>
          {running && (
            <Button icon={paused ? <Play size={14} /> : <Pause size={14} />} onClick={togglePause}>
              {paused ? t('vboard.resume') : t('vboard.pause')}
            </Button>
          )}
          {status === 'idle' ? (
            <Button
              variant="primary"
              icon={<Power size={14} />}
              disabled={!layout || !spf || editing}
              title={layout ? undefined : t('vboard.noLayout')}
              onClick={() => void start()}
            >
              {t('vboard.start')}
            </Button>
          ) : (
            <Button variant="danger" icon={<Power size={14} />} disabled={status === 'stopping'} onClick={stop}>
              {t('vboard.stop')}
            </Button>
          )}
        </div>
      </div>

      <div className="vboard-status">
        <span className={`vboard-status__dot${running && !paused ? ' is-live' : ''}${busy ? ' is-busy' : ''}`} />
        <span className="vboard-status__text">{statusText}</span>
        {layout && (
          <span className="vboard-status__meta">
            {t('vboard.top', { top: layout.top })}
            {layout.clocks.length > 0 && ` · ${layout.clocks.map((c) => `${c.signal} ${c.mhz} MHz`).join(', ')}`}
          </span>
        )}
        <Checkbox checked={realtime} onChange={setRealtime} label={t('vboard.realtime')} disabled={busy} />
      </div>
      {status === 'building' && buildLine && <div className="vboard-buildline">{buildLine}</div>}

      {editing && (
        <div className="notice vboard-edit-hint">
          <p>{t('vboard.editHint')}</p>
          <Button small icon={<TableProperties size={13} />} onClick={() => useEditor.getState().openView('board')}>
            {t('vboard.openTable')}
          </Button>
        </div>
      )}

      {layoutError && (
        <div className="notice notice--warn">
          <p>{layoutError.code === 'no_fpga_config' ? t('vboard.noFpgaConfig') : errorText(layoutError)}</p>
          {layoutError.code === 'no_fpga_config' && (
            <>
              <p>{t('vboard.chooseBoard')}</p>
              <div className="button-row">
                <select className="select" value={board ?? ''} onChange={(e) => void chooseBoard(e.target.value || null)}>
                  <option value="">{t('vboard.chooseBoardNone')}</option>
                  {boards.map((b) => (
                    <option key={b.id} value={b.id}>
                      {b.name}
                    </option>
                  ))}
                </select>
                <Button onClick={() => useEditor.getState().openView('board')}>{t('vboard.mapPins')}</Button>
              </div>
            </>
          )}
        </div>
      )}
      {board && layout && !layoutError && <p className="vboard-byname">{t('vboard.byName', { board: layout.board_name })}</p>}

      {diagnostics.length > 0 && (
        <div className="notice notice--error">
          <p>{t('vboard.buildFailed')}</p>
          <ul className="vboard-diagnostics">
            {diagnostics.slice(0, 30).map((d, i) => (
              <li key={i}>
                {d.file ? `${d.file.split(/[\\/]/).pop()}${d.line ? `:${d.line}` : ''}: ` : ''}
                {d.message}
              </li>
            ))}
          </ul>
        </div>
      )}

      {layout && media.length > 0 && (
        <div className={`vboard-media${video.length === 0 ? ' vboard-media--side-only' : ''}`}>
          {video.map((item) => (
            <Monitor key={item.id} item={item} live={running} />
          ))}
          {side.length > 0 && (
            <div className="vboard-media__side">
              {side.map((item) =>
                item.peripheral.kind === 'uart' ? (
                  <Serial key={item.id} item={item} live={running} />
                ) : (
                  <KeyboardPad key={item.id} item={item} live={running} />
                ),
              )}
            </div>
          )}
        </div>
      )}

      {layout && <Pcb peripherals={peripherals} name={layout.board_name} live={running} pick={pick} />}

      {layout && unused.length > 0 && (
        <div className="vboard-unused">
          <span>{t('vboard.unused')}</span>
          {unused.map((item) => (
            <span key={item.id} className="vboard-chip">
              {mediaName(item)}
            </span>
          ))}
          <Button small icon={<TableProperties size={13} />} onClick={() => useEditor.getState().openView('board')}>
            {t('vboard.openTable')}
          </Button>
        </div>
      )}

      {layout && layout.notes.length > 0 && (
        <details className="vboard-notes">
          <summary>{t('vboard.notes', { count: layout.notes.length })}</summary>
          <ul>
            {layout.notes.map((note, i) => (
              <li key={i}>{note}</li>
            ))}
          </ul>
        </details>
      )}

      {output.length > 0 && (
        <section className="vboard-card">
          <h3 className="vboard-card__title">{t('vboard.output')}</h3>
          <pre className="vboard-console">{output.join('\n')}</pre>
        </section>
      )}

      {target && <PortPicker target={target} onClose={() => setTarget(null)} />}
    </div>
  );
}

// ------------------------------------------------------------ a placa

type Pick = (event: ReactMouseEvent, info: Omit<Target, 'anchor'>) => boolean;

/** A placa: os displays e, embaixo, os LEDs sobre as chaves (os de mesma
 * largura ficam alinhados bit a bit) e, à direita, os LEDs que sobram sobre
 * os botões. */
function Pcb({ peripherals, name, live, pick }: { peripherals: LayoutPeripheral[]; name: string; live: boolean; pick: Pick }) {
  const t = useT();
  const displays = peripherals.filter((p) => p.peripheral.kind === 'seven_segment');
  const switches = peripherals.filter((p) => p.peripheral.kind === 'switches');
  const buttons = peripherals.filter((p) => p.peripheral.kind === 'buttons');
  const leds = peripherals.filter((p) => p.peripheral.kind === 'leds');
  const widthOf = (p: LayoutPeripheral) => p.signals[0]?.width ?? 0;
  const paired = new Set<string>();
  const columns = switches.map((sw) => {
    const led = leds.find((l) => !paired.has(l.id) && widthOf(l) === widthOf(sw));
    if (led) paired.add(led.id);
    return { sw, led };
  });
  const spareLeds = leds.filter((l) => !paired.has(l.id));
  const rightLeds = buttons.length > 0 ? spareLeds : [];
  const leftLeds = buttons.length > 0 ? [] : spareLeds;

  if (displays.length + switches.length + buttons.length + leds.length === 0) return null;
  return (
    <section className="vboard-pcb" aria-label={name}>
      <div className="vboard-pcb__silk">{name}</div>
      {displays.map((item) => (
        <Displays key={item.id} item={item} pick={pick} />
      ))}
      <div className="vboard-pcb__io">
        <div className="vboard-pcb__left">
          {leftLeds.map((led) => (
            <BitGrid key={led.id} led={led} pick={pick} />
          ))}
          {columns.map(({ sw, led }) => (
            <BitGrid key={sw.id} sw={sw} led={led} pick={pick} />
          ))}
        </div>
        {(buttons.length > 0 || rightLeds.length > 0) && (
          <div className="vboard-pcb__right">
            {rightLeds.map((led) => (
              <BitGrid key={led.id} led={led} pick={pick} />
            ))}
            {buttons.map((item) => (
              <Buttons key={item.id} item={item} pick={pick} />
            ))}
          </div>
        )}
      </div>
      {!live && <p className="vboard-pcb__hint">{t('vboard.pcbHint')}</p>}
    </section>
  );
}

/** Um rótulo de porta embaixo (ou em cima) de um elemento. */
function PortLabel({ text }: { text: string | null | undefined }) {
  return <span className={`vboard-port${text ? '' : ' is-empty'}`}>{text ?? '·'}</span>;
}

/** Uma faixa de bits seguidos da placa ligada a bits seguidos da mesma porta. */
interface Run {
  text: string | null;
  span: number;
}

/** Junta os rótulos bit a bit (`SW[17]`, `SW[16]`, ...) em faixas
 * (`SW[17:0]`, ou só `SW` quando a faixa é a porta inteira), do bit mais alto
 * ao mais baixo, como a grade os mostra. */
function runsOf(labels: (string | null | undefined)[], width: number, portWidth: (name: string) => number | undefined): Run[] {
  type Open = { base: string; invert: boolean; hi: number; lo: number; span: number } | { text: string | null; span: number };
  const runs: Open[] = [];
  for (const bit of bitsDown(width)) {
    const label = labels[bit] ?? null;
    const match = label && !label.includes(',') ? /^(!?)([^[]+)\[(\d+)\]$/.exec(label) : null;
    const last = runs[runs.length - 1];
    if (match) {
      const [, bang, base, index] = match;
      const invert = bang === '!';
      const i = Number(index);
      if (last && 'base' in last && last.base === base && last.invert === invert && last.lo === i + 1) {
        last.lo = i;
        last.span += 1;
        continue;
      }
      runs.push({ base, invert, hi: i, lo: i, span: 1 });
    } else if (!label && last && !('base' in last) && last.text === null) {
      last.span += 1;
    } else {
      runs.push({ text: label, span: 1 });
    }
  }
  return runs.map((run) => {
    if (!('base' in run)) return run;
    const bang = run.invert ? '!' : '';
    const whole = run.lo === 0 && portWidth(run.base) === run.hi + 1;
    if (whole) return { text: `${bang}${run.base}`, span: run.span };
    if (run.span === 1) return { text: `${bang}${run.base}[${run.hi}]`, span: 1 };
    return { text: `${bang}${run.base}[${run.hi}:${run.lo}]`, span: run.span };
  });
}

/** Os rótulos de porta de uma fileira, em faixas que ocupam as colunas dos
 * bits delas; a chave fica embaixo (`below`) ou em cima do rótulo. */
function PortRuns({ labels, width, below }: { labels: (string | null | undefined)[]; width: number; below: boolean }) {
  const ports = useVboard((s) => s.ports);
  const runs = runsOf(labels, width, (name) => ports?.find((p) => p.name === name)?.width);
  return (
    <div className="vboard-bits" style={{ gridTemplateColumns: `repeat(${width}, var(--vb-col))` }}>
      {runs.map((run, i) => (
        <span
          key={i}
          className={`vboard-run${below ? ' vboard-run--below' : ' vboard-run--above'}${run.text ? '' : ' is-empty'}`}
          style={{ gridColumn: `span ${run.span}` }}
          title={run.text ?? undefined}
        >
          {run.text}
        </span>
      ))}
    </div>
  );
}

/** LEDs e chaves numa grade, coluna por bit: o LED em cima da chave do
 * mesmo bit, como os LEDR sobre as SW da DE2-115. As portas ligadas vêm em
 * faixas, em cima dos LEDs e embaixo das chaves. */
function BitGrid({ sw, led, pick }: { sw?: LayoutPeripheral; led?: LayoutPeripheral; pick: Pick }) {
  const t = useT();
  const value = useVboard((s) => (sw ? s.inputs[sw.id] ?? 0 : 0));
  const setBit = useVboard((s) => s.setBit);
  const editing = useVboard((s) => s.editing);
  const swSignal = sw?.signals[0];
  const ledSignal = led?.signals[0];
  const levels = useVboard((s) => (ledSignal ? s.lights[ledSignal.name] : undefined));
  const width = swSignal?.width ?? ledSignal?.width ?? 0;
  const color = led?.peripheral.kind === 'leds' ? led.peripheral.color : 'red';
  const title = [led && t('vboard.kind.leds', { name: ledSignal?.name ?? '' }), sw && t('vboard.kind.switches', { name: swSignal?.name ?? '' })]
    .filter(Boolean)
    .join(' · ');
  const columns = { gridTemplateColumns: `repeat(${width}, var(--vb-col))` };
  return (
    <div className="vboard-group">
      <span className="vboard-group__title">{title}</span>
      {ledSignal && led && (
        <>
          <PortRuns labels={ledSignal.ports} width={width} below={false} />
          <div className="vboard-bits vboard-bits--row" style={columns}>
            {bitsDown(width).map((bit) => (
              <button
                key={bit}
                type="button"
                className={`vboard-led-btn${led.connected ? '' : ' is-off'}`}
                title={`${ledSignal.name}[${bit}]${ledSignal.ports[bit] ? `: ${ledSignal.ports[bit]}` : ''}`}
                onClick={(e) =>
                  pick(e, {
                    signal: ledSignal.name,
                    bit,
                    span: 1,
                    direction: 'output',
                    activeLow: ledSignal.active_low,
                    current: ledSignal.ports[bit] ?? null,
                  })
                }
              >
                <span className={`vboard-led vboard-led--${color}`} style={{ opacity: 0.15 + 0.85 * ((levels?.[bit] ?? 0) / 255) }} />
              </button>
            ))}
          </div>
        </>
      )}
      {swSignal && sw && (
        <div className="vboard-bits vboard-bits--row" style={columns}>
          {bitsDown(width).map((bit) => (
            <button
              key={bit}
              type="button"
              className={`vboard-switch${((value >>> bit) & 1) === 1 ? ' is-on' : ''}${sw.connected ? '' : ' is-off'}`}
              aria-pressed={((value >>> bit) & 1) === 1}
              title={`${swSignal.name}[${bit}]${swSignal.ports[bit] ? `: ${swSignal.ports[bit]}` : ''}`}
              disabled={!sw.connected && !editing}
              onClick={(e) => {
                if (
                  pick(e, {
                    signal: swSignal.name,
                    bit,
                    span: 1,
                    direction: 'input',
                    activeLow: swSignal.active_low,
                    current: swSignal.ports[bit] ?? null,
                  })
                )
                  return;
                setBit(sw.id, bit, ((value >>> bit) & 1) === 0);
              }}
            >
              <span className="vboard-switch__knob" />
            </button>
          ))}
        </div>
      )}
      <div className="vboard-bits" style={columns}>
        {bitsDown(width).map((bit) => (
          <span key={bit} className="vboard-bitnum">
            {bit}
          </span>
        ))}
      </div>
      {swSignal && <PortRuns labels={swSignal.ports} width={width} below />}
    </div>
  );
}

function Buttons({ item, pick }: { item: LayoutPeripheral; pick: Pick }) {
  const t = useT();
  const signal = item.signals[0];
  const value = useVboard((s) => s.inputs[item.id] ?? 0);
  const setBit = useVboard((s) => s.setBit);
  // Os botões presos com Shift+clique: ficam apertados até outro Shift+clique,
  // para segurar um enquanto se aperta outro (zerar na aula 9: KEY[1]
  // segurado e um aperto do KEY[0]).
  const [latched, setLatched] = useState(0);
  if (!signal) return null;
  return (
    <div className="vboard-group">
      <span className="vboard-group__title">{t('vboard.kind.buttons', { name: signal.name })}</span>
      <div className="vboard-bits" style={{ gridTemplateColumns: `repeat(${signal.width}, var(--vb-key))` }}>
        {bitsDown(signal.width).map((bit) => {
          const down = ((value >>> bit) & 1) === 1;
          const isLatched = ((latched >>> bit) & 1) === 1;
          const release = () => {
            if (isLatched) return;
            if (((useVboard.getState().inputs[item.id] ?? 0) >>> bit) & 1) setBit(item.id, bit, false);
          };
          return (
            <div key={bit} className="vboard-bit-col">
              <button
                type="button"
                className={`vboard-button${down ? ' is-down' : ''}${isLatched ? ' is-latched' : ''}${item.connected ? '' : ' is-off'}`}
                title={t('vboard.buttonHint', { name: `${signal.name}[${bit}]${signal.ports[bit] ? ` (${signal.ports[bit]})` : ''}` })}
                onPointerDown={(e) => {
                  if (
                    pick(e, {
                      signal: signal.name,
                      bit,
                      span: 1,
                      direction: 'input',
                      activeLow: signal.active_low,
                      current: signal.ports[bit] ?? null,
                    })
                  )
                    return;
                  if (e.shiftKey) {
                    setLatched(latched ^ (1 << bit));
                    setBit(item.id, bit, !isLatched);
                    return;
                  }
                  if (isLatched) return;
                  (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
                  setBit(item.id, bit, true);
                }}
                onPointerUp={release}
                onPointerCancel={release}
                onKeyDown={(e) => {
                  if ((e.key === ' ' || e.key === 'Enter') && !e.repeat) setBit(item.id, bit, true);
                }}
                onKeyUp={(e) => {
                  if (e.key === ' ' || e.key === 'Enter') release();
                }}
              />
              <span className="vboard-bitnum">{signal.width > 1 ? `${signal.name}${bit}` : signal.name}</span>
              <PortLabel text={signal.ports[bit]} />
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Os segmentos a até g num dígito de 40 x 64. */
const SEGMENTS = [
  'M8 4 L32 4 L28 9 L12 9 Z',
  'M33 5 L33 30 L29 27 L29 10 Z',
  'M33 34 L33 59 L29 54 L29 37 Z',
  'M8 60 L32 60 L28 55 L12 55 Z',
  'M7 34 L7 59 L11 54 L11 37 Z',
  'M7 5 L7 30 L11 27 L11 10 Z',
  'M9 32 L13 29.5 L27 29.5 L31 32 L27 34.5 L13 34.5 Z',
];

/** A porta de um dígito inteiro, em faixa (`seg`, `LEDR[6:0]`); com os
 * segmentos vindos de lugares diferentes, a primeira faixa e reticências. */
function digitPort(signal: LayoutSignal | undefined, portWidth: (name: string) => number | undefined): string | null {
  if (!signal) return null;
  const runs = runsOf(signal.ports, signal.width, portWidth).filter((r) => r.text);
  if (runs.length === 0) return null;
  return runs.length === 1 ? runs[0].text : `${runs[0].text}…`;
}

function Displays({ item, pick }: { item: LayoutPeripheral; pick: Pick }) {
  const lights = useVboard((s) => s.lights);
  const ports = useVboard((s) => s.ports);
  const portWidth = (name: string) => ports?.find((p) => p.name === name)?.width;
  if (item.peripheral.kind !== 'seven_segment') return null;
  return (
    <div className={`vboard-displays${item.connected ? '' : ' is-off'}`}>
      {item.peripheral.digits.map((digit) => {
        const signal = signalOf(item, digit);
        const levels = lights[digit];
        const port = digitPort(signal, portWidth);
        return (
          <button
            key={digit}
            type="button"
            className="vboard-digit-btn"
            title={`${digit}${port ? `: ${port}` : ''}`}
            onClick={(e) =>
              signal &&
              pick(e, {
                signal: digit,
                bit: null,
                span: signal.width,
                direction: 'output',
                activeLow: signal.active_low,
                current: port,
              })
            }
          >
            <svg className="vboard-digit" viewBox="0 0 40 64" role="img" aria-label={digit}>
              {SEGMENTS.map((d, i) => (
                <path key={i} d={d} className="vboard-segment" style={{ opacity: 0.1 + 0.9 * ((levels?.[i] ?? 0) / 255) }} />
              ))}
              <circle cx="36" cy="60" r="2.4" className="vboard-segment" style={{ opacity: 0.1 + 0.9 * ((levels?.[7] ?? 0) / 255) }} />
            </svg>
            <span className="vboard-digit__name">{digit}</span>
            <PortLabel text={port} />
          </button>
        );
      })}
    </div>
  );
}

// ------------------------------------------------------------ monitor, serial, teclado

/** As teclas vão para o teclado PS/2 da placa enquanto o elemento tem o foco. */
function useKeyCapture(item: LayoutPeripheral | null, live: boolean, onKey?: (code: string) => void) {
  const send = useVboard((s) => s.send);
  return {
    tabIndex: item?.connected ? 0 : undefined,
    onKeyDown: (e: ReactKeyboardEvent) => {
      if (!item?.connected || !live) return;
      e.preventDefault();
      onKey?.(e.code);
      send({ kind: 'key', device: item.id, code: e.code, pressed: true });
    },
    onKeyUp: (e: ReactKeyboardEvent) => {
      if (!item?.connected || !live) return;
      e.preventDefault();
      send({ kind: 'key', device: item.id, code: e.code, pressed: false });
    },
  };
}

function Card({ title, extra, off, children }: { title: string; extra?: ReactNode; off: boolean; children: ReactNode }) {
  const t = useT();
  return (
    <section className={`vboard-card${off ? ' is-off' : ''}`} title={off ? t('vboard.notConnected') : undefined}>
      <h3 className="vboard-card__title">
        {title}
        {extra && <span className="vboard-card__extra">{extra}</span>}
      </h3>
      {children}
    </section>
  );
}

function Monitor({ item, live }: { item: LayoutPeripheral; live: boolean }) {
  const t = useT();
  const frame = useVboard((s) => s.frames[item.id]);
  // Com teclado PS/2 ligado, a tela também o recebe: clicar nela e digitar.
  const keyboard = useVboard(
    (s) => s.layout?.peripherals.find((p) => p.connected && p.peripheral.kind === 'ps2') ?? null,
  );
  const capture = useKeyCapture(keyboard, live);
  return (
    <Card title={t('vboard.kind.vga')} extra={frame ? `${frame.width}x${frame.height}` : undefined} off={!item.connected}>
      <div className="vboard-monitor">
        <div className={`vboard-screen${keyboard ? ' vboard-keys' : ''}`} {...capture}>
          {frame ? (
            <img src={frame.url} width={frame.width} height={frame.height} alt={t('vboard.kind.vga')} draggable={false} />
          ) : (
            <span className="vboard-screen__empty">{live ? t('vboard.noSignal') : t('vboard.screenOff')}</span>
          )}
        </div>
      </div>
    </Card>
  );
}

function KeyboardPad({ item, live }: { item: LayoutPeripheral; live: boolean }) {
  const t = useT();
  const [last, setLast] = useState<string | null>(null);
  const capture = useKeyCapture(item, live, setLast);
  return (
    <Card title={t('vboard.kind.ps2')} off={!item.connected}>
      <div className="vboard-keyboard vboard-keys" {...capture}>
        <Keyboard size={18} />
        <span>{t(live ? 'vboard.keyboardHint' : 'vboard.keyboardOff')}</span>
        {last && live && <kbd className="vboard-keyboard__last">{last}</kbd>}
      </div>
    </Card>
  );
}

function Serial({ item, live }: { item: LayoutPeripheral; live: boolean }) {
  const t = useT();
  const text = useVboard((s) => s.uart[item.id] ?? '');
  const send = useVboard((s) => s.send);
  const clear = useVboard((s) => s.clearUart);
  const [line, setLine] = useState('');
  const log = useRef<HTMLPreElement>(null);
  const canSend = item.peripheral.kind === 'uart' && !!item.peripheral.rx;
  const baud = item.peripheral.kind === 'uart' ? item.peripheral.baud : null;
  useEffect(() => {
    if (log.current) log.current.scrollTop = log.current.scrollHeight;
  }, [text]);
  return (
    <Card title={t('vboard.kind.uart', { name: item.id })} extra={baud ? `${baud} bps` : undefined} off={!item.connected}>
      <pre ref={log} className="vboard-console vboard-console--serial">
        {text || <span className="muted">{t('vboard.uartEmpty')}</span>}
      </pre>
      <div className="vboard-serial__send">
        {canSend && (
          <input
            className="input"
            value={line}
            disabled={!live}
            placeholder={t('vboard.uartPlaceholder')}
            onChange={(e) => setLine(e.target.value)}
            onKeyDown={(e) => {
              if (e.key !== 'Enter') return;
              send({ kind: 'text', device: item.id, text: `${line}\r` });
              setLine('');
            }}
          />
        )}
        <Button small onClick={() => clear(item.id)}>
          {t('vboard.uartClear')}
        </Button>
      </div>
    </Card>
  );
}

// ------------------------------------------------------------ escolher a porta

/** O painel que liga um elemento da placa a uma porta do topo: as portas da
 * direção certa, bit a bit (num bit da placa) ou em faixas da largura do
 * sinal (num display inteiro), e o inverter. */
function PortPicker({ target, onClose }: { target: Target; onClose: () => void }) {
  const t = useT();
  const ports = useVboard((s) => s.ports) ?? [];
  const connect = useVboard((s) => s.connect);
  const panel = useRef<HTMLDivElement>(null);
  const [invert, setInvert] = useState(target.current?.startsWith('!') ?? false);
  const [position, setPosition] = useState({ x: target.anchor.left, y: target.anchor.bottom + 6 });

  useLayoutEffect(() => {
    if (!panel.current) return;
    const rect = panel.current.getBoundingClientRect();
    const x = Math.max(8, Math.min(target.anchor.left, window.innerWidth - rect.width - 8));
    const below = target.anchor.bottom + 6;
    const y = below + rect.height > window.innerHeight - 8 ? Math.max(8, target.anchor.top - rect.height - 6) : below;
    setPosition({ x, y });
  }, [target]);

  useEffect(() => {
    const close = (e: MouseEvent) => {
      if (!panel.current?.contains(e.target as Node)) onClose();
    };
    const escape = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('mousedown', close);
    window.addEventListener('keydown', escape);
    return () => {
      window.removeEventListener('mousedown', close);
      window.removeEventListener('keydown', escape);
    };
  }, [onClose]);

  const choose = (choice: PortChoice | null) => {
    void connect({ signal: target.signal, bit: target.bit, port: choice });
    onClose();
  };
  const candidates = ports.filter((p) => p.direction === target.direction);
  const current = target.current?.replace(/^!/, '') ?? null;
  const where = target.bit === null ? target.signal : `${target.signal}[${target.bit}]`;

  return (
    <div
      ref={panel}
      className="menu menu--floating board-panel vboard-picker"
      style={{ left: position.x, top: position.y }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <div className="vboard-picker__title">
        {t(target.direction === 'input' ? 'vboard.pick.input' : 'vboard.pick.output', { where })}
      </div>
      <div className="board-panel__fixed">
        <button type="button" className={`board-chip${current ? '' : ' is-on'}`} onClick={() => choose(null)}>
          {t('board.signal.none')}
        </button>
      </div>
      {candidates.length === 0 && <p className="muted vboard-picker__empty">{t('vboard.pick.none')}</p>}
      {target.span === 1 ? (
        <>
          {/* Num bit da placa: as portas de 1 bit juntas, e cada barramento
              com os bits dele. */}
          {candidates.some((p) => p.width === 1) && (
            <div className="board-panel__group">
              <div className="board-panel__chips">
                {candidates
                  .filter((p) => p.width === 1)
                  .map((port) => (
                    <button
                      key={port.name}
                      type="button"
                      className={`board-chip mono${current === port.name ? ' is-on' : ''}`}
                      onClick={() => choose({ port: port.name, bit: null, invert })}
                    >
                      {port.name}
                    </button>
                  ))}
              </div>
            </div>
          )}
          {candidates
            .filter((p) => p.width > 1)
            .map((port: Port) => (
              <div key={port.name} className="board-panel__group">
                <span className="board-panel__label mono">
                  {port.name}[{port.width - 1}:0]
                </span>
                <div className="board-panel__chips">
                  {bitsDown(port.width).map((bit) => (
                    <button
                      key={bit}
                      type="button"
                      className={`board-chip mono vboard-picker__bit${current === `${port.name}[${bit}]` ? ' is-on' : ''}`}
                      onClick={() => choose({ port: port.name, bit, invert })}
                    >
                      {bit}
                    </button>
                  ))}
                </div>
              </div>
            ))}
        </>
      ) : (
        // Um sinal inteiro (um display): as portas em faixas da largura dele.
        <div className="board-panel__group">
          <div className="board-panel__chips">
            {candidates.flatMap((port: Port) => {
              const starts: number[] = [];
              for (let s = 0; s < port.width; s += target.span) starts.push(s);
              return starts.map((s) => {
                const msb = Math.min(port.width - 1, s + target.span - 1);
                const label =
                  port.width === 1 || (s === 0 && msb === port.width - 1)
                    ? port.name
                    : msb === s
                      ? `${port.name}[${s}]`
                      : `${port.name}[${msb}:${s}]`;
                return (
                  <button
                    key={`${port.name}:${s}`}
                    type="button"
                    className={`board-chip mono${current === port.name && s === 0 ? ' is-on' : ''}`}
                    onClick={() => choose({ port: port.name, bit: s, invert })}
                  >
                    {label}
                  </button>
                );
              });
            })}
          </div>
        </div>
      )}
      <label className="board-panel__invert">
        <input type="checkbox" checked={invert} onChange={(e) => setInvert(e.target.checked)} />
        {t('board.invert')}
        {target.activeLow && <span className="muted">{t('vboard.pick.activeLow')}</span>}
      </label>
    </div>
  );
}
