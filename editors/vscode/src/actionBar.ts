// Os botões de build, check, sim, wave e synth na barra de status, ao lado
// do projeto ativo. Ficam fora da vista de fontes porque valem para o
// projeto inteiro e precisam estar à mão com qualquer editor ou vista
// aberta. Cada um mostra o próprio estado: girando enquanto roda (o clique
// cancela), e o fundo de erro depois de uma falha. O Build só aparece num
// projeto com processador SAPHO.

import * as vscode from 'vscode';

import type { Connection } from './connection';
import type { StatusReport } from './lace/lace-types';
import type { Feature } from './lace/version';
import type { Projects } from './projects';
import type { RunKind, Runs } from './runs';

interface Action {
  kind: RunKind;
  label: string;
  icon: string;
  command: string;
  feature: Feature;
  what: string;
  /** O clique cancela enquanto roda (o wave volta em 1,5 s e não cancela). */
  cancellable: boolean;
  /** Só num projeto com processador. */
  needsProcessor?: boolean;
}

const ACTIONS: Action[] = [
  {
    kind: 'build',
    label: 'Build',
    icon: 'tools',
    command: 'lace.build',
    feature: 'processors',
    what: 'Compilar os processadores SAPHO com o YANC (lace build)',
    cancellable: true,
    needsProcessor: true,
  },
  {
    kind: 'check',
    label: 'Check',
    icon: 'checklist',
    command: 'lace.check',
    feature: 'check',
    what: 'Verificar o Verilog do projeto (lace check)',
    cancellable: true,
  },
  {
    kind: 'sim',
    label: 'Sim',
    icon: 'play',
    command: 'lace.sim',
    feature: 'sim',
    what: 'Simular o testbench do projeto (lace sim)',
    cancellable: true,
  },
  {
    kind: 'wave',
    label: 'Wave',
    icon: 'pulse',
    command: 'lace.wave',
    feature: 'wave',
    what: 'Abrir a onda da simulação no surfer-aurora (lace wave)',
    cancellable: false,
  },
  {
    kind: 'synth',
    label: 'Synth',
    icon: 'circuit-board',
    command: 'lace.synth',
    feature: 'synth',
    what: 'Sintetizar o topo no Yosys (lace synth)',
    cancellable: true,
  },
];

/** Logo à direita do projeto ativo (LaceStatusBar, prioridade 100). */
const FIRST_PRIORITY = 99;

export class ActionBar implements vscode.Disposable {
  private readonly items = new Map<RunKind, vscode.StatusBarItem>();

  constructor() {
    ACTIONS.forEach((action, i) => {
      // Cada botão tem id próprio: o usuário esconde um só pelo menu da barra.
      const item = vscode.window.createStatusBarItem(
        `lace.action.${action.kind}`,
        vscode.StatusBarAlignment.Left,
        FIRST_PRIORITY - i,
      );
      item.name = `Lace: ${action.label}`;
      this.items.set(action.kind, item);
    });
  }

  update(connection: Connection, projects: Projects, runs: Runs, report: StatusReport | undefined): void {
    const hasProcessor = (report?.processors.length ?? 0) > 0;
    for (const action of ACTIONS) {
      const item = this.items.get(action.kind);
      if (!item) {
        continue;
      }
      if (
        connection.kind !== 'ready' ||
        !connection.features[action.feature] ||
        !projects.active ||
        (action.needsProcessor && !hasProcessor && !runs.isRunning(action.kind))
      ) {
        item.hide();
        continue;
      }
      const state = runs.state(action.kind);
      item.backgroundColor = undefined;
      if (state.running) {
        item.text = `$(sync~spin) ${action.label}`;
        const step = state.step ? `: ${state.step}` : '';
        item.tooltip = `${action.what}\nRodando${step}.${action.cancellable ? ' Clique para cancelar.' : ''}`;
        item.command = action.cancellable
          ? { command: 'lace.cancelRun', title: 'Cancelar', arguments: [action.kind] }
          : undefined;
      } else {
        const last = state.last;
        const failed = last?.outcome === 'failed' || last?.outcome === 'error';
        item.text = `$(${action.icon}) ${action.label}${failed ? ` $(error)${last?.badge ? ` ${last.badge}` : ''}` : ''}`;
        if (last?.outcome === 'failed') {
          item.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground');
        } else if (last?.outcome === 'error') {
          item.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
        }
        item.tooltip = last ? `${action.what}\nÚltima vez: ${last.summary}` : action.what;
        item.command = action.command;
      }
      item.show();
    }
  }

  dispose(): void {
    this.items.forEach((item) => item.dispose());
  }
}
