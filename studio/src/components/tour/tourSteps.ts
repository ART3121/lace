// Os passos do tour pela interface (Tour.tsx), na ordem da janela. Cada
// passo acende uma parte da janela, achada pelas classes que ela já tem; um
// passo cuja parte não está na tela quando o tour começa (a barra de
// ferramentas escondida, o painel fechado) sai da lista.
//
// Um passo com tarefa pede uma ação na parte acesa e avança sozinho quando
// `done` fica verdadeiro. Se a tarefa não dá para fazer agora (`can`) ou já
// está feita quando o passo começa, ele vira um passo comum, com o Próximo.

import {
  Columns3,
  Command,
  Compass,
  FileCode,
  FolderTree,
  Hammer,
  Keyboard,
  LayoutGrid,
  Menu,
  PanelBottom,
  Rocket,
  SquareTerminal,
  Workflow,
  type LucideIcon,
} from 'lucide-react';

import { t, type Key } from '../../i18n';
import { useDialogs } from '../../state/dialogs';
import { isOnScreen, useLayout, type ViewId } from '../../state/layout';

/** O que a tarefa de um passo lembra entre um quadro e outro (a paleta que
 * já abriu, por exemplo). Começa vazio a cada passo. */
export type TaskMemo = Record<string, boolean>;

export interface TourStep {
  id: string;
  icon: LucideIcon;
  /** O passo vale nesta janela. Sem `shown`, vale sempre. */
  shown?: () => boolean;
  /** O que fica aceso. Sem ele (ou sem o que ele acha), o cartão fica no
   * meio da janela, com tudo escurecido. */
  target?: () => Element | null;
  title: Key;
  body: () => Key;
  /** A tarefa, dita no cartão. */
  task?: Key;
  /** A tarefa dá para fazer agora. Sem `can`, dá sempre. */
  can?: () => boolean;
  done?: (memo: TaskMemo) => boolean;
}

/** O elemento, se estiver na tela (com tamanho). */
function onScreen(selector: string): Element | null {
  const element = document.querySelector(selector);
  if (!element) return null;
  const rect = element.getBoundingClientRect();
  return rect.width > 0 && rect.height > 0 ? element : null;
}

const live = () => useLayout.getState().live;
const showing = (view: ViewId) => isOnScreen(useLayout.getState(), view);
const panelVisible = () => live().regions.panel.visible;

const TOOLBAR_TARGETS = { classic: '.toolbar', titlebar: '.run--titlebar', editor: '.run--editor' } as const;

/** O Navegador de fluxo: a barra lateral onde ele está aberto ou, se não
 * estiver, o ícone dele na barra de atividades. */
function flowTarget(): Element | null {
  for (const region of ['left', 'right'] as const) {
    if (onScreen(`.region--${region} [data-view="flow"]`)) return onScreen(`.region--${region}`);
  }
  return onScreen('.activitybar [data-view-tab="flow"]');
}

/** O menu Fluxo está aberto. */
function flowMenuOpen(): boolean {
  const label = t('menu.flow');
  return [...document.querySelectorAll('.menubar__item.is-open')].some((item) => item.textContent === label);
}

/** Abriu e fechou de novo: o menu Fluxo, a paleta. */
function openedAndClosed(memo: TaskMemo, open: boolean): boolean {
  if (open) memo.opened = true;
  return !!memo.opened && !open;
}

export const TOUR_STEPS: TourStep[] = [
  { id: 'intro', icon: Compass, title: 'tour.intro.title', body: () => 'tour.intro.body' },
  {
    id: 'menus',
    icon: Menu,
    shown: () => !!onScreen('.menubar__menus'),
    target: () => onScreen('.menubar__menus'),
    title: 'tour.menus.title',
    body: () => 'tour.menus.body',
    task: 'tour.menus.task',
    done: (memo) => openedAndClosed(memo, flowMenuOpen()),
  },
  {
    id: 'toolbar',
    icon: Hammer,
    // No modelo do editor, sem aba aberta, o Executar não está na tela: o
    // cartão explica no meio da janela.
    shown: () => live().bars.toolbar,
    target: () => onScreen(TOOLBAR_TARGETS[live().bars.toolbarStyle]),
    title: 'tour.toolbar.title',
    body: () => `tour.toolbar.${live().bars.toolbarStyle}`,
  },
  {
    id: 'activity',
    icon: LayoutGrid,
    shown: () => !!onScreen('.activitybar'),
    target: () => onScreen('.activitybar'),
    title: 'tour.activity.title',
    body: () => 'tour.activity.body',
    task: 'tour.activity.task',
    can: () => !!onScreen('.activitybar [data-view-tab="flow"]'),
    done: () => showing('flow'),
  },
  { id: 'flow', icon: Workflow, shown: () => !!flowTarget(), target: flowTarget, title: 'tour.flow.title', body: () => 'tour.flow.body' },
  {
    id: 'explorer',
    icon: FolderTree,
    shown: () => !!onScreen('.activitybar'),
    target: () => onScreen('.activitybar'),
    title: 'tour.explorer.title',
    body: () => 'tour.explorer.body',
    task: 'tour.explorer.task',
    can: () => !!onScreen('.activitybar [data-view-tab="explorer"]'),
    done: () => showing('explorer'),
  },
  {
    id: 'sidebar',
    icon: FolderTree,
    shown: () => !!onScreen('.region--left'),
    target: () => onScreen('.region--left'),
    title: 'tour.sidebar.title',
    body: () => 'tour.sidebar.body',
  },
  {
    id: 'editor',
    icon: FileCode,
    shown: () => !!onScreen('.editor-area'),
    target: () => onScreen('.editor-area'),
    title: 'tour.editor.title',
    body: () => 'tour.editor.body',
  },
  {
    id: 'panel',
    icon: SquareTerminal,
    shown: () => !!onScreen('.region--panel'),
    target: () => onScreen('.region--panel'),
    title: 'tour.panel.title',
    body: () => 'tour.panel.body',
    task: 'tour.panel.task',
    can: () => !!onScreen('.region--panel [data-view-tab="problems"]'),
    done: () => showing('problems'),
  },
  {
    id: 'regions',
    icon: Columns3,
    shown: () => !!onScreen('.statusbar__regions'),
    target: () => onScreen('.statusbar__regions'),
    title: 'tour.regions.title',
    body: () => 'tour.regions.body',
    task: 'tour.regions.task',
    can: panelVisible,
    done: () => !panelVisible(),
  },
  {
    id: 'keys',
    icon: Keyboard,
    shown: () => !!onScreen('.statusbar__regions'),
    target: () => onScreen('.statusbar__regions'),
    title: 'tour.keys.title',
    body: () => 'tour.keys.body',
    task: 'tour.keys.task',
    done: panelVisible,
  },
  {
    id: 'status',
    icon: PanelBottom,
    shown: () => !!onScreen('.statusbar'),
    target: () => onScreen('.statusbar'),
    title: 'tour.status.title',
    body: () => 'tour.status.body',
  },
  {
    id: 'palette',
    icon: Command,
    title: 'tour.palette.title',
    body: () => 'tour.palette.body',
    task: 'tour.palette.task',
    done: (memo) => openedAndClosed(memo, useDialogs.getState().dialog?.kind === 'palette'),
  },
  { id: 'done', icon: Rocket, title: 'tour.done.title', body: () => 'tour.done.body' },
];
