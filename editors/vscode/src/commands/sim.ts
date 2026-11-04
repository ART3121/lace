// "AURORA: Simular": `aurora sim` no projeto (o testbench simulado), num
// testbench (que passa a ser o do projeto) ou num processador. O que o
// testbench escreve sai ao vivo, depois de "|"; no fim, as saídas das
// portas do processador, os testes cocotb e a onda.

import { basename } from 'node:path';

import * as vscode from 'vscode';

import type { AuroraRun } from '../aurora/client';
import type { SimReport } from '../aurora/aurora-types';
import { formatDuration, formatAuroraError } from '../messages';
import type { RunResult } from '../runs';
import { describeBuild, writeRegistered } from './build';
import { processorOf, targetUri } from './common';
import { runOperation, setting, type RunDeps } from './run';
import { openWave } from './wave';

export type SimTarget =
  | { kind: 'project' }
  | { kind: 'testbench'; path: string }
  | { kind: 'processor'; name: string };

export async function simulate(
  deps: RunDeps,
  target: SimTarget,
  options: { fast?: boolean } = {},
): Promise<AuroraRun<SimReport> | undefined> {
  const fast = options.fast ?? false;
  const verilator = setting('aurora.sim', 'verilator', false);
  const timeout = setting('aurora.sim', 'timeout', 0);
  let summary: RunResult | undefined;
  const done = await runOperation(
    deps,
    'sim',
    fast ? 'simFast' : 'sim',
    target.kind === 'testbench' ? target.path : undefined,
    (client, project) => ({
      title: `Simulando ${describeTarget(target, project.name)}${fast ? ', rápida (Verilator, sem onda)' : ''}`,
      reveal: true,
      restart: false,
      start: (signal, onEvent) =>
        client.sim(project.spf, {
          signal,
          onEvent,
          fast,
          verilator: !fast && verilator,
          timeoutSeconds: timeout > 0 ? timeout : undefined,
          testbench: target.kind === 'testbench' ? target.path : undefined,
          processor: target.kind === 'processor' ? target.name : undefined,
        }),
      summarize: (run) => {
        // O sim compila os processadores antes: os erros do YANC vão para o painel.
        deps.builds.apply(run.status !== 'error' ? (run.result?.builds ?? []) : [], project);
        return (summary = summarizeSim(deps.runs.output, run));
      },
    }),
  );
  if (!done) {
    return undefined;
  }
  // O sim compila os processadores e, com um testbench, troca o do projeto.
  await deps.status.refresh();
  const { run } = done;
  const output = 'Saída';
  if (run.status === 'error') {
    void vscode.window.showErrorMessage(`AURORA: ${formatAuroraError(run.error)}`);
  } else if (run.status === 'cancelled') {
    vscode.window.setStatusBarMessage('AURORA: simulação cancelada', 5000);
  } else if (run.status === 'failed') {
    void vscode.window
      .showErrorMessage(`AURORA: a simulação falhou: ${summary?.summary}.`, output)
      .then((choice) => choice === output && deps.runs.output.show());
  } else {
    const wave = 'Abrir a onda';
    const actions = run.result.simulation?.waveform ? [wave, output] : [output];
    void vscode.window
      .showInformationMessage(`AURORA: ${summary?.summary}.`, ...actions)
      .then((choice) => {
        if (choice === wave) {
          void openWave(deps, target.kind === 'processor' ? target : { kind: 'project' });
        } else if (choice === output) {
          deps.runs.output.show();
        }
      });
  }
  return run;
}

/** Escreve o resumo na saída (portas, testes, onda) e o devolve para o botão. */
export function summarizeSim(output: vscode.OutputChannel, run: AuroraRun<SimReport>): RunResult {
  if (run.status === 'error') {
    output.appendLine(`Erro: ${formatAuroraError(run.error)}`);
    return { outcome: 'error', summary: formatAuroraError(run.error) };
  }
  if (run.result === null) {
    output.appendLine('Cancelado.');
    return { outcome: 'cancelled', summary: 'cancelada' };
  }
  const { builds, simulation, outputs, report } = run.result;
  writeRegistered(output, run.result.registered);
  const brokenBuild = builds.find((b) => b.status !== 'succeeded');
  if (simulation === null) {
    if (brokenBuild) {
      output.appendLine(`=== ${describeBuild(brokenBuild)}`);
    }
    const why = brokenBuild ? `o build de ${brokenBuild.processor} terminou como ${brokenBuild.status}` : 'não rodou';
    output.appendLine(`=== Simulação não rodou: ${why}`);
    return { outcome: run.status, summary: why };
  }
  for (const port of outputs) {
    output.appendLine(
      port.error === null ? `Output ${port.port}: ${port.values.join(' ')}` : `Output ${port.port}: ${port.error}`,
    );
  }
  for (const missing of simulation.missing_inputs) {
    output.appendLine(`Aviso: falta ${missing} (o testbench o lê)`);
  }
  const tests = simulation.tests;
  if (tests) {
    for (const test of tests.cases.filter((c) => c.status === 'failed')) {
      output.appendLine(`${test.file ?? ''}${test.line ? `:${test.line}` : ''}: ${test.name}: ${test.message ?? ''}`);
    }
    output.appendLine(`Testes: ${tests.passed} de ${tests.cases.length} passaram`);
  }
  if (simulation.waveform) {
    output.appendLine(`Onda: ${simulation.waveform.path}`);
  }
  const what = `${simulation.top}${simulation.fast ? ' (rápida)' : ''}`;
  const time = formatDuration(simulation.duration_ms);
  const summary =
    simulation.status === 'succeeded'
      ? `${what} simulado em ${time}`
      : simulation.status === 'timed_out'
        ? `${what} passou do --timeout`
        : `${what} terminou como ${simulation.status}` +
          (simulation.failed_step ? ` no passo ${simulation.failed_step}` : '');
  output.appendLine(`=== ${summary}${report ? `; relatório ${report}` : ''}`);
  return {
    outcome: run.status,
    summary: tests ? `${summary}; testes: ${tests.passed} de ${tests.cases.length}` : summary,
    badge: tests && tests.failed > 0 ? String(tests.failed) : undefined,
  };
}

export function simulateTestbench(deps: RunDeps, target?: unknown): Promise<AuroraRun<SimReport> | undefined> {
  const uri = targetUri(target);
  if (uri?.scheme !== 'file') {
    void vscode.window.showWarningMessage('AURORA: escolha um testbench para simular.');
    return Promise.resolve(undefined);
  }
  return simulate(deps, { kind: 'testbench', path: uri.fsPath });
}

export function simulateProcessor(deps: RunDeps, target?: unknown): Promise<AuroraRun<SimReport> | undefined> {
  const name = processorOf(target);
  if (name === undefined) {
    void vscode.window.showWarningMessage('AURORA: escolha um processador na vista Fontes.');
    return Promise.resolve(undefined);
  }
  return simulate(deps, { kind: 'processor', name });
}

function describeTarget(target: SimTarget, project: string): string {
  switch (target.kind) {
    case 'project':
      return `o projeto ${project}`;
    case 'testbench':
      return basename(target.path);
    case 'processor':
      return `o processador ${target.name}`;
  }
}
