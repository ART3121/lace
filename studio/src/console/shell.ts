// O terminal de shell (o TCMD da AURORA): um xterm.js ligado a um
// pseudoterminal do backend (src-tauri/src/terminal.rs). Como os consoles,
// vive fora do React para não perder o que está na tela ao trocar de aba.
// Acompanha o projeto: ao abrir outro, o shell vai para a pasta dele.

import { FitAddon } from '@xterm/addon-fit';
import { Terminal, type ITheme } from '@xterm/xterm';

import { t } from '../i18n';
import { api } from '../ipc/api';
import { useProject } from '../state/project';
import { DEFAULT_THEME, resolveTheme, type Theme } from '../themes';
import { terminalTheme } from './consoles';

interface Shell {
  term: Terminal;
  host: HTMLDivElement;
  fit: FitAddon;
  id: number | null;
  exited: boolean;
  opened: boolean;
}

let shell: Shell | null = null;
let theme: Theme = resolveTheme(DEFAULT_THEME, true);
/** Os contêineres que mostram o terminal, do mais antigo ao atual: a aba
 * Terminal, em qualquer região, e a gaveta do zen. Como nos consoles, o
 * último a montar fica com ele e, ao sair, o devolve ao anterior. */
let owners: HTMLElement[] = [];

/** O tema dos consoles com o cursor visível, na cor do cursor do editor. */
function shellTheme(theme: Theme): ITheme {
  const base = terminalTheme(theme);
  return { ...base, cursor: theme.editor.cursor, cursorAccent: base.background };
}

function create(): Shell {
  const term = new Terminal({
    fontFamily: "'JetBrains Mono Variable', 'JetBrains Mono', ui-monospace, monospace",
    fontSize: 12,
    lineHeight: 1.2,
    theme: shellTheme(theme),
    cursorBlink: true,
    scrollback: 10000,
    allowProposedApi: true,
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  const host = document.createElement('div');
  host.className = 'console-host shell-terminal';
  const created: Shell = { term, host, fit, id: null, exited: false, opened: false };
  term.onData((data) => {
    if (created.id !== null && !created.exited) void api.terminal.write(created.id, data).catch(() => undefined);
  });
  term.onResize(({ cols, rows }) => {
    if (created.id !== null && !created.exited) void api.terminal.resize(created.id, cols, rows).catch(() => undefined);
  });
  return created;
}

async function spawn(target: Shell): Promise<void> {
  const cwd = useProject.getState().snapshot?.root ?? null;
  try {
    target.id = await api.terminal.spawn(cwd, target.term.cols, target.term.rows, (message) => {
      if (message.type === 'data') target.term.write(message.data);
      else {
        target.exited = true;
        target.term.writeln(`\r\n\x1b[90m${t('panel.terminalExited', { code: message.code ?? '-' })}\x1b[0m`);
      }
    });
    target.exited = false;
  } catch (error) {
    target.exited = true;
    target.term.writeln(`\x1b[31m${(error as { message?: string }).message ?? String(error)}\x1b[0m`);
  }
}

/** Mostra o terminal dentro de `container`, abrindo o shell na primeira vez.
 * Com `focus` falso, o foco fica onde está (o terminal apareceu porque um
 * layout foi aplicado, não porque o usuário o pediu). */
export function attachShell(container: HTMLElement, { focus = true }: { focus?: boolean } = {}): void {
  if (!shell) shell = create();
  const current = shell;
  owners = owners.filter((owner) => owner !== container);
  owners.push(container);
  container.appendChild(current.host);
  if (!current.opened) {
    current.term.open(current.host);
    current.opened = true;
    fitShell();
    void spawn(current);
  } else {
    fitShell();
  }
  if (focus) current.term.focus();
}

/** Tira o terminal de `container`. Se ele era o dono atual, o terminal volta
 * ao dono anterior que ainda está na página (da aba para a gaveta do zen, ou
 * o contrário); se o dono atual é outro, fica onde está. */
export function detachShell(container: HTMLElement): void {
  const current = owners[owners.length - 1] === container;
  owners = owners.filter((owner) => owner !== container && owner.isConnected);
  if (!shell || !current) return;
  const previous = owners[owners.length - 1];
  if (previous) {
    previous.appendChild(shell.host);
    requestAnimationFrame(fitShell);
  } else {
    shell.host.remove();
  }
}

export function fitShell(): void {
  if (!shell?.opened || !shell.host.isConnected) return;
  try {
    shell.fit.fit();
  } catch {
    // Sem tamanho ainda.
  }
}

/** Encerra o shell atual e abre outro, na pasta do projeto. */
export async function restartShell(): Promise<void> {
  if (!shell) return;
  if (shell.id !== null && !shell.exited) await api.terminal.kill(shell.id).catch(() => undefined);
  shell.term.reset();
  shell.exited = false;
  await spawn(shell);
  shell.term.focus();
}

// Outro projeto aberto: o shell que já roda vai para a pasta dele, com um
// `cd` digitado, como o "abrir o terminal aqui" da AURORA; o histórico e o
// que está na tela ficam. Um shell que ainda não abriu nasce na pasta nova, e
// um que terminou reabre nela.
useProject.subscribe((state, previous) => {
  const root = state.snapshot?.root ?? null;
  if (!root || root === (previous.snapshot?.root ?? null)) return;
  if (!shell || shell.id === null || shell.exited) return;
  void api.terminal.cd(shell.id, root).catch(() => undefined);
});

export function setShellTheme(next: Theme): void {
  theme = next;
  if (shell) shell.term.options.theme = shellTheme(next);
}
