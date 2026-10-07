// A barra de atividades, à esquerda ou à direita da janela: em cima, as
// vistas da barra lateral do mesmo lado (a que o layout pôs ali); embaixo,
// ferramentas e preferências. Clicar na vista que já está na frente esconde
// a barra lateral, como no VS Code.

import { Settings, Wrench } from 'lucide-react';

import { useT } from '../../i18n';
import { useApp } from '../../state/app';
import { useEditor } from '../../state/editor';
import { useLayout } from '../../state/layout';
import { VIEW_INFO, visibleViews } from '../../state/layoutModel';
import { openContextMenu } from '../common';
import { activityBarMenu, viewMenu } from './layoutMenus';
import { viewTitle } from './Region';
import { VIEW_ICONS, ViewBadge } from './viewCatalog';

export function ActivityBar({ side }: { side: 'left' | 'right' }) {
  const t = useT();
  const live = useLayout((s) => s.live);
  const toolchainMissing = useApp((s) => s.toolchain !== null && !s.toolchain.found);
  const region = live.regions[side];

  return (
    <nav className={`activitybar activitybar--${side}`} onContextMenu={(e) => openContextMenu(e, activityBarMenu(side))}>
      {visibleViews(live, side).map((view) => {
        const Icon = VIEW_ICONS[view];
        return (
          <button
            key={view}
            type="button"
            className={`activitybar__item${region.visible && region.active === view ? ' is-active' : ''}`}
            title={viewTitle(t, view)}
            aria-label={t(VIEW_INFO[view].label)}
            data-view-tab={view}
            onClick={() => useLayout.getState().toggleView(view)}
            onContextMenu={(e) => {
              e.stopPropagation();
              openContextMenu(e, viewMenu(view));
            }}
          >
            <Icon size={20} strokeWidth={1.6} />
            <ViewBadge view={view} compact />
          </button>
        );
      })}
      <div className="activitybar__spacer" />
      <button
        type="button"
        className="activitybar__item"
        title={t('sidebar.toolchain')}
        aria-label={t('sidebar.toolchain')}
        onClick={() => useEditor.getState().openView('toolchain')}
      >
        <Wrench size={19} strokeWidth={1.6} />
        {toolchainMissing && <span className="activitybar__alert" aria-hidden />}
      </button>
      <button
        type="button"
        className="activitybar__item"
        title={`${t('sidebar.settings')} (Ctrl+,)`}
        aria-label={t('sidebar.settings')}
        onClick={() => useEditor.getState().openView('settings')}
      >
        <Settings size={19} strokeWidth={1.6} />
      </button>
    </nav>
  );
}
