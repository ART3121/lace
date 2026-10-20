// O lace que a extensão usa: onde está, que versão é, quais recursos ela
// permite e por que transporte ela fala com ele (`lace.transport`: um `lace
// serve` em segundo plano, ou um processo por comando). Refeito quando
// `lace.path` ou `lace.transport` mudam, e quando o `lace serve` reaberto
// é outra versão (o binário foi trocado por fora).

import * as vscode from 'vscode';

import { CliLaceClient, type LaceCommand } from './lace/cliClient';
import type { LaceClient } from './lace/client';
import { LacePathError, locateLace } from './lace/locate';
import { ServeLaceClient } from './lace/serveClient';
import { MIN_VERSION, supportedFeatures, usable, type FeatureSet } from './lace/version';

/** Como falar com o lace (`lace.transport`). */
export type Transport = 'auto' | 'serve' | 'cli';

/** O nome que a extensão dá no `initialize` do `lace serve`. */
export const CLIENT_NAME = 'lace-vscode';

export type Connection =
  | { kind: 'checking' }
  /** Sem executável: nem `lace.path`, nem um lace no PATH. */
  | { kind: 'missing'; reason: string }
  /** O executável existe, mas não é um lace que a extensão saiba usar. */
  | { kind: 'unsupported'; reason: string }
  | {
      kind: 'ready';
      client: LaceClient;
      version: string;
      file: string;
      features: FeatureSet;
      /** O transporte em uso. */
      transport: 'serve' | 'cli';
    };

export class LaceConnection implements vscode.Disposable {
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
        if (event.affectsConfiguration('lace.path') || event.affectsConfiguration('lace.transport')) {
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
    // O --version é barato e é a única forma de saber a versão de um Lace
    // sem o serve.
    const cli = new CliLaceClient(command, this.log);
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
        reason: `o lace em ${command.file} é a versão ${version}, e a extensão precisa da ${MIN_VERSION} ou mais nova`,
      };
    }
    const off = Object.entries(features).filter(([name, on]) => !on && name !== 'serve');
    if (off.length > 0) {
      this.log.info(`lace ${version}: desligado o que ele não suporta (${off.map(([name]) => name).join(', ')})`);
    }
    const wanted = vscode.workspace.getConfiguration('lace').get<Transport>('transport', 'auto');
    let client: LaceClient = cli;
    let transport: 'serve' | 'cli' = 'cli';
    if (wanted === 'serve' && !features.serve) {
      return {
        kind: 'unsupported',
        reason: `o lace em ${command.file} é a versão ${version}, e lace.transport = serve exige a 0.9.0 ou mais nova`,
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
          return { kind: 'unsupported', reason: `o lace serve não respondeu: ${(error as Error).message}` };
        }
        this.log.warn(`O lace serve não respondeu (${(error as Error).message}); um processo por comando.`);
      }
    }
    this.log.info(`lace ${version} em ${command.file} (${transport})`);
    return { kind: 'ready', client, version, file: command.file, features, transport };
  }

  private serveClient(command: LaceCommand): ServeLaceClient {
    return new ServeLaceClient(command, this.log, {
      clientName: `${CLIENT_NAME} ${this.extensionVersion}`,
      onServerChanged: (version) => {
        this.log.info(`O lace serve agora é a versão ${version}: refazendo a conexão`);
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

  /** Encerra o `lace serve`, se houver; o `deactivate` espera. */
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
