// O RTL viewer: o esquemático do netlist da síntese, desenhado por este
// pacote. O uso segue o RTLViewer da AURORA: entrar num submódulo, voltar com
// Esc ou com o botão lateral do mouse, ir ao código com dois cliques,
// destacar uma ligação, exportar o SVG. O desenho é outro: as cores saem do
// tema (as variáveis --sch-* e --text-*), a hierarquia fica numa árvore ao
// lado, e o destaque segue a rede do netlist, não a geometria dos fios.
//
// Não conhece quem o abriga. O Studio, a extensão do VS Code e o HTML do
// `lace synth --open` passam o estado (createRtlViewerStore) e o que só eles
// sabem fazer: abrir o fonte, gravar o SVG, sintetizar de novo.

import {
  ArrowLeft,
  ArrowRight,
  ChevronRight,
  Download,
  Flag,
  Hash,
  ListTree,
  Maximize,
  PanelRight,
  RefreshCw,
  Search,
  Tags,
  ZoomIn,
  ZoomOut,
} from 'lucide-react';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';

import { Canvas, type CanvasHandle, type Selection } from './Canvas';
import { LEGEND, type Category } from './cells';
import { nodeKind, nodeName } from './describe';
import { cancelLayouts, drawModule, type Drawing } from './engine';
import { exportSvg } from './export';
import { moduleAt, moduleLabel } from './hierarchy';
import { setLang, useT, type Lang } from './i18n';
import { HierarchyTree, Inspector } from './panels';
import type { RtlViewerStore } from './store';
import { ContextMenu, DEFAULT_UI, type MenuEvent, type RtlViewerUi, type ViewerMenuItem } from './ui';
import type { SourceRef } from './yosys';

/** Um módulo com mais células que isso pede confirmação antes do layout. */
const LARGE = 1500;

type Status =
  | { state: 'idle' }
  | { state: 'large'; cells: number }
  | { state: 'drawing' }
  /** `module`: o módulo do desenho, que pode ainda não ser o pedido. */
  | { state: 'ready'; drawing: Drawing; module: string }
  | { state: 'failed'; message: string };

export interface RtlViewerProps {
  store: RtlViewerStore;
  lang?: Lang;
  /** O que mostrar sem netlist; sem isto, uma linha que diz se está lendo ou o que falhou. */
  empty?: ReactNode;
  /** Abre o fonte de uma célula ou rede; sem isto, "Ir ao código" não aparece. */
  onOpenSource?: (src: SourceRef) => void;
  /** Grava o SVG exportado; sem isto, o navegador baixa o arquivo. */
  onExport?: (fileName: string, svg: string) => void | Promise<void>;
  /** Sintetiza de novo; sem isto, o botão não aparece. */
  onResynthesize?: () => void;
  /** Uma síntese roda agora: o botão gira. */
  resynthesizing?: boolean;
  /** O botão de sintetizar fica desligado (outra operação roda). */
  resynthesizeDisabled?: boolean;
  ui?: Partial<RtlViewerUi>;
}

export function RtlViewer(props: RtlViewerProps) {
  const { store, onOpenSource, onExport, onResynthesize } = props;
  const ui = useMemo(() => ({ ...DEFAULT_UI, ...props.ui }), [props.ui]);
  const { Button, IconButton, Spinner } = ui;

  useLayoutEffect(() => setLang(props.lang ?? 'pt'), [props.lang]);
  const t = useT();
  const source = store((s) => s.source);
  const loading = store((s) => s.loading);
  const loadError = store((s) => s.error);
  const path = store((s) => s.path);
  const back = store((s) => s.back);
  const forward = store((s) => s.forward);
  const netNames = store((s) => s.netNames);
  const flags = store((s) => s.flags);
  const busWidths = store((s) => s.busWidths);
  const showTree = store((s) => s.showTree);
  const showInspector = store((s) => s.showInspector);
  const { go, enter, goBack, goForward, setOption } = store.getState();

  const canvas = useRef<CanvasHandle>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const [status, setStatus] = useState<Status>({ state: 'idle' });
  const [confirmed, setConfirmed] = useState<string | null>(null);
  const [selected, setSelected] = useState<Selection | null>(null);
  const [legend, setLegend] = useState<Category | null>(null);
  const [query, setQuery] = useState('');
  /** O resultado da busca mostrado por último (-1: nenhum). */
  const [matchIndex, setMatchIndex] = useState(-1);
  const [retry, setRetry] = useState(0);
  const [menuAt, setMenuAt] = useState<{ x: number; y: number; items: ViewerMenuItem[] } | null>(null);

  const module = source ? moduleAt(source.netlist, source.top, path) : null;
  const drawKey = source && module ? `${source.path}|${source.modifiedMs}|${module}|${netNames}|${flags}` : null;

  useEffect(() => {
    setSelected(null);
    setLegend(null);
    setMatchIndex(-1);
  }, [module]);

  useEffect(() => {
    if (!source || !module || !drawKey) {
      setStatus({ state: 'idle' });
      return;
    }
    const cells = Object.keys(source.netlist.modules[module]?.cells ?? {}).length;
    if (cells > LARGE && confirmed !== drawKey) {
      setStatus({ state: 'large', cells });
      return;
    }
    let cancelled = false;
    setStatus((current) => (current.state === 'ready' ? current : { state: 'drawing' }));
    // Só mostra "desenhando" se demorar: o que já está no cache volta logo.
    const slow = window.setTimeout(() => !cancelled && setStatus({ state: 'drawing' }), 120);
    drawModule(source, module, { netNames, flags }).then(
      (drawing) => {
        window.clearTimeout(slow);
        if (!cancelled) setStatus({ state: 'ready', drawing, module });
      },
      (error: unknown) => {
        window.clearTimeout(slow);
        if (!cancelled) setStatus({ state: 'failed', message: error instanceof Error ? error.message : String(error) });
      },
    );
    return () => {
      cancelled = true;
      window.clearTimeout(slow);
    };
  }, [source, module, netNames, flags, drawKey, confirmed, retry]);

  // Enquanto o módulo novo é desenhado, o anterior fica na tela, mas a busca,
  // os detalhes e a exportação já são do novo.
  const drawing = status.state === 'ready' && status.module === module ? status.drawing : null;
  const graph = drawing?.graph ?? null;

  // Busca: nós pelo nome, tipo ou valor; redes pelo nome (vão para a origem).
  const results = useMemo<Selection[]>(() => {
    const text = query.trim().toLowerCase();
    if (!graph || !text) return [];
    const found: Selection[] = [];
    for (const node of graph.nodes) {
      if (node.kind === 'split' || node.kind === 'join') continue;
      const haystack = [node.name, node.title, node.subtitle, node.type, nodeKind(node)].filter(Boolean).join(' ').toLowerCase();
      if (haystack.includes(text)) found.push({ kind: 'node', id: node.id });
    }
    for (const net of graph.nets) {
      if ((net.name ?? '').toLowerCase().includes(text)) found.push({ kind: 'net', id: net.id });
    }
    return found;
  }, [graph, query]);
  const matches = useMemo(() => {
    const set = new Set<string>();
    for (const result of results) {
      if (result.kind === 'node') set.add(result.id);
      else {
        const net = graph?.nets.find((n) => n.id === result.id);
        if (net) set.add(net.driver.node);
      }
    }
    return set;
  }, [results, graph]);

  const showResult = useCallback(
    (index: number) => {
      const result = results[index];
      if (!result || !graph) return;
      setMatchIndex(index);
      setSelected(result);
      const nodeId = result.kind === 'node' ? result.id : graph.nets.find((n) => n.id === result.id)?.driver.node;
      if (nodeId) canvas.current?.centerOn(nodeId);
    },
    [results, graph],
  );

  const openSource = useCallback((src: SourceRef) => onOpenSource?.(src), [onOpenSource]);

  const enterNode = useCallback(
    (nodeId: string) => {
      const node = graph?.nodes.find((n) => n.id === nodeId);
      if (node?.module && node.key) enter(node.key);
    },
    [graph, enter],
  );

  const activate = useCallback(
    (target: Selection) => {
      if (!graph) return;
      if (target.kind === 'node') {
        const node = graph.nodes.find((n) => n.id === target.id);
        if (node?.module) enterNode(node.id);
        else if (node?.src[0]) openSource(node.src[0]);
        return;
      }
      const net = graph.nets.find((n) => n.id === target.id);
      if (net?.src[0]) openSource(net.src[0]);
    },
    [graph, enterNode, openSource],
  );

  const exportFile = useCallback(async () => {
    if (!drawing || !source || !module) return;
    const name = moduleLabel(source.netlist, module);
    const svg = exportSvg(drawing, busWidths, name);
    if (onExport) {
      await onExport(`${name}.svg`, svg);
      return;
    }
    const link = document.createElement('a');
    link.href = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml' }));
    link.download = `${name}.svg`;
    link.click();
    URL.revokeObjectURL(link.href);
  }, [drawing, source, module, busWidths, onExport]);

  const resynthesize = onResynthesize && !props.resynthesizeDisabled ? onResynthesize : undefined;

  const openMenu = useCallback(
    (event: MenuEvent, items: ViewerMenuItem[]) => {
      if (ui.openContextMenu) {
        ui.openContextMenu(event, items);
        return;
      }
      event.preventDefault();
      setMenuAt({ x: event.clientX, y: event.clientY, items });
    },
    [ui],
  );

  const menu = useCallback(
    (event: MenuEvent, target: Selection | null) => {
      const node = target?.kind === 'node' ? graph?.nodes.find((n) => n.id === target.id) : null;
      const net = target?.kind === 'net' ? graph?.nets.find((n) => n.id === target.id) : null;
      const items: ViewerMenuItem[] = [];
      if (node) {
        if (node.module) items.push({ label: t('schematic.openModule'), keys: 'Enter', run: () => enterNode(node.id) });
        if (node.src[0] && onOpenSource) items.push({ label: t('schematic.goToSource'), run: () => openSource(node.src[0]!) });
        items.push({ label: t('schematic.highlight'), run: () => setSelected({ kind: 'node', id: node.id }) });
        items.push({ label: t('schematic.copyName'), run: () => void navigator.clipboard.writeText(nodeName(node)) });
        items.push({ separator: true });
      } else if (net) {
        items.push({ label: t('schematic.highlight'), run: () => setSelected({ kind: 'net', id: net.id }) });
        if (net.src[0] && onOpenSource) items.push({ label: t('schematic.goToSource'), run: () => openSource(net.src[0]!) });
        if (net.name) items.push({ label: t('schematic.copyName'), run: () => void navigator.clipboard.writeText(net.name!) });
        items.push({ separator: true });
      }
      items.push(
        { label: t('schematic.fit'), keys: 'F', run: () => canvas.current?.fit() },
        { label: t('schematic.actualSize'), keys: '0', run: () => canvas.current?.actualSize() },
        { separator: true },
        { label: t('schematic.export'), disabled: !drawing, run: () => void exportFile() },
      );
      if (onResynthesize) {
        items.push({ label: t('schematic.resynthesize'), disabled: !resynthesize, run: () => resynthesize?.() });
      }
      openMenu(event, items);
    },
    [graph, t, enterNode, openSource, onOpenSource, drawing, exportFile, onResynthesize, resynthesize, openMenu],
  );

  const navigate = useCallback((direction: 'back' | 'forward') => (direction === 'back' ? goBack() : goForward()), [goBack, goForward]);

  // Ctrl+F na vista vai para a busca do viewer, não para a de quem o abriga.
  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f') {
      event.preventDefault();
      event.stopPropagation();
      searchInput.current?.focus();
      searchInput.current?.select();
    }
  };

  if (!source) {
    if (props.empty !== undefined) return <>{props.empty}</>;
    return (
      <div className="rtlViewer rtlViewer--empty">
        <div className="rtlViewer__placeholder">
          {loading ? (
            <>
              <Spinner /> {t('schematic.loading')}
            </>
          ) : (
            <span>{loadError ? t('schematic.loadFailed', { message: loadError }) : t('schematic.noNetlist')}</span>
          )}
        </div>
      </div>
    );
  }

  const crumbs = [{ key: [] as string[], name: moduleLabel(source.netlist, source.top), detail: null as string | null }];
  {
    let current = source.top;
    path.forEach((key, i) => {
      const type = source.netlist.modules[current]?.cells?.[key]?.type ?? current;
      crumbs.push({ key: path.slice(0, i + 1), name: key, detail: moduleLabel(source.netlist, type) });
      current = type;
    });
  }

  return (
    <div className="rtlViewer" onKeyDown={onKeyDown}>
      <div className="rtlViewer__toolbar">
        <IconButton label={t('schematic.back')} disabled={!back.length && !path.length} onClick={goBack}>
          <ArrowLeft size={15} />
        </IconButton>
        <IconButton label={t('schematic.forward')} disabled={!forward.length} onClick={goForward}>
          <ArrowRight size={15} />
        </IconButton>
        <nav className="rtlViewer-crumbs" aria-label={t('schematic.tree')}>
          {crumbs.map((crumb, i) => (
            <span key={crumb.key.join('\u0000') || '/'} className="rtlViewer-crumbs__item">
              {i > 0 && <ChevronRight size={12} className="rtlViewer-crumbs__sep" />}
              <button
                type="button"
                className={`rtlViewer-crumbs__button${i === crumbs.length - 1 ? ' is-current' : ''}`}
                onClick={() => go(crumb.key)}
                title={crumb.detail ? `${crumb.name} (${crumb.detail})` : crumb.name}
              >
                {crumb.name}
                {crumb.detail && <span className="rtlViewer-crumbs__detail">{crumb.detail}</span>}
              </button>
            </span>
          ))}
        </nav>
        <div className="rtlViewer__spacer" />
        <label className="rtlViewer-search">
          <Search size={13} />
          <input
            ref={searchInput}
            value={query}
            placeholder={t('schematic.search')}
            spellCheck={false}
            onChange={(event) => {
              setQuery(event.target.value);
              setMatchIndex(-1);
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && results.length) {
                const step = event.shiftKey ? -1 : 1;
                showResult((Math.max(matchIndex, event.shiftKey ? 0 : -1) + step + results.length) % results.length);
              } else if (event.key === 'Escape') {
                setQuery('');
                canvas.current?.focus();
              }
            }}
          />
          {query && (
            <span className="rtlViewer-search__count">
              {!results.length
                ? t('schematic.noMatch')
                : matchIndex >= 0
                  ? t('schematic.searchPosition', { index: matchIndex + 1, count: results.length })
                  : t('schematic.searchCount', { count: results.length })}
            </span>
          )}
        </label>
        <div className="rtlViewer__group">
          <IconButton label={t('schematic.netNames')} active={netNames} onClick={() => setOption('netNames', !netNames)}>
            <Tags size={15} />
          </IconButton>
          <IconButton label={t('schematic.flags')} active={flags} onClick={() => setOption('flags', !flags)}>
            <Flag size={15} />
          </IconButton>
          <IconButton label={t('schematic.busWidths')} active={busWidths} onClick={() => setOption('busWidths', !busWidths)}>
            <Hash size={15} />
          </IconButton>
        </div>
        <div className="rtlViewer__group">
          <IconButton label={t('action.zoomOut')} disabled={!drawing} onClick={() => canvas.current?.zoom(0.8)}>
            <ZoomOut size={15} />
          </IconButton>
          <IconButton label={t('action.zoomIn')} disabled={!drawing} onClick={() => canvas.current?.zoom(1.25)}>
            <ZoomIn size={15} />
          </IconButton>
          <IconButton label={t('schematic.fit')} disabled={!drawing} onClick={() => canvas.current?.fit()}>
            <Maximize size={15} />
          </IconButton>
        </div>
        <div className="rtlViewer__group">
          <IconButton label={t('schematic.tree')} active={showTree} onClick={() => setOption('showTree', !showTree)}>
            <ListTree size={15} />
          </IconButton>
          <IconButton label={t('schematic.inspector')} active={showInspector} onClick={() => setOption('showInspector', !showInspector)}>
            <PanelRight size={15} />
          </IconButton>
          <IconButton label={t('schematic.export')} disabled={!drawing} onClick={() => void exportFile()}>
            <Download size={15} />
          </IconButton>
          {onResynthesize && (
            <IconButton label={t('schematic.resynthesize')} disabled={!resynthesize} onClick={() => resynthesize?.()}>
              {props.resynthesizing ? <Spinner /> : <RefreshCw size={15} />}
            </IconButton>
          )}
        </div>
      </div>

      <div className="rtlViewer__body">
        {showTree && <HierarchyTree source={source} path={path} onGo={go} />}
        <div className="rtlViewer__stage">
          {status.state === 'ready' && (
            <Canvas
              ref={canvas}
              drawing={status.drawing}
              viewKey={`${source.path}|${status.module}`}
              selected={selected}
              matches={matches}
              legend={legend}
              busWidths={busWidths}
              onSelect={setSelected}
              onActivate={activate}
              onEnter={enterNode}
              onMenu={menu}
              onNavigate={navigate}
            />
          )}
          {status.state !== 'ready' && (
            <div className="rtlViewer__placeholder">
              {status.state === 'drawing' && (
                <>
                  <Spinner /> {t('schematic.rendering', { module: module ? moduleLabel(source.netlist, module) : '' })}
                  <Button small onClick={() => cancelLayouts()}>
                    {t('schematic.cancel')}
                  </Button>
                </>
              )}
              {status.state === 'large' && (
                <>
                  <span>{t('schematic.large', { cells: status.cells })}</span>
                  <Button small onClick={() => setConfirmed(drawKey)}>
                    {t('schematic.drawAnyway')}
                  </Button>
                </>
              )}
              {status.state === 'failed' && (
                <>
                  <span>{status.message === 'cancelled' ? t('schematic.cancelled') : t('schematic.layoutFailed', { message: status.message })}</span>
                  <Button small onClick={() => setRetry((n) => n + 1)}>
                    {t('schematic.retry')}
                  </Button>
                </>
              )}
              {status.state === 'idle' && module === null && <span>{t('schematic.pathGone')}</span>}
            </div>
          )}
        </div>
        {showInspector && module && (
          <Inspector
            source={source}
            module={module}
            drawing={drawing}
            selected={selected}
            legend={legend}
            onSelect={setSelected}
            onCenter={(id) => canvas.current?.centerOn(id)}
            onEnter={enterNode}
            onOpenSource={onOpenSource ? openSource : undefined}
            onLegend={setLegend}
            Button={Button}
          />
        )}
      </div>

      <div className="rtlViewer__footer">
        <div className="rtlViewer-legend">
          {LEGEND.map((category) => (
            <span
              key={category}
              className={`rtlViewer-legend__item${legend === category ? ' is-active' : ''}`}
              onMouseEnter={() => setLegend(category)}
              onMouseLeave={() => setLegend(null)}
            >
              <span className="rtlViewer-swatch" data-cat={category} />
              {t(`schematic.family.${category}`)}
            </span>
          ))}
        </div>
        <div className="rtlViewer__spacer" />
        {drawing && (
          <span className="rtlViewer__stats">
            {t('schematic.stats', {
              cells: drawing.graph.nodes.filter((n) => n.kind === 'cell').length,
              nets: drawing.graph.nets.length,
              ms: drawing.ms,
            })}
          </span>
        )}
      </div>
      {menuAt && <ContextMenu x={menuAt.x} y={menuAt.y} items={menuAt.items} onClose={() => setMenuAt(null)} />}
    </div>
  );
}
