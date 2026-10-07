// Os botões de uma vista, mostrados no cabeçalho da região onde ela está
// (Region.tsx), como os do título de uma vista do VS Code: à direita do título
// numa barra lateral, à esquerda dos botões da região no painel. A vista os
// declara onde quiser; um portal os leva para o cabeçalho.

import { createContext, useContext, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

export const ActionsSlot = createContext<HTMLElement | null>(null);

export function ViewActions({ children }: { children: ReactNode }) {
  const slot = useContext(ActionsSlot);
  return slot ? createPortal(children, slot) : null;
}
