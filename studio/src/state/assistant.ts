// O assistente: a conversa com o Claude Code dentro do Studio, com as
// ferramentas do aurora mcp (src-tauri/src/assistant.rs, crates/aurora-agent).
// A sessão é um processo filho do backend; aqui ficam a transcrição, o
// estado do turno e o que a vista mostra. A sessão abre na primeira
// mensagem e fecha quando a pessoa pede uma conversa nova, quando o projeto
// muda ou quando o Studio sai.

import { create } from 'zustand';

import { t } from '../i18n';
import { api } from '../ipc/api';
import type { AgentEvent, AssistantInfo, IpcError } from '../ipc/types';
import { useProject } from './project';

/** Uma entrada da transcrição. */
export type ChatEntry =
  | { kind: 'user'; id: number; text: string }
  | { kind: 'assistant'; id: number; text: string; streaming: boolean }
  | {
      kind: 'tool';
      id: number;
      callId: string;
      /** O nome da ferramenta, sem o prefixo do servidor (`build`, `Read`). */
      name: string;
      input: unknown;
      result: string | null;
      isError: boolean;
      done: boolean;
    }
  | { kind: 'note'; id: number; text: string; tone: 'info' | 'warning' | 'error' };

export type AssistantStatus = 'idle' | 'starting' | 'ready' | 'busy';

interface AssistantState {
  /** Onde estão o Claude Code e o aurora, da última consulta. */
  info: AssistantInfo | null;
  checking: boolean;
  /** O identificador da sessão no backend; `null` sem sessão aberta. */
  session: number | null;
  /** O `session_id` do Claude Code, para retomar. */
  sessionId: string | null;
  model: string | null;
  status: AssistantStatus;
  entries: ChatEntry[];
  draft: string;
  /** O custo acumulado da conversa, em dólares. */
  costUsd: number;

  refreshInfo: () => Promise<AssistantInfo | null>;
  send: (text: string) => Promise<void>;
  interrupt: () => Promise<void>;
  /** Fecha a sessão e começa uma conversa nova (a transcrição some). */
  reset: () => Promise<void>;
  setDraft: (draft: string) => void;
}

let nextEntry = 1;

function entryId(): number {
  return nextEntry++;
}

/** O nome de uma ferramenta para mostrar: `mcp__aurora__build` vira `build`. */
export function toolLabel(name: string): string {
  return name.startsWith('mcp__aurora__') ? name.slice('mcp__aurora__'.length) : name;
}

function errorText(error: unknown): string {
  const e = error as Partial<IpcError> | undefined;
  return e?.message ?? String(error);
}

export const useAssistant = create<AssistantState>((set, get) => {
  const push = (entry: ChatEntry) => set((s) => ({ entries: [...s.entries, entry] }));
  const note = (text: string, tone: 'info' | 'warning' | 'error' = 'info') => push({ kind: 'note', id: entryId(), text, tone });

  /** Acrescenta texto à resposta em andamento, criando-a se preciso. */
  const appendText = (text: string) => {
    set((s) => {
      const last = s.entries[s.entries.length - 1];
      if (last && last.kind === 'assistant' && last.streaming) {
        return { entries: [...s.entries.slice(0, -1), { ...last, text: last.text + text }] };
      }
      return { entries: [...s.entries, { kind: 'assistant', id: entryId(), text, streaming: true }] };
    });
  };

  /** Fecha a resposta em andamento, se houver. */
  const settle = () => {
    set((s) => {
      const last = s.entries[s.entries.length - 1];
      if (last && last.kind === 'assistant' && last.streaming) {
        const entries = last.text.trim() === '' ? s.entries.slice(0, -1) : [...s.entries.slice(0, -1), { ...last, streaming: false }];
        return { entries };
      }
      return {};
    });
  };

  const onEvent = (event: AgentEvent) => {
    switch (event.type) {
      case 'ready':
        // O `ready` vem em resposta à primeira mensagem: o turno dela já
        // está em andamento.
        set({ status: 'busy', sessionId: event.session_id, model: event.model });
        if (event.aurora_connected === false) note(t('assistant.noAurora'), 'warning');
        break;
      case 'text':
        set({ status: 'busy' });
        appendText(event.text);
        break;
      case 'tool_use':
        settle();
        push({ kind: 'tool', id: entryId(), callId: event.id, name: toolLabel(event.name), input: event.input, result: null, isError: false, done: false });
        break;
      case 'tool_result':
        set((s) => ({
          entries: s.entries.map((e) => (e.kind === 'tool' && e.callId === event.id ? { ...e, result: event.content, isError: event.is_error, done: true } : e)),
        }));
        break;
      case 'done': {
        settle();
        const state = get();
        // Sem os pedaços (um Claude Code sem partial messages), o texto
        // final vem aqui.
        const last = state.entries[state.entries.length - 1];
        if (event.text && !(last && last.kind === 'assistant')) {
          if (event.is_error) note(event.text, 'error');
          else push({ kind: 'assistant', id: entryId(), text: event.text, streaming: false });
        } else if (event.is_error && event.text && last && last.kind === 'assistant' && last.text !== event.text) {
          note(event.text, 'error');
        }
        set({
          status: 'ready',
          costUsd: state.costUsd + (event.cost_usd ?? 0),
          sessionId: event.session_id ?? state.sessionId,
        });
        break;
      }
      case 'exited': {
        settle();
        set({ status: 'idle', session: null });
        note(t('assistant.ended', { code: event.code ?? '-' }), event.code === 0 || event.code === null ? 'info' : 'error');
        break;
      }
      case 'log':
        break;
      default:
        break;
    }
  };

  /** Abre a sessão, se não há uma. O Claude Code só diz `ready` depois da
   * primeira mensagem, então quem chama manda a mensagem em seguida, sem
   * esperar. */
  const ensureSession = async (): Promise<boolean> => {
    if (get().session !== null) return true;
    set({ status: 'starting' });
    try {
      const id = await api.assistant.start(null, onEvent);
      set({ session: id });
      return true;
    } catch (error) {
      set({ status: 'idle' });
      note(errorText(error), 'error');
      void get().refreshInfo();
      return false;
    }
  };

  return {
    info: null,
    checking: false,
    session: null,
    sessionId: null,
    model: null,
    status: 'idle',
    entries: [],
    draft: '',
    costUsd: 0,

    refreshInfo: async () => {
      set({ checking: true });
      try {
        const info = await api.assistant.info();
        set({ info, checking: false });
        return info;
      } catch {
        set({ checking: false });
        return null;
      }
    },

    send: async (text) => {
      const message = text.trim();
      if (message === '' || get().status === 'busy' || get().status === 'starting') return;
      set({ draft: '' });
      push({ kind: 'user', id: entryId(), text: message });
      const wasNew = get().session === null;
      if (!(await ensureSession())) return;
      const id = get().session;
      if (id === null) return;
      // Numa sessão nova, `starting` fica até o `ready`; numa aberta, o
      // turno já começa.
      if (!wasNew) set({ status: 'busy' });
      try {
        await api.assistant.send(id, message);
      } catch (error) {
        set({ status: 'ready' });
        note(errorText(error), 'error');
      }
    },

    interrupt: async () => {
      const id = get().session;
      if (id === null || get().status !== 'busy') return;
      try {
        await api.assistant.interrupt(id);
      } catch (error) {
        note(errorText(error), 'error');
      }
    },

    reset: async () => {
      const id = get().session;
      set({ session: null, sessionId: null, status: 'idle', entries: [], costUsd: 0 });
      if (id !== null) {
        try {
          await api.assistant.stop(id);
        } catch {
          // A sessão já tinha saído.
        }
      }
    },

    setDraft: (draft) => set({ draft }),
  };
});

// Outro projeto (ou nenhum): a sessão era daquele, com ele como projeto
// padrão das ferramentas. Fecha, e a próxima mensagem abre outra.
useProject.subscribe((state, previous) => {
  if (state.snapshot?.spf === previous.snapshot?.spf) return;
  const assistant = useAssistant.getState();
  if (assistant.session === null) return;
  const id = assistant.session;
  useAssistant.setState({ session: null, sessionId: null, status: 'idle' });
  useAssistant.setState((s) => ({ entries: [...s.entries, { kind: 'note', id: entryId(), text: t('assistant.projectChanged'), tone: 'info' }] }));
  void api.assistant.stop(id).catch(() => undefined);
});
