// Os erros e avisos do YANC no painel Problemas: os do `lace build` e os do
// build que o sim e o synth fazem antes. Uma coleção própria, separada da do
// check: um check não apaga os erros do último build, e o contrário também.
// Cada processador troca só os dele.

import { sep } from 'node:path';

import * as vscode from 'vscode';

import type { BuildResult, Severity } from './lace/lace-types';
import { countProblems, groupProblems, type Problem, type ProblemCount } from './problems';
import type { LaceProject } from './projects';

const SEVERITIES: Record<Severity, vscode.DiagnosticSeverity> = {
  error: vscode.DiagnosticSeverity.Error,
  warning: vscode.DiagnosticSeverity.Warning,
  info: vscode.DiagnosticSeverity.Information,
  unknown: vscode.DiagnosticSeverity.Information,
};

export class BuildResults implements vscode.Disposable {
  private readonly collection = vscode.languages.createDiagnosticCollection('lace-build');
  /** Os arquivos com problema de cada processador, para trocar só os dele. */
  private readonly files = new Map<string, Set<string>>();
  private readonly counts = new Map<string, ProblemCount>();
  private readonly changed = new vscode.EventEmitter<void>();
  readonly onDidChange = this.changed.event;

  problemCount(path: string): ProblemCount | undefined {
    return this.counts.get(vscode.Uri.file(path).fsPath);
  }

  clear(): void {
    this.collection.clear();
    this.files.clear();
    this.counts.clear();
    this.changed.fire();
  }

  /** Troca os problemas dos processadores destes builds pelos novos. */
  apply(builds: readonly BuildResult[], project: LaceProject): void {
    if (builds.length === 0) {
      return;
    }
    const root = project.root.endsWith(sep) ? project.root : project.root + sep;
    const inProject = (file: string) => vscode.Uri.file(file).fsPath.startsWith(root);
    for (const build of builds) {
      for (const file of this.files.get(build.processor) ?? []) {
        this.collection.delete(vscode.Uri.file(file));
        this.counts.delete(file);
      }
      const problems = groupProblems(build.diagnostics, project.root, inProject);
      const mine = new Set<string>();
      for (const [file, list] of [...problems.byFile, ...problems.outside]) {
        const uri = vscode.Uri.file(file);
        this.collection.set(uri, list.map(toDiagnostic));
        this.counts.set(uri.fsPath, countProblems(list));
        mine.add(uri.fsPath);
      }
      // As linhas sem arquivo ("it compiled!") já saíram ao vivo na saída.
      this.files.set(build.processor, mine);
    }
    this.changed.fire();
  }

  dispose(): void {
    this.collection.dispose();
    this.changed.dispose();
  }
}

function toDiagnostic(problem: Problem): vscode.Diagnostic {
  const line = Math.max((problem.line ?? 1) - 1, 0);
  const column = Math.max((problem.column ?? 1) - 1, 0);
  const range = new vscode.Range(line, column, line, Number.MAX_SAFE_INTEGER);
  const diagnostic = new vscode.Diagnostic(range, problem.message, SEVERITIES[problem.severity]);
  diagnostic.source = problem.tool;
  return diagnostic;
}
