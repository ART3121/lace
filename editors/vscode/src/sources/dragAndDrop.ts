// Arrastar na vista de fontes: um arquivo do RTL ou dos testbenches solto
// sobre outro da mesma lista muda a ordem de compilação (`aurora order`); um
// arquivo do explorador solto na vista entra no projeto (`aurora add`), como
// testbench se cair nos testbenches.

import * as vscode from 'vscode';

import type { SourceNode } from './provider';

const MIME = 'application/vnd.code.tree.aurora.sources';

export interface DropActions {
  /** Os arquivos arrastados para o lugar de `target`, ou para o começo da lista. */
  order(dragged: string[], target: string | 'start'): Promise<void>;
  add(files: string[], testbench: boolean): Promise<unknown>;
}

export class SourcesDragAndDrop implements vscode.TreeDragAndDropController<SourceNode> {
  readonly dragMimeTypes = [MIME];
  readonly dropMimeTypes = [MIME, 'text/uri-list'];

  constructor(private readonly actions: DropActions) {}

  handleDrag(source: readonly SourceNode[], data: vscode.DataTransfer): void {
    const paths = source.flatMap((node) =>
      node.kind === 'entry' && (node.entry.role === 'design' || node.entry.role === 'testbench') && node.entry.path
        ? [{ role: node.entry.role, path: node.entry.path }]
        : [],
    );
    if (paths.length > 0) {
      data.set(MIME, new vscode.DataTransferItem(paths));
    }
  }

  async handleDrop(target: SourceNode | undefined, data: vscode.DataTransfer): Promise<void> {
    const own = data.get(MIME)?.value as { role: string; path: string }[] | undefined;
    if (own) {
      // Só dentro da mesma lista: mudar de papel é tirar e pôr de novo.
      const role = own[0].role;
      const dragged = own.filter((item) => item.role === role).map((item) => item.path);
      if (target?.kind === 'entry' && target.entry.role === role && target.entry.path) {
        await this.actions.order(dragged, target.entry.path);
      } else if (target?.kind === 'group' && target.group.role === role) {
        await this.actions.order(dragged, 'start');
      }
      return;
    }
    const list = await data.get('text/uri-list')?.asString();
    if (!list) {
      return;
    }
    const files = list
      .split(/\r?\n/)
      .filter((line) => line !== '' && !line.startsWith('#'))
      .map((line) => vscode.Uri.parse(line.trim()))
      .filter((uri) => uri.scheme === 'file')
      .map((uri) => uri.fsPath);
    if (files.length > 0) {
      const role = target?.kind === 'entry' ? target.entry.role : target?.group.role;
      await this.actions.add(files, role === 'testbench');
    }
  }
}
