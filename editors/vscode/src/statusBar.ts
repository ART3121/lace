// O projeto ativo na barra de status, ou por que o aurora não está disponível.

import * as vscode from 'vscode';

import type { Connection } from './connection';
import type { Projects } from './projects';

export class AuroraStatusBar implements vscode.Disposable {
  // À esquerda dos botões de check, sim, wave e synth (ActionBar).
  private readonly item = vscode.window.createStatusBarItem('aurora.project', vscode.StatusBarAlignment.Left, 100);

  constructor() {
    this.item.name = 'Projeto do Lace';
  }

  update(connection: Connection, projects: Projects): void {
    const item = this.item;
    switch (connection.kind) {
      case 'checking':
        item.text = '$(sync~spin) Lace';
        item.tooltip = 'Procurando o aurora';
        item.command = undefined;
        break;
      case 'missing':
      case 'unsupported':
        item.text = '$(warning) Lace';
        item.tooltip = `${connection.reason}. Clique para configurar aurora.path.`;
        item.command = { command: 'workbench.action.openSettings', title: 'Configurar', arguments: ['aurora.path'] };
        break;
      case 'ready': {
        if (projects.all.length === 0) {
          item.hide();
          return;
        }
        const active = projects.active;
        item.text = active ? `$(project) ${active.name}` : '$(project) Lace: escolher o projeto';
        item.tooltip = active
          ? `Projeto ativo do Lace: ${active.spf}\nlace ${connection.version}. Clique para trocar.`
          : 'Este workspace tem mais de um projeto do Lace. Clique para escolher o ativo.';
        item.command = 'aurora.selectProject';
        break;
      }
    }
    item.show();
  }

  dispose(): void {
    this.item.dispose();
  }
}
