// "Lace: Novo projeto": `aurora new`, a partir do botão da vista Fontes numa
// pasta sem .spf (ou da paleta). O Lace cria `<pasta>/<nome>/<nome>.spf`; se
// a pasta `<nome>` já existe, o .spf vai para dentro dela, e é assim que a
// própria pasta aberta vira o projeto.

import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, join, sep } from 'node:path';

import * as vscode from 'vscode';

import type { AuroraRun } from '../aurora/client';
import type { NewReport } from '../aurora/aurora-types';
import { clientFor, showAuroraError, showProcessError, type CommandDeps } from './common';

/**
 * O nome que `aurora new` aceita (docs/CLI.md, "Projeto"): letras sem acento,
 * dígitos, `_` e `-`, começando por letra, até 64. Só decide se "Nesta pasta"
 * aparece na lista; quem confere o nome é o Lace.
 */
const PROJECT_NAME = /^[A-Za-z][A-Za-z0-9_-]{0,63}$/;

/** Os dados sem perguntar: o que os testes passam. */
interface NewProjectArgs {
  name: string;
  dir: string;
}

export async function newProject(deps: CommandDeps, target?: unknown): Promise<AuroraRun<NewReport> | undefined> {
  const client = clientFor(deps, 'newProject');
  if (!client) {
    return undefined;
  }
  const given = target as Partial<NewProjectArgs> | undefined;
  const place =
    typeof given?.name === 'string' && typeof given.dir === 'string'
      ? { name: given.name, dir: given.dir }
      : await askPlace();
  if (!place) {
    return undefined;
  }
  let run: AuroraRun<NewReport>;
  try {
    run = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Window, title: 'Lace: criando o projeto' },
      () => client.newProject(place.name, place.dir),
    );
  } catch (error) {
    showProcessError(deps, error);
    return undefined;
  }
  if (run.status !== 'succeeded') {
    if (run.status === 'error') {
      showAuroraError(run.error);
    }
    return run;
  }
  await afterCreated(deps, run.result);
  return run;
}

/** Onde e com que nome: pergunta, e devolve `undefined` se o usuário desistiu. */
async function askPlace(): Promise<NewProjectArgs | undefined> {
  const folder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  type Choice = vscode.QuickPickItem & { where: 'here' | 'inside' | 'elsewhere' };
  const items: Choice[] = [];
  if (folder && PROJECT_NAME.test(basename(folder))) {
    items.push({
      where: 'here',
      label: 'Nesta pasta',
      description: folder,
      detail: `O projeto ${basename(folder)}, com o .spf na pasta aberta`,
    });
  }
  if (folder) {
    items.push({
      where: 'inside',
      label: 'Numa pasta nova dentro desta',
      description: `${folder}${sep}<nome>`,
      detail: PROJECT_NAME.test(basename(folder))
        ? undefined
        : 'O nome da pasta aberta não serve de nome de projeto (letras sem acento, dígitos, _ e -)',
    });
  }
  items.push({ where: 'elsewhere', label: 'Em outra pasta...', description: 'Escolha onde criar a pasta do projeto' });

  const choice = items.length === 1 ? items[0] : await vscode.window.showQuickPick(items, { title: 'Novo projeto do Lace: onde criar?' });
  if (!choice) {
    return undefined;
  }
  if (choice.where === 'here' && folder) {
    // `aurora new <pasta> --dir <pai>`: a pasta existe, e o .spf vai para dentro dela.
    return { name: basename(folder), dir: dirname(folder) };
  }
  let dir = folder;
  if (choice.where === 'elsewhere' || !dir) {
    const projects = join(homedir(), 'Projetos');
    const picked = await vscode.window.showOpenDialog({
      title: 'Onde criar a pasta do projeto',
      canSelectFolders: true,
      canSelectFiles: false,
      canSelectMany: false,
      openLabel: 'Criar aqui',
      defaultUri: vscode.Uri.file(existsSync(projects) ? projects : homedir()),
    });
    if (!picked?.[0]) {
      return undefined;
    }
    dir = picked[0].fsPath;
  }
  const name = await vscode.window.showInputBox({
    title: 'Novo projeto do Lace',
    prompt: `O nome do projeto e da pasta dele, em ${dir}: letras sem acento, dígitos, _ e -, começando por letra`,
    placeHolder: 'contador',
    validateInput: (value) => (value.trim() === '' ? 'Digite o nome do projeto' : undefined),
  });
  return name ? { name: name.trim(), dir } : undefined;
}

/**
 * Dentro do workspace, o projeto novo vira o ativo e a vista Fontes aparece;
 * fora, oferece abrir a pasta dele.
 */
async function afterCreated(deps: CommandDeps, created: NewReport): Promise<void> {
  const root = vscode.Uri.file(created.root);
  if (vscode.workspace.getWorkspaceFolder(root)) {
    await deps.projects.discover();
    const project = deps.projects.all.find((p) => p.spf === vscode.Uri.file(created.path).fsPath);
    if (project) {
      await deps.projects.setActive(project);
    }
    vscode.window.setStatusBarMessage(`$(pass) Lace: projeto ${basename(created.root)} criado`, 6000);
    return;
  }
  const open = 'Abrir';
  const newWindow = 'Abrir em outra janela';
  const add = 'Acrescentar ao workspace';
  const actions = vscode.workspace.workspaceFolders?.length ? [open, newWindow, add] : [open, newWindow];
  const choice = await vscode.window.showInformationMessage(
    `Lace: projeto ${basename(created.root)} criado em ${created.root}.`,
    ...actions,
  );
  if (choice === open || choice === newWindow) {
    await vscode.commands.executeCommand('vscode.openFolder', root, { forceNewWindow: choice === newWindow });
  } else if (choice === add) {
    const count = vscode.workspace.workspaceFolders?.length ?? 0;
    vscode.workspace.updateWorkspaceFolders(count, 0, { uri: root });
  }
}
