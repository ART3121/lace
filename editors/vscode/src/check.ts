// Os erros e avisos do último `aurora check` no painel Problemas e nos selos
// da vista de fontes. Só os arquivos do projeto entram; os de fora (a
// biblioteca SAPHO do bundle, que o Icarus e o Verilator também leem e que
// o usuário não corrige) ficam na contagem da saída.

import { sep } from 'node:path';

import * as vscode from 'vscode';

import type { CheckReport, Severity } from './aurora/aurora-types';
import { countProblems, groupProblems, type Problem, type ProblemCount } from './problems';
import type { AuroraProject } from './projects';

const SEVERITIES: Record<Severity, vscode.DiagnosticSeverity> = {
  error: vscode.DiagnosticSeverity.Error,
  warning: vscode.DiagnosticSeverity.Warning,
  info: vscode.DiagnosticSeverity.Information,
  // Uma linha que a AURORA não reconheceu: fica visível, sem pesar como erro.
  unknown: vscode.DiagnosticSeverity.Information,
};

export interface CheckTotals {
  /** Nos arquivos do projeto, os que foram para o painel. */
  project: ProblemCount;
  /** Nos de fora. */
  outside: ProblemCount;
}

export class CheckResults implements vscode.Disposable {
  private readonly collection = vscode.languages.createDiagnosticCollection('aurora');
  private counts = new Map<string, ProblemCount>();
  private readonly changed = new vscode.EventEmitter<void>();
  /** Os problemas do último check mudaram. */
  readonly onDidChange = this.changed.event;

  constructor(private readonly output: vscode.OutputChannel) {}

  /** Os erros e avisos do último check num arquivo. */
  problemCount(path: string): ProblemCount | undefined {
    return this.counts.get(vscode.Uri.file(path).fsPath);
  }

  /** Esquece o último check (o projeto ativo mudou). */
  clear(): void {
    this.collection.clear();
    this.counts.clear();
    this.changed.fire();
  }

  /**
   * Troca os problemas pelos deste check. `registered` são os arquivos do
   * .spf: um .v de fora da pasta do projeto também é dele.
   */
  apply(report: CheckReport, project: AuroraProject, registered: readonly string[]): CheckTotals {
    const problems = groupProblems(report.check.diagnostics, project.root, projectFiles(project, registered));
    this.collection.clear();
    this.counts = new Map();
    for (const [file, list] of problems.byFile) {
      const uri = vscode.Uri.file(file);
      this.collection.set(uri, list.map(toDiagnostic));
      this.counts.set(uri.fsPath, countProblems(list));
    }
    this.changed.fire();
    for (const d of problems.unplaced) {
      this.output.appendLine(`${d.severity}: ${d.raw}`);
    }
    return {
      project: countProblems([...problems.byFile.values()].flat()),
      outside: countProblems([...problems.outside.values()].flat()),
    };
  }

  dispose(): void {
    this.collection.dispose();
    this.changed.dispose();
  }
}

/** Na pasta do projeto ou registrado nele. */
function projectFiles(project: AuroraProject, registered: readonly string[]): (file: string) => boolean {
  const root = project.root.endsWith(sep) ? project.root : project.root + sep;
  const known = new Set(registered.map((path) => vscode.Uri.file(path).fsPath));
  return (file) => {
    const path = vscode.Uri.file(file).fsPath;
    return path.startsWith(root) || known.has(path);
  };
}

function toDiagnostic(problem: Problem): vscode.Diagnostic {
  const line = Math.max((problem.line ?? 1) - 1, 0);
  const column = Math.max((problem.column ?? 1) - 1, 0);
  // Sem coluna (o Icarus não dá), a linha inteira.
  const range = new vscode.Range(line, column, line, Number.MAX_SAFE_INTEGER);
  const diagnostic = new vscode.Diagnostic(range, problem.message, SEVERITIES[problem.severity]);
  diagnostic.source = problem.tool;
  return diagnostic;
}
