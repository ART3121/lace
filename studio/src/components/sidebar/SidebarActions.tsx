// Os botões de uma vista da barra lateral, mostrados à direita do título
// (SideBar.tsx), como os do título de uma vista do VS Code. A vista os
// declara onde quiser; um portal os leva para o título.

import { createContext, useContext, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

export const ActionsSlot = createContext<HTMLElement | null>(null);

export function SidebarActions({ children }: { children: ReactNode }) {
  const slot = useContext(ActionsSlot);
  return slot ? createPortal(children, slot) : null;
}
