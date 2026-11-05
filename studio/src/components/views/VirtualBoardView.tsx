// A placa virtual: os periféricos da placa desenhados a partir do JSON dela
// (BoardLayout), com o estado que o `vboard.run` manda. Chaves e botões se
// clicam; com o teclado em foco, as teclas vão para o teclado PS/2; a UART
// tem um terminal. A sessão vive em state/vboard.ts.

import { Keyboard, MonitorPlay, Pause, Play, Power } from 'lucide-react';
import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';

import { errorText, useT } from '../../i18n';
import type { LayoutPeripheral, Peripheral } from '../../ipc/aurora-types';
import { api } from '../../ipc/api';
import { useEditor } from '../../state/editor';
import { useProject } from '../../state/project';
import { useVboard } from '../../state/vboard';
import { Button, Checkbox } from '../common';

/** A largura de um sinal do periférico. */
function width(item: LayoutPeripheral, name: string): number {
  return item.signals.find((s) => s.name === name)?.width ?? 0;
}

/** Os bits de `count`, do mais alto ao mais baixo, como na placa. */
function bitsDown(count: number): number[] {
  return Array.from({ length: count }, (_, i) => count - 1 - i);
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
  const [boards, setBoards] = useState<{ id: string; name: string }[]>([]);

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
  const { start, stop, togglePause, setRealtime, chooseBoard } = useVboard.getState();

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

  return (
    <div className="view-page vboard-page">
      <div className="view-page__title">
        <h1>
          <MonitorPlay size={18} /> {t('vboard.title')}
        </h1>
        <div className="button-row">
          {running && (
            <Button icon={paused ? <Play size={14} /> : <Pause size={14} />} onClick={togglePause}>
              {paused ? t('vboard.resume') : t('vboard.pause')}
            </Button>
          )}
          {status === 'idle' ? (
            <Button
              variant="primary"
              icon={<Power size={14} />}
              disabled={!layout || !spf}
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

      <div className="vboard-head">
        {layout && (
          <span>
            {t('vboard.on', { board: layout.board_name, top: layout.top })}
            {layout.clocks.length > 0 && (
              <span className="muted"> · {layout.clocks.map((c) => `${c.signal} ${c.mhz} MHz`).join(', ')}</span>
            )}
          </span>
        )}
        <span className={`vboard-head__status${running && !paused ? ' is-live' : ''}`}>{statusText}</span>
        <Checkbox checked={realtime} onChange={setRealtime} label={t('vboard.realtime')} disabled={busy} />
      </div>
      {status === 'building' && buildLine && <div className="vboard-buildline">{buildLine}</div>}

      {layoutError && (
        <div className="notice notice--warn">
          <p>{layoutError.code === 'no_fpga_config' ? t('vboard.noFpgaConfig') : errorText(layoutError)}</p>
          {layoutError.code === 'no_fpga_config' && (
            <>
              <p>{t('vboard.chooseBoard')}</p>
              <div className="button-row">
                <select
                  className="select"
                  value={board ?? ''}
                  onChange={(e) => void chooseBoard(e.target.value || null)}
                >
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
      {board && layout && !layoutError && (
        <p className="muted vboard-byname">{t('vboard.byName', { board: layout.board_name })}</p>
      )}

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

      {layout && (
        <div className="vboard">
          {layout.peripherals.map((item) => (
            <PeripheralView key={item.id} item={item} live={running} />
          ))}
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
        <section>
          <h2>{t('vboard.output')}</h2>
          <pre className="vboard-output">{output.join('\n')}</pre>
        </section>
      )}
    </div>
  );
}

function PeripheralView({ item, live }: { item: LayoutPeripheral; live: boolean }) {
  const t = useT();
  const p: Peripheral = item.peripheral;
  const title = item.connected ? undefined : t('vboard.notConnected');
  const box = (label: string, body: React.ReactNode, wide = false) => (
    <section
      className={`vboard-part vboard-part--${p.kind}${wide ? ' vboard-part--wide' : ''}${item.connected ? '' : ' is-off'}`}
      title={title}
    >
      <h3>{label}</h3>
      {body}
    </section>
  );
  switch (p.kind) {
    case 'switches':
      return box(t('vboard.kind.switches', { name: p.signal }), <Switches item={item} signal={p.signal} />);
    case 'buttons':
      return box(t('vboard.kind.buttons', { name: p.signal }), <Buttons item={item} signal={p.signal} />);
    case 'leds':
      return box(t('vboard.kind.leds', { name: p.signal }), <Leds item={item} signal={p.signal} color={p.color} />);
    case 'seven_segment':
      return box(t('vboard.kind.seven_segment'), <Digits digits={p.digits} />);
    case 'vga':
      return box(t('vboard.kind.vga'), <Screen item={item} live={live} />, true);
    case 'ps2':
      return box(t('vboard.kind.ps2'), <KeyboardPad item={item} live={live} />);
    case 'uart':
      return box(t('vboard.kind.uart', { name: item.id }), <Uart item={item} live={live} />, true);
    default:
      return null;
  }
}

function Switches({ item, signal }: { item: LayoutPeripheral; signal: string }) {
  const value = useVboard((s) => s.inputs[item.id] ?? 0);
  const setBit = useVboard((s) => s.setBit);
  return (
    <div className="vboard-row">
      {bitsDown(width(item, signal)).map((bit) => {
        const on = ((value >>> bit) & 1) === 1;
        return (
          <button
            key={bit}
            type="button"
            className={`vboard-switch${on ? ' is-on' : ''}`}
            disabled={!item.connected}
            aria-pressed={on}
            title={`${signal}[${bit}]`}
            onClick={() => setBit(item.id, bit, !on)}
          >
            <span className="vboard-switch__knob" />
            <span className="vboard-bit">{bit}</span>
          </button>
        );
      })}
    </div>
  );
}

function Buttons({ item, signal }: { item: LayoutPeripheral; signal: string }) {
  const t = useT();
  const value = useVboard((s) => s.inputs[item.id] ?? 0);
  const setBit = useVboard((s) => s.setBit);
  // Os botões presos com Shift+clique: ficam apertados até outro Shift+clique,
  // para segurar um enquanto se aperta outro (zerar na aula 9: KEY[1]
  // segurado e um aperto do KEY[0]).
  const [latched, setLatched] = useState(0);
  return (
    <div className="vboard-row">
      {bitsDown(width(item, signal)).map((bit) => {
        const down = ((value >>> bit) & 1) === 1;
        const isLatched = ((latched >>> bit) & 1) === 1;
        const release = () => {
          if (isLatched) return;
          if (((useVboard.getState().inputs[item.id] ?? 0) >>> bit) & 1) setBit(item.id, bit, false);
        };
        return (
          <button
            key={bit}
            type="button"
            className={`vboard-button${down ? ' is-down' : ''}${isLatched ? ' is-latched' : ''}`}
            disabled={!item.connected}
            title={t('vboard.buttonHint', { name: `${signal}[${bit}]` })}
            onPointerDown={(e) => {
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
          >
            <span className="vboard-bit">{bit}</span>
          </button>
        );
      })}
    </div>
  );
}

function Leds({ item, signal, color }: { item: LayoutPeripheral; signal: string; color: string }) {
  const levels = useVboard((s) => s.lights[signal]);
  return (
    <div className="vboard-row">
      {bitsDown(width(item, signal)).map((bit) => (
        <span key={bit} className="vboard-led-cell" title={`${signal}[${bit}]`}>
          <span
            className={`vboard-led vboard-led--${color}`}
            style={{ opacity: 0.12 + 0.88 * ((levels?.[bit] ?? 0) / 255) }}
          />
          <span className="vboard-bit">{bit}</span>
        </span>
      ))}
    </div>
  );
}

/** Os segmentos a até g, e o ponto, num dígito de 40 x 64. */
const SEGMENTS = [
  'M8 4 L32 4 L28 9 L12 9 Z',
  'M33 5 L33 30 L29 27 L29 10 Z',
  'M33 34 L33 59 L29 54 L29 37 Z',
  'M8 60 L32 60 L28 55 L12 55 Z',
  'M7 34 L7 59 L11 54 L11 37 Z',
  'M7 5 L7 30 L11 27 L11 10 Z',
  'M9 32 L13 29.5 L27 29.5 L31 32 L27 34.5 L13 34.5 Z',
];

function Digits({ digits }: { digits: string[] }) {
  const lights = useVboard((s) => s.lights);
  return (
    <div className="vboard-row vboard-digits">
      {digits.map((digit) => {
        const levels = lights[digit];
        return (
          <svg key={digit} className="vboard-digit" viewBox="0 0 40 64" role="img" aria-label={digit}>
            <title>{digit}</title>
            {SEGMENTS.map((d, i) => (
              <path key={i} d={d} className="vboard-segment" style={{ opacity: 0.1 + 0.9 * ((levels?.[i] ?? 0) / 255) }} />
            ))}
            <circle
              cx="36"
              cy="60"
              r="2.4"
              className="vboard-segment"
              style={{ opacity: 0.1 + 0.9 * ((levels?.[7] ?? 0) / 255) }}
            />
          </svg>
        );
      })}
    </div>
  );
}

/** As teclas vão para o teclado PS/2 da placa enquanto o elemento tem o foco. */
function useKeyCapture(item: LayoutPeripheral | null, live: boolean) {
  const send = useVboard((s) => s.send);
  return {
    tabIndex: item?.connected ? 0 : undefined,
    onKeyDown: (e: ReactKeyboardEvent) => {
      if (!item?.connected || !live) return;
      e.preventDefault();
      send({ kind: 'key', device: item.id, code: e.code, pressed: true });
    },
    onKeyUp: (e: ReactKeyboardEvent) => {
      if (!item?.connected || !live) return;
      e.preventDefault();
      send({ kind: 'key', device: item.id, code: e.code, pressed: false });
    },
  };
}

function Screen({ item, live }: { item: LayoutPeripheral; live: boolean }) {
  const t = useT();
  const frame = useVboard((s) => s.frames[item.id]);
  // Com teclado PS/2 ligado, a tela também o recebe: clicar nela e digitar.
  const keyboard = useVboard(
    (s) => s.layout?.peripherals.find((p) => p.connected && p.peripheral.kind === 'ps2') ?? null,
  );
  const capture = useKeyCapture(keyboard, live);
  return (
    <div className={`vboard-screen${keyboard ? ' vboard-keys' : ''}`} {...capture}>
      {frame ? (
        <img src={frame.url} width={frame.width} height={frame.height} alt={t('vboard.kind.vga')} draggable={false} />
      ) : (
        <span className="vboard-screen__empty">{live ? t('vboard.noSignal') : t('vboard.screenOff')}</span>
      )}
    </div>
  );
}

function KeyboardPad({ item, live }: { item: LayoutPeripheral; live: boolean }) {
  const t = useT();
  const capture = useKeyCapture(item, live);
  return (
    <div className="vboard-keyboard vboard-keys" {...capture}>
      <Keyboard size={18} />
      <span>{t(live ? 'vboard.keyboardHint' : 'vboard.keyboardOff')}</span>
    </div>
  );
}

function Uart({ item, live }: { item: LayoutPeripheral; live: boolean }) {
  const t = useT();
  const text = useVboard((s) => s.uart[item.id] ?? '');
  const send = useVboard((s) => s.send);
  const clear = useVboard((s) => s.clearUart);
  const [line, setLine] = useState('');
  const log = useRef<HTMLPreElement>(null);
  const canSend = item.peripheral.kind === 'uart' && !!item.peripheral.rx;
  useEffect(() => {
    if (log.current) log.current.scrollTop = log.current.scrollHeight;
  }, [text]);
  return (
    <div className="vboard-uart">
      <pre ref={log} className="vboard-uart__log">
        {text || <span className="muted">{t('vboard.uartEmpty')}</span>}
      </pre>
      <div className="button-row">
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
    </div>
  );
}
