// "Lace: Compilar": `aurora build`, os processadores com o YANC (C± ou C para
// Verilog, memórias e testbench). Todos, pelo botão Build da barra de status,
// ou um, pelo processador na vista. Os erros vão para o painel Problemas,
// com o arquivo e a linha do fonte.

import * as vscode from 'vscode';

import type { AuroraRun } from '../aurora/client';
import type { AddedFile, BuildReport, BuildResult } from '../aurora/aurora-types';
import { formatDuration, formatAuroraError } from '../messages';
import { describeCount } from '../problems';
import type { RunResult } from '../runs';
import { processorOf } from './common';
import { runOperation, type RunDeps } from './run';

export type BuildTarget = { kind: 'all' } | { kind: 'processor'; name: string };

export async function build(deps: RunDeps, target: BuildTarget): Promise<AuroraRun<BuildReport> | undefined> {
  let summary: RunResult | undefined;
  const done = await runOperation(deps, 'build', 'processors', undefined, (client, project) => ({
    title: target.kind === 'processor' ? `Compilando o processador ${target.name}` : 'Compilando os processadores',
    reveal: true,
    restart: false,
    start: (signal, onEvent) =>
      client.build(project.spf, {
        signal,
        onEvent,
        processors: target.kind === 'processor' ? [target.name] : undefined,
      }),
    summarize: (run) => {
      deps.builds.apply(run.status !== 'error' ? (run.result?.results ?? []) : [], project);
      return (summary = summarizeBuild(deps.runs.output, run));
    },
  }));
  if (!done) {
    return undefined;
  }
  // O build muda o "compilado" dos processadores.
  await deps.status.refresh();
  const { run } = done;
  if (run.status === 'error') {
    void vscode.window.showErrorMessage(`Lace: ${formatAuroraError(run.error)}`);
  } else if (run.status === 'failed') {
    const problems = 'Problemas';
    void vscode.window
      .showErrorMessage(`Lace: ${summary?.summary}.`, problems)
      .then((choice) => choice === problems && vscode.commands.executeCommand('workbench.actions.view.problems'));
  } else if (run.status === 'succeeded') {
    vscode.window.setStatusBarMessage(`$(pass) Lace: ${summary?.summary}`, 6000);
  }
  return run;
}

export function buildProcessor(deps: RunDeps, target?: unknown): Promise<AuroraRun<BuildReport> | undefined> {
  const name = processorOf(target);
  if (name === undefined) {
    void vscode.window.showWarningMessage('Lace: escolha um processador na vista Fontes.');
    return Promise.resolve(undefined);
  }
  return build(deps, { kind: 'processor', name });
}

/** Uma linha por processador na saída, e o resumo para o botão. */
export function summarizeBuild(output: vscode.OutputChannel, run: AuroraRun<BuildReport>): RunResult {
  if (run.status === 'error') {
    output.appendLine(`Erro: ${formatAuroraError(run.error)}`);
    return { outcome: 'error', summary: formatAuroraError(run.error) };
  }
  if (run.result === null) {
    output.appendLine('Cancelado.');
    return { outcome: 'cancelled', summary: 'cancelado' };
  }
  const { results, report } = run.result;
  if (results.length === 0) {
    output.appendLine('=== O projeto não tem processadores');
    return { outcome: run.status, summary: 'o projeto não tem processadores' };
  }
  for (const build of results) {
    output.appendLine(`=== ${describeBuild(build)}`);
  }
  writeRegistered(output, run.result.registered);
  const failed = results.filter((b) => b.status !== 'succeeded');
  const errors = results.flatMap((b) => b.diagnostics).filter((d) => d.severity === 'error').length;
  if (report) {
    output.appendLine(`Relatório ${report}`);
  }
  const summary =
    failed.length === 0
      ? results.length === 1
        ? `${results[0].processor} compilado`
        : `${results.length} processadores compilados`
      : `o build de ${failed.map((b) => b.processor).join(', ')} falhou`;
  return { outcome: run.status, summary, badge: errors > 0 ? String(errors) : undefined };
}

/**
 * O `.v` e o testbench que o build gerou e o Lace pôs no `.spf` agora.
 * Um Lace sem isso (até a 0.6.0) não manda o campo.
 */
export function writeRegistered(output: vscode.OutputChannel, files: readonly AddedFile[] | undefined): void {
  for (const file of files ?? []) {
    const testbench = file.role === 'testbench';
    const mark = file.selected ? (testbench ? ', o testbench simulado' : ', o topo') : '';
    output.appendLine(`Entrou no projeto: ${file.path} (${testbench ? 'testbench' : 'RTL'}${mark})`);
  }
}

export function describeBuild(build: BuildResult): string {
  const counts = describeCount({
    errors: build.diagnostics.filter((d) => d.severity === 'error').length,
    warnings: build.diagnostics.filter((d) => d.severity === 'warning').length,
  });
  const language = build.language === 'cmm' ? 'C±' : 'C';
  return build.status === 'succeeded'
    ? `${build.processor} (${language}) compilado em ${formatDuration(build.duration_ms)}: ${counts}`
    : `${build.processor} (${language}): ${build.status}${build.failed_step ? ` no passo ${build.failed_step}` : ''}; ${counts}`;
}
