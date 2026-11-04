// A cor do nome e o selo dos itens da vista de fontes (o esquema
// aurora-fontes; o explorador e as abas não são tocados).

import * as vscode from 'vscode';

import type { ProblemCount } from '../problems';
import { decorationFor, type SourceEntry } from './model';
import { SOURCE_SCHEME, sourceUri } from './provider';

export class SourceDecorations implements vscode.FileDecorationProvider, vscode.Disposable {
  private decorations = new Map<string, vscode.FileDecoration>();
  private readonly changed = new vscode.EventEmitter<vscode.Uri[]>();
  readonly onDidChangeFileDecorations = this.changed.event;
  private readonly registration = vscode.window.registerFileDecorationProvider(this);

  provideFileDecoration(uri: vscode.Uri): vscode.FileDecoration | undefined {
    return uri.scheme === SOURCE_SCHEME ? this.decorations.get(uri.toString()) : undefined;
  }

  /** Refaz as decorações para os itens de agora e os problemas do último check. */
  update(entries: readonly SourceEntry[], problems: (path: string) => ProblemCount | undefined): void {
    const before = [...this.decorations.keys()];
    const next = new Map<string, vscode.FileDecoration>();
    for (const entry of entries) {
      const spec = decorationFor(entry, entry.path !== null ? problems(entry.path) : undefined);
      if (spec) {
        const decoration = new vscode.FileDecoration(
          spec.badge,
          spec.tooltip,
          spec.color ? new vscode.ThemeColor(spec.color) : undefined,
        );
        next.set(sourceUri(entry).toString(), decoration);
      }
    }
    this.decorations = next;
    const touched = new Set([...before, ...next.keys()]);
    this.changed.fire([...touched].map((key) => vscode.Uri.parse(key)));
  }

  dispose(): void {
    this.registration.dispose();
    this.changed.dispose();
  }
}
