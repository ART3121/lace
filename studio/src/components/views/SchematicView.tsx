// O esquemático da última síntese: o SVG que o Yosys (show) e o Graphviz
// (dot) desenham, com zoom e arraste. Trocar o módulo desenha de novo a
// partir do mesmo netlist, sem sintetizar outra vez.
//
// O SVG entra como <img> (data URL), não no DOM: um SVG com script não
// roda dentro do Studio.

import { openPath } from '@tauri-apps/plugin-opener';
import { ExternalLink, Maximize, ZoomIn, ZoomOut } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';

import { runSynthesis } from '../../actions';
import { useT } from '../../i18n';
import { api } from '../../ipc/api';
import { useJobs } from '../../state/jobs';
import { showError } from '../../state/toasts';
import { Button, Checkbox, Empty, IconButton, Spinner } from '../common';

interface ViewBox {
  scale: number;
  x: number;
  y: number;
}

export function SchematicView() {
  const t = useT();
  const synthesis = useJobs((s) => s.synthesis);
  const schematic = useJobs((s) => s.schematic);
  const running = useJobs((s) => s.running);
  const [svg, setSvg] = useState<string | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [view, setView] = useState<ViewBox>({ scale: 1, x: 0, y: 0 });
  const [busWidths, setBusWidths] = useState(true);
  const canvas = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; vx: number; vy: number } | null>(null);

  useEffect(() => {
    let cancelled = false;
    setSvg(null);
    if (!schematic?.svg) return;
    api.fs
      .readText(schematic.svg)
      .then((file) => {
        if (!cancelled) setSvg(`data:image/svg+xml;charset=utf-8,${encodeURIComponent(file.content)}`);
      })
      .catch(showError);
    return () => {
      cancelled = true;
    };
  }, [schematic?.svg, schematic?.duration_ms]);

  const fit = useCallback(() => {
    const box = canvas.current?.getBoundingClientRect();
    if (!box || !size.width || !size.height) return;
    const scale = Math.min((box.width - 32) / size.width, (box.height - 32) / size.height, 4);
    setView({ scale, x: (box.width - size.width * scale) / 2, y: (box.height - size.height * scale) / 2 });
  }, [size]);

  useEffect(() => fit(), [fit]);

  const zoomAt = (factor: number, cx?: number, cy?: number) => {
    const box = canvas.current?.getBoundingClientRect();
    if (!box) return;
    const px = cx ?? box.width / 2;
    const py = cy ?? box.height / 2;
    setView((v) => {
      const scale = Math.min(Math.max(v.scale * factor, 0.05), 20);
      const k = scale / v.scale;
      return { scale, x: px - (px - v.x) * k, y: py - (py - v.y) * k };
    });
  };

  const render = (module: string, bus = busWidths) => {
    if (!synthesis?.netlist) return;
    void useJobs.getState().run({ flow: 'schematic', netlist: synthesis.netlist, module, bus_widths: bus }, 'schematic');
  };

  if (!schematic && !synthesis) {
    return (
      <div className="view-page">
        <Empty>
          <p>{t('schematic.empty')}</p>
          <Button variant="primary" disabled={!!running} onClick={() => void runSynthesis()}>
            {t('action.synthesize')}
          </Button>
        </Empty>
      </div>
    );
  }

  return (
    <div className="schematic">
      <div className="view-toolbar">
        <label className="view-toolbar__field">
          <span>{t('schematic.module')}</span>
          <select
            className="select select--small"
            value={schematic?.module ?? synthesis?.top ?? ''}
            disabled={!synthesis?.netlist || !!running}
            onChange={(e) => render(e.target.value)}
          >
            {(synthesis?.modules.length ? synthesis.modules : [schematic?.module ?? '']).map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        </label>
        <Checkbox
          checked={busWidths}
          disabled={!synthesis?.netlist || !!running}
          onChange={(value) => {
            setBusWidths(value);
            if (schematic) render(schematic.module, value);
          }}
          label={t('schematic.busWidths')}
        />
        <div className="view-toolbar__spacer" />
        {running?.flow === 'schematic' && <Spinner />}
        <IconButton label={t('action.zoomIn')} onClick={() => zoomAt(1.25)}>
          <ZoomIn size={15} />
        </IconButton>
        <IconButton label={t('action.zoomOut')} onClick={() => zoomAt(0.8)}>
          <ZoomOut size={15} />
        </IconButton>
        <IconButton label={t('schematic.fit')} onClick={fit}>
          <Maximize size={15} />
        </IconButton>
        <IconButton
          label={t('schematic.openExternal')}
          disabled={!schematic?.svg}
          onClick={() => schematic?.svg && void openPath(schematic.svg).catch(showError)}
        >
          <ExternalLink size={15} />
        </IconButton>
      </div>
      <div
        ref={canvas}
        className="schematic__canvas"
        onWheel={(e) => {
          const box = canvas.current!.getBoundingClientRect();
          zoomAt(e.deltaY < 0 ? 1.12 : 1 / 1.12, e.clientX - box.left, e.clientY - box.top);
        }}
        onMouseDown={(e) => {
          drag.current = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y };
        }}
        onMouseMove={(e) => {
          if (!drag.current) return;
          const d = drag.current;
          setView((v) => ({ ...v, x: d.vx + e.clientX - d.x, y: d.vy + e.clientY - d.y }));
        }}
        onMouseUp={() => (drag.current = null)}
        onMouseLeave={() => (drag.current = null)}
        onDoubleClick={fit}
      >
        {svg ? (
          <img
            src={svg}
            alt={schematic?.module ?? ''}
            draggable={false}
            onLoad={(e) => setSize({ width: e.currentTarget.naturalWidth, height: e.currentTarget.naturalHeight })}
            style={{
              width: size.width || undefined,
              transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
            }}
          />
        ) : (
          <div className="schematic__placeholder">
            {schematic && !schematic.svg ? schematic.diagnostics[0]?.message ?? t('schematic.empty') : t('schematic.rendering')}
          </div>
        )}
      </div>
      <div className="view-footnote">{t('schematic.note')}</div>
    </div>
  );
}
