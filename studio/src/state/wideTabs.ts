// As abas largas: as Preferências e o RTLViewer (o esquemático) usam a
// janela inteira. Enquanto uma delas está na frente de algum grupo do
// editor, as duas barras laterais e o painel ficam escondidos; quando
// nenhuma está, eles voltam como estavam (layout.ts, `wideSaved`). O layout
// salvo não muda com isso.

import { useEditor, type ViewKind } from './editor';
import { useLayout } from './layout';

const WIDE_TABS: readonly ViewKind[] = ['settings', 'schematic'];

function wideInFront(): boolean {
  const { tabs, groups } = useEditor.getState();
  return groups.some((group) => {
    const kind = tabs.find((tab) => tab.id === group.activeId)?.kind;
    return !!kind && kind !== 'file' && WIDE_TABS.includes(kind);
  });
}

/** Segue as abas da frente; devolve o que desliga. */
export function watchWideTabs(): () => void {
  let wide = false;
  const check = () => {
    const now = wideInFront();
    if (now === wide) return;
    wide = now;
    if (now) useLayout.getState().enterWide();
    else useLayout.getState().exitWide();
  };
  check();
  return useEditor.subscribe(check);
}
