// O grafo de um módulo do netlist, pronto para o layout: os nós (portas do
// módulo, células, constantes e as barras de fatia), as redes e as arestas.
//
// O Yosys liga tudo bit a bit. Para desenhar barramentos, cada porta que
// recebe (entrada de célula, saída do módulo) tem os bits partidos em
// trechos contíguos de uma mesma origem:
//
// | Trecho                                  | Vira                                  |
// |-----------------------------------------|---------------------------------------|
// | a porta de origem inteira               | um fio direto                         |
// | parte da porta de origem                | um fio saindo de um `split` da origem |
// | constantes                              | o valor escrito na porta, sem fio     |
// | bits que ninguém dirige                 | um "z" escrito na porta, sem fio      |
// | mais de um trecho                       | um `join` com um fio por trecho       |
//
// Cada rede é o que sai de uma porta de origem (saída de célula, entrada do
// módulo, saída de split ou join), com todos os destinos dela.
//
// Constante não é nó: um nó a mais na camada de antes empurra o vizinho
// para alinhar o fio, e numa cadeia (os 32 taps do fir, cada um com a ROM de
// coeficiente) o desenho descia em diagonal.

import {
  boxGeometry,
  boxTitle,
  cellCategory,
  cellShape,
  gateGeometry,
  muxGeometry,
  opGeometry,
  portNodeGeometry,
  regGeometry,
  sliceGeometry,
  type CellPorts,
  type Category,
  type Geometry,
  type PortGeometry,
  type Shape,
} from './cells';
import {
  constantText,
  isBlackbox,
  moduleDisplayName,
  parseSource,
  plainName,
  type Bit,
  type Direction,
  type Netlist,
  type SourceRef,
  type YosysCell,
  type YosysModule,
} from './yosys';

export type NodeKind = 'port' | 'cell' | 'split' | 'join';

export interface SchematicPort extends PortGeometry {
  /** Único no grafo: `<nó>:<índice>`. */
  id: string;
  bits: number;
  /** O que chega aqui sem fio, escrito ao lado da porta. */
  tag?: PortTag;
}

/** Uma etiqueta de porta: o rótulo de uma rede global (GraphOptions.flags)
 * ou uma constante. */
export type PortTag =
  | { kind: 'flag'; net: string; text: string }
  | { kind: 'const'; text: string; full: string; undriven: boolean };

export interface SchematicNode {
  id: string;
  kind: NodeKind;
  category: Category;
  shape: Shape;
  /** O texto principal: nome da instância, da porta ou da caixa. */
  title: string;
  /** O tipo do módulo de uma instância, ou o tipo de uma caixa. */
  subtitle: string | null;
  /** O nome da célula ou da porta no netlist. */
  key: string | null;
  /** O tipo da célula no netlist (`$add`, `$paramod...`). */
  type: string | null;
  /** O nome que o usuário deu (instância ou porta); `null` nas células com
   * nome automático do Yosys. */
  name: string | null;
  /** O módulo de uma instância, quando dá para entrar nele. */
  module: string | null;
  width: number;
  height: number;
  ports: SchematicPort[];
  src: SourceRef[];
  parameters: Record<string, string>;
}

export interface PortRef {
  node: string;
  port: string;
}

export interface SchematicNet {
  id: string;
  /** O nome público (`hide_name` 0), ou `null`. */
  name: string | null;
  /** O nome interno do Yosys, quando não há público. */
  internal: string | null;
  bits: number;
  driver: PortRef;
  loads: PortRef[];
  src: SourceRef[];
}

export interface SchematicEdge {
  id: string;
  net: string;
  source: PortRef;
  target: PortRef;
  bits: number;
  /** O nome da rede, em uma só aresta por rede. */
  label: string | null;
}

export interface SchematicGraph {
  module: string;
  nodes: SchematicNode[];
  nets: SchematicNet[];
  edges: SchematicEdge[];
}

export interface GraphOptions {
  /** O nome das redes nos fios. */
  netNames: boolean;
  /** As redes globais (entrada do módulo com muitos destinos) como rótulo
   * em cada destino, sem fio. */
  flags: boolean;
}

/** Quantos destinos uma entrada do módulo precisa ter para virar rótulo. */
export const FLAG_FANOUT = 8;

/** O teto de caracteres de uma constante no desenho; a inteira fica na dica
 * da etiqueta e nos detalhes. */
const CONST_CHARS = 18;

interface Terminal {
  node: SchematicNode;
  port: SchematicPort;
  bits: Bit[];
}

type Segment =
  | { kind: 'driver'; terminal: Terminal; lo: number; hi: number; repeat: number; bits: Bit[] }
  | { kind: 'const'; bits: Bit[] }
  | { kind: 'undriven'; bits: Bit[] };

/** As direções das portas de uma célula: as que o Yosys gravou, as do
 * módulo instanciado, ou entrada. */
function directions(netlist: Netlist, cell: YosysCell): Record<string, Direction> {
  const known = cell.port_directions ?? {};
  const definition = netlist.modules[cell.type]?.ports ?? {};
  const result: Record<string, Direction> = {};
  for (const name of Object.keys(cell.connections)) {
    result[name] = known[name] ?? definition[name]?.direction ?? 'input';
  }
  return result;
}

/** As portas da célula na ordem do desenho: a da declaração do módulo
 * instanciado, ou a das conexões. */
function cellPorts(netlist: Netlist, cell: YosysCell, dirs: Record<string, Direction>): CellPorts {
  const declared = Object.keys(netlist.modules[cell.type]?.ports ?? {});
  const names = [
    ...declared.filter((name) => name in cell.connections),
    ...Object.keys(cell.connections).filter((name) => !declared.includes(name)),
  ];
  return {
    inputs: names.filter((name) => dirs[name] !== 'output'),
    outputs: names.filter((name) => dirs[name] === 'output'),
  };
}

function bitsKey(bits: Bit[]): string {
  return bits.join(',');
}

function sliceText(lo: number, hi: number, repeat = 1): string {
  const range = lo === hi ? `${lo}` : `${hi}:${lo}`;
  return repeat > 1 ? `${range}×${repeat}` : range;
}

export function buildGraph(netlist: Netlist, moduleName: string, options: GraphOptions): SchematicGraph {
  const module: YosysModule | undefined = netlist.modules[moduleName];
  if (!module) throw new Error(`module ${moduleName} is not in the netlist`);

  const nodes: SchematicNode[] = [];
  const nets = new Map<string, SchematicNet>();
  const edges: SchematicEdge[] = [];
  const drivers: Terminal[] = [];
  const sinks: Terminal[] = [];

  // Os nomes das redes, pelos bits. Um nome público vence um interno; entre
  // dois do mesmo tipo, o mais curto.
  const publicNames = new Map<string, string>();
  const internalNames = new Map<string, string>();
  const netSources = new Map<string, SourceRef[]>();
  for (const [name, net] of Object.entries(module.netnames ?? {})) {
    const key = bitsKey(net.bits);
    const table = net.hide_name ? internalNames : publicNames;
    const current = table.get(key);
    if (current === undefined || name.length < current.length) table.set(key, plainName(name));
    if (!net.hide_name || !netSources.has(key)) netSources.set(key, parseSource(net.attributes?.src));
  }

  let counter = 0;
  const addNode = (
    base: Omit<SchematicNode, 'id' | 'ports' | 'width' | 'height'>,
    geometry: Geometry,
    widths: Record<string, number>,
  ): SchematicNode => {
    const id = `n${counter++}`;
    const node: SchematicNode = {
      ...base,
      id,
      width: geometry.width,
      height: geometry.height,
      ports: geometry.ports.map((port, i) => ({ ...port, id: `${id}:${i}`, bits: widths[port.name] ?? 1 })),
    };
    nodes.push(node);
    return node;
  };
  const portOf = (node: SchematicNode, name: string) => node.ports.find((port) => port.name === name)!;

  // Portas do módulo: as entradas (e bidirecionais) dirigem, as saídas recebem.
  const outputs: [string, Bit[]][] = [];
  for (const [name, port] of Object.entries(module.ports ?? {})) {
    if (port.direction === 'output') {
      outputs.push([name, port.bits]);
      continue;
    }
    const kind = port.direction === 'inout' ? 'inout' : 'in';
    const node = addNode(
      {
        kind: 'port',
        category: 'port',
        shape: { kind },
        title: plainName(name),
        subtitle: null,
        key: name,
        type: null,
        name: plainName(name),
        module: null,
        src: netSources.get(bitsKey(port.bits)) ?? [],
        parameters: {},
      },
      portNodeGeometry(kind, plainName(name)),
      { [plainName(name)]: port.bits.length },
    );
    drivers.push({ node, port: node.ports[0]!, bits: port.bits });
  }

  // Células.
  for (const [cellName, cell] of Object.entries(module.cells ?? {})) {
    if (!Object.keys(cell.connections).length) continue;
    const dirs = directions(netlist, cell);
    const ports = cellPorts(netlist, cell, dirs);
    const definition = netlist.modules[cell.type];
    const isModule = !!definition;
    const category = cellCategory(cell.type, isModule);
    const shape: Shape = isModule ? { kind: 'box' } : cellShape(cell.type, ports, cell.parameters);
    const name = cell.hide_name ? null : plainName(cellName);
    const typeName = isModule ? moduleDisplayName(cell.type, definition) : boxTitle(cell.type);
    let title = name ?? typeName;
    let subtitle: string | null = name ? typeName : null;
    if (category === 'mem') {
      // A memória do `$memrd`/`$memwr` é o MEMID; o título é a operação.
      title = typeName;
      const memid = cell.parameters?.MEMID;
      subtitle = memid ? plainName(memid.trimEnd()) : null;
      if (subtitle?.startsWith('$')) subtitle = null;
    }
    let geometry: Geometry;
    switch (shape.kind) {
      case 'gate':
        geometry = gateGeometry(shape, ports.inputs.length === 1);
        break;
      case 'op':
        geometry = opGeometry(shape.glyph, ports.inputs.length === 1);
        break;
      case 'mux':
        geometry = muxGeometry(shape.parallel);
        break;
      case 'reg':
        geometry = regGeometry(ports);
        break;
      default:
        geometry = boxGeometry(title, subtitle, ports, isModule && !isBlackbox(definition));
    }
    const widths = Object.fromEntries(Object.entries(cell.connections).map(([port, bits]) => [port, bits.length]));
    const node = addNode(
      {
        kind: 'cell',
        category,
        shape,
        title,
        subtitle,
        key: cellName,
        type: cell.type,
        name,
        module: isModule && !isBlackbox(definition) ? cell.type : null,
        src: parseSource(cell.attributes?.src),
        parameters: cell.parameters ?? {},
      },
      geometry,
      widths,
    );
    for (const [port, bits] of Object.entries(cell.connections)) {
      const terminal = { node, port: portOf(node, port), bits };
      if (dirs[port] === 'output') drivers.push(terminal);
      else sinks.push(terminal);
    }
  }

  for (const [name, bits] of outputs) {
    const node = addNode(
      {
        kind: 'port',
        category: 'port',
        shape: { kind: 'out' },
        title: plainName(name),
        subtitle: null,
        key: name,
        type: null,
        name: plainName(name),
        module: null,
        src: netSources.get(bitsKey(bits)) ?? [],
        parameters: {},
      },
      portNodeGeometry('out', plainName(name)),
      { [plainName(name)]: bits.length },
    );
    sinks.push({ node, port: node.ports[0]!, bits });
  }

  // Quem dirige cada bit (o primeiro, se houver conflito).
  const bitDriver = new Map<number, { terminal: Terminal; index: number }>();
  for (const terminal of drivers) {
    terminal.bits.forEach((bit, index) => {
      if (typeof bit === 'number' && !bitDriver.has(bit)) bitDriver.set(bit, { terminal, index });
    });
  }

  // Redes e arestas ---------------------------------------------------------

  const netFor = (source: PortRef, bits: Bit[], fallback: string | null): SchematicNet => {
    const id = `${source.node}/${source.port}`;
    let net = nets.get(id);
    if (!net) {
      const key = bitsKey(bits);
      const name = publicNames.get(key) ?? null;
      net = {
        id,
        name: name ?? fallback,
        internal: name ? null : internalNames.get(key) ?? null,
        bits: bits.length,
        driver: source,
        loads: [],
        src: netSources.get(key) ?? [],
      };
      nets.set(id, net);
    }
    return net;
  };

  const connect = (source: PortRef, bits: Bit[], target: PortRef, fallback: string | null = null) => {
    const net = netFor(source, bits, fallback);
    net.loads.push(target);
    edges.push({ id: `e${edges.length}`, net: net.id, source, target, bits: bits.length, label: null });
  };

  /** Escreve uma constante (ou bits sem driver, "z") na porta de destino. */
  const tie = (bits: Bit[], undriven: boolean, target: PortRef) => {
    const port = nodes.find((node) => node.id === target.node)?.ports.find((p) => p.id === target.port);
    if (!port) return;
    const full = undriven ? constantText(bits.map(() => 'z')) : constantText(bits);
    const text = full.length > CONST_CHARS ? `${full.slice(0, CONST_CHARS - 1)}…` : full;
    port.tag = { kind: 'const', text, full, undriven };
  };

  // Os splits são criados no fim, quando já se sabe que fatias cada origem
  // fornece; até lá a ligação de uma fatia fica guardada.
  const slices = new Map<Terminal, Map<string, { lo: number; hi: number }>>();
  const deferred: { terminal: Terminal; lo: number; hi: number; target: PortRef }[] = [];

  /** Liga um trecho a um destino: direto, por uma saída do split da origem
   * (adiado), ou escrevendo a constante na porta. */
  const link = (segment: Segment, target: PortRef) => {
    if (segment.kind !== 'driver') {
      tie(segment.bits, segment.kind === 'undriven', target);
      return;
    }
    const { terminal, lo, hi, repeat } = segment;
    if (lo === 0 && hi === terminal.bits.length - 1 && repeat === 1) {
      connect({ node: terminal.node.id, port: terminal.port.id }, terminal.bits, target);
      return;
    }
    let table = slices.get(terminal);
    if (!table) slices.set(terminal, (table = new Map()));
    table.set(`${lo}:${hi}`, { lo, hi });
    deferred.push({ terminal, lo, hi, target });
  };

  const segmentsOf = (bits: Bit[]): Segment[] => {
    const segments: Segment[] = [];
    for (const bit of bits) {
      const last = segments[segments.length - 1];
      if (typeof bit === 'string') {
        if (last?.kind === 'const') last.bits.push(bit);
        else segments.push({ kind: 'const', bits: [bit] });
        continue;
      }
      const driver = bitDriver.get(bit);
      if (!driver) {
        if (last?.kind === 'undriven') last.bits.push(bit);
        else segments.push({ kind: 'undriven', bits: [bit] });
        continue;
      }
      if (last?.kind === 'driver' && last.terminal === driver.terminal) {
        if (last.repeat === 1 && driver.index === last.hi + 1) {
          last.hi += 1;
          last.bits.push(bit);
          continue;
        }
        if (last.lo === last.hi && driver.index === last.hi) {
          last.repeat += 1;
          last.bits.push(bit);
          continue;
        }
      }
      segments.push({ kind: 'driver', terminal: driver.terminal, lo: driver.index, hi: driver.index, repeat: 1, bits: [bit] });
    }
    return segments;
  };

  const wiring = (kind: 'split' | 'join', labels: string[]) =>
    addNode(
      {
        kind,
        category: 'wiring',
        shape: { kind },
        title: '',
        subtitle: null,
        key: null,
        type: null,
        name: null,
        module: null,
        src: [],
        parameters: {},
      },
      sliceGeometry(kind, labels),
      {},
    );

  for (const sink of sinks) {
    if (!sink.bits.length) continue;
    const target = { node: sink.node.id, port: sink.port.id };
    const segments = segmentsOf(sink.bits);
    if (segments.length === 1) {
      link(segments[0]!, target);
      continue;
    }
    // Um join, com o MSB em cima: a entrada de baixo é o trecho do LSB.
    let offset = 0;
    const labels = segments.map((segment) => {
      const label = sliceText(offset, offset + segment.bits.length - 1);
      offset += segment.bits.length;
      return label;
    });
    const join = wiring('join', [...labels].reverse());
    const inputs = join.ports.filter((port) => !port.output).reverse();
    const output = join.ports.find((port) => port.output)!;
    output.bits = sink.bits.length;
    connect({ node: join.id, port: output.id }, sink.bits, target);
    segments.forEach((segment, i) => {
      inputs[i]!.bits = segment.bits.length;
      link(segment, { node: join.id, port: inputs[i]!.id });
    });
  }

  // Os splits, com a fatia mais alta em cima, e as ligações que esperavam
  // por eles. Uma fatia sem nome próprio se chama `origem[hi:lo]`.
  const splitPorts = new Map<string, PortRef>();
  for (const [terminal, table] of slices) {
    const list = [...table.values()].sort((a, b) => b.hi - a.hi || b.lo - a.lo);
    const split = wiring(
      'split',
      list.map((slice) => sliceText(slice.lo, slice.hi)),
    );
    const input = split.ports.find((port) => !port.output)!;
    input.bits = terminal.bits.length;
    connect({ node: terminal.node.id, port: terminal.port.id }, terminal.bits, { node: split.id, port: input.id });
    const outputs = split.ports.filter((port) => port.output);
    list.forEach((slice, i) => {
      outputs[i]!.bits = slice.hi - slice.lo + 1;
      splitPorts.set(`${terminal.port.id}/${slice.lo}:${slice.hi}`, { node: split.id, port: outputs[i]!.id });
    });
  }
  for (const { terminal, lo, hi, target } of deferred) {
    const source = splitPorts.get(`${terminal.port.id}/${lo}:${hi}`)!;
    const origin = nets.get(`${terminal.node.id}/${terminal.port.id}`)?.name;
    connect(source, terminal.bits.slice(lo, hi + 1), target, origin ? `${origin}[${sliceText(lo, hi)}]` : null);
  }

  // As redes globais: os fios somem, e cada porta de destino leva um
  // rótulo com o nome da rede. A rede continua a mesma (destaque, detalhes);
  // a porta do módulo fica sem fio. O rótulo não é nó do layout: um nó na
  // camada de antes empurraria o vizinho, e uma cadeia (os 32 taps do fir)
  // descia em diagonal.
  if (options.flags) {
    const byId = new Map(nodes.map((node) => [node.id, node]));
    const flagged = new Set<string>();
    const touched = new Set<SchematicNode>();
    for (const net of nets.values()) {
      if (byId.get(net.driver.node)?.kind !== 'port' || net.loads.length < FLAG_FANOUT || !net.name) continue;
      flagged.add(net.id);
      for (const load of net.loads) {
        const node = byId.get(load.node);
        const port = node?.ports.find((p) => p.id === load.port);
        if (!node || !port) continue;
        port.tag = { kind: 'flag', net: net.id, text: net.name };
        touched.add(node);
      }
    }
    if (flagged.size) {
      const kept = edges.filter((edge) => !flagged.has(edge.net));
      edges.length = 0;
      edges.push(...kept);
    }
    // Nas caixas, as entradas globais descem para baixo das outras, como o
    // relógio de um registrador: a entrada de dados fica perto da saída, e
    // uma cadeia de instâncias sai quase reta.
    for (const node of touched) {
      if (node.shape.kind !== 'box') continue;
      const inputs = node.ports.filter((port) => !port.output);
      const global = (port: SchematicPort) => port.tag?.kind === 'flag';
      const ordered = [...inputs.filter((port) => !global(port)), ...inputs.filter(global)];
      const outputs = node.ports.filter((port) => port.output).map((port) => port.name);
      const geometry = boxGeometry(node.title, node.subtitle, { inputs: ordered.map((port) => port.name), outputs }, !!node.module);
      for (const port of node.ports) {
        const placed = geometry.ports.find((p) => p.name === port.name && p.output === port.output);
        if (placed) port.y = placed.y;
      }
    }
  }

  // Um rótulo por rede com nome, na primeira aresta dela, quando o nome não
  // está escrito numa ponta: a porta do módulo, ou a porta da caixa de onde
  // a rede sai (a saída `instr_addr` do core ligada à rede `instr_addr`).
  if (options.netNames) {
    const byId = new Map(nodes.map((node) => [node.id, node]));
    const portName = (ref: PortRef) => byId.get(ref.node)?.ports.find((port) => port.id === ref.port)?.label;
    const labelled = new Set<string>();
    for (const edge of edges) {
      const net = nets.get(edge.net)!;
      if (!net.name || labelled.has(net.id)) continue;
      labelled.add(net.id);
      const from = byId.get(edge.source.node)!;
      if (from.kind !== 'cell') continue;
      if (from.shape.kind === 'box' && portName(net.driver) === net.name) continue;
      const named = net.loads.every((load) => {
        const node = byId.get(load.node)!;
        return node.kind === 'port' ? node.name === net.name : node.shape.kind === 'box' && portName(load) === net.name;
      });
      if (named) continue;
      edge.label = net.name;
    }
  }

  return { module: moduleName, nodes, nets: [...nets.values()], edges };
}

/** Os nós ligados a um nó, e as redes que os ligam, para o destaque. */
export function netsOfNode(graph: SchematicGraph, nodeId: string): Set<string> {
  const result = new Set<string>();
  for (const net of graph.nets) {
    if (net.driver.node === nodeId || net.loads.some((load) => load.node === nodeId)) result.add(net.id);
  }
  return result;
}

/**
 * Uma rede e as que continuam ela pelos splits e joins (que são só fiação):
 * destacar um barramento acende também as fatias dele.
 */
export function netClosure(graph: SchematicGraph, netId: string): Set<string> {
  const wiring = new Set(graph.nodes.filter((node) => node.kind === 'split' || node.kind === 'join').map((node) => node.id));
  const byDriver = new Map<string, SchematicNet[]>();
  for (const net of graph.nets) {
    const list = byDriver.get(net.driver.node) ?? [];
    list.push(net);
    byDriver.set(net.driver.node, list);
  }
  const byId = new Map(graph.nets.map((net) => [net.id, net]));
  const result = new Set<string>();
  const queue = [netId];
  while (queue.length) {
    const id = queue.pop()!;
    if (result.has(id)) continue;
    result.add(id);
    for (const load of byId.get(id)?.loads ?? []) {
      if (!wiring.has(load.node)) continue;
      for (const next of byDriver.get(load.node) ?? []) queue.push(next.id);
    }
  }
  return result;
}
