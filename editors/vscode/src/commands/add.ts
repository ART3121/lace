// Pôr arquivos Verilog no projeto: `aurora add`. Vem do + dos não
// registrados, do botão direito no explorador, de "Novo arquivo Verilog"
// (que o Lace cria a partir do modelo) e do aviso que aparece quando surge
// um .v novo fora do projeto.

import { existsSync, statSync } from 'node:fs';
import { basename, extname, isAbsolute, join, relative } from 'node:path';

import * as vscode from 'vscode';

import type { AuroraRun } from '../aurora/client';
import type { AddReport, AddedFile } from '../aurora/aurora-types';
import { nameSuggestsTestbench } from '../verilog';
import { chooseProject, clientFor, showAuroraError, showProcessError, targetPaths, type CommandDeps } from './common';

/** `aurora add` com os arquivos; abre no editor os que o Lace criou do modelo. */
export async function addFiles(
  deps: CommandDeps,
  files: readonly string[],
  testbench = false,
): Promise<AuroraRun<AddReport> | undefined> {
  if (files.length === 0) {
    void vscode.window.showWarningMessage('Lace: escolha um ou mais arquivos .v ou .sv para pôr no projeto.');
    return undefined;
  }
  const client = clientFor(deps, 'add');
  const project = client && (await chooseProject(deps.projects, files[0]));
  if (!client || !project) {
    return undefined;
  }
  try {
    const run = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Window, title: 'Lace: pondo no projeto' },
      () => client.add(project.spf, files, { testbench }),
    );
    if (run.status === 'succeeded') {
      vscode.window.setStatusBarMessage(`$(add) Lace: ${describeAdded(run.result.files)}`, 8000);
      for (const file of run.result.files.filter((f) => f.created)) {
        await vscode.window.showTextDocument(vscode.Uri.file(file.path), { preview: false });
      }
    } else if (run.status === 'error') {
      showAuroraError(run.error);
    } else {
      void vscode.window.showErrorMessage(`Lace: aurora add terminou como ${run.status}.`);
    }
    return run;
  } catch (error) {
    showProcessError(deps, error);
    return undefined;
  } finally {
    await deps.status.refresh();
  }
}

/** Do explorador ou da vista: o item clicado, a seleção inteira ou um grupo. */
export function addSelected(
  deps: CommandDeps,
  first: unknown,
  all: unknown,
  testbench = false,
): Promise<AuroraRun<AddReport> | undefined> {
  return addFiles(deps, targetPaths(first, all), testbench);
}

/** Os dados de "Novo arquivo Verilog" sem perguntar: o que os testes passam. */
interface NewFileArgs {
  path: string;
  testbench?: boolean;
}

/**
 * "Lace: Novo arquivo Verilog": pergunta o caminho e o tipo e deixa o
 * `aurora add` criar o arquivo do modelo (o módulo com portas de exemplo que
 * compilam, ou o testbench que instancia o módulo testado).
 */
export async function newFile(deps: CommandDeps, target?: unknown): Promise<AuroraRun<AddReport> | undefined> {
  const project = deps.projects.active ?? (await deps.projects.select());
  if (!project) {
    return undefined;
  }
  const given = target as Partial<NewFileArgs> | undefined;
  if (typeof given?.path === 'string') {
    const path = isAbsolute(given.path) ? given.path : join(project.root, given.path);
    return addFiles(deps, [path], given.testbench ?? false);
  }
  // Do botão direito numa pasta do explorador, o caminho já começa nela.
  const folder = target instanceof vscode.Uri ? relative(project.root, target.fsPath) : '';
  const prefix = folder && !folder.startsWith('..') ? `${folder.split('\\').join('/')}/` : '';
  const typed = await vscode.window.showInputBox({
    title: `Novo arquivo Verilog em ${project.name}`,
    prompt: 'O caminho a partir da pasta do projeto. .v e .sv viram módulo ou testbench; .py, testbench cocotb.',
    value: prefix,
    valueSelection: [prefix.length, prefix.length],
    placeHolder: 'rtl/alu.v',
    validateInput: (value) => validateNewFile(project.root, value),
  });
  if (!typed) {
    return undefined;
  }
  const path = isAbsolute(typed) ? typed : join(project.root, typed);
  let testbench = false;
  if (extname(path) !== '.py') {
    const module = { label: 'Módulo', description: 'com portas de exemplo que compilam', testbench: false };
    const bench = {
      label: 'Testbench',
      description: 'instancia o módulo testado, grava a onda e termina com $finish',
      testbench: true,
    };
    const items = nameSuggestsTestbench(basename(path)) ? [bench, module] : [module, bench];
    const kind = await vscode.window.showQuickPick(items, { title: `Que tipo de arquivo é ${basename(path)}?` });
    if (!kind) {
      return undefined;
    }
    testbench = kind.testbench;
  }
  return addFiles(deps, [path], testbench);
}

function validateNewFile(root: string, value: string): string | undefined {
  const name = value.trim();
  if (name === '' || name.endsWith('/')) {
    return 'Digite o nome do arquivo';
  }
  if (!/\.(s?v|py)$/.test(name)) {
    return 'O nome termina em .v, .sv ou .py';
  }
  if (existsSync(isAbsolute(name) ? name : join(root, name))) {
    return 'Este arquivo já existe: use Adicionar ao projeto';
  }
  return undefined;
}

/**
 * O aviso de arquivo novo: aparece quando o `aurora status` passa a listar um
 * .v ou .sv que não estava entre os não registrados. Desliga com
 * aurora.add.askForNewFiles.
 */
export async function offerToAdd(deps: CommandDeps, paths: readonly string[]): Promise<void> {
  const config = vscode.workspace.getConfiguration('aurora.add');
  if (!config.get<boolean>('askForNewFiles', true) || deps.connection.state.kind !== 'ready') {
    return;
  }
  const never = 'Não perguntar de novo';
  if (paths.length > 1) {
    const all = 'Adicionar todos';
    const names = paths.map((p) => basename(p)).join(', ');
    const choice = await vscode.window.showInformationMessage(
      `Lace: ${paths.length} arquivos Verilog novos fora do projeto: ${names}.`,
      all,
      never,
    );
    if (choice === all) {
      await addFiles(deps, paths);
    } else if (choice === never) {
      await config.update('askForNewFiles', false, vscode.ConfigurationTarget.Global);
    }
    return;
  }
  const [path] = paths;
  const name = basename(path);
  // Um arquivo vazio (o "Novo arquivo" do explorador) pode sair do modelo,
  // como no `aurora add` de um arquivo que não existe.
  // Sem --tb, o Lace decide pelo conteúdo: não há como forçar módulo.
  const template = 'Adicionar com o modelo';
  const add = 'Adicionar';
  const asBench = 'Adicionar como testbench';
  const actions = isEmptyAndClean(path)
    ? [template]
    : nameSuggestsTestbench(name)
      ? [asBench, add]
      : [add, asBench];
  const choice = await vscode.window.showInformationMessage(
    `Lace: ${name} é um arquivo novo fora do projeto.`,
    ...actions,
    never,
  );
  if (choice === template) {
    await addFromTemplate(deps, path);
  } else if (choice === add || choice === asBench) {
    await addFiles(deps, [path], choice === asBench);
  } else if (choice === never) {
    await config.update('askForNewFiles', false, vscode.ConfigurationTarget.Global);
  }
}

/**
 * Apaga o arquivo vazio e deixa o `aurora add` criá-lo do modelo, que decide
 * entre módulo e testbench pelo nome. Se o Lace recusa, o arquivo vazio
 * volta.
 */
async function addFromTemplate(deps: CommandDeps, path: string): Promise<void> {
  if (!isEmptyAndClean(path)) {
    // Ganhou conteúdo enquanto o aviso estava aberto: só registra.
    await addFiles(deps, [path]);
    return;
  }
  const uri = vscode.Uri.file(path);
  await vscode.workspace.fs.delete(uri);
  const run = await addFiles(deps, [path]);
  if (run?.status !== 'succeeded' && !existsSync(path)) {
    await vscode.workspace.fs.writeFile(uri, new Uint8Array());
  }
}

/** Vazio no disco e sem texto por gravar num editor aberto. */
function isEmptyAndClean(path: string): boolean {
  try {
    if (statSync(path).size !== 0) {
      return false;
    }
  } catch {
    return false;
  }
  const fsPath = vscode.Uri.file(path).fsPath;
  return !vscode.workspace.textDocuments.some((d) => d.uri.fsPath === fsPath && d.isDirty);
}

function describeAdded(files: readonly AddedFile[]): string {
  if (files.length !== 1) {
    return `${files.length} arquivos entraram no projeto`;
  }
  const [file] = files;
  const role = file.role === 'testbench' ? 'testbench' : 'módulo';
  const selected = file.selected ? (file.role === 'testbench' ? ', o simulado' : ', o topo') : '';
  return `${basename(file.path)} ${file.created ? 'criado' : 'registrado'} como ${role}${selected}`;
}
