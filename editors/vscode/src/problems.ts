// Os diagnósticos do `aurora check` organizados por arquivo, sem depender do
// VS Code: o painel Problemas e os selos da vista de fontes saem daqui.

import { isAbsolute, resolve } from 'node:path';

import type { Diagnostic, Severity } from './aurora/aurora-types';

export interface Problem {
  severity: Severity;
  message: string;
  tool: string;
  /** Linha, a partir de 1; `null` quando a ferramenta não disse. */
  line: number | null;
  /** Coluna, a partir de 1 (Verilator); `null` no Icarus. */
  column: number | null;
}

export interface ProblemCount {
  errors: number;
  warnings: number;
}

export interface Problems {
  /** Os problemas de cada arquivo do projeto, pelo caminho absoluto. */
  byFile: Map<string, Problem[]>;
  /**
   * Os de arquivos fora do projeto: a biblioteca SAPHO do bundle, que o
   * lint do Verilator também lê e que o usuário não corrige.
   */
  outside: Map<string, Problem[]>;
  /** O que não tem arquivo: vai só para a saída. */
  unplaced: Diagnostic[];
}

/**
 * Agrupa por arquivo e tira as repetições: o check elabora o design e, de
 * novo, o design com cada testbench, e um aviso do design sai uma vez por
 * elaboração. `inProject` separa os arquivos do projeto dos de fora.
 */
export function groupProblems(
  diagnostics: readonly Diagnostic[],
  root: string,
  inProject: (file: string) => boolean,
): Problems {
  const byFile = new Map<string, Problem[]>();
  const outside = new Map<string, Problem[]>();
  const unplaced: Diagnostic[] = [];
  const seen = new Set<string>();
  for (const d of diagnostics) {
    if (d.file === null || d.file === '') {
      unplaced.push(d);
      continue;
    }
    // A AURORA passa caminhos absolutos às ferramentas; um relativo seria da raiz.
    const file = isAbsolute(d.file) ? d.file : resolve(root, d.file);
    const key = JSON.stringify([file, d.line, d.column, d.severity, d.message]);
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    const target = inProject(file) ? byFile : outside;
    const list = target.get(file) ?? [];
    list.push({ severity: d.severity, message: d.message, tool: d.tool, line: d.line, column: d.column });
    target.set(file, list);
  }
  return { byFile, outside, unplaced };
}

export function countProblems(problems: readonly Problem[]): ProblemCount {
  return {
    errors: problems.filter((p) => p.severity === 'error').length,
    warnings: problems.filter((p) => p.severity === 'warning').length,
  };
}

/** Quantos erros e avisos, por extenso: "2 erros, 1 aviso". */
export function describeCount({ errors, warnings }: ProblemCount): string {
  const part = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  return `${part(errors, 'erro', 'erros')}, ${part(warnings, 'aviso', 'avisos')}`;
}
