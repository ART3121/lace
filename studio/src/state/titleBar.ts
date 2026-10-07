// A barra de título integrada: os menus, o título e os botões da janela numa
// faixa só (components/layout/TitleBar.tsx), no lugar da barra de título do
// sistema mais a barra de menus.
//
// No Windows e no Linux, a janela nasce com a moldura do sistema, e a
// interface a tira antes de a janela aparecer (`applyTitleBar`): se a
// interface não carregar, a janela que o backend mostra sozinho
// (REVEAL_FALLBACK, em lib.rs) ainda tem como ser movida e fechada.
//
// No macOS, a barra integrada só se define na criação da janela: o estilo
// `Overlay`, com o título do sistema escondido e os botões coloridos por cima
// da página, vem do tauri.conf.json (`titleBarStyle`, `hiddenTitle`,
// `trafficLightPosition`, que os outros sistemas ignoram).

import { getCurrentWindow } from '@tauri-apps/api/window';
import { create } from 'zustand';

export type Platform = 'windows' | 'macos' | 'linux';

/** O sistema, pelo `os` de AppInfo (`windows-x86_64`, `macos-aarch64`...). */
export function platformOf(os: string): Platform {
  if (os.startsWith('windows')) return 'windows';
  if (os.startsWith('macos')) return 'macos';
  return 'linux';
}

interface TitleBarState {
  platform: Platform;
  /** A janela está sem a barra de título do sistema. Até a abertura tirá-la
   * (ou se não der), os menus ficam na barra de menus. */
  integrated: boolean;
  maximized: boolean;
  fullscreen: boolean;
  focused: boolean;
}

export const useTitleBar = create<TitleBarState>(() => ({
  platform: 'windows',
  integrated: false,
  maximized: false,
  fullscreen: false,
  focused: true,
}));

/** Tira a moldura do sistema no Windows e no Linux. Chamado na abertura,
 * antes de a janela aparecer. */
export async function applyTitleBar(os: string): Promise<void> {
  const platform = platformOf(os);
  if (platform === 'macos') {
    useTitleBar.setState({ platform, integrated: true });
    return;
  }
  try {
    await getCurrentWindow().setDecorations(false);
    useTitleBar.setState({ platform, integrated: true });
  } catch {
    // Fora do Tauri (o Vite no navegador) não há janela: fica a barra de menus.
  }
}

/** Acompanha a janela para a faixa: maximizada (o ícone do botão do meio),
 * em tela cheia (a faixa sai) e com foco (o título apaga sem ele). */
export async function watchWindow(): Promise<() => void> {
  const window_ = getCurrentWindow();
  // Redimensionar pela borda manda dezenas de eventos: lê o estado quando
  // eles param.
  let timer = 0;
  const read = () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      void Promise.all([window_.isMaximized(), window_.isFullscreen()]).then(
        ([maximized, fullscreen]) => useTitleBar.setState({ maximized, fullscreen }),
        () => undefined,
      );
    }, 50);
  };
  const unlisten = await Promise.all([
    window_.onResized(read),
    window_.onFocusChanged(({ payload }) => useTitleBar.setState({ focused: payload })),
  ]);
  read();
  return () => {
    window.clearTimeout(timer);
    unlisten.forEach((stop) => stop());
  };
}
