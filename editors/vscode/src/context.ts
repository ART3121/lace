// As chaves de contexto que as cláusulas `when` do package.json leem: o que
// a vista mostra quando está vazia e onde cada comando aparece.

import * as vscode from 'vscode';

import type { Connection } from './connection';
import { FEATURES, type Feature } from './lace/version';
import type { StatusState } from './projectStatus';
import type { Projects } from './projects';

export type ViewState =
  | 'checking'
  | 'missing'
  | 'unsupported'
  | 'noProject'
  | 'chooseProject'
  | 'loading'
  | 'error'
  | 'ready';

export function viewState(connection: Connection, projects: Projects, status: StatusState): ViewState {
  if (connection.kind !== 'ready') {
    return connection.kind;
  }
  if (projects.all.length === 0) {
    return 'noProject';
  }
  if (!projects.active) {
    return 'chooseProject';
  }
  switch (status.kind) {
    case 'loaded':
      return 'ready';
    case 'error':
      return 'error';
    default:
      return 'loading';
  }
}

/**
 * - `lace.state`: o que a vista vazia mostra (viewsWelcome);
 * - `lace.feature.<recurso>`: o lace está pronto e a versão dele tem o recurso;
 * - `lace.projectActive`: há um projeto ativo;
 * - `lace.topFiles`, `lace.testbenchFiles`, `lace.registeredFiles`: os
 *   arquivos de cada papel, para `resourcePath in ...`.
 */
export function syncContext(connection: Connection, projects: Projects, status: StatusState): void {
  const report = status.kind === 'loaded' ? status.report : undefined;
  const setContext = (key: string, value: unknown) =>
    void vscode.commands.executeCommand('setContext', key, value);
  setContext('lace.state', viewState(connection, projects, status));
  setContext('lace.projectActive', projects.active !== undefined);
  for (const feature of Object.keys(FEATURES) as Feature[]) {
    setContext(`lace.feature.${feature}`, connection.kind === 'ready' && connection.features[feature]);
  }
  // `resourcePath in lace.topFiles` compara com o fsPath do recurso; o
  // Uri.file normaliza do mesmo jeito (a letra do drive no Windows).
  const testbenches = report?.testbench.map((file) => file.path) ?? [];
  const design = report?.synthesizable.map((file) => file.path) ?? [];
  setContext('lace.topFiles', pathSet(report?.top_level ? [report.top_level] : []));
  setContext('lace.testbenchFiles', pathSet(testbenches));
  setContext('lace.registeredFiles', pathSet([...design, ...testbenches]));
}

function pathSet(paths: string[]): Record<string, true> {
  return Object.fromEntries(paths.map((path) => [vscode.Uri.file(path).fsPath, true]));
}
