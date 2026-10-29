// "Lace: Definir como topo": `aurora top <arquivo>` no projeto ativo. Vem do
// explorador, do editor, da aba e da vista de fontes; quem decide se o
// arquivo pode ser topo é o Lace, e o erro dele aparece com a dica.

import { basename } from 'node:path';

import * as vscode from 'vscode';

import type { LaceRun } from '../lace/client';
import type { TopReport } from '../lace/lace-types';
import { formatLaceError } from '../messages';
import { chooseProject, clientFor, showLaceError, showProcessError, targetUri, type CommandDeps } from './common';

/**
 * Devolve como o `aurora top` terminou, ou `undefined` se ele nem rodou. Os
 * testes de integração leem o retorno pelo `executeCommand`.
 */
export async function setTop(deps: CommandDeps, target?: unknown): Promise<LaceRun<TopReport> | undefined> {
  const uri = targetUri(target);
  if (uri?.scheme !== 'file') {
    void vscode.window.showWarningMessage('Lace: escolha um arquivo .v ou .sv para definir como topo.');
    return undefined;
  }
  const client = clientFor(deps, 'setTop');
  const project = client && (await chooseProject(deps.projects, uri.fsPath));
  if (!client || !project) {
    return undefined;
  }
  try {
    const run = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Window, title: 'Lace: definindo o topo' },
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

function show(run: LaceRun<TopReport>, file: string): void {
  switch (run.status) {
    case 'succeeded': {
      const { module, module_error: moduleError } = run.result;
      if (moduleError) {
        void vscode.window.showWarningMessage(`Lace: ${formatLaceError(moduleError)}`);
      } else {
        vscode.window.setStatusBarMessage(`Lace: topo ${module ?? basename(file)}`, 5000);
      }
      return;
    }
    case 'error':
      showLaceError(run.error);
      return;
    default:
      void vscode.window.showErrorMessage(`Lace: aurora top terminou como ${run.status}.`);
  }
}
