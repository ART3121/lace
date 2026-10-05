// Onde a onda abre: numa aba do Studio (o cliente web do Surfer, wave_tab.rs)
// ou no surfer-aurora em janela, pela preferência `wave_viewer`. A aba só
// existe com o cliente web no bundle (`surfer_web` do ToolchainInfo); sem
// ele, a onda abre em janela.

import { create } from 'zustand';

import { useApp } from './app';
import { useEditor } from './editor';

interface WaveReloads {
  /** Um contador por onda: a aba aberta dela recarrega quando ele muda. */
  tokens: Record<string, number>;
  bump: (path: string) => void;
}

export const useWaveReloads = create<WaveReloads>((set, get) => ({
  tokens: {},
  bump: (path) => set({ tokens: { ...get().tokens, [path]: (get().tokens[path] ?? 0) + 1 } }),
}));

/** A onda abre numa aba: a preferência pede e o bundle traz o cliente web. */
export function waveInTab(): boolean {
  const { settings, toolchain } = useApp.getState();
  return (settings?.wave_viewer ?? 'tab') === 'tab' && !!toolchain?.surfer_web;
}

/** Abre a aba da onda, ou vai para ela. Com `reload`, uma aba já aberta lê
 * a onda de novo (uma simulação nova a regravou). */
export function openWaveTab(path: string, reload = false): void {
  const editor = useEditor.getState();
  const open = editor.tabs.some((tab) => tab.kind === 'wave' && tab.data?.path === path);
  editor.openView('wave', { path });
  if (open && reload) useWaveReloads.getState().bump(path);
}
