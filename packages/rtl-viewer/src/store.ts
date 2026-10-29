// O estado do RTL viewer: o netlist que ele desenha, onde ele está na
// hierarquia e as preferências. Fica fora do componente porque a vista pode
// desmontar (no Studio, quando outra aba vem para a frente), e voltar a ela
// deve voltar ao mesmo lugar.
//
// O netlist é o `hierarchy.json` que o Yosys grava na síntese
// (SynthesisResult.netlist, no JSON do `aurora synth`). Quem o lê é quem
// abriga o viewer: o Studio pelo backend, o VS Code pela extensão, o HTML do
// `aurora synth --open` de dentro dele mesmo.

import { create, type StoreApi, type UseBoundStore } from 'zustand';

import { instancesOf, validPrefix } from './hierarchy';
import { parseNetlist, topModule, type Netlist } from './yosys';

export interface LoadedNetlist {
  path: string;
  modifiedMs: number;
  netlist: Netlist;
  top: string;
}

/** Como ler o netlist do disco, no ambiente de quem abriga. */
export interface NetlistReader {
  /** A data de modificação, ou `null` se o arquivo não existe. */
  modified(path: string): Promise<number | null>;
  read(path: string): Promise<{ text: string; modifiedMs: number }>;
}

export interface ViewerOptions {
  netNames: boolean;
  /** As redes globais como rótulo em cada destino (GraphOptions.flags). */
  flags: boolean;
  busWidths: boolean;
  showTree: boolean;
  showInspector: boolean;
}

export interface RtlViewerState extends ViewerOptions {
  source: LoadedNetlist | null;
  loading: boolean;
  error: string | null;
  /** As instâncias do topo até o módulo desenhado (`[]`: o topo). */
  path: string[];
  back: string[][];
  forward: string[][];

  /** Lê o netlist, se mudou desde a última leitura. O caminho na
   * hierarquia fica, até onde ele ainda existir. */
  load: (path: string, reader: NetlistReader) => Promise<void>;
  /** Mostra um netlist já lido (o texto do `hierarchy.json`). */
  show: (path: string, text: string, modifiedMs: number) => void;
  go: (path: string[]) => void;
  /** Entra numa instância do módulo desenhado. */
  enter: (key: string) => void;
  goBack: () => void;
  goForward: () => void;
  setOption: (option: keyof ViewerOptions, value: boolean) => void;
  clear: () => void;
}

export type RtlViewerStore = UseBoundStore<StoreApi<RtlViewerState>>;

const HISTORY = 50;
const DEFAULTS: ViewerOptions = { netNames: true, flags: true, busWidths: true, showTree: true, showInspector: true };

/**
 * Um estado novo. `storageKey` guarda as preferências no `localStorage`
 * (o Studio usa `lace-studio:rtlViewer`); sem ela, valem até fechar.
 */
export function createRtlViewerStore(options: { storageKey?: string } = {}): RtlViewerStore {
  const { storageKey } = options;
  const saved = (): ViewerOptions => {
    if (!storageKey) return DEFAULTS;
    try {
      return { ...DEFAULTS, ...(JSON.parse(localStorage.getItem(storageKey) ?? '{}') as Partial<ViewerOptions>) };
    } catch {
      return DEFAULTS;
    }
  };

  return create<RtlViewerState>((set, get) => {
    const accept = (path: string, text: string, modifiedMs: number) => {
      const current = get().source;
      const netlist = parseNetlist(text);
      const top = topModule(netlist);
      if (!top) throw new Error(`${path}: no module`);
      // Outro topo é outro design: a navegação recomeça.
      const same = current?.top === top;
      const keep = same ? validPrefix(netlist, top, get().path) : [];
      set({
        source: { path, modifiedMs, netlist, top },
        loading: false,
        error: null,
        path: keep,
        back: same ? get().back : [],
        forward: same ? get().forward : [],
      });
    };
    const fail = (error: unknown) =>
      set({
        loading: false,
        error: error instanceof Error ? error.message : String((error as { message?: string }).message ?? error),
      });

    return {
      ...saved(),
      source: null,
      loading: false,
      error: null,
      path: [],
      back: [],
      forward: [],

      load: async (path, reader) => {
        const current = get().source;
        set({ loading: true, error: null });
        try {
          const modified = await reader.modified(path);
          if (current && current.path === path && modified === current.modifiedMs) {
            set({ loading: false });
            return;
          }
          const file = await reader.read(path);
          accept(path, file.text, file.modifiedMs);
        } catch (error) {
          fail(error);
        }
      },

      show: (path, text, modifiedMs) => {
        try {
          accept(path, text, modifiedMs);
        } catch (error) {
          fail(error);
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
        set({ [option]: value } as Partial<ViewerOptions>);
        if (!storageKey) return;
        const { netNames, flags, busWidths, showTree, showInspector } = get();
        try {
          localStorage.setItem(storageKey, JSON.stringify({ netNames, flags, busWidths, showTree, showInspector }));
        } catch {
          // Sem localStorage, a preferência vale até fechar.
        }
      },

      clear: () => set({ source: null, error: null, path: [], back: [], forward: [] }),
    };
  });
}
