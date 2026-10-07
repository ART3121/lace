// O que a dica e o painel de detalhes dizem de um nó ou de uma rede.

import { t } from '../i18n';
import { baseName } from '../util/paths';
import { typeLabel } from './cells';
import type { PortRef, SchematicGraph, SchematicNet, SchematicNode } from './graph';
import { parameterValue, type SourceRef } from './yosys';

export function bitsText(bits: number): string {
  return bits === 1 ? t('schematic.bit') : t('schematic.bits', { count: bits });
}

export function sourceText(src: SourceRef): string {
  return `${baseName(src.file)}:${src.line}`;
}

/** A largura que importa numa célula: a da saída. */
export function nodeWidth(node: SchematicNode): number | null {
  const outputs = node.ports.filter((port) => port.output);
  if (outputs.length === 1) return outputs[0]!.bits;
  if (node.kind === 'port') return node.ports[0]?.bits ?? null;
  return null;
}

/** O tipo do nó numa linha: "add", "módulo fir_tap", "entrada", "constante". */
export function nodeKind(node: SchematicNode): string {
  switch (node.kind) {
    case 'port':
      return t(`schematic.kind.${node.shape.kind as 'in' | 'out' | 'inout'}`);
    case 'split':
      return t('schematic.kind.split');
    case 'join':
      return t('schematic.kind.join');
    default:
      if (node.category === 'module') return t('schematic.kind.instance', { module: node.subtitle ?? node.title });
      return typeLabel(node.type ?? '');
  }
}

/** O nome do nó para listas: o nome dado, ou o tipo. */
export function nodeName(node: SchematicNode): string {
  if (node.name) return node.name;
  return nodeKind(node);
}

export function netName(net: SchematicNet): string {
  return net.name ?? net.internal ?? t('schematic.unnamed');
}

/** A ponta de uma rede: "acc.Y" ou "x". */
export function endpointText(graph: SchematicGraph, ref: PortRef): string {
  const node = graph.nodes.find((n) => n.id === ref.node);
  if (!node) return '?';
  if (node.kind === 'port') return nodeName(node);
  if (node.kind === 'split' || node.kind === 'join') return nodeKind(node);
  const port = node.ports.find((p) => p.id === ref.port);
  return `${nodeName(node)}.${port?.name ?? '?'}`;
}

/** Os parâmetros de uma célula primitiva, em decimal quando são números. */
export function cellParameters(node: SchematicNode): [string, string][] {
  return Object.entries(node.parameters).map(([key, value]) => [key, parameterValue(value)]);
}
