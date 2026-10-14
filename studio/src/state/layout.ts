// A janela como está agora: em que região fica cada vista, o que aparece, os
// tamanhos, a vista ativa de cada região, o modo do explorador, o zoom e o
// modo zen. O arranjo segue o modelo de layoutModel.ts; os layouts salvos,
// fotos dele com nome, ficam em savedLayouts.ts.
//
// Guardado no localStorage do aplicativo, por conveniência: sem ele, a
// janela abre no layout em uso. O zen não é guardado: o Studio sempre abre
// fora dele, e o que se grava durante o zen é a janela de antes dele.

import { create } from 'zustand';

import { useExperimental } from './experimental';
import {
  clampSize,
  DEFAULT_BODY,
  isConsoleChannel,
  isViewAvailable,
  isViewId,
  moveView as moveViewIn,
  normalizeBody,
  REGION_IDS,
  regionOf,
  setEnabledFeatures,
  setItemHidden as setItemHiddenIn,
  settleActive,
  setViewHidden as setViewHiddenIn,
  visibleViews,
  withRegion,
  type LayoutBars,
  type LayoutBody,
  type LayoutSizes,
  type PanelPosition,
  type RegionId,
  type ViewId,
} from './layoutModel';

export type { ConsoleChannel, ConsoleId, RegionId, ViewId } from './layoutModel';
export type ExplorerMode = 'sources' | 'hierarchy' | 'files';

/** O que o zen (e uma aba larga) esconde e devolve ao sair. */
interface ZenSaved {
  visible: Record<RegionId, boolean>;
  panelMaximized: boolean;
}

const ALL_HIDDEN: Record<RegionId, boolean> = { left: false, right: false, panel: false };

function savedRegions(live: LayoutBody, panelMaximized: boolean): ZenSaved {
  return {
    visible: { left: live.regions.left.visible, right: live.regions.right.visible, panel: live.regions.panel.visible },
    panelMaximized,
  };
}

function withVisible(body: LayoutBody, visible: Record<RegionId, boolean>): LayoutBody {
  let out = body;
  for (const id of REGION_IDS) out = withRegion(out, id, { visible: visible[id] });
  return out;
}

interface LayoutState {
  /** O arranjo da janela agora. */
  live: LayoutBody;
  panelMaximized: boolean;
  explorerMode: ExplorerMode;
  showHidden: boolean;
  /** Vistas com saída nova desde que foram vistas. */
  unread: Partial<Record<ViewId, boolean>>;
  zoom: number;
  /** Modo zen: só o editor na tela (App.tsx esconde as barras). */
  zen: boolean;
  /** As regiões de antes do zen. Dentro do zen, Ctrl+B e Ctrl+J ainda
   * mostram as regiões; ao sair, vale o que está aqui. */
  zenSaved: ZenSaved | null;
  /** As regiões de antes de uma aba larga (as Preferências, o RTLViewer)
   * ficar na frente (state/wideTabs.ts). Enquanto ela está ali, as três
   * regiões ficam escondidas; Ctrl+B e Ctrl+J ainda as mostram, e ao sair
   * vale o que está aqui, como no zen. */
  wideSaved: ZenSaved | null;
  /** A gaveta do terminal de shell, que só existe no zen. */
  zenShell: boolean;
  /** Muda a cada layout aplicado: a área de trabalho põe os tamanhos de novo. */
  epoch: number;
  /** A vista que o usuário acabou de pedir. Só ela pega o foco ao montar
   * (o terminal, a busca); uma vista que aparece porque um layout foi
   * aplicado não tira o foco do editor. */
  focusView: ViewId | null;

  /** Mostra a vista: a região dela aparece com ela na frente. Uma vista
   * escondida só volta com `explicit` (o usuário pediu por ela); sem isso,
   * devolve `false` e nada muda. Uma de recurso experimental desligado
   * nunca aparece. */
  revealView: (view: ViewId, options?: { explicit?: boolean }) => boolean;
  /** Um recurso experimental ligou ou desligou: as regiões se desenham de
   * novo sem as vistas que deixaram de existir (ou com as que voltaram), e
   * a ativa de cada uma se acerta. O layout não muda. */
  refreshViews: () => void;
  /** O clique num ícone da barra de atividades: mostra a vista ou, se ela
   * já está na frente, esconde a região, como no VS Code. */
  toggleView: (view: ViewId) => void;
  toggleRegion: (region: RegionId) => void;
  setRegionVisible: (region: RegionId, visible: boolean) => void;
  /** A visibilidade da região no layout (as chaves de Preferências > Layout
   * da janela): com uma aba larga na frente, muda a que volta ao sair dela,
   * sem mostrar a região agora. */
  setLayoutRegionVisible: (region: RegionId, visible: boolean) => void;
  /** Uma aba larga ficou na frente: esconde as regiões. No zen, nada muda. */
  enterWide: () => void;
  /** Nenhuma aba larga na frente: as regiões voltam como estavam. */
  exitWide: () => void;
  setPanelMaximized: (value: boolean) => void;
  moveView: (view: ViewId, to: RegionId, index?: number) => void;
  setViewHidden: (view: ViewId, hidden: boolean) => void;
  setPanelPosition: (position: PanelPosition) => void;
  setBar: <K extends keyof LayoutBars>(bar: K, value: LayoutBars[K]) => void;
  setItemHidden: (bar: 'toolbar' | 'statusbar', id: string, hidden: boolean) => void;
  /** Tamanhos medidos depois de o usuário arrastar uma divisão. */
  setSizes: (sizes: Partial<LayoutSizes>) => void;
  /** Troca a janela inteira por um layout (sai do zen antes). */
  applyBody: (body: LayoutBody) => void;
  /** A vista montou: pega o foco se foi ela que o usuário pediu. */
  takeFocus: (view: ViewId) => boolean;
  setExplorerMode: (mode: ExplorerMode) => void;
  setShowHidden: (value: boolean) => void;
  markUnread: (view: ViewId) => void;
  setZoom: (zoom: number) => void;
  toggleZen: () => void;
  exitZen: () => void;
  toggleZenShell: () => void;
}

const STORAGE_KEY = 'lace-studio:layout';
const STORAGE_VERSION = 2;

interface Stored {
  v: typeof STORAGE_VERSION;
  live: LayoutBody;
  explorerMode: ExplorerMode;
  showHidden: boolean;
  zoom: number;
}

interface Loaded {
  live: LayoutBody | null;
  explorerMode?: ExplorerMode;
  showHidden?: boolean;
  zoom?: number;
}

/** A versão 1 (antes dos layouts) guardava só a vista da barra lateral e a
 * aba do painel; elas viram o Padrão com a mesma vista e a mesma aba. */
function migrate(old: Record<string, unknown>): LayoutBody {
  let body = DEFAULT_BODY;
  if (isViewId(old.sidebarView)) body = withRegion(body, 'left', { active: old.sidebarView });
  if (typeof old.sidebarVisible === 'boolean') body = withRegion(body, 'left', { visible: old.sidebarVisible });
  // O console "Lace" saiu do painel, e os de etapa ficam fechados: quem
  // deixou um deles na frente passa ao INFO, que mostra tudo.
  const tab = old.panelTab === 'lace' || isConsoleChannel(old.panelTab) ? 'info' : old.panelTab;
  if (isViewId(tab)) body = withRegion(body, 'panel', { active: tab });
  if (typeof old.panelVisible === 'boolean') body = withRegion(body, 'panel', { visible: old.panelVisible });
  return normalizeBody(body);
}

function load(): Loaded {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { live: null };
    const saved = JSON.parse(raw) as Record<string, unknown>;
    const explorerMode =
      saved.explorerMode === 'sources' || saved.explorerMode === 'hierarchy' || saved.explorerMode === 'files'
        ? saved.explorerMode
        : undefined;
    return {
      live: saved.v === STORAGE_VERSION ? normalizeBody(saved.live) : migrate(saved),
      explorerMode,
      showHidden: typeof saved.showHidden === 'boolean' ? saved.showHidden : undefined,
      zoom: typeof saved.zoom === 'number' && Number.isFinite(saved.zoom) ? saved.zoom : undefined,
    };
  } catch {
    return { live: null };
  }
}

const saved = load();
let freshStart = saved.live === null;

/** Não havia janela guardada: a abertura aplica o layout em uso (App.tsx),
 * em vez de ficar no Padrão. Vale uma vez; depois disso, a janela já é a
 * que o usuário arrumou. */
export function takeFreshStart(): boolean {
  const fresh = freshStart;
  freshStart = false;
  return fresh;
}

/** A vista está na tela: não escondida, com a região dela visível e na
 * frente dela. */
export function isOnScreen(state: Pick<LayoutState, 'live'>, view: ViewId): boolean {
  const { live } = state;
  if (live.hidden.views.includes(view) || !isViewAvailable(view)) return false;
  const region = live.regions[regionOf(live, view)];
  return region.visible && region.active === view;
}

/** O arranjo sem o zen e sem a aba larga: as regiões visíveis são as de
 * antes deles. É o que se grava e o que se compara com o layout salvo. A aba
 * larga só esconde fora do zen, então as regiões dela, quando há, são as de
 * antes dos dois. */
export function effectiveLive(state: Pick<LayoutState, 'live' | 'zenSaved' | 'wideSaved'>): LayoutBody {
  const saved = state.wideSaved ?? state.zenSaved;
  return saved ? withVisible(state.live, saved.visible) : state.live;
}

/** As marcas de não lido sem as das vistas que estão na tela. */
function withoutOnScreen(unread: LayoutState['unread'], live: LayoutBody): LayoutState['unread'] {
  const next = { ...unread };
  for (const view of Object.keys(next) as ViewId[]) if (isOnScreen({ live }, view)) delete next[view];
  return next;
}

let focusTimer = 0;

export const useLayout = create<LayoutState>((set, get) => ({
  live: saved.live ?? DEFAULT_BODY,
  panelMaximized: false,
  explorerMode: saved.explorerMode ?? 'sources',
  showHidden: saved.showHidden ?? false,
  unread: {},
  zoom: saved.zoom ?? 1,
  zen: false,
  zenSaved: null,
  wideSaved: null,
  zenShell: false,
  epoch: 0,
  focusView: null,

  revealView: (view, { explicit = false } = {}) => {
    if (!isViewAvailable(view)) return false;
    const state = get();
    let live = state.live;
    if (live.hidden.views.includes(view)) {
      if (!explicit) return false;
      live = setViewHiddenIn(live, view, false);
    }
    live = withRegion(live, regionOf(live, view), { active: view, visible: true });
    const unread = { ...state.unread };
    delete unread[view];
    set({ live, unread, focusView: explicit ? view : state.focusView });
    if (explicit) {
      // O pedido de foco vale para a montagem que vem logo depois; uma vista
      // que já estava montada não monta de novo, e o pedido não pode ficar
      // pendurado para uma montagem qualquer mais tarde.
      window.clearTimeout(focusTimer);
      focusTimer = window.setTimeout(() => get().focusView === view && set({ focusView: null }), 1000);
    }
    return true;
  },
  refreshViews: () => {
    // Um objeto novo mesmo sem mudança na ativa: quem desenha as regiões lê
    // `live`, e a lista de vistas à mostra mudou.
    set({ live: { ...settleActive(get().live) } });
  },
  toggleView: (view) => {
    const { live } = get();
    const region = regionOf(live, view);
    if (isOnScreen(get(), view)) get().setRegionVisible(region, false);
    else get().revealView(view, { explicit: true });
  },
  toggleRegion: (region) => get().setRegionVisible(region, !get().live.regions[region].visible),
  setRegionVisible: (region, visible) => {
    const state = get();
    const live = withRegion(state.live, region, { visible });
    set({
      live,
      unread: withoutOnScreen(state.unread, live),
      ...(region === 'panel' ? { panelMaximized: false } : {}),
    });
  },
  setLayoutRegionVisible: (region, visible) => {
    const { wideSaved } = get();
    if (wideSaved) set({ wideSaved: { ...wideSaved, visible: { ...wideSaved.visible, [region]: visible } } });
    else get().setRegionVisible(region, visible);
  },
  enterWide: () => {
    const state = get();
    if (state.wideSaved || state.zen) return;
    set({ wideSaved: savedRegions(state.live, state.panelMaximized), live: withVisible(state.live, ALL_HIDDEN), panelMaximized: false });
  },
  exitWide: () => {
    const state = get();
    const saved = state.wideSaved;
    if (!saved) return;
    if (state.zen) {
      // O zen entrou depois e guardou as regiões escondidas: ao sair dele,
      // voltam as de antes da aba larga.
      set({ wideSaved: null, zenSaved: saved });
      return;
    }
    const live = withVisible(state.live, saved.visible);
    set({ wideSaved: null, live, panelMaximized: saved.panelMaximized, unread: withoutOnScreen(state.unread, live) });
  },
  setPanelMaximized: (value) => {
    const live = withRegion(get().live, 'panel', { visible: true });
    set({ panelMaximized: value, live });
  },
  moveView: (view, to, index) => {
    const state = get();
    const from = regionOf(state.live, view);
    let live = moveViewIn(state.live, view, to, index);
    if (from !== to) {
      // A vista aparece onde chegou; a região de onde saiu, se ficou sem
      // nenhuma vista à mostra, some.
      if (!live.hidden.views.includes(view)) live = withRegion(live, to, { visible: true });
      if (visibleViews(live, from).length === 0) live = withRegion(live, from, { visible: false });
    }
    set({ live, unread: withoutOnScreen(state.unread, live) });
  },
  setViewHidden: (view, hidden) => {
    const state = get();
    let live = setViewHiddenIn(state.live, view, hidden);
    const region = regionOf(live, view);
    if (hidden && visibleViews(live, region).length === 0) live = withRegion(live, region, { visible: false });
    set({ live, unread: withoutOnScreen(state.unread, live) });
  },
  setPanelPosition: (position) => set({ live: { ...get().live, panelPosition: position } }),
  setBar: (bar, value) => {
    const live = get().live;
    set({ live: { ...live, bars: { ...live.bars, [bar]: value } } });
  },
  setItemHidden: (bar, id, hidden) => {
    const live = get().live;
    set({ live: { ...live, hidden: { ...live.hidden, [bar]: setItemHiddenIn(live.hidden[bar], id, hidden) } } });
  },
  setSizes: (sizes) => {
    const live = get().live;
    const next = { ...live.sizes };
    let changed = false;
    for (const key of Object.keys(sizes) as (keyof LayoutSizes)[]) {
      const value = sizes[key];
      // Zero é um painel recolhido ou ainda sem tamanho, não um tamanho.
      if (value === undefined || value <= 0) continue;
      const size = clampSize(value, live.sizes[key]);
      if (Math.abs(size - live.sizes[key]) < 2) continue;
      next[key] = size;
      changed = true;
    }
    if (changed) set({ live: { ...live, sizes: next } });
  },
  applyBody: (body) => {
    const state = get();
    const normalized = normalizeBody(body);
    // Com uma aba larga na frente, as regiões do layout novo aparecem ao
    // sair dela.
    const wideSaved = state.wideSaved ? savedRegions(normalized, false) : null;
    const live = wideSaved ? withVisible(normalized, ALL_HIDDEN) : normalized;
    set({
      live,
      wideSaved,
      zen: false,
      zenSaved: null,
      zenShell: false,
      panelMaximized: false,
      unread: withoutOnScreen(state.unread, live),
      epoch: state.epoch + 1,
    });
  },
  takeFocus: (view) => {
    if (get().focusView !== view) return false;
    set({ focusView: null });
    return true;
  },
  setExplorerMode: (mode) => set({ explorerMode: mode }),
  setShowHidden: (value) => set({ showHidden: value }),
  markUnread: (view) => {
    const state = get();
    if (!isOnScreen(state, view) && !state.unread[view]) set({ unread: { ...state.unread, [view]: true } });
  },
  setZoom: (zoom) => set({ zoom: Math.min(2, Math.max(0.6, Math.round(zoom * 10) / 10)) }),
  toggleZen: () => {
    const { zen, live, panelMaximized } = get();
    if (zen) {
      get().exitZen();
      return;
    }
    set({
      zen: true,
      zenSaved: savedRegions(live, panelMaximized),
      zenShell: false,
      live: withVisible(live, ALL_HIDDEN),
      panelMaximized: false,
    });
  },
  exitZen: () => {
    const state = get();
    if (!state.zen || !state.zenSaved) return;
    // Só as regiões do zen: com uma aba larga na frente, elas continuam
    // escondidas até sair dela.
    const live = withVisible(state.live, state.zenSaved.visible);
    set({
      zen: false,
      zenSaved: null,
      zenShell: false,
      live,
      panelMaximized: state.zenSaved.panelMaximized,
      unread: withoutOnScreen(state.unread, live),
    });
  },
  toggleZenShell: () => set({ zenShell: get().zen && !get().zenShell }),
}));

// As vistas dos recursos experimentais (o console Placa) aparecem e somem
// com eles (state/experimental.ts).
useExperimental.subscribe((state, previous) => {
  if (state.enabled === previous.enabled) return;
  setEnabledFeatures(state.enabled);
  useLayout.getState().refreshViews();
});

useLayout.subscribe((state, previous) => {
  if (
    state.live === previous.live &&
    state.zenSaved === previous.zenSaved &&
    state.wideSaved === previous.wideSaved &&
    state.explorerMode === previous.explorerMode &&
    state.showHidden === previous.showHidden &&
    state.zoom === previous.zoom
  ) {
    return;
  }
  // No zen e com uma aba larga na frente, guarda o layout de antes deles:
  // fechar o Studio assim não deixa as regiões escondidas na próxima
  // abertura.
  const stored: Stored = {
    v: STORAGE_VERSION,
    live: effectiveLive(state),
    explorerMode: state.explorerMode,
    showHidden: state.showHidden,
    zoom: state.zoom,
  };
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(stored));
  } catch {
    // Sem armazenamento, a janela só não é lembrada.
  }
});
