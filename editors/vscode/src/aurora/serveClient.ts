// O AuroraClient sobre um `aurora serve` em segundo plano: um processo só,
// aberto no primeiro pedido, falando JSON-RPC por linhas (jsonRpc.ts). Os
// comandos e as vistas continuam lendo os relatórios da CLI (aurora-types.ts):
// este arquivo converte o que o protocolo devolve para essa forma.

import { PassThrough, type Readable, type Writable } from 'node:stream';

import { spawnServe, type AuroraCommand, type AuroraLog } from './cliClient';
import {
  AuroraProcessError,
  type AddOptions,
  type BuildOptions,
  type CheckOptions,
  type AuroraClient,
  type AuroraRun,
  type ListPosition,
  type ProcessorChanges,
  type RunOptions,
  type SimOptions,
  type SynthOptions,
  type WaveTarget,
} from './client';
import { RpcChannel, RpcClosedError, clip, type RpcError, type RpcNotification, type RpcReply } from './jsonRpc';
import type {
  AddReport,
  BuildReport,
  CheckReport,
  ErrorInfo,
  Event,
  FlowOutcome,
  MoveReport,
  NewReport,
  OrderReport,
  Processor,
  ProjectStatus,
  RemoveReport,
  ServerInfo,
  SimReport,
  StatusReport,
  SynthReport,
  TopReport,
  WaveOpened,
  WaveReport,
} from './aurora-types';

/** A versão do protocolo que este cliente fala (`aurora_protocol::PROTOCOL_VERSION`). */
export const PROTOCOL_VERSION = 2;

// Os códigos do JSON-RPC que o aurora serve usa (crates/aurora-cli/src/serve.rs).
const RPC_AURORA = -32000;
const RPC_CANCELLED = -32001;
const RPC_METHOD_NOT_FOUND = -32601;
const RPC_INVALID_PARAMS = -32602;

/** Quanto do stderr guardar para a mensagem de uma morte. */
const STDERR_LIMIT = 64 * 1024;

/** Quanto esperar o `shutdown` antes de matar o processo. */
const SHUTDOWN_WAIT_MS = 3000;

/** O processo do servidor, como o cliente o vê. Injetável nos testes. */
export interface ServeProcess {
  stdin: Writable;
  stdout: Readable;
  stderr: Readable | null;
  kill(): void;
  /** `close` com o código de saída e o sinal; `error` quando nem abriu. */
  on(event: 'close', listener: (code: number | null, signal: NodeJS.Signals | null) => void): this;
  on(event: 'error', listener: (error: Error) => void): this;
}

export type ServeSpawn = (command: AuroraCommand) => ServeProcess;

export interface ServeClientOptions {
  /** O nome que vai no `initialize` (`aurora-vscode 0.1.0`). */
  clientName: string;
  /** Como abrir o processo; o padrão é o `spawn` do Node. */
  spawn?: ServeSpawn;
  /** Quanto esperar o `shutdown` no `dispose`. */
  shutdownWaitMs?: number;
  /**
   * O servidor da reabertura é outro Lace (a versão mudou): o binário foi
   * trocado por fora. A conexão refaz a sondagem.
   */
  onServerChanged?: (version: string) => void;
}

interface Session {
  process: ServeProcess;
  channel: RpcChannel;
  /** Resolve com o `ServerInfo` do `initialize`. */
  ready: Promise<ServerInfo>;
  stderr: string;
  /** O servidor avisou que vai encerrar: nada mais vai para ele. */
  ending: boolean;
  /** Resolve quando o processo fecha. */
  closed: Promise<void>;
}

/** `FlowOptions` que a extensão manda em todo fluxo. */
interface FlowOptionsOut {
  command: string;
  wave: 'none';
  viewer_opener: 'vscode';
}

export class ServeAuroraClient implements AuroraClient {
  private session: Session | undefined;
  private lastVersion: string | undefined;
  private readonly handlers = new Map<number, (event: Event) => void>();
  private readonly spawnServe: ServeSpawn;

  constructor(
    readonly command: AuroraCommand,
    private readonly log: AuroraLog | undefined,
    private readonly options: ServeClientOptions,
  ) {
    this.spawnServe = options.spawn ?? defaultSpawn;
  }

  /** A versão do Lace, do `initialize`; abre o processo se preciso. */
  async version(): Promise<string> {
    const info = await this.connect();
    return info.aurora;
  }

  newProject(name: string, dir: string, options?: RunOptions): Promise<AuroraRun<NewReport>> {
    return this.call<ProjectStatus, NewReport>('project.create', { parent: dir, name }, options, (status) => ({
      message: 'Project created',
      path: status.spf,
      root: status.root,
    }));
  }

  status(spf: string, options?: RunOptions): Promise<AuroraRun<StatusReport>> {
    return this.call<ProjectStatus, StatusReport>('project.status', { project: spf }, options, statusReport);
  }

  setTop(spf: string, file: string, options?: RunOptions): Promise<AuroraRun<TopReport>> {
    return this.call<TopReport, TopReport>('project.top', { project: spf, target: file }, options, (top) => ({
      top_level: top.top_level,
      module: top.module,
      module_error: top.module_error ? errorInfo(top.module_error) : null,
    }));
  }

  add(spf: string, files: readonly string[], options: AddOptions = {}): Promise<AuroraRun<AddReport>> {
    return this.call<AddReport['files'], AddReport>(
      'project.add',
      { project: spf, files: [...files], testbench: options.testbench === true },
      options,
      (added) => ({ project: spf, files: added }),
    );
  }

  remove(spf: string, files: readonly string[], options?: RunOptions): Promise<AuroraRun<RemoveReport>> {
    return this.call<Omit<RemoveReport, 'project'>, RemoveReport>(
      'project.remove',
      { project: spf, files: [...files] },
      options,
      (outcome) => ({ project: spf, removed: outcome.removed, not_registered: outcome.not_registered }),
    );
  }

  order(spf: string, file: string, position: ListPosition, options?: RunOptions): Promise<AuroraRun<OrderReport>> {
    // O `ListPosition` do core leva o outro arquivo em `path`.
    const where =
      position.kind === 'before' || position.kind === 'after'
        ? { kind: position.kind, path: position.other }
        : { kind: position.kind };
    return this.call<OrderReport['files'], OrderReport>(
      'project.order',
      { project: spf, file, position: where },
      options,
      (files) => ({ project: spf, files }),
    );
  }

  move(spf: string, from: string, to: string, options: RunOptions & { after?: boolean } = {}): Promise<AuroraRun<MoveReport>> {
    return this.call<MoveReport['moved'][number], MoveReport>(
      'project.move',
      { project: spf, from, to, after: options.after === true },
      options,
      (moved) => ({ project: spf, moved: [moved] }),
    );
  }

  build(spf: string, options: BuildOptions = {}): Promise<AuroraRun<BuildReport>> {
    const processors = options.processors ?? [];
    const command = ['aurora build', ...processors.flatMap((name) => ['-p', name])].join(' ');
    return this.flow<BuildReport>(
      spf,
      { flow: 'build', processors: [...processors] },
      command,
      options,
      (outcome) => ({
        project: spf,
        results: outcome.builds,
        registered: outcome.registered,
        report: outcome.report,
      }),
    );
  }

  addProcessor(spf: string, name: string, language: 'cmm' | 'cpp', options?: RunOptions): Promise<AuroraRun<Processor>> {
    return this.call<Processor, Processor>('processor.add', { project: spf, spec: { name, language } }, options, same);
  }

  configureProcessor(
    spf: string,
    name: string,
    changes: ProcessorChanges,
    options?: RunOptions,
  ): Promise<AuroraRun<Processor>> {
    const config: Record<string, unknown> = {};
    if (changes.frequencyMhz !== undefined) {
      config.frequency_mhz = changes.frequencyMhz;
    }
    if (changes.clocks !== undefined) {
      config.clocks = changes.clocks;
    }
    if (changes.showArrays !== undefined) {
      config.show_arrays = changes.showArrays;
    }
    return this.call<Processor, Processor>('processor.set', { project: spf, name, config }, options, same);
  }

  check(spf: string, options: CheckOptions = {}): Promise<AuroraRun<CheckReport>> {
    const request: Record<string, unknown> = { flow: 'check', lint: options.lint === true };
    if (options.file !== undefined) {
      request.file = options.file;
    }
    if (options.processor !== undefined) {
      request.processor = options.processor;
    }
    const command = [
      'aurora check',
      ...(options.file !== undefined ? [options.file] : []),
      ...(options.processor !== undefined ? ['-p', options.processor] : []),
      ...(options.lint ? ['--lint'] : []),
    ].join(' ');
    return this.flow<CheckReport>(spf, request, command, options, (outcome, run) => {
      if (outcome.check === null && run !== 'cancelled') {
        throw new AuroraProcessError('o aurora serve terminou o check sem o resultado dele');
      }
      return { check: outcome.check as CheckReport['check'], report: outcome.report };
    });
  }

  wave(spf: string | undefined, target: WaveTarget, options?: RunOptions): Promise<AuroraRun<WaveReport>> {
    const source =
      target.kind === 'file'
        ? { kind: 'file', path: target.path }
        : target.kind === 'processor'
          ? { kind: 'project', processor: target.name }
          : { kind: 'project' };
    const params: Record<string, unknown> = { request: { source, layout: { kind: 'generated' } } };
    if (spf !== undefined) {
      params.project = spf;
    }
    return this.call<WaveOpened, WaveReport>('wave.open', params, options, (opened) => {
      if (opened.layout_error) {
        this.log?.warn(`O layout da onda não foi aplicado: ${opened.layout_error}`);
      }
      return opened;
    });
  }

  sim(spf: string, options: SimOptions = {}): Promise<AuroraRun<SimReport>> {
    const request: Record<string, unknown> = {
      flow: 'simulate',
      simulator: options.verilator || options.fast ? 'verilator' : 'icarus',
      fast: options.fast === true,
      open_wave: false,
    };
    if (options.testbench !== undefined) {
      request.testbench = options.testbench;
    }
    if (options.processor !== undefined) {
      request.processor = options.processor;
    }
    if (options.timeoutSeconds) {
      request.timeout_s = options.timeoutSeconds;
    }
    const command = [
      'aurora sim',
      ...(options.testbench !== undefined ? [options.testbench] : []),
      ...(options.processor !== undefined ? ['-p', options.processor] : []),
      ...(options.fast ? ['--fast'] : []),
      ...(options.verilator ? ['--verilator'] : []),
      ...(options.timeoutSeconds ? ['--timeout', String(options.timeoutSeconds)] : []),
    ].join(' ');
    return this.flow<SimReport>(spf, request, command, options, (outcome) => ({
      builds: outcome.builds,
      registered: outcome.registered,
      simulation: outcome.simulation,
      outputs: outcome.outputs.map((port) => ({
        port: port.port,
        path: port.path,
        values: port.values,
        error: port.error ? port.error.message : null,
      })),
      surfer_pid: outcome.wave?.pid ?? null,
      report: outcome.report,
    }));
  }

  synth(spf: string, options: SynthOptions = {}): Promise<AuroraRun<SynthReport>> {
    const request: Record<string, unknown> = {
      flow: 'synthesize',
      schematic: options.svg === true,
      open_viewer: false,
    };
    if (options.processor !== undefined) {
      request.processor = options.processor;
    }
    const command = [
      'aurora synth',
      ...(options.processor !== undefined ? ['-p', options.processor] : []),
      ...(options.svg ? ['--svg'] : []),
    ].join(' ');
    return this.flow<SynthReport>(spf, request, command, options, (outcome) => ({
      builds: outcome.builds,
      registered: outcome.registered,
      synthesis: outcome.synthesis,
      schematic: outcome.schematic,
      rtl_viewer: outcome.rtl_viewer,
      report: outcome.report,
    }));
  }

  /**
   * Encerra o servidor: manda `shutdown`, espera o processo sair até
   * `shutdownWaitMs` e, se ele não sair, o mata.
   */
  async dispose(): Promise<void> {
    const session = this.session;
    if (!session) {
      return;
    }
    this.session = undefined;
    session.ending = true;
    const wait = this.options.shutdownWaitMs ?? SHUTDOWN_WAIT_MS;
    if (!session.channel.isClosed) {
      try {
        const { reply } = session.channel.request('shutdown');
        reply.catch(() => undefined);
        session.process.stdin.end();
      } catch {
        // O canal já tinha fechado: só espera o processo.
      }
    }
    let timer: NodeJS.Timeout | undefined;
    const timeout = new Promise<'timeout'>((resolve) => {
      timer = setTimeout(() => resolve('timeout'), wait);
    });
    const outcome = await Promise.race([session.closed.then(() => 'closed' as const), timeout]);
    clearTimeout(timer);
    if (outcome === 'timeout') {
      this.log?.warn('O aurora serve não saiu depois do shutdown; encerrando o processo.');
      session.process.kill();
      await session.closed;
    }
  }

  /** A sessão aberta, ou uma nova: abre o processo e faz o `initialize`. */
  private async connect(): Promise<ServerInfo> {
    if (this.session && !this.session.ending && !this.session.channel.isClosed) {
      return this.session.ready;
    }
    const session = this.open();
    this.session = session;
    const info = await session.ready;
    if (this.lastVersion !== undefined && this.lastVersion !== info.aurora) {
      this.log?.info(`O aurora serve reaberto é a versão ${info.aurora} (antes ${this.lastVersion})`);
      this.options.onServerChanged?.(info.aurora);
    }
    this.lastVersion = info.aurora;
    return info;
  }

  private open(): Session {
    const argv = [...this.command.args, 'serve'];
    this.log?.info(`$ ${commandLine(this.command.file, argv)}`);
    let process: ServeProcess;
    try {
      process = this.spawnServe(this.command);
    } catch (error) {
      const failed = new AuroraProcessError(`não foi possível rodar ${this.command.file}: ${(error as Error).message}`);
      return {
        process: { stdin: deadWritable(), stdout: deadReadable(), stderr: null, kill() {}, on() { return this; } },
        channel: deadChannel(failed.message),
        ready: Promise.reject(failed),
        stderr: '',
        ending: true,
        closed: Promise.resolve(),
      };
    }
    const session: Partial<Session> & { stderr: string; ending: boolean } = { stderr: '', ending: false };
    const channel = new RpcChannel(process.stdin, process.stdout, {
      onNotification: (notification) => this.onNotification(session as Session, notification),
      onProtocolWarning: (message) => this.log?.warn(message),
    });
    process.stderr?.setEncoding('utf8');
    process.stderr?.on('data', (chunk: string) => {
      session.stderr = (session.stderr + chunk).slice(-STDERR_LIMIT);
      for (const line of chunk.split('\n')) {
        if (line.trim() !== '') {
          this.log?.warn(line.trimEnd());
        }
      }
    });
    const closed = new Promise<void>((resolve) => {
      process.on('error', (error) => {
        channel.close(`não foi possível rodar ${this.command.file}: ${error.message}`);
        if (this.session === session) {
          this.session = undefined;
        }
        resolve();
      });
      process.on('close', (code, signal) => {
        const how =
          code !== null ? `saiu com o código ${code}` : signal !== null ? `terminou pelo sinal ${signal}` : 'terminou';
        this.log?.info(`  o aurora serve ${how}`);
        channel.close(`o aurora serve ${how}`);
        if (this.session === session) {
          this.session = undefined;
        }
        resolve();
      });
    });
    const { reply } = channel.request('initialize', { client: this.options.clientName });
    const ready = reply.then((answer) => {
      if ('error' in answer) {
        throw new AuroraProcessError(`o aurora serve recusou o initialize: ${answer.error.message}`, null, session.stderr);
      }
      const info = answer.result as Partial<ServerInfo>;
      if (typeof info?.aurora !== 'string' || typeof info.protocol !== 'number') {
        throw new AuroraProcessError('o aurora serve respondeu ao initialize sem a versão', null, session.stderr);
      }
      if (info.protocol !== PROTOCOL_VERSION) {
        process.kill();
        throw new AuroraProcessError(
          `o aurora serve fala a versão ${info.protocol} do protocolo, e a extensão a ${PROTOCOL_VERSION}`,
          null,
          session.stderr,
        );
      }
      return info as ServerInfo;
    });
    ready.catch(() => undefined);
    Object.assign(session, { process, channel, ready, closed });
    return session as Session;
  }

  private onNotification(session: Session, notification: RpcNotification): void {
    const params = (notification.params ?? {}) as Record<string, unknown>;
    switch (notification.method) {
      case 'events': {
        const handler = typeof params.id === 'number' ? this.handlers.get(params.id) : undefined;
        if (!handler || !Array.isArray(params.events)) {
          return;
        }
        for (const event of params.events as Event[]) {
          try {
            handler(event);
          } catch (error) {
            this.log?.warn(`Erro ao tratar um evento do aurora: ${String(error)}`);
          }
        }
        return;
      }
      case 'output_omitted':
        this.log?.warn(`${String(params.lines)} linhas de ${String(params.tool)} omitidas (${String(params.step)})`);
        return;
      case 'shutdown':
        session.ending = true;
        this.log?.info(`O aurora serve vai encerrar (${String(params.reason)})`);
        return;
      default:
        // phase, build, install: a extensão não tem onde mostrar.
        return;
    }
  }

  /** Um pedido que não é fluxo: `succeeded` com o resultado convertido. */
  private async call<R, T>(
    method: string,
    params: unknown,
    options: RunOptions | undefined,
    convert: (result: R) => T,
  ): Promise<AuroraRun<T>> {
    const answer = await this.send(method, params, options);
    if (answer.kind === 'cancelled') {
      return { status: 'cancelled', result: null };
    }
    if (answer.kind === 'error') {
      return { status: 'error', error: answer.error };
    }
    return { status: 'succeeded', result: convert(answer.result as R) };
  }

  /** Um `flow.run`: o status do `FlowOutcome` decide o `AuroraRun`. */
  private async flow<T>(
    spf: string,
    request: Record<string, unknown>,
    command: string,
    options: RunOptions & { onEvent?: (event: Event) => void },
    convert: (outcome: FlowOutcome, status: 'succeeded' | 'failed' | 'cancelled') => T,
  ): Promise<AuroraRun<T>> {
    const flowOptions: FlowOptionsOut = { command, wave: 'none', viewer_opener: 'vscode' };
    const answer = await this.send(
      'flow.run',
      { project: spf, request, options: flowOptions, output: true },
      options,
      options.onEvent,
    );
    if (answer.kind === 'cancelled') {
      return { status: 'cancelled', result: null };
    }
    if (answer.kind === 'error') {
      return { status: 'error', error: answer.error };
    }
    const outcome = answer.result as FlowOutcome;
    if (typeof outcome?.status !== 'string') {
      throw new AuroraProcessError('o aurora serve terminou o fluxo sem um resultado');
    }
    if (outcome.schematic_error) {
      // A CLI imprime o erro do esquemático e sai com 2.
      return { status: 'error', error: errorInfo(outcome.schematic_error) };
    }
    const status = outcome.status === 'succeeded' ? 'succeeded' : outcome.status === 'cancelled' ? 'cancelled' : 'failed';
    return { status, result: convert(outcome, status) } as AuroraRun<T>;
  }

  /**
   * Manda o pedido, com o cancelamento pelo `$/cancel` e os eventos para
   * `onEvent`, e interpreta a resposta.
   */
  private async send(
    method: string,
    params: unknown,
    options: RunOptions | undefined,
    onEvent?: (event: Event) => void,
  ): Promise<{ kind: 'result'; result: unknown } | { kind: 'error'; error: ErrorInfo } | { kind: 'cancelled' }> {
    if (options?.signal?.aborted) {
      return { kind: 'cancelled' };
    }
    await this.connect();
    const session = this.session;
    if (!session) {
      throw new AuroraProcessError('o aurora serve fechou antes do pedido');
    }
    const started = Date.now();
    this.log?.info(`> ${method} ${summarize(params)}`);
    const { id, reply } = session.channel.request(method, params, (id) => {
      if (onEvent) {
        this.handlers.set(id, onEvent);
      }
    });
    const onAbort = () => session.channel.notify('$/cancel', { id });
    options?.signal?.addEventListener('abort', onAbort, { once: true });
    let answer: RpcReply;
    try {
      answer = await reply;
    } catch (error) {
      if (error instanceof RpcClosedError) {
        throw new AuroraProcessError(error.reason, null, session.stderr);
      }
      throw error;
    } finally {
      options?.signal?.removeEventListener('abort', onAbort);
      this.handlers.delete(id);
    }
    const elapsed = Date.now() - started;
    if ('error' in answer) {
      this.log?.info(`< ${method}: erro ${answer.error.code} em ${elapsed} ms`);
      return interpretError(answer.error, session);
    }
    this.log?.info(`< ${method}: ok em ${elapsed} ms`);
    return { kind: 'result', result: answer.result };
  }
}

function interpretError(
  error: RpcError,
  session: Session,
): { kind: 'error'; error: ErrorInfo } | { kind: 'cancelled' } {
  switch (error.code) {
    case RPC_AURORA: {
      const data = error.data as Partial<ErrorInfo> | undefined;
      if (typeof data?.code !== 'string') {
        throw new AuroraProcessError(`o aurora serve devolveu um erro sem código: ${error.message}`, null, session.stderr);
      }
      return { kind: 'error', error: { code: data.code, message: error.message, hint: data.hint ?? null } };
    }
    case RPC_CANCELLED:
      return { kind: 'cancelled' };
    case RPC_METHOD_NOT_FOUND:
    case RPC_INVALID_PARAMS:
      // O equivalente de hoje a "o clap recusou os argumentos".
      return { kind: 'error', error: { code: 'cli', message: error.message, hint: null } };
    default:
      throw new AuroraProcessError(`o aurora serve respondeu ${error.code}: ${error.message}`, null, session.stderr);
  }
}

/** O `StatusReport` da CLI a partir do `ProjectStatus` do protocolo. */
export function statusReport(status: ProjectStatus): StatusReport {
  return {
    name: status.name,
    spf: status.spf,
    root: status.root,
    synthesizable: status.synthesizable,
    testbench: status.testbenches,
    top_level: status.top_level,
    top_module: status.top_module,
    selected_testbench: status.selected_testbench,
    testbench_module: status.testbench_module,
    unregistered: status.unregistered,
    here: null,
    processors: status.processors,
    issues: status.issues,
  };
}

/** O `ErrorInfo` com `hint` sempre presente, como a CLI o escreve. */
function errorInfo(error: ErrorInfo): ErrorInfo {
  return { code: error.code, message: error.message, hint: error.hint ?? null };
}

function same<T>(value: T): T {
  return value;
}

function defaultSpawn(command: AuroraCommand): ServeProcess {
  return spawnServe(command) as unknown as ServeProcess;
}

function commandLine(file: string, args: readonly string[]): string {
  return [file, ...args].map((a) => (/[\s"']/.test(a) || a === '' ? JSON.stringify(a) : a)).join(' ');
}

/** Os parâmetros para o log, sem conteúdo de arquivos. */
function summarize(params: unknown): string {
  if (params === undefined) {
    return '';
  }
  return clip(JSON.stringify(params));
}

function deadChannel(reason: string): RpcChannel {
  const channel = new RpcChannel(new PassThrough(), new PassThrough());
  channel.close(reason);
  return channel;
}

function deadWritable(): Writable {
  return new PassThrough();
}

function deadReadable(): Readable {
  return new PassThrough();
}
