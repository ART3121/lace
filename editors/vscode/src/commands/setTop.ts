// "AURORA: Definir como topo": `aurora top <arquivo>` no projeto ativo. Vem do
// explorador, do editor, da aba e da vista de fontes; quem decide se o
// arquivo pode ser topo é a AURORA, e o erro dele aparece com a dica.

import { basename } from 'node:path';

import * as vscode from 'vscode';

import type { AuroraRun } from '../aurora/client';
import type { TopReport } from '../aurora/aurora-types';
import { formatAuroraError } from '../messages';
import { chooseProject, clientFor, showAuroraError, showProcessError, targetUri, type CommandDeps } from './common';

/**
 * Devolve como o `aurora top` terminou, ou `undefined` se ele nem rodou. Os
 * testes de integração leem o retorno pelo `executeCommand`.
 */
export async function setTop(deps: CommandDeps, target?: unknown): Promise<AuroraRun<TopReport> | undefined> {
  const uri = targetUri(target);
  if (uri?.scheme !== 'file') {
    void vscode.window.showWarningMessage('AURORA: escolha um arquivo .v ou .sv para definir como topo.');
    return undefined;
  }
  const client = clientFor(deps, 'setTop');
  const project = client && (await chooseProject(deps.projects, uri.fsPath));
  if (!client || !project) {
    return undefined;
  }
  try {
    const run = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Window, title: 'AURORA: definindo o topo' },
      () => client.setTop(project.spf, uri.fsPath),
    );
    show(run, uri.fsPath);
    return run;
  } catch (error) {
    showProcessError(deps, error);
    return undefined;
  } finally {
    await deps.status.refresh();
  }
}

function show(run: AuroraRun<TopReport>, file: string): void {
  switch (run.status) {
    case 'succeeded': {
      const { module, module_error: moduleError } = run.result;
      if (moduleError) {
        void vscode.window.showWarningMessage(`AURORA: ${formatAuroraError(moduleError)}`);
      } else {
        vscode.window.setStatusBarMessage(`AURORA: topo ${module ?? basename(file)}`, 5000);
      }
      return;
    }
    case 'error':
      showAuroraError(run.error);
      return;
    default:
      void vscode.window.showErrorMessage(`AURORA: aurora top terminou como ${run.status}.`);
  }
}
