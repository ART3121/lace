// O layout no Studio: o ELK roda num Web Worker (o `elk-worker.min.js` do
// pacote, servido como arquivo), e a interface continua respondendo enquanto
// ele trabalha. O maior módulo dos exemplos (o ula_fdiv do proc_fft, 192
// nós) leva 1,4 s.
//
// Cada desenho fica guardado pelo netlist (caminho e data) e pelo módulo:
// voltar a um módulo já visto não refaz o layout.

import ElkConstructor, { type ELK, type ElkNode } from 'elkjs/lib/elk-api.js';
import workerUrl from 'elkjs/lib/elk-worker.min.js?url';

import type { LoadedNetlist } from '../state/schematic';
import { buildGraph, type GraphOptions, type SchematicGraph } from './graph';
import { layoutGraph, type SchematicLayout } from './layout';

export interface Drawing {
  graph: SchematicGraph;
  layout: SchematicLayout;
  /** Quanto o layout levou, em milissegundos. */
  ms: number;
}

let elk: ELK | null = null;
const pending = new Set<(error: Error) => void>();

function run(graph: ElkNode): Promise<ElkNode> {
  elk ??= new ElkConstructor({ workerFactory: () => new Worker(workerUrl) });
  const engine = elk;
  return new Promise((resolve, reject) => {
    pending.add(reject);
    engine.layout(graph).then(
      (result) => {
        pending.delete(reject);
        resolve(result);
      },
      (error: unknown) => {
        pending.delete(reject);
        reject(error instanceof Error ? error : new Error(String(error)));
      },
    );
  });
}

/** Para os layouts em andamento: o worker é encerrado, e o próximo pedido
 * cria outro. */
export function cancelLayouts(): void {
  elk?.terminateWorker();
  elk = null;
  for (const reject of pending) reject(new Error('cancelled'));
  pending.clear();
}

const CACHE = 24;
const cache = new Map<string, Promise<Drawing>>();

export function drawModule(source: LoadedNetlist, module: string, options: GraphOptions): Promise<Drawing> {
  const key = `${source.path}|${source.modifiedMs}|${module}|${options.netNames}|${options.flags}`;
  const cached = cache.get(key);
  if (cached) {
    // O mais recente vai para o fim, e o mais antigo sai primeiro.
    cache.delete(key);
    cache.set(key, cached);
    return cached;
  }
  const promise = (async () => {
    const graph = buildGraph(source.netlist, module, options);
    const started = performance.now();
    const layout = await layoutGraph(graph, run);
    return { graph, layout, ms: Math.round(performance.now() - started) };
  })();
  promise.catch(() => cache.delete(key));
  cache.set(key, promise);
  while (cache.size > CACHE) cache.delete(cache.keys().next().value!);
  return promise;
}
