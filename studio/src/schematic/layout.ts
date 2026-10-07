// O layout do grafo de um módulo: o ELK (layered, da esquerda para a
// direita, fios ortogonais) posiciona os nós e roteia as arestas. As portas
// dos símbolos ficam em posição fixa (`FIXED_POS`), as entradas do módulo na
// primeira camada e as saídas na última.
//
// O ELK em si fica fora daqui (`elk.ts` no Studio, `elkjs` direto nos
// testes): este módulo só monta a entrada e lê a saída.

import type { ElkExtendedEdge, ElkNode, ElkPoint } from 'elkjs/lib/elk-api';

import { TAG_HEIGHT, tagWidth, textWidth } from './cells';
import type { SchematicGraph } from './graph';

export interface Point {
  x: number;
  y: number;
}

export interface EdgeLayout {
  /** Um traço por seção do ELK: do início ao fim, com as dobras. */
  paths: Point[][];
  junctions: Point[];
  label: { x: number; y: number; width: number; height: number; text: string } | null;
}

export interface SchematicLayout {
  width: number;
  height: number;
  nodes: Map<string, Point>;
  edges: Map<string, EdgeLayout>;
}

export type ElkLayout = (graph: ElkNode) => Promise<ElkNode>;

const LABEL_SIZE = 9;

const SIDES = { W: 'WEST', E: 'EAST', S: 'SOUTH', N: 'NORTH' } as const;

const ROOT_OPTIONS: Record<string, string> = {
  'elk.algorithm': 'layered',
  'elk.direction': 'RIGHT',
  'elk.edgeRouting': 'ORTHOGONAL',
  'elk.padding': '[top=32,left=32,bottom=32,right=32]',
  'elk.spacing.nodeNode': '22',
  'elk.spacing.edgeEdge': '7',
  'elk.spacing.edgeNode': '10',
  'elk.spacing.edgeLabel': '2',
  'elk.layered.spacing.nodeNodeBetweenLayers': '34',
  'elk.layered.spacing.edgeNodeBetweenLayers': '14',
  'elk.layered.spacing.edgeEdgeBetweenLayers': '7',
  // O NETWORK_SIMPLEX alinha melhor, mas levou 199 s no ula_fdiv do proc_fft;
  // o BRANDES_KOEPF, menos de 2 s.
  'elk.layered.nodePlacement.strategy': 'BRANDES_KOEPF',
  'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
  'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
  'elk.layered.thoroughness': '7',
  'elk.layered.edgeLabels.sideSelection': 'ALWAYS_UP',
  'elk.layered.mergeEdges': 'false',
  'elk.layered.unnecessaryBendpoints': 'false',
  'elk.separateConnectedComponents': 'true',
  'elk.spacing.componentComponent': '40',
};

export function toElk(graph: SchematicGraph, overrides: Record<string, string> = {}): ElkNode {
  const children: ElkNode[] = graph.nodes.map((node) => {
    const options: Record<string, string> = { 'elk.portConstraints': 'FIXED_POS', 'elk.portLabels.placement': 'OUTSIDE' };
    if (node.kind === 'port') {
      options['elk.layered.layering.layerConstraint'] = node.shape.kind === 'out' ? 'LAST' : 'FIRST';
    }
    return {
      id: node.id,
      width: node.width,
      height: node.height,
      layoutOptions: options,
      ports: node.ports.map((port) => ({
        id: port.id,
        x: port.x,
        y: port.y,
        width: 0,
        height: 0,
        layoutOptions: { 'elk.port.side': SIDES[port.side] },
        // A etiqueta ocupa espaço ao lado da porta (Scene.tsx, Tags).
        labels: port.tag ? [{ text: port.tag.text, width: tagWidth(port.tag.text) + 2, height: TAG_HEIGHT + 4 }] : undefined,
      })),
    };
  });
  const edges: ElkExtendedEdge[] = graph.edges.map((edge) => ({
    id: edge.id,
    sources: [edge.source.port],
    targets: [edge.target.port],
    layoutOptions: { 'elk.edge.thickness': edge.bits > 1 ? '2' : '1' },
    labels: edge.label
      ? [
          {
            text: edge.label,
            width: textWidth(edge.label, LABEL_SIZE) + 4,
            height: LABEL_SIZE + 3,
            layoutOptions: { 'elk.edgeLabels.placement': 'TAIL' },
          },
        ]
      : undefined,
  }));
  return { id: 'root', layoutOptions: { ...ROOT_OPTIONS, ...overrides }, children, edges };
}

export function fromElk(result: ElkNode): SchematicLayout {
  const nodes = new Map<string, Point>();
  for (const child of result.children ?? []) nodes.set(child.id, { x: child.x ?? 0, y: child.y ?? 0 });
  const edges = new Map<string, EdgeLayout>();
  for (const edge of (result.edges ?? []) as (ElkExtendedEdge & { junctionPoints?: ElkPoint[] })[]) {
    const paths = (edge.sections ?? []).map((section) => [
      section.startPoint,
      ...(section.bendPoints ?? []),
      section.endPoint,
    ]);
    const label = edge.labels?.[0];
    edges.set(edge.id, {
      paths,
      junctions: edge.junctionPoints ?? [],
      label:
        label && label.text
          ? { x: label.x ?? 0, y: label.y ?? 0, width: label.width ?? 0, height: label.height ?? 0, text: label.text }
          : null,
    });
  }
  return { width: result.width ?? 0, height: result.height ?? 0, nodes, edges };
}

export async function layoutGraph(graph: SchematicGraph, elk: ElkLayout): Promise<SchematicLayout> {
  return fromElk(await elk(toElk(graph)));
}

/** Um traço ortogonal com os cantos arredondados. */
export function roundedPath(points: Point[], radius = 4): string {
  if (points.length < 2) return '';
  const first = points[0]!;
  let d = `M${first.x},${first.y}`;
  for (let i = 1; i < points.length - 1; i++) {
    const prev = points[i - 1]!;
    const at = points[i]!;
    const next = points[i + 1]!;
    const inLength = Math.hypot(at.x - prev.x, at.y - prev.y);
    const outLength = Math.hypot(next.x - at.x, next.y - at.y);
    const r = Math.min(radius, inLength / 2, outLength / 2);
    if (r < 0.5) {
      d += `L${at.x},${at.y}`;
      continue;
    }
    const ax = at.x - ((at.x - prev.x) / inLength) * r;
    const ay = at.y - ((at.y - prev.y) / inLength) * r;
    const bx = at.x + ((next.x - at.x) / outLength) * r;
    const by = at.y + ((next.y - at.y) / outLength) * r;
    d += `L${ax},${ay}Q${at.x},${at.y} ${bx},${by}`;
  }
  const last = points[points.length - 1]!;
  return `${d}L${last.x},${last.y}`;
}
