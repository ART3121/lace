// O PRISM: o esquemático do netlist da última síntese, desenhado pelo
// próprio Studio (src/schematic). O uso segue o PRISM da AURORA: entrar num
// submódulo, voltar com Esc ou com o botão lateral do mouse, ir ao código
// com dois cliques, destacar uma ligação, exportar o SVG. O desenho é outro:
// as cores saem do tema, a hierarquia fica numa árvore ao lado, e o destaque
// segue a rede do netlist, não a geometria dos fios.

import { save } from '@tauri-apps/plugin-dialog';
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
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { runSynthesis } from '../../actions';
import { useT } from '../../i18n';
import { api } from '../../ipc/api';
import { Canvas, type CanvasHandle, type Selection } from '../../schematic/Canvas';
import { LEGEND, type Category } from '../../schematic/cells';
import { nodeKind, nodeName } from '../../schematic/describe';
import { cancelLayouts, drawModule, type Drawing } from '../../schematic/engine';
import { exportSvg } from '../../schematic/export';
import { moduleAt, moduleLabel } from '../../schematic/hierarchy';
import { HierarchyTree, Inspector } from '../../schematic/panels';
import type { SourceRef } from '../../schematic/yosys';
import { useEditor } from '../../state/editor';
import { useJobs } from '../../state/jobs';
import { useProject } from '../../state/project';
import { useSchematic } from '../../state/schematic';
import { showError, useToasts } from '../../state/toasts';
import { joinPath, separator } from '../../util/paths';
import { Button, Empty, IconButton, openContextMenu, Spinner, type MenuItem } from '../common';

/** Um módulo com mais células que isso pede confirmação antes do layout. */
const LARGE = 1500;

type Status =
  | { state: 'idle' }
  | { state: 'large'; cells: number }
  | { state: 'drawing' }
  /** `module`: o módulo do desenho, que pode ainda não ser o pedido. */
  | { state: 'ready'; drawing: Drawing; module: string }
  | { state: 'failed'; message: string };

/** O netlist da última síntese deste alvo que ficou no disco, para depois
 * de reabrir o Studio. */
function useNetlistOnDisk(enabled: boolean): string | null {
  const snapshot = useProject((s) => s.snapshot);
  const processor = useProject((s) => s.target);
  const [found, setFound] = useState<string | null>(null);
  // A pasta da síntese tem o nome do alvo: o processador ou o topo.
  const target = processor ?? snapshot?.top_module ?? null;
  useEffect(() => {
    setFound(null);
    if (!enabled || !snapshot || !target) return;
    const path = joinPath(snapshot.root, '.lace', 'Temp', 'synth', target, 'hierarchy.json');
    let cancelled = false;
    void api.fs
      .stat(path)
      .then((stat) => !cancelled && stat && !stat.is_dir && setFound(path))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [enabled, snapshot, target]);
  return found;
}

export function SchematicView() {
  const t = useT();
  const synthesis = useJobs((s) => s.synthesis);
  const running = useJobs((s) => s.running);
  const source = useSchematic((s) => s.source);
  const loading = useSchematic((s) => s.loading);
  const loadError = useSchematic((s) => s.error);
  const path = useSchematic((s) => s.path);
  const back = useSchematic((s) => s.back);
  const forward = useSchematic((s) => s.forward);
  const netNames = useSchematic((s) => s.netNames);
  const flags = useSchematic((s) => s.flags);
  const busWidths = useSchematic((s) => s.busWidths);
  const showTree = useSchematic((s) => s.showTree);
  const showInspector = useSchematic((s) => s.showInspector);
  const { load, go, enter, goBack, goForward, setOption } = useSchematic.getState();

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

  // O netlist acompanha a síntese: cada síntese nova regrava o arquivo.
  useEffect(() => {
    if (synthesis?.netlist) void load(synthesis.netlist);
  }, [synthesis?.netlist, synthesis?.duration_ms, load]);
  const onDisk = useNetlistOnDisk(!synthesis?.netlist && !source);

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

  const openSource = useCallback(
    (src: SourceRef) => {
      // O Yosys grava o caminho com '/'; as abas do Studio usam o separador
      // do sistema, o mesmo do netlist.
      const native = source && separator(source.path) === '\\' ? src.file.replace(/\//g, '\\') : src.file;
      void useEditor.getState().openFile(native, { line: src.line, column: src.column });
    },
    [source],
  );

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
    const root = useProject.getState().snapshot?.root;
    const file = await save({
      defaultPath: root ? joinPath(root, `${name}.svg`) : `${name}.svg`,
      filters: [{ name: 'SVG', extensions: ['svg'] }],
    });
    if (!file) return;
    try {
      await api.fs.writeText(file, exportSvg(drawing, busWidths, name));
      useToasts.getState().push({ kind: 'success', title: t('schematic.exported'), detail: file });
    } catch (error) {
      showError(error);
    }
  }, [drawing, source, module, busWidths, t]);

  const menu = useCallback(
    (event: { clientX: number; clientY: number; preventDefault: () => void }, target: Selection | null) => {
      const node = target?.kind === 'node' ? graph?.nodes.find((n) => n.id === target.id) : null;
      const net = target?.kind === 'net' ? graph?.nets.find((n) => n.id === target.id) : null;
      const items: MenuItem[] = [];
      if (node) {
        if (node.module) items.push({ label: t('schematic.openModule'), keys: 'Enter', run: () => enterNode(node.id) });
        if (node.src[0]) items.push({ label: t('schematic.goToSource'), run: () => openSource(node.src[0]!) });
        items.push({ label: t('schematic.highlight'), run: () => setSelected({ kind: 'node', id: node.id }) });
        items.push({ label: t('schematic.copyName'), run: () => void navigator.clipboard.writeText(nodeName(node)) });
        items.push({ separator: true });
      } else if (net) {
        items.push({ label: t('schematic.highlight'), run: () => setSelected({ kind: 'net', id: net.id }) });
        if (net.src[0]) items.push({ label: t('schematic.goToSource'), run: () => openSource(net.src[0]!) });
        if (net.name) items.push({ label: t('schematic.copyName'), run: () => void navigator.clipboard.writeText(net.name!) });
        items.push({ separator: true });
      }
      items.push(
        { label: t('schematic.fit'), keys: 'F', run: () => canvas.current?.fit() },
        { label: t('schematic.actualSize'), keys: '0', run: () => canvas.current?.actualSize() },
        { separator: true },
        { label: t('schematic.export'), disabled: !drawing, run: () => void exportFile() },
        { label: t('schematic.resynthesize'), disabled: !!running, run: () => void runSynthesis() },
      );
      openContextMenu(event, items);
    },
    [graph, t, enterNode, openSource, drawing, exportFile, running],
  );

  const navigate = useCallback((direction: 'back' | 'forward') => (direction === 'back' ? goBack() : goForward()), [goBack, goForward]);

  // Ctrl+F na vista vai para a busca do PRISM, não para a do editor.
  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f') {
      event.preventDefault();
      event.stopPropagation();
      searchInput.current?.focus();
      searchInput.current?.select();
    }
  };

  if (!source) {
    return (
      <div className="view-page">
        <Empty>
          {loading ? (
            <p>
              <Spinner /> {t('schematic.loading')}
            </p>
          ) : (
            <>
              <p>{loadError ? t('schematic.loadFailed', { message: loadError }) : t('schematic.empty')}</p>
              <div className="prism-empty__actions">
                <Button variant="primary" disabled={!!running} onClick={() => void runSynthesis()}>
                  {t('action.synthesize')}
                </Button>
                {onDisk && <Button onClick={() => void load(onDisk)}>{t('schematic.lastSynthesis')}</Button>}
              </div>
            </>
          )}
        </Empty>
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
    <div className="prism" onKeyDown={onKeyDown}>
      <div className="view-toolbar prism__toolbar">
        <IconButton label={t('schematic.back')} disabled={!back.length && !path.length} onClick={goBack}>
          <ArrowLeft size={15} />
        </IconButton>
        <IconButton label={t('schematic.forward')} disabled={!forward.length} onClick={goForward}>
          <ArrowRight size={15} />
        </IconButton>
        <nav className="prism-crumbs" aria-label={t('schematic.tree')}>
          {crumbs.map((crumb, i) => (
            <span key={crumb.key.join('\u0000') || '/'} className="prism-crumbs__item">
              {i > 0 && <ChevronRight size={12} className="prism-crumbs__sep" />}
              <button
                type="button"
                className={`prism-crumbs__button${i === crumbs.length - 1 ? ' is-current' : ''}`}
                onClick={() => go(crumb.key)}
                title={crumb.detail ? `${crumb.name} (${crumb.detail})` : crumb.name}
              >
                {crumb.name}
                {crumb.detail && <span className="prism-crumbs__detail">{crumb.detail}</span>}
              </button>
            </span>
          ))}
        </nav>
        <div className="view-toolbar__spacer" />
        <label className="prism-search">
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
            <span className="prism-search__count">
              {!results.length
                ? t('schematic.noMatch')
                : matchIndex >= 0
                  ? t('schematic.searchPosition', { index: matchIndex + 1, count: results.length })
                  : t('schematic.searchCount', { count: results.length })}
            </span>
          )}
        </label>
        <div className="prism__group">
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
        <div className="prism__group">
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
        <div className="prism__group">
          <IconButton label={t('schematic.tree')} active={showTree} onClick={() => setOption('showTree', !showTree)}>
            <ListTree size={15} />
          </IconButton>
          <IconButton label={t('schematic.inspector')} active={showInspector} onClick={() => setOption('showInspector', !showInspector)}>
            <PanelRight size={15} />
          </IconButton>
          <IconButton label={t('schematic.export')} disabled={!drawing} onClick={() => void exportFile()}>
            <Download size={15} />
          </IconButton>
          <IconButton label={t('schematic.resynthesize')} disabled={!!running} onClick={() => void runSynthesis()}>
            {running?.flow === 'synthesize' ? <Spinner /> : <RefreshCw size={15} />}
          </IconButton>
        </div>
      </div>

      <div className="prism__body">
        {showTree && <HierarchyTree source={source} path={path} onGo={go} />}
        <div className="prism__stage">
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
            <div className="prism__placeholder">
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
            onOpenSource={openSource}
            onLegend={setLegend}
          />
        )}
      </div>

      <div className="prism__footer">
        <div className="prism-legend">
          {LEGEND.map((category) => (
            <span
              key={category}
              className={`prism-legend__item${legend === category ? ' is-active' : ''}`}
              onMouseEnter={() => setLegend(category)}
              onMouseLeave={() => setLegend(null)}
            >
              <span className="prism-swatch" data-cat={category} />
              {t(`schematic.family.${category}`)}
            </span>
          ))}
        </div>
        <div className="view-toolbar__spacer" />
        {drawing && (
          <span className="prism__stats">
            {t('schematic.stats', {
              cells: drawing.graph.nodes.filter((n) => n.kind === 'cell').length,
              nets: drawing.graph.nets.length,
              ms: drawing.ms,
            })}
          </span>
        )}
      </div>
    </div>
  );
}
