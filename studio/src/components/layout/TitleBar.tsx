// A barra de título integrada (state/titleBar.ts): os menus à esquerda, o
// título no meio e, no Windows e no Linux, os botões da janela à direita. No
// macOS, os botões coloridos do sistema ficam por cima da faixa, no canto
// esquerdo, e ela deixa o espaço deles.
//
// A faixa inteira arrasta a janela (`data-tauri-drag-region="deep"`: o script
// do Tauri ignora o que é clicável, como os menus e os botões), e dois
// cliques nela maximizam e restauram.

import { getCurrentWindow } from '@tauri-apps/api/window';

import { useT } from '../../i18n';
import { useProject, windowTitle } from '../../state/project';
import { useTitleBar } from '../../state/titleBar';
import { showError } from '../../state/toasts';
import { LaceMark } from '../common';
import { chromeContextMenu, Menus } from './MenuBar';

export function TitleBar({ menus }: { menus: boolean }) {
  const mac = useTitleBar((s) => s.platform === 'macos');
  const focused = useTitleBar((s) => s.focused);
  const title = windowTitle(useProject((s) => s.snapshot?.name ?? null));
  return (
    <div
      className={`titlebar${mac ? ' titlebar--mac' : ''}${focused ? '' : ' is-inactive'}`}
      data-tauri-drag-region="deep"
      onContextMenu={chromeContextMenu}
    >
      <div className="titlebar__start">
        {!mac && <LaceMark className="menubar__logo" />}
        {menus && <Menus />}
      </div>
      <div className="titlebar__title">{title}</div>
      <div className="titlebar__end">{!mac && <WindowControls />}</div>
    </div>
  );
}

/** Uma ação da janela; um erro (uma permissão faltando) vira aviso. */
function windowAction(action: () => Promise<void>) {
  return () => void action().catch(showError);
}

/** Minimizar, maximizar ou restaurar e fechar, como os do Windows. Fechar
 * passa pelo `onCloseRequested` de App.tsx, que pergunta pelos arquivos não
 * salvos. Ficam fora do Tab, como os do sistema. */
function WindowControls() {
  const t = useT();
  const maximized = useTitleBar((s) => s.maximized);
  const middle = maximized ? t('window.restore') : t('window.maximize');
  return (
    <div className="window-controls">
      <button
        type="button"
        className="window-controls__button"
        tabIndex={-1}
        aria-label={t('window.minimize')}
        title={t('window.minimize')}
        onClick={windowAction(() => getCurrentWindow().minimize())}
      >
        <svg viewBox="0 0 10 10" aria-hidden="true" shapeRendering="crispEdges">
          <path d="M0 5.5h10" />
        </svg>
      </button>
      <button
        type="button"
        className="window-controls__button"
        tabIndex={-1}
        aria-label={middle}
        title={middle}
        onClick={windowAction(() => getCurrentWindow().toggleMaximize())}
      >
        {maximized ? (
          <svg viewBox="0 0 10 10" aria-hidden="true" shapeRendering="crispEdges">
            <rect x="0.5" y="2.5" width="7" height="7" />
            <path d="M2.5 2.5v-2h7v7h-2" />
          </svg>
        ) : (
          <svg viewBox="0 0 10 10" aria-hidden="true" shapeRendering="crispEdges">
            <rect x="0.5" y="0.5" width="9" height="9" />
          </svg>
        )}
      </button>
      <button
        type="button"
        className="window-controls__button window-controls__button--close"
        tabIndex={-1}
        aria-label={t('common.close')}
        title={t('common.close')}
        onClick={windowAction(() => getCurrentWindow().close())}
      >
        <svg viewBox="0 0 10 10" aria-hidden="true">
          <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" />
        </svg>
      </button>
    </div>
  );
}
