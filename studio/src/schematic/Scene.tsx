// O desenho de um módulo: os símbolos, os fios, as junções e os rótulos, em
// SVG no próprio DOM. As cores são as variáveis `--sch-*` do tema
// (schematicColors, themes/index.ts) e as classes estão em app.css, seção
// PRISM: trocar de tema repinta sem refazer o layout.
//
// Nada aqui trata evento. Cada nó leva `data-node`, cada fio `data-net` e
// cada rótulo de rede global `data-flag`, e o Canvas escuta no elemento de
// cima.

import { memo, type ReactNode } from 'react';

import { TAG_HEIGHT, tagWidth, type Shape } from './cells';
import type { SchematicEdge, SchematicGraph, SchematicNode } from './graph';
import { roundedPath, type EdgeLayout, type Point, type SchematicLayout } from './layout';

export interface Focus {
  /** As redes acesas; as outras ficam apagadas. */
  nets: Set<string>;
  /** Os nós acesos. */
  nodes: Set<string>;
}

export interface SceneProps {
  graph: SchematicGraph;
  layout: SchematicLayout;
  focus: Focus | null;
  selected: string | null;
  /** Os nós que a busca achou. */
  matches: Set<string>;
  busWidths: boolean;
}

// Símbolos --------------------------------------------------------------------

function Stub({ from, to }: { from: Point; to: Point }) {
  return <line className="sch-stub" x1={from.x} y1={from.y} x2={to.x} y2={to.y} />;
}

/** Os traços curtos que levam das portas fixas até o contorno do símbolo. */
function Stubs({ node, inset }: { node: SchematicNode; inset: (y: number) => number }) {
  return (
    <>
      {node.ports.map((port) => {
        if (port.side === 'W') return <Stub key={port.id} from={{ x: 0, y: port.y }} to={{ x: inset(port.y), y: port.y }} />;
        if (port.side === 'E') return <Stub key={port.id} from={{ x: node.width - 6, y: port.y }} to={{ x: node.width, y: port.y }} />;
        return <Stub key={port.id} from={{ x: port.x, y: node.height - 6 }} to={{ x: port.x, y: port.y }} />;
      })}
    </>
  );
}

function PortSymbol({ node }: { node: SchematicNode }) {
  const { width: w, height: h } = node;
  const kind = node.shape.kind;
  const points =
    kind === 'in'
      ? `0,0 ${w - 9},0 ${w},${h / 2} ${w - 9},${h} 0,${h} 5,${h / 2}`
      : kind === 'out'
        ? `0,0 ${w - 9},0 ${w},${h / 2} ${w - 9},${h} 0,${h}`
        : `0,${h / 2} 7,0 ${w - 7},0 ${w},${h / 2} ${w - 7},${h} 7,${h}`;
  return (
    <>
      <polygon className="sch-body" points={points} />
      <text className="sch-title" x={kind === 'inout' ? 11 : kind === 'in' ? 10 : 7} y={h / 2}>
        {node.title}
      </text>
    </>
  );
}

const BUBBLE = 3;

function GateSymbol({ node, shape }: { node: SchematicNode; shape: Extract<Shape, { kind: 'gate' }> }) {
  const { width: w, height: h } = node;
  const inverted = shape.gate === 'nand' || shape.gate === 'nor' || shape.gate === 'xnor' || shape.gate === 'not';
  const right = inverted ? w - 6 - BUBBLE * 2 : w - 6;
  const mid = h / 2;
  let body: string;
  let back: string | null = null;
  // Onde o contorno de trás corta a altura y: os traços das entradas param ali.
  let inset = (_y: number) => 4;
  switch (shape.gate) {
    case 'and':
    case 'nand':
      body = `M4,0H${right - mid}A${mid},${mid} 0 0 1 ${right - mid},${h}H4Z`;
      break;
    case 'or':
    case 'nor':
    case 'xor':
    case 'xnor': {
      const left = shape.gate.startsWith('x') ? 9 : 4;
      body = `M${left},0Q${right - 14},0 ${right},${mid}Q${right - 14},${h} ${left},${h}Q${left + 8},${mid} ${left},0Z`;
      if (shape.gate.startsWith('x')) back = `M3,${h}Q11,${mid} 3,0`;
      const backX = shape.gate.startsWith('x') ? 3 : left;
      inset = (y) => {
        const t = 1 - y / h;
        return backX * (1 - t) ** 2 + 2 * (backX + 8) * t * (1 - t) + backX * t ** 2;
      };
      break;
    }
    default:
      body = `M4,1L${right},${mid}L4,${h - 1}Z`;
  }
  return (
    <>
      <Stubs node={node} inset={inset} />
      <path className="sch-body" d={body} />
      {back && <path className="sch-line" d={back} />}
      {inverted && <circle className="sch-body" cx={right + BUBBLE} cy={mid} r={BUBBLE} />}
      {shape.glyph && (
        <text className="sch-gate-glyph" x={shape.gate === 'not' || shape.gate === 'buf' ? 12 : right - mid - 2} y={mid} textAnchor="middle">
          {shape.glyph}
        </text>
      )}
    </>
  );
}

function OpSymbol({ node, glyph }: { node: SchematicNode; glyph: string }) {
  const { width: w, height: h } = node;
  return (
    <>
      <Stubs node={node} inset={() => 6} />
      <rect className="sch-body" x={2} y={0.5} width={w - 4} height={h - 1} rx={(h - 1) / 2} />
      <text className="sch-glyph" x={w / 2} y={h / 2 + 0.5} textAnchor="middle">
        {glyph}
      </text>
    </>
  );
}

function MuxSymbol({ node }: { node: SchematicNode }) {
  const { width: w, height: h } = node;
  const right = w - 4;
  return (
    <>
      <Stubs node={node} inset={() => 4} />
      <polygon className="sch-body" points={`4,0 ${right},9 ${right},${h - 9} 4,${h}`} />
      {node.ports
        .filter((port) => port.label)
        .map((port) => (
          <text key={port.id} className="sch-pin" x={9} y={port.y}>
            {port.label}
          </text>
        ))}
    </>
  );
}

function RegSymbol({ node, shape }: { node: SchematicNode; shape: Extract<Shape, { kind: 'reg' }> }) {
  const { width: w, height: h } = node;
  const clock = node.ports.find((port) => port.side === 'W' && !port.label);
  return (
    <>
      <Stubs node={node} inset={() => 4} />
      <rect className="sch-body" x={4} y={0.5} width={w - 8} height={h - 1} rx={3} />
      <text className="sch-kind" x={w / 2} y={9} textAnchor="middle">
        {shape.title}
      </text>
      {node.ports
        .filter((port) => port.label)
        .map((port) => (
          <text key={port.id} className="sch-pin" x={port.output ? w - 9 : 9} y={port.y} textAnchor={port.output ? 'end' : 'start'}>
            {port.label}
          </text>
        ))}
      {clock && (
        <>
          <path className="sch-line" d={`M4,${clock.y - 4.5}L11,${clock.y}L4,${clock.y + 4.5}`} />
          {shape.clockInverted && <circle className="sch-body" cx={1} cy={clock.y} r={2.5} />}
        </>
      )}
    </>
  );
}

function BoxSymbol({ node }: { node: SchematicNode }) {
  const { width: w, height: h } = node;
  const header = node.subtitle ? 36 : 24;
  return (
    <>
      <Stubs node={node} inset={() => 4} />
      <rect className="sch-body" x={4} y={0.5} width={w - 8} height={h - 1} rx={6} />
      <path className="sch-header" d={`M4.5,${header}H${w - 4.5}`} />
      <text className="sch-title sch-title--box" x={12} y={14}>
        {node.title}
      </text>
      {node.subtitle && (
        <text className="sch-subtitle" x={12} y={27}>
          {node.subtitle}
        </text>
      )}
      {node.module && (
        <g className="sch-enter">
          <rect className="sch-enter__hit" x={w - 24} y={2} width={18} height={18} rx={4} />
          <path className="sch-enter__icon" d={`M${w - 17},8.5h5.5v5.5M${w - 11.5},8.5l-6,6`} />
        </g>
      )}
      {node.ports.map((port) => (
        <text
          key={port.id}
          className="sch-pin"
          x={port.output ? w - 11 : 11}
          y={port.y}
          textAnchor={port.output ? 'end' : 'start'}
        >
          {port.label}
        </text>
      ))}
    </>
  );
}

function SliceSymbol({ node }: { node: SchematicNode }) {
  const { width: w, height: h } = node;
  const split = node.kind === 'split';
  const barX = split ? 4 : w - 8;
  return (
    <>
      {node.ports.map((port) => {
        const many = port.label !== null;
        const [x1, x2] = split ? (many ? [barX + 4, w] : [0, barX]) : many ? [0, barX] : [barX + 4, w];
        return (
          <g key={port.id}>
            <line className={`sch-stub${port.bits > 1 ? ' sch-stub--bus' : ''}`} x1={x1} y1={port.y} x2={x2} y2={port.y} />
            {many && (
              <text className="sch-slice" x={split ? w - 2 : 2} y={port.y - 5.5} textAnchor={split ? 'end' : 'start'}>
                {port.label}
              </text>
            )}
          </g>
        );
      })}
      <rect className="sch-bar" x={barX} y={0} width={4} height={h} rx={2} />
    </>
  );
}

function Symbol({ node }: { node: SchematicNode }) {
  const shape = node.shape;
  switch (shape.kind) {
    case 'in':
    case 'out':
    case 'inout':
      return <PortSymbol node={node} />;
    case 'gate':
      return <GateSymbol node={node} shape={shape} />;
    case 'op':
      return <OpSymbol node={node} glyph={shape.glyph} />;
    case 'mux':
      return <MuxSymbol node={node} />;
    case 'reg':
      return <RegSymbol node={node} shape={shape} />;
    case 'split':
    case 'join':
      return <SliceSymbol node={node} />;
    default:
      return <BoxSymbol node={node} />;
  }
}

interface NodeProps {
  node: SchematicNode;
  at: Point;
  state: string;
}

const NodeView = memo(function NodeView({ node, at, state }: NodeProps) {
  const classes = ['sch-node', `sch-node--${node.shape.kind}`];
  if (node.module) classes.push('sch-node--enter');
  if (state) classes.push(state);
  return (
    <g
      className={classes.join(' ')}
      data-node={node.id}
      data-cat={node.category}
      transform={`translate(${at.x},${at.y})`}>
      <rect className="sch-hit" x={0} y={0} width={node.width} height={node.height} />
      <Symbol node={node} />
    </g>
  );
});

// Fios ------------------------------------------------------------------------

interface WireProps {
  edge: SchematicEdge;
  layout: EdgeLayout;
  state: string;
}

const WireView = memo(function WireView({ edge, layout, state }: WireProps) {
  const d = layout.paths.map((points) => roundedPath(points)).join('');
  const bus = edge.bits > 1;
  return (
    <g className={`sch-wire${bus ? ' sch-wire--bus' : ''}${state ? ` ${state}` : ''}`} data-net={edge.net}>
      <path className="sch-wire__hit" d={d} />
      <path className="sch-wire__line" d={d} />
      {layout.junctions.map((point, i) => (
        <circle key={i} className="sch-junction" cx={point.x} cy={point.y} r={bus ? 3 : 2.4} />
      ))}
    </g>
  );
});

/** A largura de um barramento, uma vez por porta de origem: um traço
 * oblíquo no primeiro trecho, com o número em cima. */
function BusMarks({ graph, layout }: { graph: SchematicGraph; layout: SchematicLayout }) {
  const seen = new Set<string>();
  const marks: { key: string; x: number; y: number; bits: number }[] = [];
  for (const edge of graph.edges) {
    if (edge.bits < 2 || seen.has(edge.source.port)) continue;
    const path = layout.edges.get(edge.id)?.paths[0];
    if (!path || path.length < 2) continue;
    const [a, b] = path as [Point, Point];
    if (a.y !== b.y || Math.abs(b.x - a.x) < 22) continue;
    seen.add(edge.source.port);
    const x = a.x + Math.sign(b.x - a.x) * 11;
    marks.push({ key: edge.source.port, x, y: a.y, bits: edge.bits });
  }
  return (
    <g className="sch-busmarks">
      {marks.map((mark) => (
        <g key={mark.key} data-net-source={mark.key}>
          <line className="sch-busmark" x1={mark.x - 3} y1={mark.y + 4} x2={mark.x + 3} y2={mark.y - 4} />
          <text className="sch-buswidth" x={mark.x} y={mark.y - 6.5} textAnchor="middle">
            {mark.bits}
          </text>
        </g>
      ))}
    </g>
  );
}

/**
 * As etiquetas das portas que recebem sem fio: o rótulo de uma rede global
 * (`clk`, `rst`) ou uma constante. À esquerda das entradas, apontando para
 * a porta; embaixo da seleção de um mux. O layout reservou o espaço (rótulo
 * de porta do ELK, layout.ts).
 */
function Tags({ graph, layout, focus }: { graph: SchematicGraph; layout: SchematicLayout; focus: Focus | null }) {
  const tags: ReactNode[] = [];
  for (const node of graph.nodes) {
    const at = layout.nodes.get(node.id);
    if (!at) continue;
    for (const port of node.ports) {
      const tag = port.tag;
      if (!tag) continue;
      const w = tagWidth(tag.text);
      const h = TAG_HEIGHT;
      const x = at.x + port.x;
      const y = at.y + port.y;
      let shape: string;
      let text: Point;
      if (port.side === 'S') {
        const top = y + 3;
        shape = `M${x - w / 2},${top + 4}H${x - 4}L${x},${top}L${x + 4},${top + 4}H${x + w / 2}V${top + 4 + h}H${x - w / 2}Z`;
        text = { x: x - w / 2 + 6, y: top + 4 + h / 2 };
      } else {
        const left = x - w - 1;
        shape = `M${left},${y - h / 2}H${x - 7}L${x - 1},${y}L${x - 7},${y + h / 2}H${left}Z`;
        text = { x: left + 5, y };
      }
      let className = `sch-tag sch-tag--${tag.kind}`;
      if (tag.kind === 'const' && tag.undriven) className += ' sch-tag--undriven';
      if (focus) className += tag.kind === 'flag' && focus.nets.has(tag.net) ? ' is-hot' : ' is-dim';
      tags.push(
        <g key={port.id} className={className} data-flag={tag.kind === 'flag' ? tag.net : undefined}>
          {tag.kind === 'const' && <title>{tag.full}</title>}
          <path className="sch-tag__shape" d={shape} />
          <text className="sch-tag__text" x={text.x} y={text.y}>
            {tag.text}
          </text>
        </g>,
      );
    }
  }
  return <g className="sch-tags">{tags}</g>;
}

function Labels({ graph, layout, focus }: { graph: SchematicGraph; layout: SchematicLayout; focus: Focus | null }) {
  return (
    <g className="sch-labels">
      {graph.edges.map((edge) => {
        const label = layout.edges.get(edge.id)?.label;
        if (!label) return null;
        const hot = focus?.nets.has(edge.net);
        return (
          <text
            key={edge.id}
            className={`sch-netname${hot ? ' is-hot' : focus ? ' is-dim' : ''}`}
            data-net={edge.net}
            x={label.x + 2}
            y={label.y + label.height - 3}
          >
            {label.text}
          </text>
        );
      })}
    </g>
  );
}

// Cena ------------------------------------------------------------------------

export const SchematicScene = memo(function SchematicScene({ graph, layout, focus, selected, matches, busWidths }: SceneProps) {
  return (
    <g className={`sch-scene${focus ? ' has-focus' : ''}`}>
      <g className="sch-wires">
        {graph.edges.map((edge) => {
          const edgeLayout = layout.edges.get(edge.id);
          if (!edgeLayout) return null;
          const state = focus ? (focus.nets.has(edge.net) ? 'is-hot' : 'is-dim') : '';
          return <WireView key={edge.id} edge={edge} layout={edgeLayout} state={state} />;
        })}
      </g>
      {busWidths && <BusMarks graph={graph} layout={layout} />}
      <g className="sch-nodes">
        {graph.nodes.map((node) => {
          const at = layout.nodes.get(node.id);
          if (!at) return null;
          const states: string[] = [];
          if (node.id === selected) states.push('is-selected');
          if (matches.has(node.id)) states.push('is-match');
          if (focus) states.push(focus.nodes.has(node.id) ? 'is-hot' : 'is-dim');
          return <NodeView key={node.id} node={node} at={at} state={states.join(' ')} />;
        })}
      </g>
      <Tags graph={graph} layout={layout} focus={focus} />
      <Labels graph={graph} layout={layout} focus={focus} />
    </g>
  );
});
