// O PRISM: o netlist que a vista desenha, onde ela está na hierarquia e as
// preferências dela. Fica fora do componente porque a vista desmonta quando
// outra aba vem para a frente, e voltar a ela deve voltar ao mesmo lugar.
//
// O netlist é o `hierarchy.json` da última síntese (SynthesisResult.netlist)
// ou, depois de reabrir o Studio, o que ficou em `.lace/Temp/synth/<alvo>/`.

import { create } from 'zustand';

import { api } from '../ipc/api';
import { instancesOf, validPrefix } from '../schematic/hierarchy';
import { parseNetlist, topModule, type Netlist } from '../schematic/yosys';

export interface LoadedNetlist {
  path: string;
  modifiedMs: number;
  netlist: Netlist;
  top: string;
}

interface Persisted {
  netNames: boolean;
  /** As redes globais como rótulo em cada destino (GraphOptions.flags). */
  flags: boolean;
  busWidths: boolean;
  showTree: boolean;
  showInspector: boolean;
}

interface SchematicState extends Persisted {
  source: LoadedNetlist | null;
  loading: boolean;
  error: string | null;
  /** As instâncias do topo até o módulo desenhado (`[]`: o topo). */
  path: string[];
  back: string[][];
  forward: string[][];

  /** Lê o netlist, se mudou desde a última leitura. O caminho na
   * hierarquia fica, até onde ele ainda existir. */
  load: (path: string) => Promise<void>;
  go: (path: string[]) => void;
  /** Entra numa instância do módulo desenhado. */
  enter: (key: string) => void;
  goBack: () => void;
  goForward: () => void;
  setOption: (option: keyof Persisted, value: boolean) => void;
  clear: () => void;
}

const STORAGE_KEY = 'lace-studio:prism';

function persisted(): Persisted {
  const defaults: Persisted = { netNames: true, flags: true, busWidths: true, showTree: true, showInspector: true };
  try {
    return { ...defaults, ...(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}') as Partial<Persisted>) };
  } catch {
    return defaults;
  }
}

const HISTORY = 50;

export const useSchematic = create<SchematicState>((set, get) => ({
  ...persisted(),
  source: null,
  loading: false,
  error: null,
  path: [],
  back: [],
  forward: [],

  load: async (path) => {
    const current = get().source;
    set({ loading: true, error: null });
    try {
      const stat = await api.fs.stat(path);
      if (current && current.path === path && stat && stat.modified_ms === current.modifiedMs) {
        set({ loading: false });
        return;
      }
      const file = await api.fs.readText(path);
      if (file.too_large) throw new Error(`${path}: too large`);
      const netlist = parseNetlist(file.content);
      const top = topModule(netlist);
      if (!top) throw new Error(`${path}: no module`);
      // Outro topo é outro design: a navegação recomeça.
      const same = current?.top === top;
      const keep = same ? validPrefix(netlist, top, get().path) : [];
      set({
        source: { path, modifiedMs: file.modified_ms, netlist, top },
        loading: false,
        path: keep,
        back: same ? get().back : [],
        forward: same ? get().forward : [],
      });
    } catch (error) {
      set({ loading: false, error: error instanceof Error ? error.message : String((error as { message?: string }).message ?? error) });
    }
  },

  go: (path) => {
    const { path: current, back } = get();
    if (current.join('\u0000') === path.join('\u0000')) return;
    set({ path, back: [...back, current].slice(-HISTORY), forward: [] });
  },

  enter: (key) => {
    const { source, path } = get();
    if (!source) return;
    const module = path.reduce<string | null>(
      (m, k) => (m ? source.netlist.modules[m]?.cells?.[k]?.type ?? null : null),
      source.top,
    );
    if (!module || !instancesOf(source.netlist, module).some((instance) => instance.key === key)) return;
    get().go([...path, key]);
  },

  goBack: () => {
    const { back, path, forward } = get();
    const previous = back[back.length - 1];
    if (!previous) {
      // Sem histórico, voltar sobe um nível, como o Esc da AURORA.
      if (path.length) set({ path: path.slice(0, -1), forward: [path, ...forward].slice(0, HISTORY) });
      return;
    }
    set({ path: previous, back: back.slice(0, -1), forward: [path, ...forward].slice(0, HISTORY) });
  },

  goForward: () => {
    const { back, path, forward } = get();
    const next = forward[0];
    if (!next) return;
    set({ path: next, back: [...back, path].slice(-HISTORY), forward: forward.slice(1) });
  },

  setOption: (option, value) => {
    set({ [option]: value } as Partial<Persisted>);
    const { netNames, flags, busWidths, showTree, showInspector } = get();
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({ netNames, flags, busWidths, showTree, showInspector }));
    } catch {
      // Sem localStorage, a preferência vale até fechar o Studio.
    }
  },

  clear: () => set({ source: null, error: null, path: [], back: [], forward: [] }),
}));
