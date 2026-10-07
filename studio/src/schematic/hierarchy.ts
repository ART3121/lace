// A árvore de instâncias do netlist, a partir do topo. O PRISM navega por
// caminhos de instância (`['canal_i', 'tap[3].celula']`): o módulo desenhado
// é o da última instância, e o caminho dá a trilha no alto da vista.

import { isBlackbox, moduleDisplayName, plainName, type Netlist } from './yosys';

export interface Instance {
  /** O nome da célula no netlist (a chave em `cells`). */
  key: string;
  /** O nome para mostrar. */
  name: string;
  /** O módulo instanciado, como está no netlist. */
  module: string;
}

/** As instâncias de módulo do design dentro de um módulo, na ordem do
 * netlist. As caixas-pretas ficam de fora: não há o que desenhar nelas. */
export function instancesOf(netlist: Netlist, module: string): Instance[] {
  const cells = netlist.modules[module]?.cells ?? {};
  const result: Instance[] = [];
  for (const [key, cell] of Object.entries(cells)) {
    const definition = netlist.modules[cell.type];
    if (!definition || isBlackbox(definition)) continue;
    result.push({ key, name: plainName(key), module: cell.type });
  }
  return result;
}

/** O módulo no fim de um caminho; `null` se o caminho não existe mais (o
 * netlist mudou). */
export function moduleAt(netlist: Netlist, top: string, path: string[]): string | null {
  let module = top;
  for (const key of path) {
    const cell = netlist.modules[module]?.cells?.[key];
    if (!cell || !netlist.modules[cell.type]) return null;
    module = cell.type;
  }
  return module;
}

/** O caminho mais longo de `path` que ainda existe no netlist. */
export function validPrefix(netlist: Netlist, top: string, path: string[]): string[] {
  for (let length = path.length; length > 0; length--) {
    if (moduleAt(netlist, top, path.slice(0, length))) return path.slice(0, length);
  }
  return [];
}

export function moduleLabel(netlist: Netlist, module: string): string {
  return moduleDisplayName(module, netlist.modules[module]);
}
