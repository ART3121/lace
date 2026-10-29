// O que todo comando faz antes de chamar a AURORA: conferir que o aurora está
// pronto e tem o recurso, achar o projeto e o arquivo do clique.

import { basename } from 'node:path';

import * as vscode from 'vscode';

import type { AuroraConnection } from '../connection';
import type { AuroraClient } from '../aurora/client';
import type { ErrorInfo } from '../aurora/aurora-types';
import { FEATURES, type Feature } from '../aurora/version';
import { formatAuroraError } from '../messages';
import type { ProjectStatus } from '../projectStatus';
import type { AuroraProject, Projects } from '../projects';
import { isSourceNode, type SourceNode } from '../sources/provider';

export interface CommandDeps {
  connection: AuroraConnection;
  projects: Projects;
  status: ProjectStatus;
  log: vscode.LogOutputChannel;
}

/** O cliente, se o aurora está pronto e a versão tem o recurso; senão avisa e devolve `undefined`. */
export function clientFor(deps: CommandDeps, feature: Feature): AuroraClient | undefined {
  const connection = deps.connection.state;
  if (connection.kind !== 'ready') {
    void vscode.window.showWarningMessage('AURORA: o aurora não está disponível. Veja a saída AURORA.');
    return undefined;
  }
  if (!connection.features[feature]) {
    void vscode.window.showWarningMessage(
      `AURORA: isto precisa da AURORA ${FEATURES[feature]} ou mais novo, e o instalado é o ${connection.version}.`,
    );
    return undefined;
  }
  return connection.client;
}

/**
 * O projeto ativo (ou o que o usuário escolher, se não há). Com `file`, se o
 * arquivo está na pasta de outro projeto do workspace, pergunta antes: senão
 * o comando agiria no projeto errado.
 */
export async function chooseProject(projects: Projects, file?: string): Promise<AuroraProject | undefined> {
  const active = projects.active ?? (await projects.select());
  if (!active || file === undefined) {
    return active;
  }
  const owner = projects.owner(file);
  if (!owner || owner.spf === active.spf) {
    return active;
  }
  const useOwner = `Usar ${owner.name}`;
  const keep = `Manter ${active.name}`;
  const choice = await vscode.window.showWarningMessage(
    `AURORA: ${basename(file)} está na pasta do projeto ${owner.name}, e o projeto ativo é ${active.name}.`,
    useOwner,
    keep,
  );
  if (choice === useOwner) {
    await projects.setActive(owner);
    return owner;
  }
  return choice === keep ? active : undefined;
}

/** O arquivo do clique: o URI do explorador ou do editor, o item da vista, ou o editor ativo. */
export function targetUri(target: unknown): vscode.Uri | undefined {
  if (target instanceof vscode.Uri) {
    return target;
  }
  if (isSourceNode(target) && target.kind === 'entry' && target.entry.path !== null) {
    return vscode.Uri.file(target.entry.path);
  }
  return vscode.window.activeTextEditor?.document.uri;
}

/**
 * Os arquivos de um clique que pode ter vários: o VS Code passa o item
 * clicado e, com seleção múltipla, a lista inteira. Um grupo vale pelos
 * itens dele.
 */
export function targetPaths(first: unknown, all: unknown): string[] {
  const items = Array.isArray(all) && all.length > 0 ? all : first === undefined ? [] : [first];
  const paths = items.flatMap((item): string[] => {
    if (item instanceof vscode.Uri) {
      return item.scheme === 'file' ? [item.fsPath] : [];
    }
    if (isSourceNode(item)) {
      const entries = item.kind === 'group' ? item.group.entries : [item.entry];
      return entries.flatMap((e) => (e.path !== null ? [e.path] : []));
    }
    return [];
  });
  return [...new Set(paths)];
}

/** O processador de um item da vista, pelo nome. */
export function processorOf(target: unknown): string | undefined {
  const node = target as SourceNode | undefined;
  return isSourceNode(node) && node.kind === 'entry' && node.entry.role === 'processor' ? node.entry.label : undefined;
}

export function showAuroraError(error: ErrorInfo): void {
  void vscode.window.showErrorMessage(`AURORA: ${formatAuroraError(error)}`);
}

/** O aurora não rodou, ou saiu fora do contrato: a mensagem vai para a tela e para a saída AURORA. */
export function showProcessError(deps: CommandDeps, error: unknown): void {
  const message = (error as Error).message;
  deps.log.error(message);
  void vscode.window.showErrorMessage(`AURORA: ${message}`);
}
