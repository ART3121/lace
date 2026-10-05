// A barra lateral: o título da vista, os botões dela à direita do título
// (SidebarActions.tsx) e o conteúdo.

import { useState } from 'react';

import { useT, type Key } from '../../i18n';
import { useLayout, type SidebarView } from '../../state/layout';
import { useProject } from '../../state/project';
import { Explorer } from './Explorer';
import { FlowNavigator } from './FlowNavigator';
import { ReportsPanel } from './ReportsPanel';
import { SearchPanel } from './SearchPanel';
import { ActionsSlot } from './SidebarActions';

const TITLES: Record<SidebarView, Key> = {
  explorer: 'sidebar.explorer',
  flow: 'sidebar.flow',
  search: 'sidebar.search',
  reports: 'sidebar.reports',
};

export function SideBar() {
  const t = useT();
  const view = useLayout((s) => s.sidebarView);
  const project = useProject((s) => s.snapshot?.name);
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  return (
    <aside className="sidebar">
      <header className="sidebar__header">
        <span className="sidebar__title">{t(TITLES[view])}</span>
        {project && <span className="sidebar__project">{project}</span>}
        <div className="sidebar__actions" ref={setSlot} />
      </header>
      <ActionsSlot.Provider value={slot}>
        {view === 'explorer' && <Explorer />}
        {view === 'flow' && <FlowNavigator />}
        {view === 'search' && <SearchPanel />}
        {view === 'reports' && <ReportsPanel />}
      </ActionsSlot.Provider>
    </aside>
  );
}
