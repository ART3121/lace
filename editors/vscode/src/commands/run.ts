// O que build, check, sim, wave e synth dividem: o aurora e o projeto, o executor
// (Runs) e o que fazer quando já há uma operação do mesmo tipo rodando.

import * as vscode from 'vscode';

import type { BuildResults } from '../buildResults';
import type { CheckResults } from '../check';
import type { RtlViewerPanel } from '../rtlViewer';
import type { AuroraClient, AuroraRun } from '../aurora/client';
import type { Feature } from '../aurora/version';
import type { AuroraProject } from '../projects';
import type { RunKind, RunRequest, Runs } from '../runs';
import { chooseProject, clientFor, showProcessError, type CommandDeps } from './common';

export interface RunDeps extends CommandDeps {
  runs: Runs;
  checks: CheckResults;
  builds: BuildResults;
  viewer: RtlViewerPanel;
}

const BUSY: Record<RunKind, string> = {
  build: 'uma compilação',
  check: 'uma verificação',
  sim: 'uma simulação',
  wave: 'a abertura de uma onda',
  synth: 'uma síntese',
};

/**
 * Acha o aurora e o projeto e roda a operação. `undefined` se ela não rodou
 * (sem aurora, sem projeto, outra do mesmo tipo rodando) ou se o aurora saiu
 * fora do contrato (a mensagem já apareceu).
 */
export async function runOperation<T>(
  deps: RunDeps,
  kind: RunKind,
  feature: Feature,
  file: string | undefined,
  build: (client: AuroraClient, project: AuroraProject) => RunRequest<T>,
): Promise<{ run: AuroraRun<T>; project: AuroraProject } | undefined> {
  const client = clientFor(deps, feature);
  const project = client && (await chooseProject(deps.projects, file));
  if (!client || !project) {
    return undefined;
  }
  try {
    const run = await deps.runs.run(kind, build(client, project));
    if (run === 'busy') {
      const cancel = 'Cancelar a que está rodando';
      void vscode.window
        .showInformationMessage(`AURORA: já há ${BUSY[kind]} rodando.`, cancel)
        .then((choice) => choice === cancel && deps.runs.cancel(kind));
      return undefined;
    }
    return { run, project };
  } catch (error) {
    showProcessError(deps, error);
    return undefined;
  }
}

/** A configuração da extensão. */
export function setting<T>(section: string, key: string, fallback: T): T {
  return vscode.workspace.getConfiguration(section).get<T>(key, fallback);
}
