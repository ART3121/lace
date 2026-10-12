// Os projetos do Lace do workspace (os .spf) e qual deles está ativo. Com um
// só, ele é o ativo; com mais de um, o usuário escolhe, e a escolha fica
// guardada no workspace.

import { basename, dirname, sep } from 'node:path';

import * as vscode from 'vscode';

export interface LaceProject {
  readonly uri: vscode.Uri;
  /** O .spf, o `-C` de todo comando. */
  readonly spf: string;
  /** A pasta do .spf. */
  readonly root: string;
  readonly name: string;
}

const ACTIVE_KEY = 'lace.activeProject';
// O .lace/ é do próprio Lace (temporários, relatórios), nunca um projeto.
const EXCLUDE = '**/{node_modules,.git,.lace}/**';

export class Projects implements vscode.Disposable {
  private list: LaceProject[] = [];
  private activeSpf: string | undefined;
  private readonly changed = new vscode.EventEmitter<void>();
  /** A lista de projetos ou o projeto ativo mudou. */
  readonly onDidChange = this.changed.event;
  private readonly disposables: vscode.Disposable[];

  constructor(private readonly memento: vscode.Memento) {
    // Só criar e apagar: a mudança de conteúdo é com o ProjectStatus.
    const watcher = vscode.workspace.createFileSystemWatcher('**/*.spf', false, true, false);
    this.disposables = [
      this.changed,
      watcher,
      watcher.onDidCreate(() => void this.discover()),
      watcher.onDidDelete(() => void this.discover()),
      vscode.workspace.onDidChangeWorkspaceFolders(() => void this.discover()),
    ];
  }

  get all(): readonly LaceProject[] {
    return this.list;
  }

  get active(): LaceProject | undefined {
    return this.list.find((p) => p.spf === this.activeSpf);
  }

  async discover(): Promise<void> {
    const uris = await vscode.workspace.findFiles('**/*.spf', EXCLUDE);
    this.list = uris.map(toProject).sort((a, b) => a.spf.localeCompare(b.spf));
    this.activeSpf = this.pickActive();
    this.changed.fire();
  }

  async setActive(project: LaceProject): Promise<void> {
    await this.memento.update(ACTIVE_KEY, project.spf);
    if (project.spf !== this.activeSpf) {
      this.activeSpf = project.spf;
      this.changed.fire();
    }
  }

  async select(): Promise<LaceProject | undefined> {
    if (this.list.length === 0) {
      void vscode.window.showInformationMessage('Nenhum projeto do Lace (.spf) neste workspace.');
      return undefined;
    }
    const items = this.list.map((project) => ({
      label: project.name,
      description: vscode.workspace.asRelativePath(project.uri),
      detail: project.spf === this.activeSpf ? 'o projeto ativo' : undefined,
      project,
    }));
    const pick = await vscode.window.showQuickPick(items, {
      title: 'Projeto ativo do Lace',
      placeHolder: 'Os comandos e a vista Fontes usam este projeto',
    });
    if (pick) {
      await this.setActive(pick.project);
    }
    return pick?.project;
  }

  /**
   * O projeto cuja pasta contém o arquivo, o mais fundo: o .spf que o Lace
   * acharia subindo as pastas a partir do arquivo.
   */
  owner(file: string): LaceProject | undefined {
    const path = vscode.Uri.file(file).fsPath;
    return this.list
      .filter((p) => path.startsWith(p.root.endsWith(sep) ? p.root : p.root + sep))
      .sort((a, b) => b.root.length - a.root.length)[0];
  }

  private pickActive(): string | undefined {
    const known = (spf: string | undefined) => spf !== undefined && this.list.some((p) => p.spf === spf);
    if (known(this.activeSpf)) {
      return this.activeSpf;
    }
    const remembered = this.memento.get<string>(ACTIVE_KEY);
    if (known(remembered)) {
      return remembered;
    }
    return this.list.length === 1 ? this.list[0].spf : undefined;
  }

  dispose(): void {
    this.disposables.forEach((d) => d.dispose());
  }
}

function toProject(uri: vscode.Uri): LaceProject {
  return {
    uri,
    spf: uri.fsPath,
    root: dirname(uri.fsPath),
    name: basename(uri.fsPath, '.spf'),
  };
}
