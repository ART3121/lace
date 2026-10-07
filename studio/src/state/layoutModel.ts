// O modelo do layout da janela: as vistas, as regiões onde elas ficam, as
// barras e o que está escondido. Sem React e sem store: a store da janela
// (layout.ts), os layouts salvos (savedLayouts.ts) e as Preferências usam as
// mesmas funções.
//
// Três regiões fixas, como no VS Code: a barra lateral esquerda, a direita e
// o painel (embaixo ou à direita do editor). Cada uma das onze vistas fica em
// exatamente uma delas e pode ir para qualquer outra. O editor e as abas de
// vista (Preferências, Wave, Esquemático) ficam no centro, fora disso.
//
// O layout guarda o que está escondido, não o que aparece: uma vista ou um
// item que uma versão futura acrescentar aparece sozinho, e uma vista que não
// está em região nenhuma vai para a região padrão dela.

import type { Key } from '../i18n';

export const CONSOLE_CHANNELS = ['cmm', 'asm', 'verilog', 'wave', 'prism'] as const;
export type ConsoleChannel = (typeof CONSOLE_CHANNELS)[number];

export const VIEW_IDS = ['explorer', 'flow', 'search', 'reports', ...CONSOLE_CHANNELS, 'problems', 'terminal'] as const;
export type ViewId = (typeof VIEW_IDS)[number];

export const REGION_IDS = ['left', 'right', 'panel'] as const;
export type RegionId = (typeof REGION_IDS)[number];

export type PanelPosition = 'bottom' | 'right';
export type ActivityBarSide = 'left' | 'right' | 'hidden';

export interface ViewInfo {
  label: Key;
  /** Onde a vista fica quando o layout não diz. */
  region: RegionId;
  /** A ação que revela a vista (o atalho dela aparece nas dicas). */
  action: string;
}

export const VIEW_INFO: Record<ViewId, ViewInfo> = {
  explorer: { label: 'sidebar.explorer', region: 'left', action: 'viewExplorer' },
  flow: { label: 'sidebar.flow', region: 'left', action: 'viewFlow' },
  search: { label: 'sidebar.search', region: 'left', action: 'findInFiles' },
  reports: { label: 'sidebar.reports', region: 'left', action: 'viewReports' },
  cmm: { label: 'panel.cmm', region: 'panel', action: 'viewConsoleCmm' },
  asm: { label: 'panel.asm', region: 'panel', action: 'viewConsoleAsm' },
  verilog: { label: 'panel.verilog', region: 'panel', action: 'viewConsoleVerilog' },
  wave: { label: 'panel.wave', region: 'panel', action: 'viewConsoleWave' },
  prism: { label: 'panel.prism', region: 'panel', action: 'viewConsolePrism' },
  problems: { label: 'panel.problems', region: 'panel', action: 'showProblems' },
  terminal: { label: 'panel.terminal', region: 'panel', action: 'toggleTerminal' },
};

export const REGION_LABELS: Record<RegionId, Key> = {
  left: 'layout.region.left',
  right: 'layout.region.right',
  panel: 'layout.region.panel',
};

/** Os itens da barra de ferramentas que podem ser escondidos, na ordem dela.
 * `group` junta os botões que ficam lado a lado, entre separadores. */
export const TOOLBAR_ITEMS: { id: string; label: Key; group: 'file' | 'target' | 'flow' | 'running' }[] = [
  { id: 'newProject', label: 'action.newProject', group: 'file' },
  { id: 'openProject', label: 'action.openProject', group: 'file' },
  { id: 'save', label: 'action.save', group: 'file' },
  { id: 'target', label: 'toolbar.target', group: 'target' },
  // Os botões do fluxo com o nome que aparece neles (C±, Verilog, Wave...).
  { id: 'build', label: 'toolbar.build', group: 'flow' },
  { id: 'check', label: 'toolbar.check', group: 'flow' },
  { id: 'simulate', label: 'toolbar.simulate', group: 'flow' },
  { id: 'fastSim', label: 'toolbar.fastSim', group: 'flow' },
  { id: 'openWave', label: 'toolbar.openWave', group: 'flow' },
  { id: 'synthesize', label: 'toolbar.synthesize', group: 'flow' },
  { id: 'running', label: 'layout.item.running', group: 'running' },
];

/** Os itens da barra de status que podem ser escondidos. A espera de um
 * atalho de duas etapas e a linha do Vim não estão aqui: aparecem sempre. */
export const STATUS_ITEMS: { id: string; label: Key }[] = [
  { id: 'project', label: 'layout.item.project' },
  { id: 'top', label: 'layout.item.top' },
  { id: 'testbench', label: 'layout.item.testbench' },
  { id: 'target', label: 'toolbar.target' },
  { id: 'operation', label: 'layout.item.operation' },
  { id: 'problems', label: 'panel.problems' },
  { id: 'cursor', label: 'layout.item.cursor' },
  { id: 'language', label: 'layout.item.language' },
  { id: 'simulator', label: 'layout.item.simulator' },
  { id: 'bundle', label: 'layout.item.bundle' },
  { id: 'layout', label: 'layout.item.layout' },
];

export interface RegionLayout {
  /** As vistas da região, na ordem das abas ou dos ícones. Uma vista
   * escondida continua aqui: ao reaparecer, volta ao mesmo lugar. */
  views: ViewId[];
  active: ViewId | null;
  visible: boolean;
}

export interface LayoutSizes {
  /** Larguras e alturas em pixels CSS (com o zoom, como o resto). */
  left: number;
  right: number;
  panelBottom: number;
  panelRight: number;
}

export interface LayoutBars {
  menubar: boolean;
  toolbar: boolean;
  statusbar: boolean;
  activitybar: ActivityBarSide;
}

export interface LayoutBody {
  regions: Record<RegionId, RegionLayout>;
  panelPosition: PanelPosition;
  sizes: LayoutSizes;
  bars: LayoutBars;
  hidden: { views: ViewId[]; toolbar: string[]; statusbar: string[] };
}

/** A versão do formato. Um layout gravado por uma versão mais nova do Studio
 * é lido como der e fica só para leitura, como os prontos. */
export const LAYOUT_VERSION = 1;

export interface LayoutDef extends LayoutBody {
  v: number;
  id: string;
  name: string;
}

export const DEFAULT_SIZES: LayoutSizes = { left: 290, right: 290, panelBottom: 260, panelRight: 420 };

/** Os limites só barram valores absurdos; os mínimos de verdade são os dos
 * painéis (Workbench.tsx). */
const SIZE_MIN = 60;
const SIZE_MAX = 4000;

/** O Padrão: exatamente a janela de antes dos layouts. */
export const DEFAULT_BODY: LayoutBody = {
  regions: {
    left: { views: ['explorer', 'flow', 'search', 'reports'], active: 'explorer', visible: true },
    right: { views: [], active: null, visible: false },
    panel: { views: [...CONSOLE_CHANNELS, 'problems', 'terminal'], active: 'cmm', visible: true },
  },
  panelPosition: 'bottom',
  sizes: DEFAULT_SIZES,
  bars: { menubar: true, toolbar: true, statusbar: true, activitybar: 'left' },
  hidden: { views: [], toolbar: [], statusbar: [] },
};

export interface Preset {
  id: string;
  /** O nome é traduzido: o layout pronto troca de nome com o idioma. */
  nameKey: Key;
  body: LayoutBody;
}

export const DEFAULT_LAYOUT_ID = 'default';

export const PRESETS: Preset[] = [{ id: DEFAULT_LAYOUT_ID, nameKey: 'layout.preset.default', body: DEFAULT_BODY }];

// Validação ----------------------------------------------------------------

type Loose = Record<string, unknown>;

function isObject(value: unknown): value is Loose {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function isViewId(value: unknown): value is ViewId {
  return typeof value === 'string' && (VIEW_IDS as readonly string[]).includes(value);
}

export function isRegionId(value: unknown): value is RegionId {
  return typeof value === 'string' && (REGION_IDS as readonly string[]).includes(value);
}

function uniqueStrings(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return [...new Set(value.filter((item): item is string => typeof item === 'string'))];
}

export function clampSize(value: unknown, fallback: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
  return Math.round(Math.min(SIZE_MAX, Math.max(SIZE_MIN, value)));
}

/** A primeira vista da região que não está escondida. */
function firstShown(views: ViewId[], hidden: ViewId[]): ViewId | null {
  return views.find((view) => !hidden.includes(view)) ?? null;
}

/** Acerta a vista ativa de cada região: uma que está nela e não está
 * escondida, ou nenhuma. */
function settleActive(body: LayoutBody): LayoutBody {
  let regions = body.regions;
  for (const id of REGION_IDS) {
    const region = regions[id];
    const ok = region.active !== null && region.views.includes(region.active) && !body.hidden.views.includes(region.active);
    if (ok) continue;
    regions = { ...regions, [id]: { ...region, active: firstShown(region.views, body.hidden.views) } };
  }
  return regions === body.regions ? body : { ...body, regions };
}

/**
 * Um layout válido a partir do que veio do disco (ou de qualquer lugar).
 * O que falta ou não serve fica com o valor de `fallback`; uma vista que
 * aparece em duas regiões fica na primeira (esquerda, direita, painel); uma
 * que não está em nenhuma vai para a região padrão dela. Os itens
 * escondidos das barras guardam também ids desconhecidos, para um layout
 * de uma versão mais nova voltar inteiro.
 */
export function normalizeBody(raw: unknown, fallback: LayoutBody = DEFAULT_BODY): LayoutBody {
  const source = isObject(raw) ? raw : {};
  const regionsRaw = isObject(source.regions) ? source.regions : null;
  const seen = new Set<ViewId>();
  const regions = {} as Record<RegionId, RegionLayout>;
  for (const id of REGION_IDS) {
    const region = regionsRaw && isObject(regionsRaw[id]) ? regionsRaw[id] : null;
    const views: ViewId[] = [];
    const listed = region ? region.views : regionsRaw ? [] : fallback.regions[id].views;
    if (Array.isArray(listed)) {
      for (const view of listed) {
        if (isViewId(view) && !seen.has(view)) {
          seen.add(view);
          views.push(view);
        }
      }
    }
    regions[id] = {
      views,
      active: region && isViewId(region.active) ? region.active : fallback.regions[id].active,
      visible: region && typeof region.visible === 'boolean' ? region.visible : fallback.regions[id].visible,
    };
  }
  for (const view of VIEW_IDS) {
    if (!seen.has(view)) regions[VIEW_INFO[view].region].views.push(view);
  }

  const hiddenRaw = isObject(source.hidden) ? source.hidden : {};
  const sizesRaw = isObject(source.sizes) ? source.sizes : {};
  const barsRaw = isObject(source.bars) ? source.bars : {};
  const bool = (value: unknown, otherwise: boolean) => (typeof value === 'boolean' ? value : otherwise);

  return settleActive({
    regions,
    panelPosition: source.panelPosition === 'bottom' || source.panelPosition === 'right' ? source.panelPosition : fallback.panelPosition,
    sizes: {
      left: clampSize(sizesRaw.left, fallback.sizes.left),
      right: clampSize(sizesRaw.right, fallback.sizes.right),
      panelBottom: clampSize(sizesRaw.panelBottom, fallback.sizes.panelBottom),
      panelRight: clampSize(sizesRaw.panelRight, fallback.sizes.panelRight),
    },
    bars: {
      menubar: bool(barsRaw.menubar, fallback.bars.menubar),
      toolbar: bool(barsRaw.toolbar, fallback.bars.toolbar),
      statusbar: bool(barsRaw.statusbar, fallback.bars.statusbar),
      activitybar:
        barsRaw.activitybar === 'left' || barsRaw.activitybar === 'right' || barsRaw.activitybar === 'hidden'
          ? barsRaw.activitybar
          : fallback.bars.activitybar,
    },
    hidden: {
      views: uniqueStrings(hiddenRaw.views).filter(isViewId),
      toolbar: uniqueStrings(hiddenRaw.toolbar),
      statusbar: uniqueStrings(hiddenRaw.statusbar),
    },
  });
}

/** Um layout salvo, validado; `null` sem id ou sem nome. `newer`: gravado
 * por uma versão mais nova do formato. */
export function parseLayoutDef(raw: unknown): (LayoutDef & { newer: boolean }) | null {
  if (!isObject(raw)) return null;
  const { id, name } = raw;
  if (typeof id !== 'string' || !id.trim() || typeof name !== 'string' || !name.trim()) return null;
  const v = typeof raw.v === 'number' && Number.isFinite(raw.v) ? raw.v : 1;
  return { ...normalizeBody(raw), v, id, name: name.trim(), newer: v > LAYOUT_VERSION };
}

/** Só o corpo de um layout salvo, para comparar ou aplicar. */
export function bodyOf(def: LayoutBody): LayoutBody {
  const { regions, panelPosition, sizes, bars, hidden } = def;
  return { regions, panelPosition, sizes, bars, hidden };
}

// Consultas e mudanças -------------------------------------------------------

export function regionOf(body: LayoutBody, view: ViewId): RegionId {
  return REGION_IDS.find((id) => body.regions[id].views.includes(view)) ?? VIEW_INFO[view].region;
}

/** As vistas da região que aparecem, na ordem. */
export function visibleViews(body: LayoutBody, region: RegionId): ViewId[] {
  return body.regions[region].views.filter((view) => !body.hidden.views.includes(view));
}

export function withRegion(body: LayoutBody, region: RegionId, change: Partial<RegionLayout>): LayoutBody {
  return { ...body, regions: { ...body.regions, [region]: { ...body.regions[region], ...change } } };
}

/** Leva a vista para `to`, na posição `index` (no fim, sem ela), e a deixa
 * ativa lá. Na região de onde saiu, a ativa passa à primeira que sobrou. */
export function moveView(body: LayoutBody, view: ViewId, to: RegionId, index?: number): LayoutBody {
  const from = regionOf(body, view);
  const regions = { ...body.regions };
  regions[from] = { ...regions[from], views: regions[from].views.filter((v) => v !== view) };
  const target = regions[to].views.filter((v) => v !== view);
  const at = index === undefined ? target.length : Math.max(0, Math.min(target.length, index));
  target.splice(at, 0, view);
  regions[to] = { ...regions[to], views: target, active: body.hidden.views.includes(view) ? regions[to].active : view };
  return settleActive({ ...body, regions });
}

/** Esconde ou mostra uma vista. A ativa escondida passa à próxima. */
export function setViewHidden(body: LayoutBody, view: ViewId, hidden: boolean): LayoutBody {
  const views = body.hidden.views.filter((v) => v !== view);
  if (hidden) views.push(view);
  return settleActive({ ...body, hidden: { ...body.hidden, views } });
}

export function setItemHidden(list: string[], id: string, hidden: boolean): string[] {
  const rest = list.filter((item) => item !== id);
  return hidden ? [...rest, id] : rest;
}

/** A barra de ferramentas e a de status: o item aparece? */
export function itemShown(body: LayoutBody, bar: 'toolbar' | 'statusbar', id: string): boolean {
  return !body.hidden[bar].includes(id);
}

const sameList = <T>(a: readonly T[], b: readonly T[]) => a.length === b.length && a.every((item, i) => item === b[i]);
const sameSet = (a: readonly string[], b: readonly string[]) => a.length === b.length && a.every((item) => b.includes(item));

/**
 * O mesmo arranjo? Entram as vistas e a ordem delas em cada região, o que
 * está escondido, as barras, a posição do painel, a visibilidade das regiões
 * e os tamanhos, com folga de `tolerancePx`. A vista ativa de cada região
 * não entra: trocar de aba é navegar, não mudar o layout.
 */
export function sameBody(a: LayoutBody, b: LayoutBody, tolerancePx = 16): boolean {
  for (const id of REGION_IDS) {
    if (!sameList(a.regions[id].views, b.regions[id].views)) return false;
    if (a.regions[id].visible !== b.regions[id].visible) return false;
  }
  if (a.panelPosition !== b.panelPosition) return false;
  const bars = Object.keys(a.bars) as (keyof LayoutBars)[];
  if (bars.some((bar) => a.bars[bar] !== b.bars[bar])) return false;
  if (!sameSet(a.hidden.views, b.hidden.views)) return false;
  if (!sameSet(a.hidden.toolbar, b.hidden.toolbar)) return false;
  if (!sameSet(a.hidden.statusbar, b.hidden.statusbar)) return false;
  const sizes = Object.keys(a.sizes) as (keyof LayoutSizes)[];
  return sizes.every((size) => Math.abs(a.sizes[size] - b.sizes[size]) <= tolerancePx);
}
