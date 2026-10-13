// Os consoles do painel inferior, cada um um terminal xterm.js somente
// leitura:
//
// - o INFO, com tudo o que as operações escrevem, em ordem, uma rodada depois
//   da outra;
// - um terminal por etapa, como os terminais da AURORA (TCMM, TASM, TVERI,
//   TWAVE, TRTLViewer), fechado até o usuário abrir. Ele recebe as linhas da
//   etapa dele, que também vão para o INFO.
//
// | Etapa    | Na AURORA | O que recebe |
// |----------|-----------|--------------|
// | cmm      | TCMM      | cpppp, cmmcomp, cppcomp (passos preprocess e compile) |
// | asm      | TASM      | appcomp e asmcomp (pre_assemble e assemble) |
// | verilog  | TVERI     | iverilog -tnull e verilator --lint-only (check_syntax, lint) |
// | wave     | TWAVE     | iverilog/vvp ou verilator (elaborate, verilate, simulate) |
// | rtlViewer    | TRTLViewer    | yosys e dot (synthesize, graph, render) |
// | fpga     | (nenhum)  | o Quartus da placa (synthesize, fit, bitstream, timing, program) |
//
// Cada rodada começa com um cabeçalho (a operação e a hora) e termina com o
// status, o tempo e os problemas; o formato das linhas está em format.ts.
// Um terminal de etapa aberto no meio do caminho mostra o que a etapa já
// escreveu (`log`), com os cabeçalhos das rodadas em que ela escreveu algo.
// A saída do `lace install` e do `lace update` aparece na tela do bundle
// (`views/ToolchainView.tsx`).
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
import { Terminal, type IMarker, type ITheme } from '@xterm/xterm';

import type { ConsoleChannel, ConsoleId } from '../state/layout';
import { DEFAULT_THEME, legible, resolveTheme, type Theme } from '../themes';
import { header } from './format';

/** O estilo de uma linha do registro do `lace install` e do `lace update`,
 * que a tela do bundle mostra fora dos consoles. */
export type LineStyle = 'plain' | 'error' | 'warning' | 'info' | 'success' | 'dim' | 'title' | 'command';

/** Quantas linhas das etapas guardar para os terminais abertos depois: o
 * mesmo que cada terminal guarda (`scrollback`). */
const LOG_LINES = 20000;

/**
 * O tema xterm.js de um tema do Studio (themes/). Nos consoles o cursor
 * fica da cor do fundo, porque não há entrada; o terminal de shell troca
 * o cursor pelo do editor (shell.ts).
 *
 * O Studio escreve com as cores ANSI 31 (erro), 33 (aviso), 32 (sucesso) e
 * 90 (apagado), em format.ts; as ferramentas usam as outras também. O
 * apagado é o do nome de cada ferramenta e do comando, então precisa ser
 * lido: em vários temas o `brightBlack` original fica abaixo de 3:1 sobre o
 * fundo (no Solarized é a própria cor do fundo), e aqui ele vai na direção do
 * texto até chegar a 3:1.
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
  /** A rodada da última linha escrita aqui; 0 antes da primeira. */
  run: number;
  /** Já tem alguma linha: o cabeçalho da próxima rodada vem depois de uma
   * linha em branco. */
  written: boolean;
}

/** Uma linha de etapa que já saiu, com o estilo aplicado. */
interface LoggedLine {
  stage: ConsoleChannel;
  line: string;
  run: number;
}

const consoles = new Map<ConsoleId, ConsoleEntry>();
const log: LoggedLine[] = [];
/** O cabeçalho de cada rodada que ainda tem linhas em `log`. */
const runTitles = new Map<number, string>();
let currentRun = 0;
/** A primeira linha de erro da rodada atual no INFO. */
let runError: IMarker | null = null;
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

function create(id: ConsoleId): ConsoleEntry {
  const term = new Terminal({
    disableStdin: true,
    convertEol: true,
    scrollback: LOG_LINES,
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
  host.dataset.console = id;
  const entry: ConsoleEntry = { term, host, fit, search, opened: false, owners: [], run: 0, written: false };
  // Um terminal de etapa aberto agora mostra o que a etapa já escreveu.
  if (id !== 'info') {
    for (const logged of log) if (logged.stage === id) writeStage(entry, logged.line, logged.run);
  }
  return entry;
}

function getConsole(id: ConsoleId): ConsoleEntry {
  let entry = consoles.get(id);
  if (!entry) {
    entry = create(id);
    consoles.set(id, entry);
  }
  return entry;
}

/** Uma linha num terminal de etapa, depois do cabeçalho da rodada dela se
 * for a primeira dessa rodada aqui. */
function writeStage(entry: ConsoleEntry, line: string, run: number): void {
  if (run !== entry.run) {
    const title = runTitles.get(run);
    if (title) {
      if (entry.written) entry.term.writeln('');
      entry.term.writeln(title);
    }
    entry.run = run;
  }
  entry.term.writeln(line);
  entry.written = true;
}

function remember(stage: ConsoleChannel, line: string): void {
  log.push({ stage, line, run: currentRun });
  if (log.length <= LOG_LINES * 1.25) return;
  log.splice(0, log.length - LOG_LINES);
  for (const run of runTitles.keys()) if (run < log[0].run) runTitles.delete(run);
}

/** Põe o console dentro de `container` (quando a aba dele aparece). */
export function attachConsole(id: ConsoleId, container: HTMLElement): void {
  const entry = getConsole(id);
  entry.owners = entry.owners.filter((owner) => owner !== container);
  entry.owners.push(container);
  container.appendChild(entry.host);
  if (!entry.opened) {
    entry.term.open(entry.host);
    entry.opened = true;
  }
  fitConsole(id);
}

/** Tira o console de `container`. Se ele era o dono atual, o console volta
 * ao dono anterior que ainda está na página; sem nenhum, sai da página. */
export function detachConsole(id: ConsoleId, container: HTMLElement): void {
  const entry = consoles.get(id);
  if (!entry) return;
  const current = entry.owners[entry.owners.length - 1] === container;
  entry.owners = entry.owners.filter((owner) => owner !== container && owner.isConnected);
  if (!current) return;
  const previous = entry.owners[entry.owners.length - 1];
  if (previous) {
    previous.appendChild(entry.host);
    requestAnimationFrame(() => fitConsole(id));
  } else {
    entry.host.remove();
  }
}

export function fitConsole(id: ConsoleId): void {
  const entry = consoles.get(id);
  if (!entry?.opened || !entry.host.isConnected) return;
  try {
    entry.fit.fit();
  } catch {
    // Sem tamanho ainda (painel recolhido): o próximo ajuste resolve.
  }
}

/** Começa uma rodada: o cabeçalho sai no INFO agora e, num terminal de
 * etapa, antes da primeira linha que a rodada escrever nele. */
export function beginRun(title: string): void {
  currentRun += 1;
  runError?.dispose();
  runError = null;
  const line = header(title);
  runTitles.set(currentRun, line);
  const info = getConsole('info');
  // Se a rodada anterior falhou, o INFO ficou parado no primeiro erro dela;
  // a nova volta a acompanhar a saída.
  info.term.scrollToBottom();
  if (info.written) info.term.writeln('');
  info.term.writeln(line);
  info.written = true;
  info.run = currentRun;
}

/** O fim da rodada, só no INFO (format.ts, `footer`). */
export function endRun(line: string): void {
  const info = getConsole('info');
  info.term.writeln(line);
  info.written = true;
}

/** Escreve uma linha já formatada (format.ts), sem o fim de linha, no INFO
 * e, com `stage`, também no terminal dessa etapa, se ele já foi aberto;
 * senão ela fica guardada para quando ele abrir. `error` marca a linha como
 * erro: a primeira da rodada é onde o INFO para numa falha. */
export function writeLine(stage: ConsoleChannel | null, line: string, error = false): void {
  const info = getConsole('info');
  if (error && currentRun > 0 && !runError) {
    // O marcador vai na linha que acabou de sair: o cursor já está na de baixo.
    info.term.writeln(line, () => {
      runError ??= info.term.registerMarker(-1) ?? null;
    });
  } else {
    info.term.writeln(line);
  }
  info.written = true;
  if (!stage) return;
  remember(stage, line);
  const entry = consoles.get(stage);
  if (entry) writeStage(entry, line, currentRun);
}

/** Rola o INFO até o primeiro erro da rodada, com duas linhas antes dele. */
export function revealRunError(): void {
  const info = consoles.get('info');
  if (!info || !runError || runError.isDisposed || runError.line < 0) return;
  info.term.scrollToLine(Math.max(0, runError.line - 2));
}

/** Limpa um console. Os outros ficam como estão, e um terminal de etapa que
 * abrir depois ainda mostra o que a etapa escreveu. */
export function clearConsole(id: ConsoleId): void {
  const entry = consoles.get(id);
  if (!entry) return;
  entry.term.clear();
  entry.term.reset();
  entry.written = false;
  if (id === 'info') {
    runError?.dispose();
    runError = null;
  }
}

export function findInConsole(id: ConsoleId, query: string, previous = false): boolean {
  const entry = consoles.get(id);
  if (!entry || !query) return false;
  return previous ? entry.search.findPrevious(query) : entry.search.findNext(query);
}

export function setConsoleTheme(theme: Theme): void {
  currentTheme = terminalTheme(theme);
  for (const entry of consoles.values()) entry.term.options.theme = currentTheme;
}

export function setConsoleFontSize(size: number): void {
  fontSize = size;
  for (const [id, entry] of consoles) {
    entry.term.options.fontSize = size;
    fitConsole(id);
  }
}
