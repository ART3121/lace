// O modo zen: só o editor na tela. App.tsx esconde as barras; aqui ficam o
// que o zen põe no lugar delas e o que acontece ao entrar e sair.
//
// - ZenShell: o terminal de shell numa gaveta embaixo do editor (Ctrl+`),
//   o mesmo do painel, com o mesmo processo e histórico.
// - ZenHud: um indicador pequeno no canto, com a operação rodando, o último
//   resultado por alguns segundos, a espera de um atalho de duas etapas e
//   a linha do Vim, que no resto do tempo mora na barra de status.
// - watchZen: tela cheia e o aviso de como sair.

import { getCurrentWindow } from '@tauri-apps/api/window';
import { CircleAlert, CircleCheck, TriangleAlert } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { useChord } from '../../actions';
import { attachShell, detachShell, fitShell } from '../../console/shell';
import { activeEditor } from '../../editor/host';
import { setVimStatusNode } from '../../editor/vim';
import { formatDuration, t, useT, type Key } from '../../i18n';
import { useApp } from '../../state/app';
import { useJobs } from '../../state/jobs';
import { useLayout } from '../../state/layout';
import { showError, useToasts } from '../../state/toasts';
import { Kbd, Spinner } from '../common';
import { Elapsed } from './StatusBar';

/** Quanto tempo o resultado de uma operação fica no indicador. */
const RESULT_MS = 6000;

/** Quantas vezes o aviso de como sair aparece. */
const HINT_LIMIT = 3;
const HINT_KEY = 'lace-studio:zen-hints';

/** A largura do editor centralizado: 110 colunas da fonte do editor (a
 * JetBrains Mono tem 0,6 em de avanço) mais a margem dos números de linha. */
export function zenWidth(fontSize: number): number {
  return Math.round(110 * 0.6 * fontSize + 80);
}

export function ZenShell() {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    attachShell(node);
    const observer = new ResizeObserver(() => fitShell());
    observer.observe(node);
    return () => {
      observer.disconnect();
      detachShell(node);
      // A gaveta fechou: o foco volta ao editor, em vez de ficar em lugar nenhum.
      activeEditor()?.focus();
    };
  }, []);
  return (
    <div className="zen-shell">
      <div ref={ref} className="console zen-shell__term" />
    </div>
  );
}

export function ZenHud() {
  const t = useT();
  const running = useJobs((s) => s.running);
  const last = useJobs((s) => s.last);
  const problems = useJobs((s) => s.problems);
  const pending = useChord((s) => s.pending);
  const vim = useApp((s) => s.settings?.editor.vim_mode ?? false);
  const [showLast, setShowLast] = useState(false);

  useEffect(() => {
    const left = last ? last.finishedAt + RESULT_MS - Date.now() : 0;
    setShowLast(left > 0);
    if (left <= 0) return;
    const timer = window.setTimeout(() => setShowLast(false), left);
    return () => window.clearTimeout(timer);
  }, [last]);

  const errors = problems.filter((p) => p.severity === 'error').length;
  const warnings = problems.filter((p) => p.severity === 'warning').length;

  return (
    <div className="zen-hud" aria-live="polite">
      {vim && <span ref={setVimStatusNode} className="zen-hud__item statusbar__vim zen-hud__vim" />}
      {pending && (
        <span className="zen-hud__item">
          <Kbd keys={pending} />
          {t('status.chordPending')}
        </span>
      )}
      {running ? (
        <span className="zen-hud__item">
          <Spinner size={12} />
          {running.phase ? t(`console.phase.${running.phase}` as Key) : t('common.running')}
          <Elapsed since={running.startedAt} />
        </span>
      ) : (
        showLast &&
        last && (
          <button
            type="button"
            className={`zen-hud__item zen-hud__result ${last.succeeded ? 'text-ok' : 'text-error'}`}
            title={t('action.showProblems')}
            onClick={() => useLayout.getState().showPanel('problems')}
          >
            {last.succeeded ? <CircleCheck size={13} /> : <CircleAlert size={13} />}
            {last.succeeded
              ? t('status.lastOk', { flow: t(`flowName.${last.flow}` as Key), time: formatDuration(last.durationMs) })
              : t('status.lastFailed', {
                  flow: t(`flowName.${last.flow}` as Key),
                  status: last.status === 'error' ? t('console.status.failed') : t(`console.status.${last.status}` as Key),
                })}
            {errors + warnings > 0 && (
              <span className="zen-hud__count">
                <CircleAlert size={12} /> {errors} <TriangleAlert size={12} /> {warnings}
              </span>
            )}
          </button>
        )
      )}
    </div>
  );
}

/** O aviso de como sair, nas primeiras vezes. */
function hint(): void {
  let shown = 0;
  try {
    shown = Number(localStorage.getItem(HINT_KEY) ?? 0);
    if (shown >= HINT_LIMIT) return;
    localStorage.setItem(HINT_KEY, String(shown + 1));
  } catch {
    // Sem armazenamento, o aviso aparece sempre.
  }
  useToasts.getState().push({ kind: 'info', title: t('zen.entered'), detail: t('zen.howToExit') }, 5000);
}

/**
 * Acompanha o zen: ao entrar, tela cheia (se a preferência pedir) e o aviso;
 * ao sair, a janela volta ao tamanho de antes. Se ela já estava em tela
 * cheia antes do zen, continua. Devolve a função que para de acompanhar.
 */
export function watchZen(): () => void {
  let fullscreenBefore: boolean | null = null;
  const window_ = getCurrentWindow();
  return useLayout.subscribe((state, previous) => {
    if (state.zen === previous.zen) return;
    if (state.zen) {
      hint();
      if (!useApp.getState().settings?.zen?.fullscreen) return;
      void (async () => {
        fullscreenBefore = await window_.isFullscreen();
        if (!fullscreenBefore) await window_.setFullscreen(true);
      })().catch(showError);
    } else {
      const restore = fullscreenBefore === false;
      fullscreenBefore = null;
      if (restore) void window_.setFullscreen(false).catch(showError);
    }
  });
}
