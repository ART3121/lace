// O que está visível: a vista da barra lateral, o painel inferior e a aba
// dele, o modo do explorador e o modo zen. Guardado no localStorage do
// aplicativo, por conveniência: um valor perdido só volta ao padrão. O zen
// não é guardado: o Studio sempre abre fora dele.

import { create } from 'zustand';

export type SidebarView = 'explorer' | 'flow' | 'search' | 'reports';
export type ConsoleChannel = 'cmm' | 'asm' | 'verilog' | 'wave' | 'prism' | 'lace';
export type PanelTab = ConsoleChannel | 'problems' | 'terminal';
export type ExplorerMode = 'sources' | 'hierarchy' | 'files';

/** O que o zen esconde e devolve ao sair. */
interface ZenSaved {
  sidebarVisible: boolean;
  panelVisible: boolean;
  panelMaximized: boolean;
}

interface LayoutState {
  sidebarView: SidebarView;
  sidebarVisible: boolean;
  panelVisible: boolean;
  panelMaximized: boolean;
  panelTab: PanelTab;
  explorerMode: ExplorerMode;
  showHidden: boolean;
  /** Canais com saída nova desde que foram vistos. */
  unread: Partial<Record<PanelTab, boolean>>;
  zoom: number;
  /** Modo zen: só o editor na tela (App.tsx esconde as barras). */
  zen: boolean;
  /** O layout de antes do zen. Dentro do zen, Ctrl+B e Ctrl+J ainda mostram
   * a barra lateral e o painel; ao sair, vale o que está aqui. */
  zenSaved: ZenSaved | null;
  /** A gaveta do terminal de shell, que só existe no zen. */
  zenShell: boolean;

  showSidebar: (view: SidebarView) => void;
  toggleSidebar: () => void;
  showPanel: (tab?: PanelTab) => void;
  togglePanel: () => void;
  setPanelMaximized: (value: boolean) => void;
  setExplorerMode: (mode: ExplorerMode) => void;
  setShowHidden: (value: boolean) => void;
  markUnread: (tab: PanelTab) => void;
  setZoom: (zoom: number) => void;
  toggleZen: () => void;
  exitZen: () => void;
  toggleZenShell: () => void;
}

const STORAGE_KEY = 'lace-studio:layout';

type Persisted = Pick<
  LayoutState,
  'sidebarView' | 'sidebarVisible' | 'panelVisible' | 'panelTab' | 'explorerMode' | 'showHidden' | 'zoom'
>;

function load(): Partial<Persisted> {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}') as Partial<Persisted>;
  } catch {
    return {};
  }
}

const saved = load();

export const useLayout = create<LayoutState>((set, get) => ({
  sidebarView: saved.sidebarView ?? 'explorer',
  sidebarVisible: saved.sidebarVisible ?? true,
  panelVisible: saved.panelVisible ?? true,
  panelMaximized: false,
  panelTab: saved.panelTab ?? 'lace',
  explorerMode: saved.explorerMode ?? 'sources',
  showHidden: saved.showHidden ?? false,
  unread: {},
  zoom: saved.zoom ?? 1,
  zen: false,
  zenSaved: null,
  zenShell: false,

  showSidebar: (view) => {
    const { sidebarView, sidebarVisible } = get();
    // Clicar na vista que já está aberta fecha a barra, como no VS Code.
    if (view === sidebarView && sidebarVisible) set({ sidebarVisible: false });
    else set({ sidebarView: view, sidebarVisible: true });
  },
  toggleSidebar: () => set({ sidebarVisible: !get().sidebarVisible }),
  showPanel: (tab) => {
    const unread = { ...get().unread };
    const next = tab ?? get().panelTab;
    delete unread[next];
    set({ panelVisible: true, panelTab: next, unread });
  },
  togglePanel: () => set({ panelVisible: !get().panelVisible, panelMaximized: false }),
  setPanelMaximized: (value) => set({ panelMaximized: value, panelVisible: true }),
  setExplorerMode: (mode) => set({ explorerMode: mode }),
  setShowHidden: (value) => set({ showHidden: value }),
  markUnread: (tab) => {
    const { panelTab, panelVisible, unread } = get();
    if ((tab !== panelTab || !panelVisible) && !unread[tab]) set({ unread: { ...unread, [tab]: true } });
  },
  setZoom: (zoom) => set({ zoom: Math.min(2, Math.max(0.6, Math.round(zoom * 10) / 10)) }),
  toggleZen: () => {
    const { zen, sidebarVisible, panelVisible, panelMaximized } = get();
    if (zen) {
      get().exitZen();
      return;
    }
    set({
      zen: true,
      zenSaved: { sidebarVisible, panelVisible, panelMaximized },
      zenShell: false,
      sidebarVisible: false,
      panelVisible: false,
      panelMaximized: false,
    });
  },
  exitZen: () => {
    const { zen, zenSaved } = get();
    if (!zen) return;
    set({ zen: false, zenSaved: null, zenShell: false, ...zenSaved });
  },
  toggleZenShell: () => set({ zenShell: get().zen && !get().zenShell }),
}));

useLayout.subscribe((state) => {
  // No zen, guarda o layout de antes dele: fechar o Studio no zen não deixa
  // a barra lateral e o painel escondidos na próxima abertura.
  const visible = state.zenSaved ?? state;
  const persisted: Persisted = {
    sidebarView: state.sidebarView,
    sidebarVisible: visible.sidebarVisible,
    panelVisible: visible.panelVisible,
    panelTab: state.panelTab,
    explorerMode: state.explorerMode,
    showHidden: state.showHidden,
    zoom: state.zoom,
  };
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(persisted));
  } catch {
    // Sem armazenamento, o layout só não é lembrado.
  }
});
