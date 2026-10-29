// O formato das linhas do INFO e dos terminais de etapa.
//
// A cor só diz como uma coisa terminou: vermelho é erro, amarelo é aviso e
// verde é sucesso, sempre num marcador (✗, !, ✓) ou na palavra de gravidade
// de uma mensagem, nunca na linha inteira; o marcador tem forma própria, e
// quem não distingue as cores lê do mesmo jeito. O negrito marca o começo da
// rodada e de cada fase; o apagado, o que é secundário (o comando, o nome da
// ferramenta, onde ficou um arquivo). O que uma ferramenta escreve sai como
// ela escreveu, recuado embaixo do nome dela:
//
//   ── Simulação · 14:02:31 ────────────────────────
//   $ aurora-ide sim -p soma
//   ▸ Compilando processadores (YANC)
//     cmmcomp
//       (o que o cmmcomp escreveu)
//   ✓ Build de soma (C±, 100 MHz, 2000 clocks): terminou em 155 ms
//   ▸ Simulando
//     saída do testbench
//       (o $display do testbench)
//   ✓ Simulação de soma_tb (Icarus): terminou em 524 ms
//       Saída 0: 55
//       Relatório run-000042 gravado no histórico
//   ── ✓ Terminou em 0,7 s · erros: 0, avisos: 0 ───

import type { Status } from '../ipc/lace-types';

const RED = '\x1b[31m';
const YELLOW = '\x1b[33m';
const GREEN = '\x1b[32m';
const BOLD = '\x1b[1m';
// O apagado é o `brightBlack` do tema, que consoles.ts deixa legível.
const DIM = '\x1b[90m';
const RESET = '\x1b[0m';
const RULE = '─';

/** Como uma coisa terminou, para o marcador. */
export type Outcome = 'ok' | 'warning' | 'error' | 'skipped';

const MARKS: Record<Outcome, string> = {
  ok: `${GREEN}✓${RESET}`,
  warning: `${YELLOW}!${RESET}`,
  error: `${RED}✗${RESET}`,
  skipped: `${DIM}-${RESET}`,
};

/** O recuo de um bloco (o nome de uma ferramenta, um aviso da fase) e o do
 * que vem dentro dele. */
const BLOCK = '  ';
const BODY = '    ';

/** O marcador de um status do Lace: cancelar e passar do prazo não são
 * falha da ferramenta. */
export function outcomeOf(status: Status): Outcome {
  if (status === 'succeeded') return 'ok';
  if (status === 'cancelled' || status === 'timed_out') return 'warning';
  return 'error';
}

/** O começo de uma rodada: a operação e a hora. */
export function header(title: string): string {
  return `${BOLD}${RULE}${RULE} ${title} ${RULE.repeat(24)}${RESET}`;
}

/** O fim de uma rodada, com o marcador de como ela terminou. */
export function footer(outcome: Outcome, text: string): string {
  return `${DIM}${RULE}${RULE}${RESET} ${MARKS[outcome]} ${text} ${DIM}${RULE.repeat(24)}${RESET}`;
}

/** O comando que o Studio rodou, como na CLI. */
export function command(text: string): string {
  return `${DIM}$ ${text}${RESET}`;
}

/** O começo de uma fase (compilar, simular, sintetizar). */
export function phase(title: string): string {
  return `${BOLD}▸ ${title}${RESET}`;
}

/** O nome de quem escreve as linhas seguintes: a ferramenta, o testbench, o
 * comando exato de um passo (com o modo detalhado). */
export function block(title: string): string {
  return `${BLOCK}${DIM}${title}${RESET}`;
}

/** Uma linha que a ferramenta ou o testbench escreveu, sem mudança. */
export function output(line: string): string {
  return `${BODY}${line}`;
}

// As palavras de gravidade das ferramentas: `error`, `%Error`, `fatal`,
// `warning`, `%Warning-WIDTH` e o "heads up" dos avisos do YANC.
const SEVERITY_WORD = /\b(error|fatal|warning)\b|\bheads up\b/i;

/** Uma mensagem que a ferramenta escreveu e que o Lace reconhece como dela
 * (erro, aviso, nota): só a primeira palavra de gravidade ganha cor. */
export function toolMessage(line: string): string {
  return `${BODY}${line.replace(SEVERITY_WORD, (word) => `${/error|fatal/i.test(word) ? RED : YELLOW}${word}${RESET}`)}`;
}

/** A mensagem de ferramenta é um erro: marca o primeiro erro da rodada. */
export function isErrorMessage(line: string): boolean {
  return /\b(error|fatal)\b/i.test(line);
}

/** O resultado de uma etapa, com o marcador de como terminou. */
export function result(outcome: Outcome, text: string): string {
  return `${MARKS[outcome]} ${text}`;
}

/** Um aviso ou um erro de uma fase que não é linha de ferramenta: um passo
 * que falhou, uma entrada que falta, o pedido de cancelamento. */
export function attention(outcome: Outcome, text: string): string {
  return `${BLOCK}${MARKS[outcome]} ${text}`;
}

/** Um item de um resultado com marcador próprio (um teste cocotb). */
export function item(outcome: Outcome, text: string): string {
  return `${BODY}${MARKS[outcome]} ${text}`;
}

/** Um dado de um resultado (o que uma porta escreveu). */
export function detail(text: string): string {
  return `${BODY}${text}`;
}

/** Um dado secundário de um resultado (onde ficou a onda, o relatório). */
export function note(text: string): string {
  return `${BODY}${DIM}${text}${RESET}`;
}

/** Um diagnóstico que o Lace montou (`arquivo:linha: gravidade: mensagem`),
 * com a cor só na gravidade. */
export function diagnostic(where: string, severity: string, label: string, message: string): string {
  const color = severity === 'error' ? RED : severity === 'warning' ? YELLOW : DIM;
  return `${BODY}${where}${color}${label}${RESET}: ${message}`;
}
