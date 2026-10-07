// A área do desenho: arrastar move, a roda aproxima no ponto do mouse, e
// os cliques chegam à vista como seleção (um clique), ativação (dois
// cliques) ou entrada no submódulo (o ícone no canto da caixa). O zoom e a
// posição de cada módulo ficam guardados, para voltar a ele como estava.

import { forwardRef, useCallback, useEffect, useId, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState } from 'react';

import { useT } from '../i18n';
import type { Category } from './cells';
import { bitsText, netName, nodeKind, sourceText } from './describe';
import type { Drawing } from './engine';
import { netClosure, netsOfNode, type SchematicGraph } from './graph';
import { SchematicScene, type Focus } from './Scene';

export type Selection = { kind: 'node' | 'net'; id: string };

export interface CanvasHandle {
  fit: () => void;
  zoom: (factor: number) => void;
  actualSize: () => void;
  centerOn: (nodeId: string) => void;
  focus: () => void;
}

interface View {
  scale: number;
  x: number;
  y: number;
}

interface Props {
  drawing: Drawing;
  /** O módulo desenhado, para guardar o zoom dele. */
  viewKey: string;
  selected: Selection | null;
  matches: Set<string>;
  /** A família que a legenda destaca. */
  legend: Category | null;
  busWidths: boolean;
  onSelect: (selection: Selection | null) => void;
  /** Dois cliques: entrar no submódulo, ou ir ao código. */
  onActivate: (selection: Selection) => void;
  onEnter: (nodeId: string) => void;
  onMenu: (event: { clientX: number; clientY: number; preventDefault: () => void }, target: Selection | null) => void;
  onNavigate: (direction: 'back' | 'forward') => void;
}

const MIN_SCALE = 0.04;
const MAX_SCALE = 6;
const FIT_MAX = 1.25;
const DOUBLE_CLICK_MS = 350;

const views = new Map<string, View>();

function targetOf(element: EventTarget | null): Selection | null {
  if (!(element instanceof Element)) return null;
  // Um rótulo de rede global vale pela rede.
  const flag = element.closest('[data-flag]');
  if (flag) return { kind: 'net', id: flag.getAttribute('data-flag')! };
  const node = element.closest('[data-node]');
  if (node) return { kind: 'node', id: node.getAttribute('data-node')! };
  const net = element.closest('[data-net]');
  if (net) return { kind: 'net', id: net.getAttribute('data-net')! };
  return null;
}

/** O que acende quando uma rede ou um nó está em foco. */
export function focusOf(graph: SchematicGraph, target: Selection): Focus {
  const nets = target.kind === 'net' ? netClosure(graph, target.id) : netsOfNode(graph, target.id);
  const nodes = new Set<string>(target.kind === 'node' ? [target.id] : []);
  for (const net of graph.nets) {
    if (!nets.has(net.id)) continue;
    nodes.add(net.driver.node);
    for (const load of net.loads) nodes.add(load.node);
  }
  return { nets, nodes };
}

function categoryFocus(graph: SchematicGraph, category: Category): Focus {
  return { nets: new Set(), nodes: new Set(graph.nodes.filter((node) => node.category === category).map((node) => node.id)) };
}

export const Canvas = forwardRef<CanvasHandle, Props>(function Canvas(props, ref) {
  const { drawing, viewKey, selected, matches, legend, busWidths } = props;
  const { graph, layout } = drawing;
  const t = useT();
  const gridId = useId();
  const element = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [view, setView] = useState<View>(() => views.get(viewKey) ?? { scale: 1, x: 0, y: 0 });
  const [panning, setPanning] = useState(false);
  const [hover, setHover] = useState<(Selection & { x: number; y: number }) | null>(null);
  const gesture = useRef<{ x: number; y: number; vx: number; vy: number; moved: boolean; target: Selection | null; enter: string | null } | null>(null);
  const lastClick = useRef<{ at: number; key: string } | null>(null);
  const handlers = useRef(props);
  handlers.current = props;

  // Tamanho da área, para ajustar e para o minimapa.
  useLayoutEffect(() => {
    const el = element.current!;
    const observer = new ResizeObserver(() => setSize({ width: el.clientWidth, height: el.clientHeight }));
    observer.observe(el);
    setSize({ width: el.clientWidth, height: el.clientHeight });
    return () => observer.disconnect();
  }, []);

  const fitView = useCallback((): View | null => {
    if (!size.width || !size.height || !layout.width || !layout.height) return null;
    const scale = Math.min((size.width - 40) / layout.width, (size.height - 40) / layout.height, FIT_MAX);
    return {
      scale,
      x: (size.width - layout.width * scale) / 2,
      y: (size.height - layout.height * scale) / 2,
    };
  }, [size, layout]);

  // Um desenho novo abre como estava da última vez, ou ajustado à janela.
  const shown = useRef<{ key: string; layout: unknown } | null>(null);
  useLayoutEffect(() => {
    if (!size.width) return;
    if (shown.current?.key === viewKey && shown.current.layout === layout) return;
    shown.current = { key: viewKey, layout };
    const saved = views.get(viewKey);
    const next = saved ?? fitView();
    if (next) setView(next);
  }, [viewKey, layout, size.width, fitView]);

  // Só guarda depois que o desenho deste módulo apareceu: antes disso a
  // vista ainda é a inicial, e guardá-la impediria o ajuste à janela.
  useEffect(() => {
    if (shown.current?.key === viewKey) views.set(viewKey, view);
  }, [viewKey, view]);

  const zoomAt = useCallback((factor: number, cx?: number, cy?: number) => {
    const el = element.current;
    if (!el) return;
    const px = cx ?? el.clientWidth / 2;
    const py = cy ?? el.clientHeight / 2;
    setView((v) => {
      const scale = Math.min(Math.max(v.scale * factor, MIN_SCALE), MAX_SCALE);
      const k = scale / v.scale;
      return { scale, x: px - (px - v.x) * k, y: py - (py - v.y) * k };
    });
  }, []);

  useImperativeHandle(
    ref,
    () => ({
      fit: () => {
        const next = fitView();
        if (next) setView(next);
      },
      zoom: (factor) => zoomAt(factor),
      actualSize: () => zoomAt(1 / view.scale),
      centerOn: (nodeId) => {
        const at = layout.nodes.get(nodeId);
        const node = graph.nodes.find((n) => n.id === nodeId);
        if (!at || !node) return;
        setView((v) => {
          const scale = Math.max(v.scale, 0.8);
          return {
            scale,
            x: size.width / 2 - (at.x + node.width / 2) * scale,
            y: size.height / 2 - (at.y + node.height / 2) * scale,
          };
        });
      },
      focus: () => element.current?.focus(),
    }),
    [fitView, zoomAt, view.scale, layout, graph, size],
  );

  // A roda precisa de um ouvinte não passivo para impedir a rolagem.
  useEffect(() => {
    const el = element.current!;
    const wheel = (event: WheelEvent) => {
      event.preventDefault();
      if (event.shiftKey) {
        setView((v) => ({ ...v, x: v.x - (event.deltaY || event.deltaX) }));
        return;
      }
      const box = el.getBoundingClientRect();
      const step = event.deltaMode === 1 ? 0.05 : 0.0015;
      zoomAt(Math.exp(-event.deltaY * step), event.clientX - box.left, event.clientY - box.top);
    };
    el.addEventListener('wheel', wheel, { passive: false });
    return () => el.removeEventListener('wheel', wheel);
  }, [zoomAt]);

  // O que acende: a rede sob o mouse, a família da legenda ou a seleção.
  const hoverNet = hover?.kind === 'net' ? hover.id : null;
  const focus = useMemo<Focus | null>(() => {
    if (hoverNet) return focusOf(graph, { kind: 'net', id: hoverNet });
    if (legend) return categoryFocus(graph, legend);
    if (selected) return focusOf(graph, selected);
    return null;
  }, [graph, hoverNet, legend, selected]);

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button === 3 || event.button === 4) {
      event.preventDefault();
      handlers.current.onNavigate(event.button === 3 ? 'back' : 'forward');
      return;
    }
    if (event.button !== 0 && event.button !== 1) return;
    element.current?.focus();
    const target = event.button === 0 ? targetOf(event.target) : null;
    const enter =
      event.button === 0 && event.target instanceof Element && event.target.closest('.sch-enter')
        ? target?.id ?? null
        : null;
    gesture.current = { x: event.clientX, y: event.clientY, vx: view.x, vy: view.y, moved: false, target, enter };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const g = gesture.current;
    if (g) {
      const dx = event.clientX - g.x;
      const dy = event.clientY - g.y;
      if (!g.moved && Math.hypot(dx, dy) < 4) return;
      if (!g.moved) {
        g.moved = true;
        setPanning(true);
        setHover(null);
      }
      setView((v) => ({ ...v, x: g.vx + dx, y: g.vy + dy }));
      return;
    }
    const target = targetOf(event.target);
    const box = element.current!.getBoundingClientRect();
    setHover(target ? { ...target, x: event.clientX - box.left, y: event.clientY - box.top } : null);
  };

  const onPointerUp = (event: React.PointerEvent<HTMLDivElement>) => {
    const g = gesture.current;
    gesture.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    setPanning(false);
    if (!g || g.moved || event.button !== 0) return;
    if (g.enter) {
      handlers.current.onEnter(g.enter);
      return;
    }
    const key = g.target ? `${g.target.kind}:${g.target.id}` : '';
    const now = performance.now();
    const double = lastClick.current && lastClick.current.key === key && now - lastClick.current.at < DOUBLE_CLICK_MS;
    lastClick.current = double ? null : { at: now, key };
    if (double && g.target) handlers.current.onActivate(g.target);
    else if (double && !g.target) {
      const next = fitView();
      if (next) setView(next);
    } else handlers.current.onSelect(g.target);
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey) return;
    const { onNavigate, onSelect, onEnter } = handlers.current;
    let handled = true;
    if (event.altKey && event.key === 'ArrowLeft') onNavigate('back');
    else if (event.altKey && event.key === 'ArrowRight') onNavigate('forward');
    else if (event.altKey) handled = false;
    else if (event.key === '+' || event.key === '=') zoomAt(1.25);
    else if (event.key === '-' || event.key === '_') zoomAt(0.8);
    else if (event.key === '0') zoomAt(1 / view.scale);
    else if (event.key === 'f' || event.key === 'F') {
      const next = fitView();
      if (next) setView(next);
    } else if (event.key === 'Escape') {
      if (selected) onSelect(null);
      else onNavigate('back');
    } else if (event.key === 'Backspace') onNavigate('back');
    else if (event.key === 'Enter' && selected?.kind === 'node') onEnter(selected.id);
    else if (event.key.startsWith('Arrow')) {
      const step = 60;
      const dx = event.key === 'ArrowLeft' ? step : event.key === 'ArrowRight' ? -step : 0;
      const dy = event.key === 'ArrowUp' ? step : event.key === 'ArrowDown' ? -step : 0;
      setView((v) => ({ ...v, x: v.x + dx, y: v.y + dy }));
    } else handled = false;
    if (handled) {
      event.preventDefault();
      event.stopPropagation();
    }
  };

  // A grade acompanha o desenho; de longe demais ela some.
  const step = 16 * view.scale;
  const grid = step >= 7;

  return (
    <div
      ref={element}
      className={`prism-canvas${panning ? ' is-panning' : ''}`}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={() => {
        gesture.current = null;
        setPanning(false);
      }}
      onPointerLeave={() => setHover(null)}
      onKeyDown={onKeyDown}
      onContextMenu={(event) => {
        event.preventDefault();
        handlers.current.onMenu(event, targetOf(event.target));
      }}
    >
      <svg className="prism-canvas__svg">
        {grid && (
          <defs>
            <pattern
              id={gridId}
              width={step}
              height={step}
              x={((view.x % step) + step) % step}
              y={((view.y % step) + step) % step}
              patternUnits="userSpaceOnUse"
            >
              <circle className="prism-canvas__dot" cx={step / 2} cy={step / 2} r={view.scale >= 1 ? 1.1 : 0.9} />
            </pattern>
          </defs>
        )}
        {grid && <rect width="100%" height="100%" fill={`url(#${CSS.escape(gridId)})`} />}
        <g transform={`translate(${view.x},${view.y}) scale(${view.scale})`}>
          <SchematicScene
            graph={graph}
            layout={layout}
            focus={focus}
            selected={selected?.kind === 'node' ? selected.id : null}
            matches={matches}
            busWidths={busWidths}
          />
        </g>
      </svg>
      {hover && !panning && <Tooltip graph={graph} target={hover} />}
      <Minimap drawing={drawing} view={view} size={size} onMove={(x, y) => setView((v) => ({ ...v, x, y }))} />
      <div className="prism-canvas__zoom" title={t('schematic.actualSize')}>
        {Math.round(view.scale * 100)}%
      </div>
    </div>
  );
});

function Tooltip({ graph, target }: { graph: SchematicGraph; target: Selection & { x: number; y: number } }) {
  const t = useT();
  const style = { left: target.x + 14, top: target.y + 16 };
  if (target.kind === 'net') {
    const net = graph.nets.find((n) => n.id === target.id);
    if (!net) return null;
    return (
      <div className="prism-tip" style={style}>
        <div className="prism-tip__title">{netName(net)}</div>
        <div className="prism-tip__line">
          {bitsText(net.bits)} · {t('schematic.loadsCount', { count: net.loads.length })}
        </div>
      </div>
    );
  }
  const node = graph.nodes.find((n) => n.id === target.id);
  if (!node || node.kind === 'split' || node.kind === 'join') return null;
  const width = node.ports.find((port) => port.output)?.bits ?? node.ports[0]?.bits;
  const hint = node.module ? t('schematic.hint.enter') : node.src.length ? t('schematic.hint.source') : null;
  return (
    <div className="prism-tip" style={style}>
      <div className="prism-tip__title">
        <span className="prism-swatch" data-cat={node.category} />
        {node.name ?? node.title}
      </div>
      <div className="prism-tip__line">
        {nodeKind(node)}
        {width ? ` · ${bitsText(width)}` : ''}
      </div>
      {node.src[0] && <div className="prism-tip__line prism-tip__muted">{sourceText(node.src[0])}</div>}
      {hint && <div className="prism-tip__line prism-tip__muted">{hint}</div>}
    </div>
  );
}

const MINIMAP_WIDTH = 168;
const MINIMAP_HEIGHT = 112;

/** O desenho inteiro em miniatura, com o retângulo da parte visível. Clicar
 * ou arrastar leva a vista para lá. Some quando tudo já cabe na tela. */
function Minimap({
  drawing,
  view,
  size,
  onMove,
}: {
  drawing: Drawing;
  view: View;
  size: { width: number; height: number };
  onMove: (x: number, y: number) => void;
}) {
  const { graph, layout } = drawing;
  const dragging = useRef(false);
  const scale = Math.min(MINIMAP_WIDTH / layout.width, MINIMAP_HEIGHT / layout.height);
  const width = layout.width * scale;
  const height = layout.height * scale;
  const visible = {
    x: -view.x / view.scale,
    y: -view.y / view.scale,
    width: size.width / view.scale,
    height: size.height / view.scale,
  };
  const fits =
    visible.x <= 0 && visible.y <= 0 && visible.x + visible.width >= layout.width && visible.y + visible.height >= layout.height;
  if (fits || !layout.width || !layout.height) return null;

  const moveTo = (event: React.PointerEvent<SVGSVGElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    const cx = (event.clientX - box.left) / scale;
    const cy = (event.clientY - box.top) / scale;
    onMove(size.width / 2 - cx * view.scale, size.height / 2 - cy * view.scale);
  };

  return (
    <svg
      className="prism-minimap"
      width={width}
      height={height}
      onPointerDown={(event) => {
        event.stopPropagation();
        dragging.current = true;
        event.currentTarget.setPointerCapture(event.pointerId);
        moveTo(event);
      }}
      onPointerMove={(event) => {
        event.stopPropagation();
        if (dragging.current) moveTo(event);
      }}
      onPointerUp={(event) => {
        event.stopPropagation();
        dragging.current = false;
      }}
    >
      {graph.nodes.map((node) => {
        const at = layout.nodes.get(node.id);
        if (!at || node.kind === 'split' || node.kind === 'join') return null;
        return (
          <rect
            key={node.id}
            className="prism-minimap__node"
            data-cat={node.category}
            x={at.x * scale}
            y={at.y * scale}
            width={Math.max(1.5, node.width * scale)}
            height={Math.max(1.5, node.height * scale)}
          />
        );
      })}
      <rect
        className="prism-minimap__view"
        x={visible.x * scale}
        y={visible.y * scale}
        width={visible.width * scale}
        height={visible.height * scale}
      />
    </svg>
  );
}
