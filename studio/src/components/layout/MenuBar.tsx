// A barra de menus: Arquivo, Editar, Exibir, Projeto, Fluxo, Ferramentas,
// Ajuda. Cada item é uma ação de actions.ts; o atalho aparece ao lado, e a
// marca nas que ligam e desligam. O Exibir tem submenus (Consoles,
// Aparência): solto, ele não caberia na altura mínima da janela.

import { useEffect, useRef, useState } from 'react';

import { useT, type Key } from '../../i18n';
import { useApp } from '../../state/app';
import { useJobs } from '../../state/jobs';
import { useLayout } from '../../state/layout';
import { useProject } from '../../state/project';
import { LaceMark, MenuList, openContextMenu, type MenuItem } from '../common';
import { actionItem, APPEARANCE_ENTRIES, chromeMenu, LAYOUT_ENTRIES } from './layoutMenus';

/** Uma ação, um separador ou um submenu (um nível só). */
type Entry = string | { submenu: Key; entries: readonly string[] };

const CONSOLE_ENTRIES = ['viewConsoleCmm', 'viewConsoleAsm', 'viewConsoleVerilog', 'viewConsoleWave', 'viewConsolePrism'];

const MENUS: { label: Key; entries: Entry[] }[] = [
  {
    label: 'menu.file',
    entries: ['newProject', 'openProject', 'recent', 'closeProject', '-', 'newFile', 'newFolder', 'openFile', '-', 'save', 'saveAll', '-', 'closeTab', 'reopenTab', '-', 'revealProject', 'settings', '-', 'quit'],
  },
  { label: 'menu.edit', entries: ['undo', 'redo', '-', 'find', 'replace', 'findInFiles', '-', 'goToLine', 'formatDocument', '-', 'toggleVim'] },
  {
    label: 'menu.view',
    entries: [
      'commandPalette',
      'quickOpen',
      '-',
      'viewExplorer',
      'viewFlow',
      'viewSearch',
      'viewReports',
      { submenu: 'menu.consoles', entries: CONSOLE_ENTRIES },
      'toggleTerminal',
      'showProblems',
      '-',
      { submenu: 'menu.appearance', entries: APPEARANCE_ENTRIES },
      { submenu: 'menu.layout', entries: LAYOUT_ENTRIES },
      'toggleZen',
      '-',
      'splitEditor',
      'closeEditorGroup',
      '-',
      'zoomIn',
      'zoomOut',
      'zoomReset',
      '-',
      'selectTheme',
      'toggleTheme',
    ],
  },
  {
    label: 'menu.project',
    entries: ['addVerilog', 'newVerilog', 'newTestbench', 'newCocotb', 'newProcessor', '-', 'chooseTop', 'chooseTestbench', '-', 'refreshProject'],
  },
  {
    label: 'menu.flow',
    entries: ['build', 'check', 'lint', '-', 'simulate', 'fastSim', 'openWave', '-', 'synthesize', 'showSchematic', 'showStatistics', 'viewHierarchy', '-', 'fullFlow', 'cancel', '-', 'useIcarus', 'useVerilator'],
  },
  { label: 'menu.tools', entries: ['toolchain', 'installComponents', 'checkUpdates', '-', 'history', 'lastReport', 'compareReports', 'cleanReports'] },
  { label: 'menu.help', entries: ['laceDocs', 'saphoManual', '-', 'shortcuts', 'about'] },
];

function useItems(entries: Entry[]): MenuItem[] {
  const t = useT();
  const recent = useApp((s) => s.recent);
  // Redesenha quando muda o que habilita as ações ou o que elas marcam.
  useApp((s) => s.settings);
  useApp((s) => s.toolchain);
  useJobs((s) => s.running);
  useProject((s) => s.snapshot);
  useLayout((s) => s.live);
  useLayout((s) => s.panelMaximized);

  const build = (list: readonly Entry[]): MenuItem[] => {
    const items: MenuItem[] = [];
    for (const entry of list) {
      if (entry === '-') {
        items.push({ separator: true });
      } else if (typeof entry === 'object') {
        items.push({ label: t(entry.submenu), submenu: build(entry.entries) });
      } else if (entry === 'recent') {
        const existing = recent.filter((r) => r.exists).slice(0, 8);
        items.push({ label: t('menu.openRecent'), disabled: true });
        if (existing.length === 0) items.push({ label: `   ${t('menu.noRecent')}`, disabled: true });
        for (const r of existing) {
          items.push({ label: `   ${r.name}`, run: () => void useProject.getState().open(r.spf) });
        }
      } else {
        items.push(actionItem(entry));
      }
    }
    return items;
  };
  return build(entries);
}

function Menu({
  label,
  entries,
  open,
  onOpen,
  onHover,
  onClose,
}: {
  label: Key;
  entries: Entry[];
  open: boolean;
  onOpen: () => void;
  onHover: () => void;
  onClose: () => void;
}) {
  const t = useT();
  const items = useItems(entries);
  return (
    <div className="menubar__menu">
      <button
        type="button"
        className={`menubar__item${open ? ' is-open' : ''}`}
        onClick={() => (open ? onClose() : onOpen())}
        onMouseEnter={onHover}
      >
        {t(label)}
      </button>
      {open && (
        <div className="menu menu--dropdown" role="menu">
          <MenuList items={items} onDone={onClose} />
        </div>
      )}
    </div>
  );
}

export function MenuBar() {
  const [open, setOpen] = useState<number | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open === null) return;
    const close = (e: MouseEvent) => {
      if (ref.current && e.target instanceof Node && ref.current.contains(e.target)) return;
      setOpen(null);
    };
    const key = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(null);
    const blur = () => setOpen(null);
    window.addEventListener('mousedown', close, true);
    window.addEventListener('keydown', key, true);
    window.addEventListener('blur', blur);
    return () => {
      window.removeEventListener('mousedown', close, true);
      window.removeEventListener('keydown', key, true);
      window.removeEventListener('blur', blur);
    };
  }, [open]);

  return (
    <div
      className="menubar"
      ref={ref}
      onContextMenu={(e) => {
        if (e.target === e.currentTarget) openContextMenu(e, chromeMenu());
      }}
    >
      <LaceMark className="menubar__logo" />
      {MENUS.map((menu, index) => (
        <Menu
          key={menu.label}
          label={menu.label}
          entries={menu.entries}
          open={open === index}
          onOpen={() => setOpen(index)}
          onHover={() => open !== null && setOpen(index)}
          onClose={() => setOpen(null)}
        />
      ))}
    </div>
  );
}
