// O ServeAuroraClient contra um aurora serve falso dentro do processo: o
// handshake, o que cada método manda, a volta para os relatórios da CLI, os
// eventos, o cancelamento, a morte do processo e o encerramento.

import * as assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { PassThrough } from 'node:stream';

import { AuroraProcessError } from '../../src/aurora/client';
import type { AuroraCommand } from '../../src/aurora/cliClient';
import type {
  CheckReport,
  Event,
  ProjectStatus,
  SimReport,
  StatusReport,
  SynthReport,
  TopReport,
} from '../../src/aurora/aurora-types';
import { ServeAuroraClient, type ServeProcess } from '../../src/aurora/serveClient';
import { RESPONSES } from '../support/fixtures';

interface Incoming {
  id?: number;
  method: string;
  params?: Record<string, unknown>;
}

/** O que o servidor falso responde a um pedido. */
type Script = (request: Incoming, server: FakeServe) => void;

/** Um aurora serve falso: os streams de um processo, roteado por `script`. */
class FakeServe extends EventEmitter implements ServeProcess {
  readonly stdin = new PassThrough();
  readonly stdout = new PassThrough();
  readonly stderr = new PassThrough();
  readonly received: Incoming[] = [];
  killed = false;
  private rest = '';

  constructor(
    private readonly script: Script,
    readonly info: Record<string, unknown> = { aurora: '0.9.0', protocol: 2, prefix: null, experimental: {}, methods: [] },
  ) {
    super();
    this.stdin.setEncoding('utf8');
    this.stdin.on('data', (chunk: string) => {
      const parts = (this.rest + chunk).split('\n');
      this.rest = parts.pop() ?? '';
      for (const line of parts.filter((l) => l.trim() !== '')) {
        const request = JSON.parse(line) as Incoming;
        this.received.push(request);
        if (request.method === 'initialize') {
          this.respond(request.id as number, this.info);
        } else if (request.method === 'shutdown') {
          this.respond(request.id as number, null);
          setImmediate(() => this.close(0));
        } else {
          this.script(request, this);
        }
      }
    });
  }

  respond(id: number, result: unknown): void {
    this.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', id, result })}\n`);
  }

  fail(id: number, code: number, message: string, data?: unknown): void {
    const error: Record<string, unknown> = { code, message };
    if (data !== undefined) {
      error.data = data;
    }
    this.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', id, error })}\n`);
  }

  notify(method: string, params: unknown): void {
    this.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', method, params })}\n`);
  }

  writeStderr(text: string): void {
    this.stderr.write(text);
  }

  close(code: number | null, signal: NodeJS.Signals | null = null): void {
    this.stdout.end();
    this.stderr.end();
    this.emit('close', code, signal);
  }

  kill(): void {
    this.killed = true;
    setImmediate(() => this.close(null, 'SIGTERM'));
  }

  /** Os pedidos recebidos além do initialize. */
  get requests(): Incoming[] {
    return this.received.filter((r) => r.method !== 'initialize');
  }
}

const command: AuroraCommand = { file: '/usr/bin/lace', args: [] };
const spf = '/tmp/projeto demo/demo.spf';

function recorded<T>(file: string): T {
  return JSON.parse(readFileSync(join(RESPONSES, file), 'utf8').replaceAll('${ROOT}', '/tmp/projeto demo')) as T;
}

/** O `ProjectStatus` do protocolo a partir do `StatusReport` gravado. */
function projectStatus(): ProjectStatus {
  const report = recorded<StatusReport>('status.json');
  const { testbench, here, ...rest } = report;
  void here;
  return {
    ...rest,
    testbenches: testbench,
    top_module_error: null,
    wave_selection: [],
    waveform: null,
    top_candidates: [],
    processors: report.processors as ProjectStatus['processors'],
  };
}

/** Os eventos e o resultado de uma gravação do --events. */
function recordedEvents(file: string): { events: Event[]; result: Record<string, unknown> } {
  const lines = readFileSync(join(RESPONSES, file), 'utf8')
    .replaceAll('${ROOT}', '/tmp/projeto demo')
    .split('\n')
    .filter((l) => l.trim() !== '')
    .map((l) => JSON.parse(l) as Record<string, unknown>);
  const last = lines.pop() as { event: string; result: Record<string, unknown> };
  assert.equal(last.event, 'result');
  return { events: lines as unknown as Event[], result: last.result };
}

/** Um `FlowOutcome` a partir do relatório gravado da CLI. */
function outcomeFrom(flow: string, report: Record<string, unknown>, status = 'succeeded'): Record<string, unknown> {
  const outputs = ((report.outputs as { error: string | null }[] | undefined) ?? []).map((o) => ({
    ...o,
    error: o.error === null ? null : { code: 'io', message: o.error },
  }));
  return {
    flow,
    command: `aurora ${flow}`,
    status,
    succeeded: status === 'succeeded',
    builds: report.builds ?? report.results ?? [],
    registered: report.registered ?? [],
    check: report.check ?? null,
    lint_missing: null,
    hierarchy: null,
    not_built: [],
    simulation: report.simulation ?? null,
    outputs,
    synthesis: report.synthesis ?? null,
    schematic: report.schematic ?? null,
    schematic_error: null,
    rtl_viewer: report.rtl_viewer ?? null,
    rtl_viewer_error: null,
    wave: null,
    wave_error: null,
    report: report.report ?? null,
    report_error: null,
    fpga: null,
    fpga_program: null,
  };
}

interface Setup {
  client: ServeAuroraClient;
  servers: FakeServe[];
  warnings: string[];
  infos: string[];
}

function setup(script: Script, info?: Record<string, unknown>): Setup {
  const servers: FakeServe[] = [];
  const warnings: string[] = [];
  const infos: string[] = [];
  const client = new ServeAuroraClient(
    command,
    { info: (m) => infos.push(m), warn: (m) => warnings.push(m) },
    {
      clientName: 'aurora-vscode teste',
      shutdownWaitMs: 200,
      spawn: () => {
        const server = new FakeServe(script, info);
        servers.push(server);
        return server;
      },
    },
  );
  return { client, servers, warnings, infos };
}

const nothing: Script = (request, server) => server.fail(request.id as number, -32601, `Unknown method ${request.method}`);

describe('ServeAuroraClient', () => {
  it('abre o processo no primeiro pedido, manda initialize com o nome do cliente e lê a versão', async () => {
    const { client, servers, infos } = setup(nothing);
    assert.equal(servers.length, 0);
    assert.equal(await client.version(), '0.9.0');
    assert.equal(servers.length, 1);
    assert.deepEqual(servers[0].received, [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { client: 'aurora-vscode teste' } },
    ] as unknown as Incoming[]);
    // Outra chamada não abre outro processo.
    assert.equal(await client.version(), '0.9.0');
    assert.equal(servers.length, 1);
    assert.ok(infos.some((m) => m.startsWith('$ /usr/bin/lace serve')));
    await client.dispose();
  });

  it('recusa um protocolo de outra versão e mata o processo', async () => {
    const { client, servers } = setup(nothing, { aurora: '1.0.0', protocol: 3 });
    await assert.rejects(client.version(), (error: unknown) => {
      assert.ok(error instanceof AuroraProcessError);
      assert.match(error.message, /versão 3 do protocolo/);
      return true;
    });
    assert.ok(servers[0].killed);
  });

  it('cada método manda o method e os params do protocolo', async () => {
    const { client, servers } = setup((request, server) => {
      switch (request.method) {
        case 'project.status':
          return server.respond(request.id as number, projectStatus());
        case 'project.top':
          return server.respond(request.id as number, { top_level: '/x.v', module: 'x', module_error: null });
        case 'project.add':
          return server.respond(request.id as number, []);
        case 'project.remove':
          return server.respond(request.id as number, { removed: ['/a.v'], not_registered: [] });
        case 'project.order':
          return server.respond(request.id as number, []);
        case 'project.move':
          return server.respond(request.id as number, { from: '/a.v', to: '/b.v', registered: [] });
        case 'project.create':
          return server.respond(request.id as number, projectStatus());
        case 'processor.add':
        case 'processor.set':
          return server.respond(request.id as number, { name: 'soma' });
        case 'wave.open':
          return server.respond(request.id as number, { waveform: '/w.fst', pid: 1, log: '/l', layout: null });
        case 'flow.run': {
          const flow = (request.params as { request: { flow: string } }).request.flow;
          return server.respond(
            request.id as number,
            outcomeFrom(flow, flow === 'check' ? { check: { status: 'succeeded' } } : {}),
          );
        }
        default:
          return nothing(request, server);
      }
    });
    await client.status(spf);
    await client.setTop(spf, '/x.v');
    await client.add(spf, ['/a.v', '/b.v'], { testbench: true });
    await client.add(spf, ['/c.v']);
    await client.remove(spf, ['/a.v']);
    await client.order(spf, '/a.v', { kind: 'before', other: '/b.v' });
    await client.order(spf, '/a.v', { kind: 'last' });
    await client.move(spf, '/a.v', '/b.v', { after: true });
    await client.newProject('novo', '/tmp/pasta nova');
    await client.addProcessor(spf, 'soma', 'cmm');
    await client.configureProcessor(spf, 'soma', { frequencyMhz: 50, showArrays: true });
    await client.wave(spf, { kind: 'processor', name: 'soma' });
    await client.wave(undefined, { kind: 'file', path: '/w.fst' });
    await client.build(spf, { processors: ['soma'] });
    await client.check(spf, { file: '/a.v', lint: true });
    await client.sim(spf, { processor: 'soma', fast: true, timeoutSeconds: 30 });
    await client.sim(spf, { testbench: '/tb.v' });
    await client.synth(spf, { svg: true });
    const sent = servers[0].requests.map((r) => [r.method, r.params]);
    assert.deepEqual(sent, [
      ['project.status', { project: spf }],
      ['project.top', { project: spf, target: '/x.v' }],
      ['project.add', { project: spf, files: ['/a.v', '/b.v'], testbench: true }],
      ['project.add', { project: spf, files: ['/c.v'], testbench: false }],
      ['project.remove', { project: spf, files: ['/a.v'] }],
      ['project.order', { project: spf, file: '/a.v', position: { kind: 'before', path: '/b.v' } }],
      ['project.order', { project: spf, file: '/a.v', position: { kind: 'last' } }],
      ['project.move', { project: spf, from: '/a.v', to: '/b.v', after: true }],
      ['project.create', { parent: '/tmp/pasta nova', name: 'novo' }],
      ['processor.add', { project: spf, spec: { name: 'soma', language: 'cmm' } }],
      ['processor.set', { project: spf, name: 'soma', config: { frequency_mhz: 50, show_arrays: true } }],
      [
        'wave.open',
        { project: spf, request: { source: { kind: 'project', processor: 'soma' }, layout: { kind: 'generated' } } },
      ],
      ['wave.open', { request: { source: { kind: 'file', path: '/w.fst' }, layout: { kind: 'generated' } } }],
      [
        'flow.run',
        {
          project: spf,
          request: { flow: 'build', processors: ['soma'] },
          options: { command: 'aurora build -p soma', wave: 'none', viewer_opener: 'vscode' },
          output: true,
        },
      ],
      [
        'flow.run',
        {
          project: spf,
          request: { flow: 'check', lint: true, file: '/a.v' },
          options: { command: 'aurora check /a.v --lint', wave: 'none', viewer_opener: 'vscode' },
          output: true,
        },
      ],
      [
        'flow.run',
        {
          project: spf,
          request: {
            flow: 'simulate',
            simulator: 'verilator',
            fast: true,
            open_wave: false,
            processor: 'soma',
            timeout_s: 30,
          },
          options: { command: 'aurora sim -p soma --fast --timeout 30', wave: 'none', viewer_opener: 'vscode' },
          output: true,
        },
      ],
      [
        'flow.run',
        {
          project: spf,
          request: { flow: 'simulate', simulator: 'icarus', fast: false, open_wave: false, testbench: '/tb.v' },
          options: { command: 'aurora sim /tb.v', wave: 'none', viewer_opener: 'vscode' },
          output: true,
        },
      ],
      [
        'flow.run',
        {
          project: spf,
          request: { flow: 'synthesize', schematic: true, open_viewer: false },
          options: { command: 'aurora synth --svg', wave: 'none', viewer_opener: 'vscode' },
          output: true,
        },
      ],
    ]);
    await client.dispose();
  });

  it('project.status vira o StatusReport da CLI, com testbench e here', async () => {
    const { client } = setup((request, server) => server.respond(request.id as number, projectStatus()));
    const run = await client.status(spf);
    assert.equal(run.status, 'succeeded');
    const report = (run as { result: StatusReport }).result;
    const original = recorded<StatusReport>('status.json');
    assert.deepEqual(report.testbench, original.testbench);
    assert.deepEqual(report.synthesizable, original.synthesizable);
    assert.equal(report.top_module, original.top_module);
    assert.equal(report.here, null);
    assert.equal(report.name, 'demo');
    assert.ok(!('testbenches' in report));
    await client.dispose();
  });

  it('flow.run entrega os eventos em lotes, na ordem, antes de resolver, e converte o SimReport', async () => {
    const sim = recordedEvents('sim.events.jsonl');
    const { client } = setup((request, server) => {
      const id = request.id as number;
      server.notify('phase', { id, phase: 'build' });
      const half = Math.ceil(sim.events.length / 2);
      server.notify('events', { id, events: sim.events.slice(0, half) });
      server.notify('build', { id, result: {} });
      server.notify('events', { id, events: sim.events.slice(half) });
      server.notify('events', { id: id + 100, events: [{ event: 'step_started' }] });
      server.respond(id, outcomeFrom('simulate', sim.result));
    });
    const events: Event[] = [];
    let resolvedBefore = false;
    const promise = client.sim(spf, { onEvent: (event) => events.push(event) }).then((run) => {
      resolvedBefore = events.length === sim.events.length;
      return run;
    });
    const run = await promise;
    assert.ok(resolvedBefore, 'os eventos chegam antes de a promessa resolver');
    assert.deepEqual(events, sim.events);
    assert.equal(run.status, 'succeeded');
    const report = (run as { result: SimReport }).result;
    assert.deepEqual(report.builds, sim.result.builds);
    assert.deepEqual(report.simulation, sim.result.simulation);
    assert.equal(report.surfer_pid, null);
    assert.equal(report.report, sim.result.report);
    await client.dispose();
  });

  it('onEvent recebe cada evento e uma exceção nele não derruba o pedido', async () => {
    const check = recordedEvents('check.events.jsonl');
    const { client, warnings } = setup((request, server) => {
      const id = request.id as number;
      server.notify('events', { id, events: check.events });
      server.respond(id, outcomeFrom('check', check.result));
    });
    const seen: Event[] = [];
    const run = await client.check(spf, {
      onEvent: (event) => {
        seen.push(event);
        if (seen.length === 1) {
          throw new Error('boom');
        }
      },
    });
    assert.deepEqual(seen, check.events);
    assert.ok(warnings.some((w) => /boom/.test(w)));
    assert.equal(run.status, 'succeeded');
    assert.deepEqual((run as { result: CheckReport }).result.check, check.result.check);
    await client.dispose();
  });

  it('o status do outcome decide o AuroraRun: failed, timed_out e cancelled', async () => {
    let status = 'failed';
    const { client } = setup((request, server) => server.respond(request.id as number, outcomeFrom('build', {}, status)));
    assert.equal((await client.build(spf)).status, 'failed');
    status = 'timed_out';
    assert.equal((await client.build(spf)).status, 'failed');
    status = 'crashed';
    assert.equal((await client.build(spf)).status, 'failed');
    status = 'cancelled';
    const run = await client.build(spf);
    assert.equal(run.status, 'cancelled');
    assert.ok(run.status === 'cancelled' && run.result !== null, 'cancelado com resultado traz o resultado');
    await client.dispose();
  });

  it('PortValues.error vira texto no SimReport, e schematic_error vira status error', async () => {
    const { client } = setup((request, server) => {
      const params = request.params as { request: { flow: string } };
      if (params.request.flow === 'simulate') {
        return server.respond(
          request.id as number,
          outcomeFrom('simulate', { outputs: [{ port: 0, path: '/o.txt', values: [1, 2], error: 'sem arquivo' }] }),
        );
      }
      const outcome = outcomeFrom('synthesize', {});
      outcome.schematic_error = { code: 'module_not_found', message: 'No module x', hint: 'aurora synth' };
      return server.respond(request.id as number, outcome);
    });
    const sim = await client.sim(spf);
    assert.deepEqual((sim as { result: SimReport }).result.outputs, [
      { port: 0, path: '/o.txt', values: [1, 2], error: 'sem arquivo' },
    ]);
    const synth = await client.synth(spf);
    assert.deepEqual(synth, {
      status: 'error',
      error: { code: 'module_not_found', message: 'No module x', hint: 'aurora synth' },
    });
    await client.dispose();
  });

  it('um erro -32000 vira status error com o código do core e a dica; sem data.code é AuroraProcessError', async () => {
    let withData = true;
    const { client } = setup((request, server) =>
      server.fail(
        request.id as number,
        -32000,
        'Na AURORA project here',
        withData ? { code: 'project_not_found', message: 'Na AURORA project here', hint: 'Create one with: aurora new' } : undefined,
      ),
    );
    const run = await client.status(spf);
    assert.deepEqual(run, {
      status: 'error',
      error: { code: 'project_not_found', message: 'Na AURORA project here', hint: 'Create one with: aurora new' },
    });
    withData = false;
    await assert.rejects(client.status(spf), AuroraProcessError);
    await client.dispose();
  });

  it('-32601 e -32602 viram code cli; -32001 vira cancelled; -32003 e -32700 lançam', async () => {
    let code = -32601;
    const { client } = setup((request, server) => server.fail(request.id as number, code, `erro ${code}`));
    assert.deepEqual(await client.status(spf), {
      status: 'error',
      error: { code: 'cli', message: 'erro -32601', hint: null },
    });
    code = -32602;
    assert.equal((await client.status(spf)).status, 'error');
    code = -32001;
    assert.deepEqual(await client.status(spf), { status: 'cancelled', result: null });
    code = -32003;
    await assert.rejects(client.status(spf), AuroraProcessError);
    code = -32700;
    await assert.rejects(client.status(spf), AuroraProcessError);
    await client.dispose();
  });

  it('o módulo de topo sem dica ganha hint null, como a CLI', async () => {
    const { client } = setup((request, server) =>
      server.respond(request.id as number, {
        top_level: '/x.v',
        module: null,
        module_error: { code: 'module_not_found', message: 'x' },
      }),
    );
    const run = await client.setTop(spf, '/x.v');
    assert.deepEqual((run as { result: TopReport }).result.module_error, {
      code: 'module_not_found',
      message: 'x',
      hint: null,
    });
    await client.dispose();
  });

  it('cancelado antes de começar não manda nada; abortado no meio manda $/cancel uma vez', async () => {
    let pendingId: number | undefined;
    let cancelled = false;
    const { client, servers } = setup((request, server) => {
      if (request.method === '$/cancel') {
        cancelled = true;
        server.respond(pendingId as number, outcomeFrom('build', {}, 'cancelled'));
        return;
      }
      pendingId = request.id as number;
    });
    const aborted = new AbortController();
    aborted.abort();
    assert.deepEqual(await client.build(spf, { signal: aborted.signal }), { status: 'cancelled', result: null });
    assert.equal(servers.length, 0, 'nem abre o processo');

    const controller = new AbortController();
    const promise = client.build(spf, { signal: controller.signal });
    await new Promise((r) => setTimeout(r, 20));
    controller.abort();
    controller.abort();
    const run = await promise;
    assert.ok(cancelled);
    assert.equal(run.status, 'cancelled');
    assert.ok(run.status === 'cancelled' && run.result !== null);
    const cancels = servers[0].received.filter((r) => r.method === '$/cancel');
    assert.deepEqual(cancels, [{ jsonrpc: '2.0', method: '$/cancel', params: { id: pendingId } }] as unknown as Incoming[]);
    await client.dispose();
  });

  it('a morte do processo rejeita o pedido com AuroraProcessError e o próximo pedido reabre', async () => {
    const { client, servers, warnings } = setup((request, server) => {
      if (servers.length === 1) {
        server.writeStderr('thread panicked\n');
        server.close(101);
        return;
      }
      server.respond(request.id as number, projectStatus());
    });
    await assert.rejects(client.status(spf), (error: unknown) => {
      assert.ok(error instanceof AuroraProcessError);
      assert.match(error.message, /código 101/);
      assert.match(error.stderr, /panicked/);
      return true;
    });
    assert.ok(warnings.some((w) => /panicked/.test(w)));
    const run = await client.status(spf);
    assert.equal(run.status, 'succeeded');
    assert.equal(servers.length, 2);
    assert.equal(servers[1].received[0].method, 'initialize');
    await client.dispose();
  });

  it('depois de shutdown { reason: updated } o próximo pedido abre outro processo, e a versão nova avisa', async () => {
    const changed: string[] = [];
    const servers: FakeServe[] = [];
    const client = new ServeAuroraClient(command, undefined, {
      clientName: 'x',
      shutdownWaitMs: 200,
      onServerChanged: (v) => changed.push(v),
      spawn: () => {
        const version = servers.length === 0 ? '0.9.0' : '0.10.0';
        const server = new FakeServe(
          (request, me) => {
            if (servers.length === 1) {
              me.notify('shutdown', { reason: 'updated' });
              me.respond(request.id as number, projectStatus());
              setImmediate(() => me.close(0));
              return;
            }
            me.respond(request.id as number, projectStatus());
          },
          { aurora: version, protocol: 2 },
        );
        servers.push(server);
        return server;
      },
    });
    assert.equal((await client.status(spf)).status, 'succeeded');
    await new Promise((r) => setTimeout(r, 20));
    assert.equal((await client.status(spf)).status, 'succeeded');
    assert.equal(servers.length, 2);
    assert.deepEqual(changed, ['0.10.0']);
    assert.equal(await client.version(), '0.10.0');
    await client.dispose();
  });

  it('dispose manda shutdown e espera o processo; sem resposta, mata', async () => {
    const polite = setup(nothing);
    await polite.client.version();
    await polite.client.dispose();
    assert.deepEqual(
      polite.servers[0].received.map((r) => r.method),
      ['initialize', 'shutdown'],
    );
    assert.ok(!polite.servers[0].killed);

    const stubborn: FakeServe[] = [];
    const client = new ServeAuroraClient(command, undefined, {
      clientName: 'x',
      shutdownWaitMs: 50,
      spawn: () => {
        const server = new FakeServe(nothing);
        // Um servidor que ignora o shutdown: responde ao initialize e mais nada.
        server.stdin.removeAllListeners('data');
        server.stdin.on('data', (chunk: string) => {
          const line = JSON.parse(String(chunk).split('\n')[0]) as Incoming;
          if (line.method === 'initialize') {
            server.respond(line.id as number, server.info);
          }
        });
        stubborn.push(server);
        return server;
      },
    });
    await client.version();
    await client.dispose();
    assert.ok(stubborn[0].killed);
  });

  it('o status concorrente a um fluxo é respondido fora de ordem e cada um recebe o seu', async () => {
    const { client } = setup((request, server) => {
      const id = request.id as number;
      if (request.method === 'flow.run') {
        setTimeout(() => server.respond(id, outcomeFrom('build', {})), 30);
      } else {
        server.respond(id, projectStatus());
      }
    });
    const [build, status] = await Promise.all([client.build(spf), client.status(spf)]);
    assert.equal(build.status, 'succeeded');
    assert.equal(status.status, 'succeeded');
    assert.ok('results' in (build as { result: object }).result);
    assert.ok('testbench' in (status as { result: object }).result);
    await client.dispose();
  });

  it('o SynthReport leva o netlist e o esquemático', async () => {
    const synth = recordedEvents('synth.events.jsonl');
    const { client } = setup((request, server) => server.respond(request.id as number, outcomeFrom('synthesize', synth.result)));
    const run = await client.synth(spf, { svg: true });
    const report = (run as { result: SynthReport }).result;
    assert.deepEqual(report.synthesis, synth.result.synthesis);
    assert.deepEqual(report.schematic, synth.result.schematic);
    assert.equal(report.rtl_viewer, synth.result.rtl_viewer ?? null);
    await client.dispose();
  });
});
