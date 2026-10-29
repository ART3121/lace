// As operações que rodam ferramentas (build, check, sim, wave, synth): uma de cada
// tipo por vez, a saída ao vivo no canal "Lace: ferramentas", o
// cancelamento (SIGTERM ao aurora) e o estado que os botões da barra de status
// mostram.

import * as vscode from 'vscode';

import type { AuroraRun } from './aurora/client';
import type { Event } from './aurora/aurora-types';

export type RunKind = 'build' | 'check' | 'sim' | 'wave' | 'synth';

export const RUN_KINDS: readonly RunKind[] = ['build', 'check', 'sim', 'wave', 'synth'];

export type RunOutcome = 'succeeded' | 'failed' | 'cancelled' | 'error';

export interface RunResult {
  outcome: RunOutcome;
  /** Uma linha para a dica do botão: "2 erros, 3 avisos". */
  summary: string;
  /** Ao lado do nome no botão, numa falha: o número de erros. */
  badge?: string;
}

export interface RunState {
  running: boolean;
  /** O passo que roda agora, como "simulate (vvp)". */
  step?: string;
  last?: RunResult;
}

export interface RunRequest<T> {
  /** O título na saída; sem ele (o wave), nada vai para a saída. */
  title?: string;
  /** Mostra o canal da saída ao começar. */
  reveal: boolean;
  /**
   * Com uma do mesmo tipo rodando: `true` cancela a anterior e começa esta
   * (o check, cujo resultado velho não serve); `false` não começa.
   */
  restart: boolean;
  start: (signal: AbortSignal, onEvent: (event: Event) => void) => Promise<AuroraRun<T>>;
  /** Resume o resultado para o botão. */
  summarize: (run: AuroraRun<T>) => RunResult;
}

export class Runs implements vscode.Disposable {
  private readonly states = new Map<RunKind, RunState>();
  private readonly controllers = new Map<RunKind, AbortController>();
  private readonly tasks = new Map<RunKind, Promise<unknown>>();
  private readonly changed = new vscode.EventEmitter<RunKind>();
  readonly onDidChange = this.changed.event;

  constructor(readonly output: vscode.OutputChannel) {}

  state(kind: RunKind): RunState {
    return this.states.get(kind) ?? { running: false };
  }

  isRunning(kind: RunKind): boolean {
    return this.state(kind).running;
  }

  /** Cancela: o aurora recebe SIGTERM, encerra a ferramenta e sai com 130. */
  cancel(kind: RunKind): void {
    this.controllers.get(kind)?.abort();
  }

  cancelAll(): void {
    for (const controller of this.controllers.values()) {
      controller.abort();
    }
  }

  /** Esquece o resultado da última (o projeto ativo mudou). */
  forget(kind: RunKind): void {
    this.set(kind, { running: this.isRunning(kind) });
  }

  /** `'busy'` quando já há uma do mesmo tipo rodando e `restart` é falso. */
  async run<T>(kind: RunKind, request: RunRequest<T>): Promise<AuroraRun<T> | 'busy'> {
    if (this.isRunning(kind)) {
      if (!request.restart) {
        return 'busy';
      }
      this.cancel(kind);
      await this.tasks.get(kind)?.catch(() => undefined);
    }
    const controller = new AbortController();
    this.controllers.set(kind, controller);
    this.set(kind, { running: true, last: this.state(kind).last });
    if (request.title) {
      this.output.appendLine('');
      this.output.appendLine(`=== ${request.title} (${new Date().toLocaleTimeString()})`);
      if (request.reveal) {
        this.output.show(true);
      }
    }
    const log = request.title !== undefined;
    const task = request.start(controller.signal, (event) => this.onEvent(kind, event, log));
    this.tasks.set(kind, task);
    try {
      const run = await task;
      this.set(kind, { running: false, last: request.summarize(run) });
      return run;
    } catch (error) {
      this.set(kind, { running: false, last: { outcome: 'error', summary: (error as Error).message } });
      throw error;
    } finally {
      if (this.controllers.get(kind) === controller) {
        this.controllers.delete(kind);
        this.tasks.delete(kind);
      }
    }
  }

  private onEvent(kind: RunKind, event: Event, log: boolean): void {
    switch (event.event) {
      case 'step_started':
        this.set(kind, { ...this.state(kind), step: `${event.step} (${event.tool})` });
        if (log) {
          this.output.appendLine(`$ ${[event.command.program, ...event.command.args].map(quote).join(' ')}`);
        }
        break;
      case 'output':
        if (log) {
          // O que o testbench escreve sai depois de "|", como na CLI.
          const program = event.step === 'simulate' && !event.diagnostic;
          this.output.appendLine(program ? `  | ${event.line}` : event.line);
        }
        break;
      case 'step_finished': {
        if (log) {
          const t = event.termination;
          const how = t.kind === 'exited' ? `saiu com ${t.value}` : 'value' in t ? `${t.kind} ${t.value}` : t.kind;
          this.output.appendLine(`  ${event.tool} ${how} em ${event.duration_ms} ms`);
        }
        break;
      }
    }
  }

  private set(kind: RunKind, state: RunState): void {
    this.states.set(kind, state);
    this.changed.fire(kind);
  }

  dispose(): void {
    this.cancelAll();
    this.changed.dispose();
  }
}

function quote(arg: string): string {
  return /[\s"']/.test(arg) || arg === '' ? JSON.stringify(arg) : arg;
}
