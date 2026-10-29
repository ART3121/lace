// "AURORA: Abrir a onda": `aurora wave`, que abre o surfer-aurora e volta com
// ele aberto (o aurora espera 1,5 s para ver se o Surfer ficou de pé). A onda
// do projeto é a do testbench simulado; a de um processador, a da simulação
// dele; e um .vcd, .fst ou .ghw abre direto, com ou sem projeto.

import { basename } from 'node:path';

import * as vscode from 'vscode';

import type { AuroraRun, WaveTarget } from '../aurora/client';
import type { WaveReport } from '../aurora/aurora-types';
import { formatAuroraError } from '../messages';
import { chooseProject, clientFor, processorOf, showAuroraError, showProcessError, targetUri } from './common';
import type { RunDeps } from './run';

export async function openWave(deps: RunDeps, target: WaveTarget): Promise<AuroraRun<WaveReport> | undefined> {
  const client = clientFor(deps, 'wave');
  if (!client) {
    return undefined;
  }
  const project = target.kind === 'file' ? deps.projects.active : await chooseProject(deps.projects);
  if (target.kind !== 'file' && !project) {
    return undefined;
  }
  try {
    const run = await deps.runs.run('wave', {
      reveal: false,
      restart: false,
      start: () => client.wave(project?.spf, target),
      summarize: (r) =>
        r.status === 'succeeded'
          ? { outcome: 'succeeded', summary: `${basename(r.result.waveform)} aberta` }
          : r.status === 'error'
            ? { outcome: 'error', summary: formatAuroraError(r.error) }
            : { outcome: r.status, summary: r.status },
    });
    if (run === 'busy') {
      return undefined;
    }
    if (run.status === 'succeeded') {
      vscode.window.setStatusBarMessage(`$(pulse) AURORA: ${basename(run.result.waveform)} aberta no surfer-aurora`, 6000);
    } else if (run.status === 'error') {
      showAuroraError(run.error);
    } else {
      void vscode.window.showErrorMessage(`AURORA: aurora wave terminou como ${run.status}.`);
    }
    return run;
  } catch (error) {
    showProcessError(deps, error);
    return undefined;
  }
}

export function openProcessorWave(deps: RunDeps, target?: unknown): Promise<AuroraRun<WaveReport> | undefined> {
  const name = processorOf(target);
  if (name === undefined) {
    void vscode.window.showWarningMessage('AURORA: escolha um processador na vista Fontes.');
    return Promise.resolve(undefined);
  }
  return openWave(deps, { kind: 'processor', name });
}

export function openWaveFile(deps: RunDeps, target?: unknown): Promise<AuroraRun<WaveReport> | undefined> {
  const uri = targetUri(target);
  if (uri?.scheme !== 'file') {
    void vscode.window.showWarningMessage('AURORA: escolha um arquivo .vcd, .fst ou .ghw.');
    return Promise.resolve(undefined);
  }
  return openWave(deps, { kind: 'file', path: uri.fsPath });
}
