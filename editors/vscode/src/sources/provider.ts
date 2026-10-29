// A vista "Lace: fontes": os grupos de model.ts como árvore do VS Code.
//
// A descrição ao lado do nome não aceita cor na API do VS Code; o que se
// destaca é o nome (a cor) e o selo à direita, que vêm de um provedor de
// decoração (decorations.ts). Para a decoração valer só aqui, e não no
// explorador nem nas abas, cada item tem um URI do esquema lace-fontes.

import * as vscode from 'vscode';

import type { ProjectStatus } from '../projectStatus';
import { sourceGroups, TONE_COLORS, type SourceEntry, type SourceGroup, type SourceRole } from './model';

export const SOURCE_SCHEME = 'lace-fontes';

export type SourceNode = { kind: 'group'; group: SourceGroup } | { kind: 'entry'; entry: SourceEntry };

export function isSourceNode(value: unknown): value is SourceNode {
  const kind = (value as { kind?: unknown } | undefined)?.kind;
  return kind === 'group' || kind === 'entry';
}

/**
 * O URI do item na vista: o caminho do arquivo no esquema lace-fontes, com
 * o papel na query, porque o mesmo arquivo pode aparecer num aviso e no
 * grupo dele.
 */
export function sourceUri(entry: SourceEntry): vscode.Uri {
  const path = entry.path !== null ? vscode.Uri.file(entry.path).path : `/${entry.id}`;
  return vscode.Uri.from({ scheme: SOURCE_SCHEME, path, query: entry.id });
}

const GROUP_ICONS: Record<SourceRole, string> = {
  issue: 'warning',
  design: 'symbol-module',
  testbench: 'beaker',
  processor: 'circuit-board',
  unregistered: 'question',
};

export class SourcesProvider implements vscode.TreeDataProvider<SourceNode>, vscode.Disposable {
  private groups: SourceGroup[] = [];
  private readonly changed = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this.changed.event;
  private readonly subscription: vscode.Disposable;

  constructor(private readonly status: ProjectStatus) {
    this.subscription = status.onDidChange(() => this.update());
    this.update();
  }

  /** Os itens que a vista mostra agora, para as decorações. */
  get entries(): SourceEntry[] {
    return this.groups.flatMap((g) => g.entries);
  }

  getChildren(node?: SourceNode): SourceNode[] {
    if (node === undefined) {
      return this.groups.map((group) => ({ kind: 'group', group }));
    }
    if (node.kind === 'group') {
      return node.group.entries.map((entry) => ({ kind: 'entry', entry }));
    }
    return [];
  }

  getTreeItem(node: SourceNode): vscode.TreeItem {
    if (node.kind === 'group') {
      const { group } = node;
      const item = new vscode.TreeItem(group.label, vscode.TreeItemCollapsibleState.Expanded);
      item.id = `group:${group.role}`;
      item.contextValue = `aurora.group.${group.role}`;
      item.description = String(group.entries.length);
      item.iconPath = new vscode.ThemeIcon(GROUP_ICONS[group.role]);
      return item;
    }
    const { entry } = node;
    const item = new vscode.TreeItem(entry.label, vscode.TreeItemCollapsibleState.None);
    item.id = entry.id;
    item.resourceUri = sourceUri(entry);
    item.description = entry.description;
    item.tooltip = entry.tooltip;
    item.contextValue = entry.contextValue;
    item.iconPath = icon(entry);
    if (entry.path !== null) {
      item.command = { command: 'vscode.open', title: 'Abrir', arguments: [vscode.Uri.file(entry.path)] };
    }
    return item;
  }

  private update(): void {
    const state = this.status.current;
    this.groups = state.kind === 'loaded' ? sourceGroups(state.report) : [];
    this.changed.fire();
  }

  dispose(): void {
    this.subscription.dispose();
    this.changed.dispose();
  }
}

function icon(entry: SourceEntry): vscode.ThemeIcon {
  const color = entry.tone ? new vscode.ThemeColor(TONE_COLORS[entry.tone]) : undefined;
  switch (entry.role) {
    case 'processor':
      return new vscode.ThemeIcon('circuit-board', color);
    case 'issue':
      return new vscode.ThemeIcon('warning', color);
    default:
      // O ícone do tipo de arquivo, do tema de ícones.
      return vscode.ThemeIcon.File;
  }
}
