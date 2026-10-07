// A janela inteira: a barra de título integrada (TitleBar.tsx) ou a barra de
// menus, barra de ferramentas, a área de trabalho (Workbench.tsx: barra de
// atividades, barras laterais, editor e painel) e barra de status. Aqui
// também ficam as ligações globais: atalhos, vigia de arquivos, arrastar e
// soltar, fechar com arquivos não salvos.

import { getCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, type CSSProperties } from 'react';

import { chordActions, handleShortcut, runAction } from './actions';
import { ContextMenuHost } from './components/common';
import { Dialogs } from './components/dialogs/Dialogs';
import { MenuBar } from './components/layout/MenuBar';
import { StatusBar } from './components/layout/StatusBar';
import { TitleBar } from './components/layout/TitleBar';
import { Toolbar } from './components/layout/Toolbar';
import { Workbench } from './components/layout/Workbench';
import { watchZen, ZenHud, zenWidth } from './components/layout/Zen';
import { externalDrop, externalLeave, externalOver } from './components/sidebar/dnd';
import { Toasts } from './components/Toasts';
import { setConsoleTheme, setLinkHandler } from './console/consoles';
import { setShellTheme } from './console/shell';
import { bindEditorChord } from './editor/monaco';
import { t } from './i18n';
import { api } from './ipc/api';
import { useApp } from './state/app';
import { confirm } from './state/dialogs';
import { useEditor } from './state/editor';
import { useHierarchy } from './state/hierarchy';
import { takeFreshStart, useLayout } from './state/layout';
import { useProject } from './state/project';
import { activeLayout } from './state/savedLayouts';
import { applyTitleBar, useTitleBar, watchWindow } from './state/titleBar';
import { showError } from './state/toasts';
import type { Theme } from './themes';
import { extension, resolveFrom } from './util/paths';

/**
 * Mostra a janela, uma vez. Ela nasce escondida (`visible: false` em
 * tauri.conf.json) e só aparece com a interface desenhada, no tema e no zoom
 * das preferências: sem isso, ela abriria em branco enquanto o JavaScript
 * carrega (1,5 s a 4 s no desenvolvimento, com o Vite servindo os módulos um
 * a um). Se a interface não chegar aqui, o backend mostra a janela sozinho
 * depois de um tempo (`REVEAL_FALLBACK`, em lib.rs).
 */
let revealed = false;
const REVEAL_WAIT_MS = 1500;
function reveal() {
  if (revealed) return;
  revealed = true;
  void getCurrentWindow()
    .show()
    .catch(() => undefined);
}

/** O que roda uma vez, ao abrir a janela. */
async function start(): Promise<() => void> {
  const cleanups: (() => void)[] = [];

  const settings = await useApp.getState().init();
  // A barra de título integrada antes de a janela aparecer: no Windows e no
  // Linux, a moldura do sistema sai (state/titleBar.ts).
  const os = useApp.getState().info?.os;
  if (os) await applyTitleBar(os);
  cleanups.push(await watchWindow());
  // Sem a janela guardada no localStorage (a primeira abertura, ou o
  // armazenamento da WebView limpo), ela abre no layout em uso. A janela
  // ainda está escondida: o primeiro quadro já sai nele.
  if (takeFreshStart() && settings) useLayout.getState().applyBody(activeLayout(settings).body);
  const applyTheme = (theme: Theme) => {
    setConsoleTheme(theme);
    setShellTheme(theme);
  };
  applyTheme(useApp.getState().theme);
  cleanups.push(useApp.subscribe((s, prev) => s.theme !== prev.theme && applyTheme(s.theme)));

  // Os atalhos de duas etapas (Ctrl+K Z) também valem com o foco no editor.
  for (const a of chordActions()) bindEditorChord(a.keys!, () => runAction(a.id));
  cleanups.push(watchZen());

  await getCurrentWebview()
    .setZoom(useLayout.getState().zoom)
    .catch(() => undefined);
  // Tema e zoom aplicados. A janela espera o último projeto reabrir, para o
  // primeiro quadro já ser o de trabalho, mas no máximo REVEAL_WAIT_MS: um
  // projeto grande não segura a abertura.
  const revealTimer = window.setTimeout(reveal, REVEAL_WAIT_MS);
  cleanups.push(() => window.clearTimeout(revealTimer));

  // Links arquivo:linha dos consoles abrem no editor.
  setLinkHandler((path, line, column) => {
    const root = useProject.getState().snapshot?.root;
    void useEditor.getState().openFile(root ? resolveFrom(root, path) : path, { line, column });
  });

  // O que mudou no disco: árvore, retrato do projeto e abas abertas.
  let refreshTimer = 0;
  const unlistenFs = await api.events.onFsChanged(({ paths }) => {
    useProject.getState().bumpTree();
    void useEditor.getState().onDiskChange(paths);
    if (paths.some((p) => ['v', 'sv', 'vh', 'svh'].includes(extension(p)))) useHierarchy.getState().markStale();
    window.clearTimeout(refreshTimer);
    refreshTimer = window.setTimeout(() => void useProject.getState().refresh(), 400);
  });
  cleanups.push(unlistenFs);

  // Arrastar arquivos do sistema: um .spf abre o projeto; no explorador, a
  // pasta debaixo do ponteiro recebe uma cópia, e Módulos ou Testbenches
  // registram o Verilog (components/sidebar/dnd.ts); no resto da janela, os
  // arquivos abrem no editor.
  const unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === 'over') {
      externalOver(payload.position);
      return;
    }
    if (payload.type === 'leave') {
      externalLeave();
      return;
    }
    if (payload.type !== 'drop') return;
    const paths = payload.paths;
    const spf = paths.find((p) => extension(p) === 'spf');
    if (spf) {
      externalLeave();
      void useProject.getState().open(spf);
      return;
    }
    void externalDrop(paths, payload.position).then((handled) => {
      if (!handled) for (const p of paths) void useEditor.getState().openFile(p);
    });
  });
  cleanups.push(unlistenDrop);

  // Fechar a janela com arquivos não salvos pergunta antes.
  const window_ = getCurrentWindow();
  const unlistenClose = await window_.onCloseRequested(async (event) => {
    const snapshot = useProject.getState().snapshot;
    if (snapshot) useEditor.getState().persistSession(snapshot.spf);
    const dirty = useEditor.getState().tabs.filter((tab) => tab.dirty);
    if (dirty.length === 0) return;
    event.preventDefault();
    const answer = await confirm({
      title: t('dialog.unsaved.title'),
      message: t('dialog.unsavedMany.message', { count: dirty.length }),
      buttons: [
        { label: t('action.saveAll'), value: 'save', primary: true },
        { label: t('common.discard'), value: 'discard', danger: true },
        { label: t('common.cancel'), value: 'cancel' },
      ],
    });
    if (answer === 'save' && !(await useEditor.getState().saveAll())) return;
    if (answer === 'save' || answer === 'discard') await window_.destroy();
  });
  cleanups.push(unlistenClose);

  // Reabre o último projeto, como a AURORA.
  const last = useApp.getState().recent[0] ?? (await api.app.recent().catch(() => []))[0];
  if (settings?.restore_last_project && last?.exists) {
    await useProject.getState().open(last.spf);
  } else {
    useEditor.getState().openView('welcome');
  }
  // Um quadro para o React desenhar o projeto antes de a janela aparecer.
  window.setTimeout(reveal, 0);

  return () => cleanups.forEach((cleanup) => cleanup());
}

export function App() {
  const zen = useLayout((s) => s.zen);
  const bars = useLayout((s) => s.live.bars);
  const menus = !zen && bars.menubar;
  // A faixa integrada sai em tela cheia (o zen com tela cheia, a tela cheia
  // do macOS): não há janela para mover, e os menus, se à vista, voltam para
  // a barra de menus. Esconder a barra de menus no layout tira só os menus
  // da faixa: ela é a barra de título.
  const titleBar = useTitleBar((s) => s.integrated && !s.fullscreen);
  // Sem a barra de status, o indicador do zen fica no lugar dela: a linha do
  // Vim, a espera de um atalho e o resultado da operação continuam à vista.
  const hud = zen || !bars.statusbar;
  const zenShell = useLayout((s) => s.zenShell);
  // No zen, com um grupo só, o editor e a gaveta do shell ficam numa coluna
  // centralizada (Preferências > Editor > Modo zen).
  const centered = useApp((s) => s.settings?.zen?.center_layout ?? true);
  const fontSize = useApp((s) => s.settings?.editor.font_size ?? 13);
  const singleGroup = useEditor((s) => s.groups.length === 1);
  const zenCentered = zen && centered && singleGroup;

  useEffect(() => {
    window.addEventListener('keydown', handleShortcut, true);

    let stop: (() => void) | null = null;
    let cancelled = false;
    void start().then(
      (cleanup) => {
        if (cancelled) cleanup();
        else stop = cleanup;
        // Roteiro de fumaça, só na build de desenvolvimento (src/dev/smoke.ts).
        if (import.meta.env.DEV) void import('./dev/smoke').then((m) => m.runSmokeScript());
      },
      (error) => {
        // A abertura parou no meio: a janela aparece com o erro.
        reveal();
        showError(error);
      },
    );
    return () => {
      cancelled = true;
      window.removeEventListener('keydown', handleShortcut, true);
      stop?.();
    };
  }, []);

  const appClass = `app${zen ? ' app--zen' : ''}${hud ? ' app--hud' : ''}${zenCentered ? ' app--zen-centered' : ''}${zen && zenShell ? ' app--zen-shell' : ''}`;
  return (
    <div className={appClass} style={zenCentered ? ({ '--zen-width': `${zenWidth(fontSize)}px` } as CSSProperties) : undefined}>
      {titleBar ? <TitleBar menus={menus} /> : menus && <MenuBar />}
      {!zen && bars.toolbar && <Toolbar />}
      <Workbench />
      {hud ? <ZenHud /> : <StatusBar />}
      <Dialogs />
      <ContextMenuHost />
      <Toasts />
    </div>
  );
}
