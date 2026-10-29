// O aurora que a extensão usa: onde está, que versão é, quais recursos ela
// permite e por que transporte ela fala com ele (`aurora.transport`: um `aurora
// serve` em segundo plano, ou um processo por comando). Refeito quando
// `aurora.path` ou `aurora.transport` mudam, e quando o `aurora serve` reaberto
// é outra versão (o binário foi trocado por fora).

import * as vscode from 'vscode';

import { CliAuroraClient, type AuroraCommand } from './aurora/cliClient';
import type { AuroraClient } from './aurora/client';
import { AuroraPathError, locateAurora } from './aurora/locate';
import { ServeAuroraClient } from './aurora/serveClient';
import { MIN_VERSION, supportedFeatures, usable, type FeatureSet } from './aurora/version';

/** Como falar com o aurora (`aurora.transport`). */
export type Transport = 'auto' | 'serve' | 'cli';

/** O nome que a extensão dá no `initialize` do `aurora serve`. */
export const CLIENT_NAME = 'aurora-vscode';

export type Connection =
  | { kind: 'checking' }
  /** Sem executável: nem `aurora.path`, nem um aurora no PATH. */
  | { kind: 'missing'; reason: string }
  /** O executável existe, mas não é um aurora que a extensão saiba usar. */
  | { kind: 'unsupported'; reason: string }
  | {
      kind: 'ready';
      client: AuroraClient;
      version: string;
      file: string;
      features: FeatureSet;
      /** O transporte em uso. */
      transport: 'serve' | 'cli';
    };

export class AuroraConnection implements vscode.Disposable {
  private current: Connection = { kind: 'checking' };
  private generation = 0;
  private readonly changed = new vscode.EventEmitter<Connection>();
  readonly onDidChange = this.changed.event;
  private readonly disposables: vscode.Disposable[];

  constructor(
    private readonly log: vscode.LogOutputChannel,
    /** A versão da extensão, para o `initialize`. */
    private readonly extensionVersion = '0.0.0',
  ) {
    this.disposables = [
      this.changed,
      vscode.workspace.onDidChangeConfiguration((event) => {
        if (event.affectsConfiguration('aurora.path') || event.affectsConfiguration('aurora.transport')) {
          void this.connect();
        }
      }),
    ];
  }

  get state(): Connection {
    return this.current;
  }

  /** Acha o aurora e confere a versão. Só o resultado da última chamada vale. */
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
      const configure = 'Configurar aurora.path';
      void vscode.window.showWarningMessage(`Lace: ${next.reason}.`, configure).then((choice) => {
        if (choice === configure) {
          void vscode.commands.executeCommand('workbench.action.openSettings', 'aurora.path');
        }
      });
    }
  }

  private async probe(): Promise<Connection> {
    const configured = vscode.workspace.getConfiguration('aurora').get<string>('path');
    let command;
    try {
      command = locateAurora(configured);
    } catch (error) {
      if (error instanceof AuroraPathError) {
        return { kind: 'missing', reason: error.message };
      }
      throw error;
    }
    if (command === undefined) {
      return { kind: 'missing', reason: 'o aurora não está no PATH, e aurora.path está vazio' };
    }
    // O --version é barato e é a única forma de saber a versão de um Lace
    // sem o serve.
    const cli = new CliAuroraClient(command, this.log);
    let version: string;
    try {
      version = await cli.version();
    } catch (error) {
      return { kind: 'unsupported', reason: (error as Error).message };
    }
    const features = supportedFeatures(version);
    if (!usable(features)) {
      return {
        kind: 'unsupported',
        reason: `o aurora em ${command.file} é a versão ${version}, e a extensão precisa da ${MIN_VERSION} ou mais nova`,
      };
    }
    const off = Object.entries(features).filter(([name, on]) => !on && name !== 'serve');
    if (off.length > 0) {
      this.log.info(`aurora ${version}: desligado o que ele não suporta (${off.map(([name]) => name).join(', ')})`);
    }
    const wanted = vscode.workspace.getConfiguration('aurora').get<Transport>('transport', 'auto');
    let client: AuroraClient = cli;
    let transport: 'serve' | 'cli' = 'cli';
    if (wanted === 'serve' && !features.serve) {
      return {
        kind: 'unsupported',
        reason: `o aurora em ${command.file} é a versão ${version}, e aurora.transport = serve exige a 0.9.0 ou mais nova`,
      };
    }
    if (wanted !== 'cli' && features.serve) {
      const serve = this.serveClient(command);
      try {
        await serve.version();
        client = serve;
        transport = 'serve';
      } catch (error) {
        await serve.dispose();
        if (wanted === 'serve') {
          return { kind: 'unsupported', reason: `o aurora serve não respondeu: ${(error as Error).message}` };
        }
        this.log.warn(`O aurora serve não respondeu (${(error as Error).message}); um processo por comando.`);
      }
    }
    this.log.info(`aurora ${version} em ${command.file} (${transport})`);
    return { kind: 'ready', client, version, file: command.file, features, transport };
  }

  private serveClient(command: AuroraCommand): ServeAuroraClient {
    return new ServeAuroraClient(command, this.log, {
      clientName: `${CLIENT_NAME} ${this.extensionVersion}`,
      onServerChanged: (version) => {
        this.log.info(`O aurora serve agora é a versão ${version}: refazendo a conexão`);
        void this.connect();
      },
    });
  }

  private set(next: Connection): void {
    const previous = this.current;
    this.current = next;
    this.changed.fire(next);
    if (previous.kind === 'ready' && previous.client !== (next.kind === 'ready' ? next.client : undefined)) {
      void previous.client.dispose?.();
    }
  }

  /** Encerra o `aurora serve`, se houver; o `deactivate` espera. */
  async shutdown(): Promise<void> {
    const current = this.current;
    if (current.kind === 'ready') {
      await current.client.dispose?.();
    }
  }

  dispose(): void {
    this.disposables.forEach((d) => d.dispose());
  }
}
