// O conteúdo da vista de fontes a partir do `aurora status --json`, sem
// depender do VS Code: os grupos, o texto de cada item, o contextValue que
// os menus filtram e a cor e o selo que o destacam.

import { basename, dirname, relative } from 'node:path';

import type { ProcessorStatus, ProjectIssue, StatusReport } from '../aurora/aurora-types';
import { describeCount, type ProblemCount } from '../problems';
import { isTestbenchName, isVerilogFile } from '../verilog';

export type SourceRole = 'issue' | 'design' | 'testbench' | 'processor' | 'unregistered';

/** O destaque de um item: a cor do nome e, em alguns, o selo. */
export type Tone = 'top' | 'simulated' | 'built' | 'notBuilt' | 'unregistered' | 'issue';

export interface SourceEntry {
  /** Único na vista, para o VS Code manter a seleção entre as atualizações. */
  id: string;
  role: SourceRole;
  /** O arquivo que o clique abre; `null` num aviso sem arquivo. */
  path: string | null;
  label: string;
  description: string;
  tooltip: string;
  /**
   * `aurora.<papel>`, seguido de `.top` (o topo), `.selected` (o testbench
   * simulado) ou `.built` (o processador compilado), de `.settop` quando
   * "Definir como topo" se aplica, de `.first` e `.last` no primeiro e no
   * último da lista (o que "Subir" e "Descer" não movem) e de `.verilog`
   * num .v ou .sv (o que o `aurora check <arquivo>` aceita; um testbench
   * cocotb não).
   */
  contextValue: string;
  tone?: Tone;
}

export interface SourceGroup {
  role: SourceRole;
  label: string;
  entries: SourceEntry[];
}

const GROUP_LABELS: Record<SourceRole, string> = {
  issue: 'Avisos do projeto',
  design: 'RTL',
  testbench: 'Testbenches',
  processor: 'Processadores',
  unregistered: 'Não registrados',
};

/**
 * As cores de cada tom, como chaves de tema: as `aurora.*` são do package.json
 * (contributes.colors), e quem quiser outras troca em
 * workbench.colorCustomizations.
 */
export const TONE_COLORS: Record<Tone, string> = {
  top: 'aurora.topForeground',
  simulated: 'aurora.simulatedForeground',
  built: 'aurora.builtForeground',
  notBuilt: 'aurora.notBuiltForeground',
  unregistered: 'aurora.unregisteredForeground',
  issue: 'list.warningForeground',
};

const TONE_BADGES: Partial<Record<Tone, string>> = {
  top: 'T',
  simulated: 'S',
  built: 'C',
  unregistered: 'N',
};

/**
 * Os grupos da vista, sem os vazios. O design fica na ordem do `.spf`, que é
 * a de compilação: um `` `define `` só vale para os arquivos de baixo.
 */
export function sourceGroups(status: StatusReport): SourceGroup[] {
  const groups: SourceGroup[] = [
    group('issue', status.issues.map((issue, i) => issueEntry(status, issue, i))),
    group('design', status.synthesizable.map((file, i, all) => designEntry(status, file.path, place(i, all)))),
    group('testbench', status.testbench.map((file, i, all) => testbenchEntry(status, file.path, place(i, all)))),
    group('processor', status.processors.map((processor) => processorEntry(processor))),
    group('unregistered', status.unregistered.map((path) => unregisteredEntry(status, path))),
  ];
  return groups.filter((g) => g.entries.length > 0);
}

export interface DecorationSpec {
  badge?: string;
  /** Uma chave de cor do tema. */
  color?: string;
  tooltip?: string;
}

/**
 * A cor e o selo de um item. Um erro do último check passa na frente do
 * papel: o selo vira o número de erros, em vermelho. Avisos só aparecem no
 * selo de quem não tem outro.
 */
export function decorationFor(entry: SourceEntry, problems?: ProblemCount): DecorationSpec | undefined {
  // O fonte de um processador só passa pelo build (o YANC); o Verilog, pelo check.
  const last = entry.role === 'processor' ? 'Último build' : 'Último check';
  const tooltip = problems && problems.errors + problems.warnings > 0 ? `${last}: ${describeCount(problems)}` : undefined;
  if (problems && problems.errors > 0) {
    return { badge: countBadge(problems.errors), color: 'list.errorForeground', tooltip };
  }
  const color = entry.tone ? TONE_COLORS[entry.tone] : undefined;
  const badge = entry.tone ? TONE_BADGES[entry.tone] : undefined;
  if (badge === undefined && problems && problems.warnings > 0) {
    return { badge: countBadge(problems.warnings), color: color ?? 'list.warningForeground', tooltip };
  }
  if (color === undefined && badge === undefined && tooltip === undefined) {
    return undefined;
  }
  return { badge, color, tooltip };
}

// O selo tem no máximo dois caracteres.
function countBadge(n: number): string {
  return n > 9 ? '9+' : String(n);
}

function group(role: SourceRole, entries: SourceEntry[]): SourceGroup {
  return { role, label: GROUP_LABELS[role], entries };
}

function issueEntry(status: StatusReport, issue: ProjectIssue, index: number): SourceEntry {
  return {
    id: `issue:${index}:${issue.kind}`,
    role: 'issue',
    path: issue.path,
    label: issue.message,
    description: issue.path ? relative(status.root, issue.path) : '',
    tooltip: issue.path ? `${issue.message}\n${issue.path}` : issue.message,
    contextValue: 'aurora.issue',
    tone: 'issue',
  };
}

/** Onde o arquivo está na lista do `.spf`. */
interface Place {
  first: boolean;
  last: boolean;
}

function place(index: number, all: readonly unknown[]): Place {
  return { first: index === 0, last: index === all.length - 1 };
}

function designEntry(status: StatusReport, path: string, at: Place): SourceEntry {
  const isTop = path === status.top_level;
  const module = isTop && status.top_module ? ` (módulo ${status.top_module})` : '';
  return fileEntry(status, 'design', path, {
    flag: isTop ? 'top' : undefined,
    flagText: isTop ? 'TOPO' : undefined,
    tone: isTop ? 'top' : undefined,
    tooltip: isTop ? `Topo do design${module}` : 'Módulo do design',
    at,
  });
}

function testbenchEntry(status: StatusReport, path: string, at: Place): SourceEntry {
  const isSelected = path === status.selected_testbench;
  return fileEntry(status, 'testbench', path, {
    flag: isSelected ? 'selected' : undefined,
    flagText: isSelected ? 'SIMULADO' : undefined,
    tone: isSelected ? 'simulated' : undefined,
    tooltip: isSelected ? 'O testbench que aurora sim simula' : 'Testbench',
    at,
  });
}

function unregisteredEntry(status: StatusReport, path: string): SourceEntry {
  return fileEntry(status, 'unregistered', path, {
    tone: 'unregistered',
    tooltip: 'Fora do projeto: o + o acrescenta (aurora add)',
  });
}

function fileEntry(
  status: StatusReport,
  role: SourceRole,
  path: string,
  extra: { flag?: string; flagText?: string; tone?: Tone; tooltip: string; at?: Place },
): SourceEntry {
  const name = basename(path);
  const folder = relative(status.root, dirname(path));
  const canBeTop =
    role !== 'testbench' && isVerilogFile(name) && !isTestbenchName(name) && path !== status.top_level;
  return {
    id: `${role}:${path}`,
    role,
    path,
    label: name,
    description: [extra.flagText, folder].filter((part) => part).join(' · '),
    tooltip: `${extra.tooltip}\n${path}`,
    contextValue: contextValue(role, extra.flag, canBeTop, isVerilogFile(name), extra.at),
    tone: extra.tone,
  };
}

function processorEntry(processor: ProcessorStatus): SourceEntry {
  const language = processor.language === 'cmm' ? 'C±' : 'C';
  return {
    id: `processor:${processor.name}`,
    role: 'processor',
    path: processor.source,
    label: processor.name,
    description: processor.built ? 'COMPILADO' : 'NÃO COMPILADO',
    tooltip:
      `Processador SAPHO em ${language}, ${processor.frequency_mhz} MHz, ` +
      `${processor.clocks} clocks simulados\n${processor.source}`,
    contextValue: contextValue('processor', processor.built ? 'built' : undefined, false, false),
    tone: processor.built ? 'built' : 'notBuilt',
  };
}

function contextValue(
  role: SourceRole,
  flag: string | undefined,
  canBeTop: boolean,
  verilog: boolean,
  at?: Place,
): string {
  return [
    'aurora',
    role,
    flag,
    canBeTop ? 'settop' : undefined,
    at?.first ? 'first' : undefined,
    at?.last ? 'last' : undefined,
    verilog ? 'verilog' : undefined,
  ]
    .filter((part) => part)
    .join('.');
}
