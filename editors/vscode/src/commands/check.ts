// "Lace: Verificar": `aurora check` no projeto, num arquivo ou num
// processador. A saída sai ao vivo, os problemas vão para o painel, e um
// check novo cancela o que ainda roda (o resultado dele já estaria velho).

import { basename } from 'node:path';

import * as vscode from 'vscode';

import type { LaceRun } from '../lace/client';
import type { CheckReport } from '../lace/lace-types';
import { formatLaceError } from '../messages';
import { describeCount } from '../problems';
import type { RunResult } from '../runs';
import { processorOf, targetUri } from './common';
import { runOperation, setting, type RunDeps } from './run';

export type CheckScope =
  | { kind: 'project' }
  | { kind: 'file'; file: string }
  | { kind: 'processor'; name: string };

export interface CheckRequestOptions {
  /** Sem valor, segue lace.check.lint. */
  lint?: boolean;
  /**
   * Pedido pelo usuário: mostra a saída e a mensagem de falha. O check ao
   * gravar fica no botão da barra de status e no painel Problemas.
   */
  interactive?: boolean;
}

export async function check(
  deps: RunDeps,
  scope: CheckScope,
  options: CheckRequestOptions = {},
): Promise<LaceRun<CheckReport> | undefined> {
  const lint = options.lint ?? setting('lace.check', 'lint', false);
  const interactive = options.interactive ?? true;
  let summary: RunResult | undefined;
  const done = await runOperation(deps, 'check', 'check', scope.kind === 'file' ? scope.file : undefined, (client, project) => ({
    title: `Verificando ${describeScope(scope, project.name)}${lint ? ', com lint' : ''}`,
    reveal: interactive,
    restart: true,
    start: (signal, onEvent) =>
      client.check(project.spf, {
        lint,
        signal,
        onEvent,
        file: scope.kind === 'file' ? scope.file : undefined,
        processor: scope.kind === 'processor' ? scope.name : undefined,
      }),
    summarize: (run) => (summary = summarizeCheck(deps, run, project)),
  }));
  if (!done) {
    return undefined;
  }
  const { run, project } = done;
  const what = describeScope(scope, project.name);
  if (run.status === 'error') {
    if (interactive) {
      void vscode.window.showErrorMessage(`Lace: ${formatLaceError(run.error)}`);
    }
  } else if (run.status === 'failed' && interactive) {
    const problems = 'Problemas';
    const output = 'Saída';
    void vscode.window
      .showErrorMessage(`Lace: o check de ${what} falhou: ${summary?.summary}.`, problems, output)
      .then((choice) => {
        if (choice === problems) {
          void vscode.commands.executeCommand('workbench.actions.view.problems');
        } else if (choice === output) {
          deps.runs.output.show();
        }
      });
  } else if (run.status === 'succeeded') {
    vscode.window.setStatusBarMessage(`$(pass) Lace: ${what} sem erros (${summary?.summary})`, 6000);
  }
  return run;
}

/** Põe os problemas no painel e escreve o resumo na saída. */
function summarizeCheck(
  deps: RunDeps,
  run: LaceRun<CheckReport>,
  project: { root: string; name: string; spf: string; uri: vscode.Uri },
): RunResult {
  const output = deps.runs.output;
  if (run.status === 'error') {
    output.appendLine(`Erro: ${formatLaceError(run.error)}`);
    return { outcome: 'error', summary: formatLaceError(run.error) };
  }
  if (run.result === null) {
    output.appendLine('Cancelado.');
    return { outcome: 'cancelled', summary: 'cancelado' };
  }
  const report = deps.status.report;
  const registered = report ? [...report.synthesizable, ...report.testbench].map((f) => f.path) : [];
  const totals = deps.checks.apply(run.result, project, registered);
  const { check, report: id } = run.result;
  const outside =
    totals.outside.errors + totals.outside.warnings > 0
      ? ` (mais ${describeCount(totals.outside)} em arquivos fora do projeto)`
      : '';
  const summary = describeCount(totals.project);
  output.appendLine(
    `=== ${check.status === 'succeeded' ? 'Sem erros' : `Terminou como ${check.status}`}: ` +
      `${summary}${outside} em ${check.duration_ms} ms${id ? `; relatório ${id}` : ''}`,
  );
  return {
    outcome: run.status,
    summary,
    badge: totals.project.errors > 0 ? String(totals.project.errors) : undefined,
  };
}

export function checkFile(deps: RunDeps, target?: unknown): Promise<LaceRun<CheckReport> | undefined> {
  const uri = targetUri(target);
  if (uri?.scheme !== 'file') {
    void vscode.window.showWarningMessage('Lace: escolha um arquivo .v ou .sv para verificar.');
    return Promise.resolve(undefined);
  }
  return check(deps, { kind: 'file', file: uri.fsPath });
}

export function checkProcessor(deps: RunDeps, target?: unknown): Promise<LaceRun<CheckReport> | undefined> {
  const name = processorOf(target);
  if (name === undefined) {
    void vscode.window.showWarningMessage('Lace: escolha um processador na vista Fontes.');
    return Promise.resolve(undefined);
  }
  return check(deps, { kind: 'processor', name });
}

function describeScope(scope: CheckScope, project: string): string {
  switch (scope.kind) {
    case 'project':
      return `o projeto ${project}`;
    case 'file':
      return basename(scope.file);
    case 'processor':
      return `o processador ${scope.name}`;
  }
}
