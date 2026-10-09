// A onda numa aba: o cliente web do Surfer num iframe, servido pelo backend
// (wave_tab.rs) e ligado a um `surfer-aurora server` desta aba. Abrir a vista
// sobe o servidor; fechar, ou trocar de onda, o encerra.

import { AudioWaveform, ExternalLink, ListFilter, RotateCcw, RotateCw } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { openWaveWindow, signalChoice } from '../../actions';
import { useT } from '../../i18n';
import { api } from '../../ipc/api';
import type { IpcError, WaveTab } from '../../ipc/types';
import { confirm, openDialog } from '../../state/dialogs';
import { useProject } from '../../state/project';
import { showError } from '../../state/toasts';
import { useWaveReloads } from '../../state/waves';
import { relativeTo, samePath } from '../../util/paths';
import { Button, IconButton, Spinner } from '../common';

type State = { status: 'loading' } | { status: 'ready'; tab: WaveTab } | { status: 'error'; error: IpcError };

/** Por quanto tempo, depois que a página carrega, a aba pede ao cliente para
 * redesenhar, e de quanto em quanto. */
const NUDGE_FOR_MS = 15000;
const NUDGE_EVERY_MS = 250;

/** O cliente web do Surfer só trata as mensagens que chegam (a onda, os
 * tradutores, o layout, que vêm por HTTP depois da página) quando desenha um
 * quadro, e só desenha com entrada do usuário ou com uma mensagem injetada.
 * Sem isso, a aba ficaria na tela inicial até o mouse passar por ela. Esta
 * mensagem não muda nada: só pede um quadro novo. */
function nudge(frame: HTMLIFrameElement | null) {
  frame?.contentWindow?.postMessage({ command: 'InjectMessage', message: '"InvalidateDrawCommands"' }, '*');
}

export function WaveView({ path }: { path: string }) {
  const t = useT();
  const root = useProject((s) => s.snapshot?.root ?? null);
  // A escolha de sinais vale para a onda do testbench do projeto; a de um
  // processador grava tudo.
  const projectWave = useProject((s) => samePath(s.snapshot?.waveform, path));
  const chosen = useProject((s) => s.snapshot?.wave_selection?.length ?? 0);
  const reload = useWaveReloads((s) => s.tokens[path] ?? 0);
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<State>({ status: 'loading' });
  const [painted, setPainted] = useState(false);
  const frame = useRef<HTMLIFrameElement>(null);

  // Com o foco na aba, as teclas são do Surfer; o cliente repassa as dos
  // atalhos do Studio (teclas de função e Ctrl, Alt ou Cmd), injetadas no
  // index.html pelo backend (wave_tab.rs), e elas viram teclas desta janela.
  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (!frame.current || event.source !== frame.current.contentWindow) return;
      const data = event.data as { lace?: string } & KeyboardEventInit;
      if (data?.lace !== 'key') return;
      const { key, code, ctrlKey, shiftKey, altKey, metaKey } = data;
      frame.current.blur();
      document.body.dispatchEvent(
        new KeyboardEvent('keydown', { key, code, ctrlKey, shiftKey, altKey, metaKey, bubbles: true, cancelable: true }),
      );
    };
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  }, []);

  useEffect(() => {
    if (!painted) return;
    const started = Date.now();
    const timer = window.setInterval(() => {
      if (Date.now() - started > NUDGE_FOR_MS) window.clearInterval(timer);
      else nudge(frame.current);
    }, NUDGE_EVERY_MS);
    return () => window.clearInterval(timer);
  }, [painted]);

  useEffect(() => {
    let alive = true;
    let opened: string | null = null;
    setState({ status: 'loading' });
    setPainted(false);
    api.waveTab
      .open(path)
      .then((tab) => {
        if (!alive) {
          void api.waveTab.close(tab.id).catch(() => undefined);
          return;
        }
        opened = tab.id;
        setState({ status: 'ready', tab });
      })
      .catch((error: IpcError) => alive && setState({ status: 'error', error }));
    return () => {
      alive = false;
      if (opened) void api.waveTab.close(opened).catch(() => undefined);
    };
  }, [path, reload, attempt]);

  const shown = root ? relativeTo(path, root) : path;
  const summary =
    state.status === 'ready'
      ? state.tab.processors
          .map((p) => {
            const parts = [t('wave.variables', { count: p.variables })];
            if (p.assembly) parts.push(t('wave.assembly'));
            if (p.source) parts.push(t('wave.source'));
            if (p.outdated) parts.push(t('wave.outdated'));
            return `${p.processor}: ${parts.join(', ')}`;
          })
          .join(' · ')
      : '';
  const tab = state.status === 'ready' ? state.tab : null;
  const layoutPath = tab?.saved_layout ? (root ? relativeTo(tab.saved_layout, root) : tab.saved_layout) : null;

  // Apaga o layout que o usuário salvou e abre a aba de novo, com o gerado.
  const resetLayout = async () => {
    if (!layoutPath) return;
    const answer = await confirm({
      title: t('dialog.resetLayout.title'),
      message: t('dialog.resetLayout.message', { path: layoutPath }),
      buttons: [
        { label: t('dialog.resetLayout.confirm'), value: 'reset', danger: true },
        { label: t('common.cancel'), value: 'cancel' },
      ],
    });
    if (answer !== 'reset') return;
    try {
      await api.wave.resetLayout(path);
      setAttempt((n) => n + 1);
    } catch (error) {
      showError(error);
    }
  };

  return (
    <div className="wave-view">
      <div className="wave-view__bar">
        <AudioWaveform size={14} className="wave-view__icon" />
        <span className="mono wave-view__text" title={path}>
          {shown}
        </span>
        {summary && <span className="muted wave-view__text">{summary}</span>}
        <span className="wave-view__spacer" />
        {layoutPath && (
          <span
            className="muted wave-view__text wave-view__layout"
            title={t(tab?.customized ? 'wave.customLayout' : 'wave.savedLayout', { path: layoutPath })}
          >
            {t(tab?.customized ? 'wave.customLayout' : 'wave.savedLayout', { path: layoutPath })}
          </span>
        )}
        {layoutPath && tab?.customized && (
          <IconButton label={t('wave.resetLayout')} onClick={() => void resetLayout()}>
            <RotateCcw size={14} />
          </IconButton>
        )}
        {projectWave && (
          <Button
            small
            className="wave-view__signals"
            icon={<ListFilter size={14} />}
            title={t('action.waveSignals')}
            onClick={() => openDialog({ kind: 'waveSignals' })}
          >
            {t('wave.signals', { choice: signalChoice(chosen) })}
          </Button>
        )}
        <IconButton label={t('wave.reload')} onClick={() => setAttempt((n) => n + 1)}>
          <RotateCw size={14} />
        </IconButton>
        <IconButton label={t('wave.openWindow')} onClick={() => void openWaveWindow(path)}>
          <ExternalLink size={14} />
        </IconButton>
      </div>
      <div className="wave-view__body">
        {state.status === 'error' ? (
          <div className="wave-view__message">
            <p>{t('wave.failed')}</p>
            <p className="muted mono">{state.error.message}</p>
            <div className="button-row">
              <Button icon={<RotateCw size={14} />} onClick={() => setAttempt((n) => n + 1)}>
                {t('wave.retry')}
              </Button>
              <Button icon={<ExternalLink size={14} />} onClick={() => void openWaveWindow(path)}>
                {t('wave.openWindow')}
              </Button>
            </div>
          </div>
        ) : (
          <>
            {state.status === 'ready' && (
              <iframe
                key={state.tab.id}
                ref={frame}
                className="wave-view__frame"
                src={state.tab.url}
                title={shown}
                sandbox="allow-scripts allow-same-origin allow-downloads"
                onLoad={() => setPainted(true)}
              />
            )}
            {(state.status === 'loading' || !painted) && (
              <div className="wave-view__veil">
                <Spinner />
                <span>{t('wave.loading')}</span>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
