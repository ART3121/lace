// As ferramentas de arquivo do projeto: tirar do projeto (`lace remove`, o
// arquivo fica no disco), mudar a ordem de compilação (`lace order`, pelos
// botões e arrastando na vista) e renomear ou mover (`lace move`). E o que
// acontece no explorador do VS Code: um arquivo do projeto renomeado ou
// movido lá é registrado no lugar novo (`lace move --after`); um apagado sai
// do projeto (`lace remove`).

import { existsSync } from 'node:fs';
import { basename, extname, isAbsolute, join, relative, resolve, sep } from 'node:path';

import * as vscode from 'vscode';

import type { ListPosition, LaceRun } from '../lace/client';
import type { MoveReport, OrderReport, RemoveReport, StatusReport } from '../lace/lace-types';
import { FEATURES } from '../lace/version';
import type { LaceProject } from '../projects';
import { isSourceNode } from '../sources/provider';
import { chooseProject, clientFor, showLaceError, showProcessError, targetPaths, type CommandDeps } from './common';

/** Os arquivos registrados no `.spf`, nas duas listas. */
export function registeredPaths(status: StatusReport | undefined): string[] {
  return status ? [...status.synthesizable, ...status.testbench].map((f) => f.path) : [];
}

/** `path` é `dir` ou está dentro dele. */
export function isInside(path: string, dir: string): boolean {
  return path === dir || path.startsWith(dir.endsWith(sep) ? dir : dir + sep);
}

/** "Tirar do projeto": `lace remove` dos registrados da seleção. O arquivo fica no disco. */
export async function removeFromProject(
  deps: CommandDeps,
  first: unknown,
  all: unknown,
): Promise<LaceRun<RemoveReport> | undefined> {
  const registered = new Set(registeredPaths(deps.status.report));
  const paths = targetPaths(first, all).filter((path) => registered.has(path));
  if (paths.length === 0) {
    void vscode.window.showWarningMessage('Lace: escolha um ou mais arquivos do projeto.');
    return undefined;
  }
  const client = clientFor(deps, 'files');
  const project = client && (await chooseProject(deps.projects, paths[0]));
  if (!client || !project) {
    return undefined;
  }
  const run = await runAndRefresh(deps, () => client.remove(project.spf, paths));
  if (run?.status === 'succeeded') {
    const what = run.result.removed.length === 1 ? basename(run.result.removed[0]) : `${run.result.removed.length} arquivos`;
    vscode.window.setStatusBarMessage(`$(remove) Lace: ${what} saiu do projeto; continua no disco`, 8000);
  }
  return run;
}

export type OrderMove = 'up' | 'down' | 'first' | 'last';

/** "Subir", "Descer", "Primeiro", "Último": a ordem em que os compiladores leem. */
export async function reorder(
  deps: CommandDeps,
  target: unknown,
  move: OrderMove,
): Promise<LaceRun<OrderReport> | undefined> {
  const path = listedPath(target);
  const list = path === undefined ? undefined : listOf(deps.status.report, path);
  if (path === undefined || list === undefined) {
    void vscode.window.showWarningMessage('Lace: escolha um arquivo do RTL ou dos testbenches na vista Fontes.');
    return undefined;
  }
  const position = positionFor(list, path, move);
  if (position === undefined) {
    return undefined;
  }
  return order(deps, path, position);
}

/** A posição de `lace order` para o movimento; `undefined` se o arquivo já está lá. */
export function positionFor(list: readonly string[], path: string, move: OrderMove): ListPosition | undefined {
  const index = list.indexOf(path);
  switch (move) {
    case 'up':
      return index > 0 ? { kind: 'before', other: list[index - 1] } : undefined;
    case 'down':
      return index >= 0 && index < list.length - 1 ? { kind: 'after', other: list[index + 1] } : undefined;
    case 'first':
      return index > 0 ? { kind: 'first' } : undefined;
    case 'last':
      return index >= 0 && index < list.length - 1 ? { kind: 'last' } : undefined;
  }
}

/**
 * Arrastar na vista: os arquivos soltos sobre outro da mesma lista vão para
 * o lugar dele (antes, quem sobe; depois, quem desce); soltos sobre o grupo,
 * para o começo. Na ordem em que estavam.
 */
export async function dropInList(deps: CommandDeps, dragged: readonly string[], target: string | 'start'): Promise<void> {
  const list = listOf(deps.status.report, target === 'start' ? dragged[0] : target);
  if (!list) {
    return;
  }
  const moving = list.filter((path) => dragged.includes(path) && path !== target);
  if (moving.length === 0) {
    return;
  }
  if (target === 'start') {
    for (const path of [...moving].reverse()) {
      if ((await order(deps, path, { kind: 'first' }, false))?.status !== 'succeeded') break;
    }
  } else if (list.indexOf(moving[0]) > list.indexOf(target)) {
    for (const path of moving) {
      if ((await order(deps, path, { kind: 'before', other: target }, false))?.status !== 'succeeded') break;
    }
  } else {
    for (const path of [...moving].reverse()) {
      if ((await order(deps, path, { kind: 'after', other: target }, false))?.status !== 'succeeded') break;
    }
  }
  await deps.status.refresh();
}

async function order(
  deps: CommandDeps,
  path: string,
  position: ListPosition,
  refresh = true,
): Promise<LaceRun<OrderReport> | undefined> {
  const client = clientFor(deps, 'files');
  const project = client && (await chooseProject(deps.projects, path));
  if (!client || !project) {
    return undefined;
  }
  return runAndRefresh(deps, () => client.order(project.spf, path, position), refresh);
}

/** Para os testes: o caminho novo, sem perguntar. */
interface RenameArgs {
  to: string;
}

/**
 * "Renomear ou mover": `lace move`, que move no disco e mantém o arquivo
 * no projeto, com o topo e o testbench simulado. O caminho é relativo à
 * pasta do projeto: trocar a pasta move. As abas abertas acompanham.
 */
export async function renameInProject(
  deps: CommandDeps,
  target: unknown,
  args?: unknown,
): Promise<LaceRun<MoveReport> | undefined> {
  const from = listedPath(target) ?? unregisteredPath(target);
  if (from === undefined) {
    void vscode.window.showWarningMessage('Lace: escolha um arquivo na vista Fontes.');
    return undefined;
  }
  const client = clientFor(deps, 'files');
  const project = client && (await chooseProject(deps.projects, from));
  if (!client || !project) {
    return undefined;
  }
  const current = relative(project.root, from);
  const name = basename(from);
  const start = current.length - name.length;
  const typed =
    (args as RenameArgs | undefined)?.to ??
    (await vscode.window.showInputBox({
      title: `Renomear ou mover ${name}`,
      prompt: 'O caminho a partir da pasta do projeto. Os arquivos registrados continuam no projeto.',
      value: current,
      valueSelection: [start, start + name.length - extname(name).length],
      validateInput: (value) => validateDestination(project, from, value),
    }));
  if (typed === undefined) {
    return undefined;
  }
  const to = isAbsolute(typed) ? typed : resolve(project.root, typed);
  const run = await runAndRefresh(deps, () => client.move(project.spf, from, to));
  if (run?.status === 'succeeded') {
    await followInEditors(from, to);
  }
  return run;
}

function validateDestination(project: LaceProject, from: string, value: string): string | undefined {
  if (value.trim() === '') {
    return 'Dê o caminho novo.';
  }
  const to = isAbsolute(value) ? value : resolve(project.root, value);
  if (!isInside(to, project.root) || to === project.root) {
    return 'O caminho tem que ficar dentro da pasta do projeto.';
  }
  if (to === from) {
    return 'É o caminho de agora.';
  }
  if (existsSync(to)) {
    return 'Já existe um arquivo ou pasta com esse caminho.';
  }
  return undefined;
}

/**
 * As abas do arquivo movido (ou dos de dentro da pasta) passam ao caminho
 * novo. Uma aba com alterações não salvas fica: fechar perderia o que não
 * foi salvo, e o VS Code a grava no caminho velho.
 */
export async function followInEditors(from: string, to: string): Promise<void> {
  const tabs = vscode.window.tabGroups.all.flatMap((group) =>
    group.tabs.flatMap((tab) => {
      if (!(tab.input instanceof vscode.TabInputText) || tab.input.uri.scheme !== 'file') {
        return [];
      }
      const path = tab.input.uri.fsPath;
      return isInside(path, from) ? [{ tab, group, moved: path === from ? to : join(to, path.slice(from.length + 1)) }] : [];
    }),
  );
  const dirty: string[] = [];
  for (const { tab, group, moved } of tabs) {
    if (tab.isDirty) {
      dirty.push(basename(moved));
      continue;
    }
    const old = (tab.input as vscode.TabInputText).uri.toString();
    await vscode.window.showTextDocument(vscode.Uri.file(moved), {
      viewColumn: group.viewColumn,
      preview: tab.isPreview,
      preserveFocus: !tab.isActive,
    });
    // Uma aba de pré-visualização já foi substituída pela nova; as outras
    // continuam lá, e os objetos Tab de antes podem não valer mais.
    const stale = vscode.window.tabGroups.all
      .flatMap((g) => g.tabs)
      .filter((t) => t.input instanceof vscode.TabInputText && t.input.uri.toString() === old && !t.isDirty);
    if (stale.length > 0) {
      await vscode.window.tabGroups.close(stale);
    }
  }
  if (dirty.length > 0) {
    void vscode.window.showWarningMessage(
      `Lace: ${dirty.join(', ')} tem alterações não salvas e continua aberto no caminho antigo. Use Salvar como.`,
    );
  }
}

/**
 * O que o explorador fez com arquivos do projeto ativo. Cada renomeação que
 * toca um registrado vira um `lace move --after`, e cada exclusão, um
 * `lace remove` dos registrados que se foram. Desligado com
 * `lace.files.followExplorer`.
 */
export class ExplorerFollower implements vscode.Disposable {
  private readonly subscriptions: vscode.Disposable[];
  /** Uma por vez, na ordem: o .spf é gravado a cada uma. */
  private queue: Promise<unknown> = Promise.resolve();

  constructor(private readonly deps: CommandDeps) {
    this.subscriptions = [
      vscode.workspace.onDidRenameFiles((event) => {
        const moves = event.files.filter((f) => f.oldUri.scheme === 'file' && f.newUri.scheme === 'file');
        this.enqueue(() => this.renamed(moves.map((f) => ({ from: f.oldUri.fsPath, to: f.newUri.fsPath }))));
      }),
      vscode.workspace.onDidDeleteFiles((event) => {
        const paths = event.files.filter((uri) => uri.scheme === 'file').map((uri) => uri.fsPath);
        this.enqueue(() => this.deleted(paths));
      }),
    ];
  }

  /** Espera o que o explorador já pediu (para os testes). */
  idle(): Promise<unknown> {
    return this.queue;
  }

  private enqueue(task: () => Promise<void>): void {
    if (!vscode.workspace.getConfiguration('lace.files').get<boolean>('followExplorer', true)) {
      return;
    }
    this.queue = this.queue.then(task).catch((error: unknown) => showProcessError(this.deps, error));
  }

  private async renamed(moves: readonly { from: string; to: string }[]): Promise<void> {
    const { project, status } = this.current();
    if (!project || !status) {
      return;
    }
    const registered = registeredPaths(status);
    let changed = false;
    for (const { from, to } of moves) {
      if (!isInside(from, project.root)) {
        continue;
      }
      const processor = status.processors.find(
        (p) => isInside(p.dir, from) || p.source === from || [join(p.dir, 'Software'), join(p.dir, 'Hardware'), join(p.dir, 'Simulation')].includes(from),
      );
      if (processor) {
        void vscode.window.showWarningMessage(
          `Lace: o Lace acha o processador ${processor.name} pelo nome da pasta e do fonte. ` +
            `Com ${basename(from)} renomeado, o projeto não acha mais o processador: volte o nome (Desfazer).`,
        );
        continue;
      }
      if (!registered.some((path) => isInside(path, from))) {
        continue;
      }
      const client = this.client();
      if (!client) {
        return;
      }
      const run = await client.move(project.spf, from, to, { after: true });
      changed = true;
      if (run.status === 'error') {
        const connection = this.deps.connection.state;
        const old = connection.kind === 'ready' && !connection.features.moveAfter;
        if (run.error.code === 'cli' && old) {
          void vscode.window.showWarningMessage(
            `Lace: o Lace ${connection.version} não registra o que foi renomeado fora dele ` +
              `(precisa do ${FEATURES.moveAfter}): o .spf ainda aponta para ${basename(from)}. ` +
              'Volte o nome e use Renomear na vista Fontes.',
          );
        } else {
          showLaceError(run.error);
        }
        break;
      }
    }
    if (changed) {
      await this.deps.status.refresh();
    }
  }

  private async deleted(paths: readonly string[]): Promise<void> {
    const { project, status } = this.current();
    if (!project || !status) {
      return;
    }
    const gone = registeredPaths(status).filter((file) => paths.some((path) => isInside(file, path)));
    const client = gone.length > 0 ? this.client() : undefined;
    if (!client) {
      return;
    }
    const run = await client.remove(project.spf, gone);
    if (run.status === 'error') {
      showLaceError(run.error);
    } else if (run.status === 'succeeded') {
      const what = gone.length === 1 ? basename(gone[0]) : `${gone.length} arquivos`;
      vscode.window.setStatusBarMessage(`$(remove) Lace: ${what} apagado e tirado do projeto`, 8000);
    }
    await this.deps.status.refresh();
  }

  /** O projeto ativo e o status dele, se o status é mesmo dele. */
  private current(): { project?: LaceProject; status?: StatusReport } {
    const project = this.deps.projects.active;
    const status = this.deps.status.report;
    return project && status?.spf === project.spf ? { project, status } : {};
  }

  /** Sem aviso: o explorador não pediu nada ao Lace. */
  private client() {
    const connection = this.deps.connection.state;
    return connection.kind === 'ready' && connection.features.files ? connection.client : undefined;
  }

  dispose(): void {
    this.subscriptions.forEach((s) => s.dispose());
  }
}

/** O arquivo de um item do RTL ou dos testbenches. */
function listedPath(target: unknown): string | undefined {
  if (!isSourceNode(target) || target.kind !== 'entry') {
    return undefined;
  }
  const { role, path } = target.entry;
  return (role === 'design' || role === 'testbench') && path !== null ? path : undefined;
}

function unregisteredPath(target: unknown): string | undefined {
  if (target instanceof vscode.Uri) {
    return target.scheme === 'file' ? target.fsPath : undefined;
  }
  return isSourceNode(target) && target.kind === 'entry' && target.entry.role === 'unregistered'
    ? (target.entry.path ?? undefined)
    : undefined;
}

/** A lista do `.spf` em que o arquivo está, na ordem. */
function listOf(status: StatusReport | undefined, path: string): string[] | undefined {
  for (const list of [status?.synthesizable ?? [], status?.testbench ?? []]) {
    const paths = list.map((f) => f.path);
    if (paths.includes(path)) {
      return paths;
    }
  }
  return undefined;
}

async function runAndRefresh<T>(
  deps: CommandDeps,
  start: () => Promise<LaceRun<T>>,
  refresh = true,
): Promise<LaceRun<T> | undefined> {
  try {
    const run = await start();
    if (run.status === 'error') {
      showLaceError(run.error);
    }
    return run;
  } catch (error) {
    showProcessError(deps, error);
    return undefined;
  } finally {
    if (refresh) {
      await deps.status.refresh();
    }
  }
}
