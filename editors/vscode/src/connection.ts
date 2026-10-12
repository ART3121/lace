// O lace que a extensão usa: onde está, que versão é e quais recursos ela
// permite. Refeito quando `lace.path` muda.

import * as vscode from 'vscode';

import { CliLaceClient } from './lace/cliClient';
import type { LaceClient } from './lace/client';
import { LacePathError, locateLace } from './lace/locate';
import { MIN_VERSION, supportedFeatures, type FeatureSet } from './lace/version';

export type Connection =
  | { kind: 'checking' }
  /** Sem executável: nem `lace.path`, nem um lace no PATH. */
  | { kind: 'missing'; reason: string }
  /** O executável existe, mas não é um lace que a extensão saiba usar. */
  | { kind: 'unsupported'; reason: string }
  | { kind: 'ready'; client: LaceClient; version: string; file: string; features: FeatureSet };

export class LaceConnection implements vscode.Disposable {
  private current: Connection = { kind: 'checking' };
  private generation = 0;
  private readonly changed = new vscode.EventEmitter<Connection>();
  readonly onDidChange = this.changed.event;
  private readonly disposables: vscode.Disposable[];

  constructor(private readonly log: vscode.LogOutputChannel) {
    this.disposables = [
      this.changed,
      vscode.workspace.onDidChangeConfiguration((event) => {
        if (event.affectsConfiguration('lace.path')) {
          void this.connect();
        }
      }),
    ];
  }

  get state(): Connection {
    return this.current;
  }

  /** Acha o lace e confere a versão. Só o resultado da última chamada vale. */
  async connect(): Promise<void> {
    const generation = ++this.generation;
    this.set({ kind: 'checking' });
    const next = await this.probe();
    if (generation !== this.generation) {
      return;
    }
    this.set(next);
    if (next.kind === 'missing' || next.kind === 'unsupported') {
      this.log.warn(next.reason);
      const configure = 'Configurar lace.path';
      void vscode.window.showWarningMessage(`Lace: ${next.reason}.`, configure).then((choice) => {
        if (choice === configure) {
          void vscode.commands.executeCommand('workbench.action.openSettings', 'lace.path');
        }
      });
    }
  }

  private async probe(): Promise<Connection> {
    const configured = vscode.workspace.getConfiguration('lace').get<string>('path');
    let command;
    try {
      command = locateLace(configured);
    } catch (error) {
      if (error instanceof LacePathError) {
        return { kind: 'missing', reason: error.message };
      }
      throw error;
    }
    if (command === undefined) {
      return { kind: 'missing', reason: 'o lace não está no PATH, e lace.path está vazio' };
    }
    const client = new CliLaceClient(command, this.log);
    let version: string;
    try {
      version = await client.version();
    } catch (error) {
      return { kind: 'unsupported', reason: (error as Error).message };
    }
    const features = supportedFeatures(version);
    if (!Object.values(features).some((on) => on)) {
      return {
        kind: 'unsupported',
        reason: `o lace em ${command.file} é a versão ${version}, e a extensão precisa da ${MIN_VERSION} ou mais nova`,
      };
    }
    const off = Object.entries(features).filter(([, on]) => !on);
    if (off.length > 0) {
      this.log.info(`lace ${version}: desligado o que ele não suporta (${off.map(([name]) => name).join(', ')})`);
    }
    this.log.info(`lace ${version} em ${command.file}`);
    return { kind: 'ready', client, version, file: command.file, features };
  }

  private set(next: Connection): void {
    this.current = next;
    this.changed.fire(next);
  }

  dispose(): void {
    this.disposables.forEach((d) => d.dispose());
  }
}
