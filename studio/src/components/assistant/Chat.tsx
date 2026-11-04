// A conversa com o assistente: a transcrição (as mensagens, as chamadas de
// ferramenta e os avisos), o campo de texto e os botões. A mesma tela serve
// à vista da barra lateral (`side`) e à aba inteira (`tab`); só o cabeçalho
// muda. Sem o Claude Code na máquina, a tela diz como instalar e abre as
// Preferências.

import { Bot, Hammer, PanelRight, Settings, Square, SendHorizontal, Sparkles, SquarePen, Wrench } from 'lucide-react';
import { useEffect, useRef, useState, type KeyboardEvent } from 'react';

import { useT } from '../../i18n';
import { useApp } from '../../state/app';
import { useAssistant, type ChatEntry } from '../../state/assistant';
import { useEditor } from '../../state/editor';
import { useLayout } from '../../state/layout';
import { useSettingsPage } from '../../state/settingsPage';
import { Button, Empty, IconButton, Spinner } from '../common';
import { ViewActions } from '../layout/ViewActions';
import { Markdown } from './markdown';

export function Chat({ mode }: { mode: 'side' | 'tab' }) {
  const t = useT();
  const info = useAssistant((s) => s.info);
  const checking = useAssistant((s) => s.checking);
  const status = useAssistant((s) => s.status);
  const entries = useAssistant((s) => s.entries);
  const model = useAssistant((s) => s.model);
  const costUsd = useAssistant((s) => s.costUsd);
  const hasProject = useApp((s) => s.settings !== null) && useAssistant((s) => s.info?.project) !== null;

  useEffect(() => {
    if (!info && !checking) void useAssistant.getState().refreshInfo();
  }, [info, checking]);

  const openSettings = () => {
    useSettingsPage.getState().setPage('assistant');
    useEditor.getState().openView('settings');
  };

  const actions = (
    <>
      <IconButton label={t('assistant.new')} onClick={() => void useAssistant.getState().reset()} disabled={entries.length === 0 && status === 'idle'}>
        <SquarePen size={15} />
      </IconButton>
      {mode === 'side' ? (
        <IconButton label={t('assistant.openTab')} onClick={() => useEditor.getState().openView('assistant')}>
          <Sparkles size={15} />
        </IconButton>
      ) : (
        <IconButton label={t('assistant.openSide')} onClick={() => useLayout.getState().revealView('assistant', { explicit: true })}>
          <PanelRight size={15} />
        </IconButton>
      )}
      <IconButton label={t('settings.assistant')} onClick={openSettings}>
        <Settings size={15} />
      </IconButton>
    </>
  );

  const missing = info !== null && info.claude === null;
  const body = missing ? (
    <Empty icon={<Bot size={28} />}>
      <p>{t('assistant.missing')}</p>
      <p className="muted">{t('assistant.missingHint')}</p>
      <div className="chat__empty-actions">
        <Button variant="primary" icon={<Settings size={14} />} onClick={openSettings}>
          {t('assistant.openSettings')}
        </Button>
        <Button onClick={() => void useAssistant.getState().refreshInfo()}>{t('assistant.recheck')}</Button>
      </div>
    </Empty>
  ) : (
    <Transcript entries={entries} status={status} />
  );

  const footer = (
    <div className="chat__status">
      {model && <span title={t('assistant.model')}>{model}</span>}
      {costUsd > 0 && <span title={t('assistant.costHint')}>{t('assistant.cost', { cost: costUsd.toFixed(3) })}</span>}
      {info?.version && !model && <span className="muted">{t('assistant.found', { version: info.version })}</span>}
      {!hasProject && info && <span className="muted">{t('assistant.noProject')}</span>}
    </div>
  );

  return (
    <div className={`chat chat--${mode}`}>
      {mode === 'side' ? (
        <ViewActions>{actions}</ViewActions>
      ) : (
        <div className="chat__header">
          <h1>
            <Bot size={18} /> {t('assistant.title')}
          </h1>
          <div className="chat__header-actions">{actions}</div>
        </div>
      )}
      <div className="chat__body">{body}</div>
      {!missing && <Composer disabled={missing} />}
      {footer}
    </div>
  );
}

function Transcript({ entries, status }: { entries: ChatEntry[]; status: string }) {
  const t = useT();
  const end = useRef<HTMLDivElement>(null);
  const container = useRef<HTMLDivElement>(null);
  const [follow, setFollow] = useState(true);

  // Acompanha o fim enquanto a pessoa não rolou para cima.
  useEffect(() => {
    if (follow) end.current?.scrollIntoView({ block: 'end' });
  }, [entries, status, follow]);

  const onScroll = () => {
    const el = container.current;
    if (!el) return;
    setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < 40);
  };

  if (entries.length === 0) {
    return (
      <Empty icon={<Sparkles size={28} />}>
        <p>{t('assistant.welcome')}</p>
        <p className="muted">{t('assistant.welcomeHint')}</p>
      </Empty>
    );
  }
  return (
    <div className="chat__transcript" ref={container} onScroll={onScroll}>
      {entries.map((entry) => (
        <Entry key={entry.id} entry={entry} />
      ))}
      {(status === 'starting' || status === 'busy') && (
        <div className="chat__thinking">
          <Spinner /> {t(status === 'starting' ? 'assistant.starting' : 'assistant.thinking')}
        </div>
      )}
      <div ref={end} />
    </div>
  );
}

function Entry({ entry }: { entry: ChatEntry }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  switch (entry.kind) {
    case 'user':
      return (
        <div className="chat__message chat__message--user">
          <div className="chat__bubble">{entry.text}</div>
        </div>
      );
    case 'assistant':
      return (
        <div className="chat__message chat__message--assistant">
          <div className="chat__bubble chat__markdown">
            <Markdown text={entry.text} />
            {entry.streaming && <span className="chat__caret" aria-hidden />}
          </div>
        </div>
      );
    case 'tool': {
      const Icon = entry.name === 'Read' || entry.name === 'Grep' || entry.name === 'Glob' ? Wrench : Hammer;
      return (
        <div className={`chat__tool${entry.isError ? ' chat__tool--error' : ''}${entry.done ? '' : ' chat__tool--running'}`}>
          <button type="button" className="chat__tool-head" onClick={() => setOpen((v) => !v)} aria-expanded={open}>
            {entry.done ? <Icon size={13} /> : <Spinner size={13} />}
            <span className="chat__tool-name">{entry.name}</span>
            <span className="chat__tool-args">{summarizeInput(entry.input)}</span>
            {entry.isError && <span className="chat__tool-flag">{t('assistant.toolFailed')}</span>}
          </button>
          {open && (
            <div className="chat__tool-detail">
              <pre>{JSON.stringify(entry.input, null, 2)}</pre>
              {entry.result !== null && <pre>{clip(entry.result, 4000)}</pre>}
            </div>
          )}
        </div>
      );
    }
    case 'note':
      return <div className={`chat__note chat__note--${entry.tone}`}>{entry.text}</div>;
  }
}

function Composer({ disabled }: { disabled: boolean }) {
  const t = useT();
  const draft = useAssistant((s) => s.draft);
  const status = useAssistant((s) => s.status);
  const busy = status === 'busy' || status === 'starting';
  const area = useRef<HTMLTextAreaElement>(null);

  // O campo cresce com o texto, até um limite.
  useEffect(() => {
    const el = area.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
  }, [draft]);

  const send = () => void useAssistant.getState().send(draft);
  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      if (!busy) send();
    }
    if (e.key === 'Escape' && busy) void useAssistant.getState().interrupt();
  };

  return (
    <div className="chat__composer">
      <textarea
        ref={area}
        className="chat__input"
        rows={1}
        value={draft}
        placeholder={t('assistant.placeholder')}
        disabled={disabled}
        onChange={(e) => useAssistant.getState().setDraft(e.target.value)}
        onKeyDown={onKey}
      />
      {busy ? (
        <IconButton label={t('assistant.stop')} className="chat__send" onClick={() => void useAssistant.getState().interrupt()} disabled={status === 'starting'}>
          <Square size={15} />
        </IconButton>
      ) : (
        <IconButton label={t('assistant.send')} className="chat__send" onClick={send} disabled={disabled || draft.trim() === ''}>
          <SendHorizontal size={15} />
        </IconButton>
      )}
    </div>
  );
}

/** Os argumentos de uma chamada numa linha: `project=..., flow=sim`. */
function summarizeInput(input: unknown): string {
  if (!input || typeof input !== 'object') return '';
  const parts = Object.entries(input as Record<string, unknown>)
    .filter(([, v]) => v !== null && v !== undefined && v !== '')
    .map(([k, v]) => `${k}=${typeof v === 'string' ? shortPath(v) : JSON.stringify(v)}`);
  return clip(parts.join(' '), 120);
}

function shortPath(value: string): string {
  const parts = value.split(/[\\/]/);
  return parts.length > 2 ? parts.slice(-2).join('/') : value;
}

function clip(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max)}…` : text;
}
