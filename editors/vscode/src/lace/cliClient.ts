// O LaceClient pela linha de comando, e o único lugar da extensão que cria
// processos (o `aurora serve` do serveClient.ts também nasce aqui). Cada
// chamada roda `lace -C <.spf> --json ...` (ou `--events`) sem shell, com os
// caminhos como argumentos, e interpreta o código de saída como o
// docs/CLI.md descreve.

import { spawn, type ChildProcess } from 'node:child_process';

import {
  LaceProcessError,
  type AddOptions,
  type BuildOptions,
  type CheckOptions,
  type LaceClient,
  type LaceRun,
  type ListPosition,
  type ProcessorChanges,
  type RunOptions,
  type SimOptions,
  type SynthOptions,
  type WaveTarget,
} from './client';
import type {
  AddReport,
  BuildReport,
  CheckReport,
  ErrorInfo,
  Event,
  MoveReport,
  NewReport,
  OrderReport,
  Processor,
  RemoveReport,
  SimReport,
  StatusReport,
  SynthReport,
  TopReport,
  WaveReport,
} from './lace-types';
import { LineSplitter } from './lines';
import { parseVersionLine } from './version';

/** Como iniciar o lace: o executável e os argumentos que vêm antes dos do comando. */
export interface LaceCommand {
  file: string;
  args: readonly string[];
}

/** Onde o cliente registra o que roda (no VS Code, o canal de saída Lace). */
export interface LaceLog {
  info(message: string): void;
  warn(message: string): void;
}

/**
 * Abre um `aurora serve` com stdin, stdout e stderr em pipes: o processo do
 * ServeLaceClient. Sem shell e sem janela, como os comandos.
 */
export function spawnServe(command: LaceCommand): ChildProcess {
  return spawn(command.file, [...command.args, 'serve'], {
    shell: false,
    windowsHide: true,
    stdio: ['pipe', 'pipe', 'pipe'],
  });
}

export interface JsonOptions extends RunOptions {
  /** O `-C`: o `.spf` do projeto, ou uma pasta dentro dele. */
  project?: string;
}

export interface EventsOptions extends JsonOptions {
  /** Cada evento do Core, assim que a linha dele chega. */
  onEvent?: (event: Event) => void;
}

// docs/CLI.md, "Códigos de saída".
const EXIT_SUCCEEDED = 0;
const EXIT_FAILED = 1;
const EXIT_ERROR = 2;
const EXIT_CANCELLED = 130;

/** Quanto do stderr guardar: o bastante para a mensagem, sem crescer sem fim. */
const STDERR_LIMIT = 64 * 1024;

interface Finished {
  code: number | null;
  signal: NodeJS.Signals | null;
  stderr: string;
  /** O cancelamento foi pedido enquanto o processo rodava. */
  cancelled: boolean;
}

export class CliLaceClient implements LaceClient {
  constructor(
    readonly command: LaceCommand,
    private readonly log?: LaceLog,
  ) {}

  /**
   * A versão, da linha do `aurora --version`. É o único texto que a extensão
   * lê do lace: nenhum comando dá a versão em JSON.
   */
  async version(): Promise<string> {
    let stdout = '';
    const done = await this.spawn(['--version'], {}, (chunk) => {
      stdout += chunk;
    });
    if (done.code !== EXIT_SUCCEEDED) {
      throw new LaceProcessError(`aurora --version ${describe(done)}`, done.code, done.stderr);
    }
    const version = parseVersionLine(stdout);
    if (version === undefined) {
      throw new LaceProcessError(
        `${this.command.file} não parece ser o lace: --version escreveu "${clip(stdout.trim())}"`,
        done.code,
        done.stderr,
      );
    }
    return version;
  }

  newProject(name: string, dir: string, options?: RunOptions): Promise<LaceRun<NewReport>> {
    // Sem -C: o projeto ainda não existe, e a pasta vai inteira no --dir.
    return this.runJson<NewReport>(['new', name, '--dir', dir], { ...options });
  }

  status(spf: string, options?: RunOptions): Promise<LaceRun<StatusReport>> {
    return this.runJson<StatusReport>(['status'], { ...options, project: spf });
  }

  setTop(spf: string, file: string, options?: RunOptions): Promise<LaceRun<TopReport>> {
    return this.runJson<TopReport>(['top', file], { ...options, project: spf });
  }

  add(spf: string, files: readonly string[], options: AddOptions = {}): Promise<LaceRun<AddReport>> {
    const args = ['add', ...files, ...(options.testbench ? ['--tb'] : [])];
    return this.runJson<AddReport>(args, { signal: options.signal, project: spf });
  }

  remove(spf: string, files: readonly string[], options?: RunOptions): Promise<LaceRun<RemoveReport>> {
    return this.runJson<RemoveReport>(['remove', ...files], { ...options, project: spf });
  }

  order(spf: string, file: string, position: ListPosition, options?: RunOptions): Promise<LaceRun<OrderReport>> {
    const where =
      position.kind === 'before'
        ? ['--before', position.other]
        : position.kind === 'after'
          ? ['--after', position.other]
          : [`--${position.kind}`];
    return this.runJson<OrderReport>(['order', file, ...where], { ...options, project: spf });
  }

  move(spf: string, from: string, to: string, options: RunOptions & { after?: boolean } = {}): Promise<LaceRun<MoveReport>> {
    const args = ['move', ...(options.after ? ['--after'] : []), from, to];
    return this.runJson<MoveReport>(args, { signal: options.signal, project: spf });
  }

  build(spf: string, options: BuildOptions = {}): Promise<LaceRun<BuildReport>> {
    const args = ['build', ...(options.processors ?? []).flatMap((name) => ['-p', name])];
    return this.runEvents<BuildReport>(args, { signal: options.signal, onEvent: options.onEvent, project: spf });
  }

  addProcessor(spf: string, name: string, language: 'cmm' | 'cpp', options?: RunOptions): Promise<LaceRun<Processor>> {
    return this.runJson<Processor>(['proc', 'add', name, '--lang', language], { ...options, project: spf });
  }

  configureProcessor(
    spf: string,
    name: string,
    changes: ProcessorChanges,
    options?: RunOptions,
  ): Promise<LaceRun<Processor>> {
    const args = [
      'proc',
      'set',
      name,
      ...(changes.frequencyMhz !== undefined ? ['--freq', String(changes.frequencyMhz)] : []),
      ...(changes.clocks !== undefined ? ['--clocks', String(changes.clocks)] : []),
      ...(changes.showArrays !== undefined ? ['--arrays', String(changes.showArrays)] : []),
    ];
    return this.runJson<Processor>(args, { ...options, project: spf });
  }

  check(spf: string, options: CheckOptions = {}): Promise<LaceRun<CheckReport>> {
    const args = [
      'check',
      ...(options.file !== undefined ? [options.file] : []),
      ...(options.processor !== undefined ? ['-p', options.processor] : []),
      ...(options.lint ? ['--lint'] : []),
    ];
    return this.runEvents<CheckReport>(args, { signal: options.signal, onEvent: options.onEvent, project: spf });
  }

  wave(spf: string | undefined, target: WaveTarget, options?: RunOptions): Promise<LaceRun<WaveReport>> {
    const args =
      target.kind === 'processor'
        ? ['wave', '-p', target.name]
        : target.kind === 'file'
          ? ['wave', target.path]
          : ['wave'];
    return this.runJson<WaveReport>(args, { ...options, project: spf });
  }

  sim(spf: string, options: SimOptions = {}): Promise<LaceRun<SimReport>> {
    const args = [
      'sim',
      ...(options.testbench !== undefined ? [options.testbench] : []),
      ...(options.processor !== undefined ? ['-p', options.processor] : []),
      ...(options.fast ? ['--fast'] : []),
      ...(options.verilator ? ['--verilator'] : []),
      ...(options.timeoutSeconds ? ['--timeout', String(options.timeoutSeconds)] : []),
    ];
    return this.runEvents<SimReport>(args, { signal: options.signal, onEvent: options.onEvent, project: spf });
  }

  synth(spf: string, options: SynthOptions = {}): Promise<LaceRun<SynthReport>> {
    const args = [
      'synth',
      ...(options.processor !== undefined ? ['-p', options.processor] : []),
      ...(options.svg ? ['--svg'] : []),
    ];
    return this.runEvents<SynthReport>(args, { signal: options.signal, onEvent: options.onEvent, project: spf });
  }

  /** Roda com `--json`: o stdout é um objeto só, escrito no fim. */
  async runJson<T>(args: readonly string[], options: JsonOptions = {}): Promise<LaceRun<T>> {
    if (options.signal?.aborted) {
      return { status: 'cancelled', result: null };
    }
    let stdout = '';
    const done = await this.spawn(withGlobals('--json', args, options.project), options, (chunk) => {
      stdout += chunk;
    });
    return interpret<T>(done, parseObject(stdout, done));
  }

  /**
   * Roda com `--events`: um objeto JSON por linha, cada evento do Core
   * enquanto as ferramentas rodam e, na última linha, `{"event": "result"}`
   * com o mesmo objeto do `--json` (ou o erro, no código 2).
   */
  async runEvents<T>(args: readonly string[], options: EventsOptions = {}): Promise<LaceRun<T>> {
    if (options.signal?.aborted) {
      return { status: 'cancelled', result: null };
    }
    const lines = new LineSplitter();
    let result: unknown;
    let badLine: string | undefined;
    const take = (line: string) => {
      if (line.trim() === '') {
        return;
      }
      let value: unknown;
      try {
        value = JSON.parse(line);
      } catch {
        badLine ??= line;
        return;
      }
      if (isResultLine(value)) {
        result = value.result;
        return;
      }
      try {
        options.onEvent?.(value as Event);
      } catch (error) {
        this.log?.warn(`Erro ao tratar um evento do lace: ${String(error)}`);
      }
    };
    const done = await this.spawn(withGlobals('--events', args, options.project), options, (chunk) => {
      lines.push(chunk).forEach(take);
    });
    lines.end().forEach(take);
    if (badLine !== undefined) {
      throw protocolError(`uma linha do --events não é JSON: ${clip(badLine)}`, done);
    }
    return interpret<T>(done, result);
  }

  private spawn(
    args: readonly string[],
    options: RunOptions,
    onStdout: (chunk: string) => void,
  ): Promise<Finished> {
    const argv = [...this.command.args, ...args];
    this.log?.info(`$ ${commandLine(this.command.file, argv)}`);
    const started = Date.now();
    return new Promise((resolve, reject) => {
      // Sem shell: cada caminho vai inteiro num argumento, com espaço ou não.
      // O stdin fechado faz o lace ver que não há terminal (nada de pergunta).
      const child = spawn(this.command.file, argv, {
        shell: false,
        windowsHide: true,
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      let stderr = '';
      let cancelled = false;
      let settled = false;
      const onAbort = () => {
        cancelled = true;
        // Sem argumento: SIGTERM no Unix, que o lace trata como Ctrl+C:
        // encerra a ferramenta com tudo o que ela iniciou e sai com 130.
        // Nunca SIGKILL, que mata só o lace e deixa a ferramenta, num grupo
        // de processos próprio, rodando sozinha (docs/CLI.md, "Interromper").
        // No Windows, o kill encerra o lace e o Job Object leva a ferramenta.
        child.kill();
      };
      options.signal?.addEventListener('abort', onAbort, { once: true });
      child.stdout.setEncoding('utf8');
      child.stderr.setEncoding('utf8');
      child.stdout.on('data', onStdout);
      child.stderr.on('data', (chunk: string) => {
        stderr = (stderr + chunk).slice(-STDERR_LIMIT);
      });
      child.on('error', (error) => {
        if (settled) {
          return;
        }
        settled = true;
        options.signal?.removeEventListener('abort', onAbort);
        reject(new LaceProcessError(`não foi possível rodar ${this.command.file}: ${error.message}`));
      });
      child.on('close', (code, signal) => {
        if (settled) {
          return;
        }
        settled = true;
        options.signal?.removeEventListener('abort', onAbort);
        const done: Finished = { code, signal, stderr, cancelled };
        this.log?.info(`  ${describe(done)} em ${Date.now() - started} ms`);
        if (stderr.trim() !== '') {
          this.log?.warn(stderr.trimEnd());
        }
        resolve(done);
      });
    });
  }
}

function withGlobals(mode: '--json' | '--events', args: readonly string[], project?: string): string[] {
  return project === undefined ? [mode, ...args] : ['-C', project, mode, ...args];
}

function interpret<T>(done: Finished, payload: unknown): LaceRun<T> {
  // Cancelado sem resultado: no Windows o kill encerra o lace antes de ele
  // escrever o objeto.
  if (done.cancelled && payload === undefined) {
    return { status: 'cancelled', result: null };
  }
  switch (done.code) {
    case EXIT_SUCCEEDED:
      return { status: 'succeeded', result: required<T>(payload, done) };
    case EXIT_FAILED:
      return { status: 'failed', result: required<T>(payload, done) };
    case EXIT_CANCELLED:
      return { status: 'cancelled', result: (payload ?? null) as T | null };
    case EXIT_ERROR:
      return { status: 'error', error: errorInfo(payload, done) };
  }
  throw new LaceProcessError(`o lace ${describe(done)}`, done.code, done.stderr);
}

function required<T>(payload: unknown, done: Finished): T {
  if (payload === undefined) {
    throw protocolError(`o lace ${describe(done)} sem escrever o resultado em JSON`, done);
  }
  return payload as T;
}

function errorInfo(payload: unknown, done: Finished): ErrorInfo {
  if (payload === undefined) {
    // Erro de argumento: o clap o detecta antes de o comando começar e o
    // escreve em texto no stderr, sem JSON (docs/CLI.md, "JSON"). O texto só
    // vai para a mensagem; o código é o dos erros da própria linha de comando.
    const line = done.stderr.split('\n').find((l) => l.trim() !== '') ?? '';
    const message = line.trim().replace(/^error:\s*/i, '') || 'o lace recusou os argumentos';
    return { code: 'cli', message: message[0].toUpperCase() + message.slice(1), hint: null };
  }
  const error = (payload as { error?: unknown }).error as Partial<ErrorInfo> | undefined;
  if (typeof error?.code !== 'string' || typeof error.message !== 'string') {
    throw protocolError('o lace saiu com 2 sem um objeto de erro', done);
  }
  return { code: error.code, message: error.message, hint: error.hint ?? null };
}

function parseObject(stdout: string, done: Finished): unknown {
  const text = stdout.trim();
  if (text === '') {
    return undefined;
  }
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    if (done.cancelled) {
      return undefined;
    }
    throw protocolError(`o stdout do --json não é JSON: ${clip(text)}`, done);
  }
  if (typeof value !== 'object' || value === null) {
    throw protocolError(`o stdout do --json não é um objeto: ${clip(text)}`, done);
  }
  return value;
}

function isResultLine(value: unknown): value is { event: 'result'; result: unknown } {
  return (
    typeof value === 'object' &&
    value !== null &&
    (value as { event?: unknown }).event === 'result' &&
    'result' in value
  );
}

function protocolError(message: string, done: Finished): LaceProcessError {
  return new LaceProcessError(message, done.code, done.stderr);
}

function describe(done: Finished): string {
  if (done.code !== null) {
    return `saiu com o código ${done.code}`;
  }
  return done.signal !== null ? `terminou pelo sinal ${done.signal}` : 'terminou sem código de saída';
}

function commandLine(file: string, args: readonly string[]): string {
  return [file, ...args].map((a) => (/[\s"']/.test(a) || a === '' ? JSON.stringify(a) : a)).join(' ');
}

function clip(text: string): string {
  return text.length > 200 ? `${text.slice(0, 200)}...` : text;
}
