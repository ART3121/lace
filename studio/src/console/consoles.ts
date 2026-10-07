// Os consoles do painel inferior: um terminal xterm.js somente leitura por
// canal, como os terminais da AURORA (TCMM, TASM, TVERI, TWAVE, TPRISM).
//
// | Canal    | Na AURORA | O que recebe |
// |----------|-----------|--------------|
// | cmm      | TCMM      | cpppp, cmmcomp, cppcomp (passos preprocess e compile) |
// | asm      | TASM      | appcomp e asmcomp (pre_assemble e assemble) |
// | verilog  | TVERI     | iverilog -tnull e verilator --lint-only (check_syntax, lint) |
// | wave     | TWAVE     | iverilog/vvp ou verilator (elaborate, verilate, simulate) |
// | prism    | TPRISM    | yosys e dot (synthesize, graph, render) |
//
// O comando de cada operação e os avisos dela (falha ao iniciar, pedido de
// cancelamento) vão para o console da operação. A saída do `lace install` e
// do `lace update` aparece na tela do bundle (`views/ToolchainView.tsx`).
//
// Os terminais vivem fora do React: são criados uma vez e o elemento deles
// é movido para dentro do componente quando ele aparece. Assim a saída não
// se perde ao trocar de aba, e escrever não redesenha a interface.
//
// Quem mostra um console é o dono dele, e o último a montar fica com o
// elemento. Ao sair, o dono devolve o console ao anterior, se ainda houver
// um: um console que muda de região passa pelos dois sem ficar órfão.

import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon } from '@xterm/addon-search';
import { Terminal, type ITheme } from '@xterm/xterm';

import type { ConsoleChannel } from '../state/layout';
import { DEFAULT_THEME, legible, resolveTheme, type Theme } from '../themes';

export type LineStyle = 'plain' | 'error' | 'warning' | 'info' | 'success' | 'dim' | 'title' | 'command';

const STYLE_CODES: Record<LineStyle, string> = {
  plain: '',
  error: '\x1b[31m',
  warning: '\x1b[33m',
  info: '\x1b[36m',
  success: '\x1b[32m',
  dim: '\x1b[90m',
  title: '\x1b[1m',
  command: '\x1b[35m',
};
const RESET = '\x1b[0m';

/**
 * O tema xterm.js de um tema do Studio (themes/). Nos consoles o cursor
 * fica da cor do fundo, porque não há entrada; o terminal de shell troca
 * o cursor pelo do editor (shell.ts).
 *
 * O Studio escreve com as cores ANSI 31 (erro), 33 (aviso), 32 (sucesso),
 * 36 (informação), 90 (apagado) e 35 (comando); as ferramentas usam as
 * outras também. O apagado é o das linhas de diagnóstico das ferramentas,
 * então precisa ser lido: em vários temas o `brightBlack` original fica
 * abaixo de 3:1 sobre o fundo (no Solarized é a própria cor do fundo), e
 * aqui ele vai na direção do texto até chegar a 3:1.
 */
export function terminalTheme(theme: Theme): ITheme {
  const { background, selection, ...colors } = theme.terminal;
  const bg = background ?? theme.ui.bgEditor;
  return {
    ...colors,
    brightBlack: legible(colors.brightBlack, colors.foreground, bg, 3),
    background: bg,
    cursor: bg,
    selectionBackground: selection,
  };
}

interface ConsoleEntry {
  term: Terminal;
  host: HTMLDivElement;
  fit: FitAddon;
  search: SearchAddon;
  opened: boolean;
  /** Os contêineres que mostram o console, do mais antigo ao atual. */
  owners: HTMLElement[];
}

const consoles = new Map<ConsoleChannel, ConsoleEntry>();
let currentTheme: ITheme = terminalTheme(resolveTheme(DEFAULT_THEME, true));
let fontSize = 12;

/** Quem abre um arquivo a partir de um link `arquivo:linha` no console. */
type LinkHandler = (path: string, line: number, column: number) => void;
let linkHandler: LinkHandler | null = null;

export function setLinkHandler(handler: LinkHandler): void {
  linkHandler = handler;
}

// `caminho/arquivo.ext:linha[:coluna]`, como o Icarus, o Verilator, o Yosys
// e o Lace escrevem. Caminho com espaço não vira link.
const FILE_LINK =
  /((?:[A-Za-z]:)?[\w.~/\\-]*[\w-]+\.(?:v|sv|vh|svh|cmm|cpp|c|h|hpp|asm|txt|mif|ys|dot|json|spf|py)):(\d+)(?::(\d+))?/g;

function create(channel: ConsoleChannel): ConsoleEntry {
  const term = new Terminal({
    disableStdin: true,
    convertEol: true,
    scrollback: 20000,
    fontFamily: "'JetBrains Mono Variable', 'JetBrains Mono', ui-monospace, monospace",
    fontSize,
    lineHeight: 1.25,
    theme: currentTheme,
    cursorStyle: 'bar',
    cursorInactiveStyle: 'none',
    allowProposedApi: true,
  });
  const fit = new FitAddon();
  const search = new SearchAddon();
  term.loadAddon(fit);
  term.loadAddon(search);
  term.registerLinkProvider({
    provideLinks(y, callback) {
      const line = term.buffer.active.getLine(y - 1)?.translateToString(true) ?? '';
      const links = [];
      for (const match of line.matchAll(FILE_LINK)) {
        const start = match.index ?? 0;
        const [text, path, lineNo, column] = match;
        links.push({
          range: { start: { x: start + 1, y }, end: { x: start + text.length, y } },
          text,
          decorations: { underline: true, pointerCursor: true },
          activate: () => linkHandler?.(path, Number(lineNo), Number(column ?? 1)),
        });
      }
      callback(links);
    },
  });
  const host = document.createElement('div');
  host.className = 'console-host';
  host.dataset.channel = channel;
  return { term, host, fit, search, opened: false, owners: [] };
}

export function getConsole(channel: ConsoleChannel): ConsoleEntry {
  let entry = consoles.get(channel);
  if (!entry) {
    entry = create(channel);
    consoles.set(channel, entry);
  }
  return entry;
}

/** Põe o console dentro de `container` (quando a aba dele aparece). */
export function attachConsole(channel: ConsoleChannel, container: HTMLElement): void {
  const entry = getConsole(channel);
  entry.owners = entry.owners.filter((owner) => owner !== container);
  entry.owners.push(container);
  container.appendChild(entry.host);
  if (!entry.opened) {
    entry.term.open(entry.host);
    entry.opened = true;
  }
  fitConsole(channel);
}

/** Tira o console de `container`. Se ele era o dono atual, o console volta
 * ao dono anterior que ainda está na página; sem nenhum, sai da página. */
export function detachConsole(channel: ConsoleChannel, container: HTMLElement): void {
  const entry = consoles.get(channel);
  if (!entry) return;
  const current = entry.owners[entry.owners.length - 1] === container;
  entry.owners = entry.owners.filter((owner) => owner !== container && owner.isConnected);
  if (!current) return;
  const previous = entry.owners[entry.owners.length - 1];
  if (previous) {
    previous.appendChild(entry.host);
    requestAnimationFrame(() => fitConsole(channel));
  } else {
    entry.host.remove();
  }
}

export function fitConsole(channel: ConsoleChannel): void {
  const entry = consoles.get(channel);
  if (!entry?.opened || !entry.host.isConnected) return;
  try {
    entry.fit.fit();
  } catch {
    // Sem tamanho ainda (painel recolhido): o próximo ajuste resolve.
  }
}

/** Escreve uma linha (sem o fim de linha) com um estilo. */
export function writeLine(channel: ConsoleChannel, text: string, style: LineStyle = 'plain'): void {
  const code = STYLE_CODES[style];
  getConsole(channel).term.writeln(code ? `${code}${text}${RESET}` : text);
}

export function clearConsole(channel: ConsoleChannel): void {
  const entry = consoles.get(channel);
  entry?.term.clear();
  entry?.term.reset();
}

export function findInConsole(channel: ConsoleChannel, query: string, previous = false): boolean {
  const entry = consoles.get(channel);
  if (!entry || !query) return false;
  return previous ? entry.search.findPrevious(query) : entry.search.findNext(query);
}

export function setConsoleTheme(theme: Theme): void {
  currentTheme = terminalTheme(theme);
  for (const entry of consoles.values()) entry.term.options.theme = currentTheme;
}

export function setConsoleFontSize(size: number): void {
  fontSize = size;
  for (const [channel, entry] of consoles) {
    entry.term.options.fontSize = size;
    fitConsole(channel);
  }
}

/** O estilo de uma linha de ferramenta, por palavras-chave. Serve só para
 * colorir; a classificação de verdade é a dos diagnósticos do Lace. */
export function styleForToolLine(line: string, diagnostic: boolean, stderr: boolean): LineStyle {
  if (!diagnostic && !stderr) return 'plain';
  const lower = line.toLowerCase();
  if (/\b(error|fatal|syntax error)\b|^%error/.test(lower)) return 'error';
  if (/\bwarning\b|^%warning|heads up/.test(lower)) return 'warning';
  if (diagnostic) return 'dim';
  return stderr ? 'warning' : 'plain';
}
