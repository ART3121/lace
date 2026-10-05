// A barra de atividades, à esquerda: as vistas da barra lateral em cima,
// ferramentas e preferências embaixo.

import { FolderTree, ScrollText, Search, Settings, Workflow, Wrench, type LucideIcon } from 'lucide-react';

import { useT, type Key } from '../../i18n';
import { useApp } from '../../state/app';
import { useEditor } from '../../state/editor';
import { useLayout, type SidebarView } from '../../state/layout';

const VIEWS: { id: SidebarView; icon: LucideIcon; label: Key; keys?: string }[] = [
  { id: 'explorer', icon: FolderTree, label: 'sidebar.explorer', keys: 'Ctrl+Shift+E' },
  { id: 'flow', icon: Workflow, label: 'sidebar.flow' },
  { id: 'search', icon: Search, label: 'sidebar.search', keys: 'Ctrl+Shift+F' },
  { id: 'reports', icon: ScrollText, label: 'sidebar.reports' },
];

export function ActivityBar() {
  const t = useT();
  const view = useLayout((s) => s.sidebarView);
  const visible = useLayout((s) => s.sidebarVisible);
  const toolchainMissing = useApp((s) => s.toolchain !== null && !s.toolchain.found);

  return (
    <nav className="activitybar">
      {VIEWS.map(({ id, icon: Icon, label, keys }) => (
        <button
          key={id}
          type="button"
          className={`activitybar__item${visible && view === id ? ' is-active' : ''}`}
          title={keys ? `${t(label)} (${keys})` : t(label)}
          aria-label={t(label)}
          onClick={() => useLayout.getState().showSidebar(id)}
        >
          <Icon size={20} strokeWidth={1.6} />
        </button>
      ))}
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
