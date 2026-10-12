// O `lace status` do projeto ativo, relido depois de cada comando da
// extensão, quando o .spf muda no disco (o terminal, o Studio, um editor) e
// quando um .v ou .sv é criado ou apagado (a lista dos não registrados).

import * as vscode from 'vscode';

import type { Connection, LaceConnection } from './connection';
import type { ErrorInfo, StatusReport } from './lace/lace-types';
import type { Projects } from './projects';
import { Coalescer, ContentMemory } from './refresh';

export type StatusState =
  /** Nada a ler: sem um lace utilizável ou sem projeto ativo. */
  | { kind: 'idle' }
  /** A primeira leitura do projeto ativo ainda não voltou. */
  | { kind: 'loading' }
  | { kind: 'loaded'; spf: string; report: StatusReport }
  | { kind: 'error'; spf: string; error: ErrorInfo };

/** Quanto juntar os avisos do vigia antes de reler, como o watcher.rs do Studio. */
const DEBOUNCE_MS = 300;

export class ProjectStatus implements vscode.Disposable {
  private state: StatusState = { kind: 'idle' };
  private loadedFor: string | undefined;
  private readonly memory = new ContentMemory();
  private readonly coalescer = new Coalescer(() => this.load());
  private timer: NodeJS.Timeout | undefined;
  private force = false;
  private readonly changed = new vscode.EventEmitter<StatusState>();
  /** Cada leitura terminada, com ou sem erro. */
  readonly onDidChange = this.changed.event;
  private readonly foundNewFiles = new vscode.EventEmitter<string[]>();
  /**
   * Arquivos que passaram a aparecer entre os não registrados do projeto,
   * desde a leitura anterior. A primeira leitura de um projeto não conta:
   * o que já estava fora antes de abrir não é novo.
   */
  readonly onDidFindNewFiles = this.foundNewFiles.event;
  private readonly disposables: vscode.Disposable[];

  constructor(
    private readonly connection: LaceConnection,
    private readonly projects: Projects,
    private readonly log: vscode.LogOutputChannel,
  ) {
    const spfWatcher = vscode.workspace.createFileSystemWatcher('**/*.spf', true, false, true);
    const sourceWatcher = vscode.workspace.createFileSystemWatcher('**/*.{v,sv}', false, true, false);
    const onSource = (uri: vscode.Uri) => {
      const active = this.projects.active;
      if (active && this.projects.owner(uri.fsPath)?.spf === active.spf) {
        this.schedule(true);
      }
    };
    this.disposables = [
      this.changed,
      this.foundNewFiles,
      spfWatcher,
      sourceWatcher,
      spfWatcher.onDidChange((uri) => {
        if (uri.fsPath === this.projects.active?.spf) {
          this.schedule(false);
        }
      }),
      sourceWatcher.onDidCreate(onSource),
      sourceWatcher.onDidDelete(onSource),
      this.connection.onDidChange(() => void this.refresh()),
      this.projects.onDidChange(() => {
        if (this.projects.active?.spf !== this.loadedFor) {
          void this.refresh();
        }
      }),
    ];
  }

  get current(): StatusState {
    return this.state;
  }

  get report(): StatusReport | undefined {
    return this.state.kind === 'loaded' ? this.state.report : undefined;
  }

  /** Relê agora; um pedido que chega durante uma leitura vira uma leitura a mais, depois dela. */
  refresh(): Promise<void> {
    return this.coalescer.run();
  }

  /**
   * Agenda uma releitura para daqui a 300 ms, juntando os avisos que chegam
   * nesse meio-tempo. `force` relê mesmo que o .spf não tenha mudado (um
   * arquivo novo na pasta).
   */
  private schedule(force: boolean): void {
    this.force ||= force;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.onWatched(), DEBOUNCE_MS);
  }

  private async onWatched(): Promise<void> {
    const force = this.force;
    this.force = false;
    const project = this.projects.active;
    if (!project) {
      return;
    }
    if (!force) {
      // O vigia avisa de qualquer gravação, inclusive de uma que deixou o
      // .spf como estava. Só conteúdo novo relê: assim, se um comando que a
      // própria releitura roda gravasse o .spf igual, o ciclo para aqui.
      const content = await readFile(project.uri);
      if (content !== undefined && !this.memory.changed(content)) {
        this.log.debug('O .spf foi gravado sem mudar; nada a reler.');
        return;
      }
    }
    await this.refresh();
  }

  private async load(): Promise<void> {
    const connection = this.connection.state;
    const project = this.projects.active;
    this.loadedFor = project?.spf;
    if (connection.kind !== 'ready' || !connection.features.sources || !project) {
      this.memory.forget();
      this.set({ kind: 'idle' });
      return;
    }
    if (!('spf' in this.state) || this.state.spf !== project.spf) {
      this.set({ kind: 'loading' });
    }
    // O conteúdo lido antes do status: um aviso do vigia com este mesmo
    // conteúdo, que chega depois, não relê de novo.
    const content = await readFile(project.uri);
    if (content === undefined) {
      this.memory.forget();
    } else {
      this.memory.remember(content);
    }
    const next = await this.read(connection, project.spf);
    // Se o lace ou o projeto ativo mudaram enquanto o status rodava, já há
    // outra leitura marcada, e este resultado não vale mais.
    if (this.connection.state !== connection || this.projects.active?.spf !== project.spf) {
      return;
    }
    const before = this.state.kind === 'loaded' && this.state.spf === project.spf ? this.state.report : undefined;
    this.set(next);
    if (before && next.kind === 'loaded') {
      const fresh = next.report.unregistered.filter((path) => !before.unregistered.includes(path));
      if (fresh.length > 0) {
        this.foundNewFiles.fire(fresh);
      }
    }
  }

  private async read(connection: Extract<Connection, { kind: 'ready' }>, spf: string): Promise<StatusState> {
    try {
      const run = await connection.client.status(spf);
      switch (run.status) {
        case 'succeeded':
          return { kind: 'loaded', spf, report: run.result };
        case 'error':
          return { kind: 'error', spf, error: run.error };
        default:
          return { kind: 'error', spf, error: { code: 'unexpected', message: `lace status: ${run.status}` } };
      }
    } catch (error) {
      return { kind: 'error', spf, error: { code: 'process', message: (error as Error).message } };
    }
  }

  private set(next: StatusState): void {
    this.state = next;
    this.changed.fire(next);
  }

  dispose(): void {
    clearTimeout(this.timer);
    this.disposables.forEach((d) => d.dispose());
  }
}

async function readFile(uri: vscode.Uri): Promise<Uint8Array | undefined> {
  try {
    return await vscode.workspace.fs.readFile(uri);
  } catch {
    return undefined;
  }
}
