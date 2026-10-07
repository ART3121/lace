// Os painéis do PRISM: a árvore de instâncias à esquerda e os detalhes do
// que está selecionado à direita.

import { ChevronDown, ChevronRight, CornerDownRight, FileCode2 } from 'lucide-react';
import { useEffect, useMemo, useState, type ReactNode } from 'react';

import { useT } from '../i18n';
import type { LoadedNetlist } from '../state/schematic';
import type { Selection } from './Canvas';
import { LEGEND, type Category } from './cells';
import { bitsText, cellParameters, endpointText, netName, nodeKind, nodeName, sourceText } from './describe';
import type { Drawing } from './engine';
import { instancesOf, moduleLabel } from './hierarchy';
import { moduleParameters, type SourceRef } from './yosys';

// Hierarquia --------------------------------------------------------------------

export function HierarchyTree({
  source,
  path,
  onGo,
}: {
  source: LoadedNetlist;
  path: string[];
  onGo: (path: string[]) => void;
}) {
  const t = useT();
  const [open, setOpen] = useState<Set<string>>(() => new Set(['']));
  // O caminho atual fica sempre aberto.
  useEffect(() => {
    setOpen((current) => {
      const next = new Set(current);
      for (let i = 0; i <= path.length; i++) next.add(path.slice(0, i).join('\u0000'));
      return next.size === current.size ? current : next;
    });
  }, [path]);

  const current = path.join('\u0000');
  const rows: ReactNode[] = [];
  const visit = (module: string, at: string[], label: string, detail: string | null, depth: number) => {
    const key = at.join('\u0000');
    const children = depth > 48 ? [] : instancesOf(source.netlist, module);
    const expanded = open.has(key);
    rows.push(
      <div
        key={key || '/'}
        className={`tree__row prism-tree__row${key === current ? ' is-selected' : ''}`}
        style={{ paddingLeft: 6 + depth * 12 }}
        title={detail ? `${label} (${detail})` : label}
        onClick={() => onGo(at)}
      >
        <span
          className="prism-tree__toggle"
          onClick={(event) => {
            event.stopPropagation();
            setOpen((s) => {
              const next = new Set(s);
              if (next.has(key)) next.delete(key);
              else next.add(key);
              return next;
            });
          }}
        >
          {children.length > 0 && (expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />)}
        </span>
        <span className="prism-tree__name">{label}</span>
        {detail && <span className="tree__detail">{detail}</span>}
      </div>,
    );
    if (!expanded) return;
    for (const instance of children) {
      visit(instance.module, [...at, instance.key], instance.name, moduleLabel(source.netlist, instance.module), depth + 1);
    }
  };
  visit(source.top, [], moduleLabel(source.netlist, source.top), t('schematic.topTag'), 0);

  return (
    <div className="prism-side prism-side--tree">
      <div className="prism-side__title">{t('schematic.tree')}</div>
      <div className="prism-side__body">{rows}</div>
    </div>
  );
}

// Detalhes ------------------------------------------------------------------------

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="prism-props__row">
      <span className="prism-props__label">{label}</span>
      <span className="prism-props__value">{children}</span>
    </div>
  );
}

const PARAMETERS = 8;

/** Os parâmetros, com os primeiros à vista: o processador SAPHO tem dezenas. */
function Parameters({ list }: { list: [string, string][] }) {
  const t = useT();
  const [all, setAll] = useState(false);
  if (!list.length) return null;
  const shown = all ? list : list.slice(0, PARAMETERS);
  return (
    <>
      <div className="prism-props__section">{t('schematic.parameters')}</div>
      {shown.map(([key, value]) => (
        <Row key={key} label={key}>
          <span className="mono">{value}</span>
        </Row>
      ))}
      {list.length > PARAMETERS && (
        <button type="button" className="prism-link prism-props__more" onClick={() => setAll(!all)}>
          {all ? t('schematic.fewer') : t('schematic.allParameters', { count: list.length })}
        </button>
      )}
    </>
  );
}

function SourceLink({ src, onOpen }: { src: SourceRef; onOpen: (src: SourceRef) => void }) {
  return (
    <button type="button" className="prism-link" title={`${src.file}:${src.line}`} onClick={() => onOpen(src)}>
      <FileCode2 size={12} />
      {sourceText(src)}
    </button>
  );
}

export function Inspector({
  source,
  module,
  drawing,
  selected,
  legend,
  onSelect,
  onCenter,
  onEnter,
  onOpenSource,
  onLegend,
}: {
  source: LoadedNetlist;
  module: string;
  drawing: Drawing | null;
  selected: Selection | null;
  legend: Category | null;
  onSelect: (selection: Selection) => void;
  onCenter: (nodeId: string) => void;
  onEnter: (nodeId: string) => void;
  onOpenSource: (src: SourceRef) => void;
  onLegend: (category: Category | null) => void;
}) {
  const t = useT();
  const graph = drawing?.graph ?? null;
  const pick = (selection: Selection) => {
    onSelect(selection);
    if (!graph) return;
    const nodeId = selection.kind === 'node' ? selection.id : graph.nets.find((n) => n.id === selection.id)?.driver.node;
    if (nodeId) onCenter(nodeId);
  };

  const counts = useMemo(() => {
    const result = new Map<Category, number>();
    for (const node of graph?.nodes ?? []) {
      if (node.kind === 'cell') result.set(node.category, (result.get(node.category) ?? 0) + 1);
    }
    return result;
  }, [graph]);

  let content: ReactNode = null;
  const node = selected?.kind === 'node' ? graph?.nodes.find((n) => n.id === selected.id) : null;
  const net = selected?.kind === 'net' ? graph?.nets.find((n) => n.id === selected.id) : null;

  if (graph && node) {
    const parameters =
      node.category === 'module' && node.type ? moduleParameters(node.type, source.netlist.modules[node.type]) : cellParameters(node);
    content = (
      <>
        <div className="prism-props__head">
          <span className="prism-swatch" data-cat={node.category} />
          <span className="prism-props__name">{nodeName(node)}</span>
        </div>
        <div className="prism-props__kind">{nodeKind(node)}</div>
        {node.module && (
          <button type="button" className="btn btn--default btn--sm prism-props__action" onClick={() => onEnter(node.id)}>
            <CornerDownRight size={13} />
            <span>{t('schematic.openModule')}</span>
          </button>
        )}
        {node.src.map((src, i) => (
          <Row key={i} label={i === 0 ? t('schematic.source') : ''}>
            <SourceLink src={src} onOpen={onOpenSource} />
          </Row>
        ))}
        <Parameters list={parameters} />
        {node.ports.length > 0 && (
          <>
            <div className="prism-props__section">{t('schematic.ports')}</div>
            {node.ports.map((port) => {
              const portNet = port.output
                ? graph.nets.find((n) => n.driver.port === port.id)
                : graph.nets.find((n) => n.loads.some((load) => load.port === port.id));
              return (
                <Row key={port.id} label={port.label ?? port.name}>
                  {port.tag?.kind === 'const' ? (
                    <span className="mono" title={port.tag.full}>
                      {port.tag.undriven ? t('schematic.kind.undriven') : `= ${port.tag.text}`}
                    </span>
                  ) : portNet ? (
                    <button type="button" className="prism-link" onClick={() => pick({ kind: 'net', id: portNet.id })}>
                      {port.output ? '→ ' : '← '}
                      {netName(portNet)}
                    </button>
                  ) : (
                    <span className="muted">{t('schematic.unconnected')}</span>
                  )}
                  <span className="prism-props__bits">{port.bits}</span>
                </Row>
              );
            })}
          </>
        )}
      </>
    );
  } else if (graph && net) {
    const driver = graph.nodes.find((n) => n.id === net.driver.node);
    content = (
      <>
        <div className="prism-props__head">
          <span className="prism-swatch prism-swatch--wire" />
          <span className="prism-props__name">{netName(net)}</span>
        </div>
        <div className="prism-props__kind">
          {t('schematic.net')} · {bitsText(net.bits)}
        </div>
        {net.src[0] && (
          <Row label={t('schematic.source')}>
            <SourceLink src={net.src[0]} onOpen={onOpenSource} />
          </Row>
        )}
        <div className="prism-props__section">{t('schematic.driver')}</div>
        {driver && (
          <button type="button" className="prism-link prism-link--row" onClick={() => pick({ kind: 'node', id: driver.id })}>
            {endpointText(graph, net.driver)}
          </button>
        )}
        <div className="prism-props__section">{t('schematic.loads', { count: net.loads.length })}</div>
        {net.loads.map((load, i) => (
          <button key={i} type="button" className="prism-link prism-link--row" onClick={() => pick({ kind: 'node', id: load.node })}>
            {endpointText(graph, load)}
          </button>
        ))}
      </>
    );
  } else {
    const definition = source.netlist.modules[module];
    const parameters = moduleParameters(module, definition);
    const ports = Object.entries(definition?.ports ?? {});
    const instances = instancesOf(source.netlist, module);
    content = (
      <>
        <div className="prism-props__head">
          <span className="prism-swatch" data-cat="module" />
          <span className="prism-props__name">{moduleLabel(source.netlist, module)}</span>
        </div>
        <div className="prism-props__kind">{t('schematic.moduleSummary')}</div>
        <div className="prism-props__section">{t('schematic.cells')}</div>
        {LEGEND.filter((category) => counts.get(category)).map((category) => (
          <div
            key={category}
            className={`prism-props__row prism-family${legend === category ? ' is-active' : ''}`}
            onMouseEnter={() => onLegend(category)}
            onMouseLeave={() => onLegend(null)}
          >
            <span className="prism-props__label">
              <span className="prism-swatch" data-cat={category} />
              {t(`schematic.family.${category}`)}
            </span>
            <span className="prism-props__value">{counts.get(category)}</span>
          </div>
        ))}
        {!counts.size && <div className="muted prism-props__empty">{drawing ? t('schematic.noCells') : '…'}</div>}
        <div className="prism-props__section">{t('schematic.ports')}</div>
        {ports.map(([name, port]) => (
          <Row key={name} label={name}>
            <span className="muted">{t(`schematic.kind.${port.direction === 'input' ? 'in' : port.direction === 'output' ? 'out' : 'inout'}`)}</span>
            <span className="prism-props__bits">{port.bits.length}</span>
          </Row>
        ))}
        {instances.length > 0 && (
          <>
            <div className="prism-props__section">{t('schematic.instances')}</div>
            {instances.map((instance) => {
              const nodeId = graph?.nodes.find((n) => n.key === instance.key)?.id;
              return (
                <button
                  key={instance.key}
                  type="button"
                  className="prism-link prism-link--row"
                  disabled={!nodeId}
                  onClick={() => nodeId && pick({ kind: 'node', id: nodeId })}
                  onDoubleClick={() => nodeId && onEnter(nodeId)}
                >
                  {instance.name}
                  <span className="tree__detail">{moduleLabel(source.netlist, instance.module)}</span>
                </button>
              );
            })}
          </>
        )}
        <Parameters list={parameters} />
      </>
    );
  }

  return (
    <div className="prism-side prism-side--inspector">
      <div className="prism-side__title">{t('schematic.inspector')}</div>
      <div className="prism-side__body prism-props">{content}</div>
    </div>
  );
}
