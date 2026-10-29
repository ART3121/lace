// O CliAuroraClient contra o aurora falso: os códigos de saída, o erro do código
// 2, o --events e o cancelamento. O aurora falso devolve o que o aurora de
// verdade respondeu (test/fixtures/responses/).

import * as assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';

import { AuroraProcessError } from '../../src/aurora/client';
import { CliAuroraClient } from '../../src/aurora/cliClient';
import type {
  BuildReport,
  CheckReport,
  Event,
  MoveReport,
  OrderReport,
  RemoveReport,
  SimReport,
  StatusReport,
  SynthReport,
  WaveReport,
} from '../../src/aurora/aurora-types';
import { FAKE_AURORA, fakeAuroraDir, auroraCalls, auroraWaiting, recordedCases, until } from '../support/fixtures';

// As pastas não precisam existir: o aurora falso só usa o caminho. Os nomes
// são os das gravações, que o aurora falso confere.
const base = join(tmpdir(), 'aurora teste com espaço');
const demo = join(base, 'projeto demo');
const spf = join(demo, 'demo.spf');
const broken = join(base, 'erro', 'erro.spf');
const endless = join(base, 'infinito', 'infinito.spf');

describe('CliAuroraClient', () => {
  let log: string;
  let client: CliAuroraClient;

  beforeEach(() => {
    const dir = fakeAuroraDir();
    log = dir.log;
    process.env.FAKE_AURORA_LOG = dir.log;
    process.env.FAKE_AURORA_STATE = dir.state;
    // O aurora falso é um script: roda pelo Node deste processo, em qualquer
    // sistema (no Windows, um .mjs não executa sozinho).
    client = new CliAuroraClient({ file: process.execPath, args: [FAKE_AURORA] });
  });

  afterEach(() => {
    delete process.env.FAKE_AURORA_LOG;
    delete process.env.FAKE_AURORA_STATE;
  });

  it('lê a versão da linha do aurora --version', async () => {
    const recorded = recordedCases().recordedWith.replace(/^aurora /, '');
    assert.equal(await client.version(), recorded);
  });

  it('passa -C com o .spf e --json antes do comando, cada caminho num argumento só', async () => {
    const file = join(demo, 'rtl', 'somador.v');
    await client.setTop(spf, file);
    assert.deepEqual(auroraCalls(log), [['-C', spf, '--json', 'top', file]]);
  });

  it('código 0: devolve o objeto do --json', async () => {
    const run = await client.status(spf);
    assert.equal(run.status, 'succeeded');
    const report = (run as { result: StatusReport }).result;
    assert.equal(report.root, demo);
    assert.equal(report.top_module, 'contador');
    // O aurora falso troca só o ${ROOT}; o resto do caminho é o gravado.
    assert.equal(report.top_level, `${demo}/rtl/contador.v`);
  });

  it('código 1: devolve o resultado da operação que falhou', async () => {
    const run = await client.runJson<CheckReport>(['check'], { project: broken });
    assert.equal(run.status, 'failed');
    assert.equal((run as { result: CheckReport }).result.check.status, 'failed');
  });

  it('código 2: devolve o error.code, a mensagem e a dica', async () => {
    const run = await client.runJson(['status'], { project: join(base, 'sem projeto') });
    assert.equal(run.status, 'error');
    if (run.status === 'error') {
      assert.equal(run.error.code, 'project_not_found');
      assert.match(run.error.message, /Na AURORA project here/);
      assert.match(run.error.hint ?? '', /^Create one with: aurora new/);
    }
  });

  it('código 2 sem dica: hint fica null', async () => {
    const run = await client.setTop(spf, join(demo, 'tb', 'contador_tb.v'));
    assert.deepEqual(run.status === 'error' && run.error, {
      code: 'invalid_name',
      message: "Invalid name 'contador_tb.v': A testbench (tb_<name>.v, <name>_tb.v) cannot be the top level",
      hint: null,
    });
  });

  it('código 2 de erro de argumento, que vem em texto: código cli e a linha do clap', async () => {
    const run = await client.runJson(['top', '--nao-existe'], { project: spf });
    assert.deepEqual(run.status === 'error' && run.error, {
      code: 'cli',
      message: "Unexpected argument '--nao-existe' found",
      hint: null,
    });
  });

  it('um código fora do contrato vira AuroraProcessError, com o stderr', async () => {
    await assert.rejects(client.runJson(['panico']), (error: unknown) => {
      assert.ok(error instanceof AuroraProcessError);
      assert.equal(error.exitCode, 101);
      assert.match(error.stderr, /panicked/);
      return true;
    });
  });

  it('stdout que não é JSON, ou código 0 sem JSON, vira AuroraProcessError', async () => {
    await assert.rejects(client.runJson(['texto']), AuroraProcessError);
    await assert.rejects(client.runJson(['mudo']), AuroraProcessError);
  });

  it('um executável que não existe vira AuroraProcessError', async () => {
    const missing = new CliAuroraClient({ file: join(base, 'nao-existe', 'aurora'), args: [] });
    await assert.rejects(missing.version(), AuroraProcessError);
  });

  it('cancelado antes de começar não roda o aurora', async () => {
    const controller = new AbortController();
    controller.abort();
    assert.deepEqual(await client.status(spf, { signal: controller.signal }), {
      status: 'cancelled',
      result: null,
    });
    assert.deepEqual(auroraCalls(log), []);
  });

  describe('--events', () => {
    it('entrega cada evento na ordem e devolve a linha result', async () => {
      const events: Event[] = [];
      const run = await client.runEvents<CheckReport>(['check'], { project: spf, onEvent: (e) => events.push(e) });
      assert.equal(run.status, 'succeeded');
      assert.deepEqual(auroraCalls(log), [['-C', spf, '--events', 'check']]);
      assert.equal(events[0].event, 'step_started');
      assert.equal(events.at(-1)?.event, 'step_finished');
      const kinds = new Set(events.map((e) => e.event));
      assert.deepEqual([...kinds].sort(), ['output', 'step_finished', 'step_started']);
      assert.equal((run as { result: CheckReport }).result.check.status, 'succeeded');
    });

    it('um comando que não roda ferramenta escreve só a linha final', async () => {
      const events: Event[] = [];
      const run = await client.runEvents<StatusReport>(['status'], { project: spf, onEvent: (e) => events.push(e) });
      assert.deepEqual(events, []);
      assert.equal(run.status === 'succeeded' && run.result.name, 'demo');
    });

    it('uma linha que não é JSON vira AuroraProcessError', async () => {
      await assert.rejects(client.runEvents(['texto']), AuroraProcessError);
    });
  });

  describe('check, wave e add', () => {
    it('check: o projeto, um arquivo, com lint e um processador, pelo --events', async () => {
      const file = join(demo, 'rtl', 'somador.v');
      const runs = [
        await client.check(spf),
        await client.check(spf, { file }),
        await client.check(spf, { lint: true }),
        await client.check(spf, { processor: 'soma' }),
      ];
      assert.deepEqual(auroraCalls(log), [
        ['-C', spf, '--events', 'check'],
        ['-C', spf, '--events', 'check', file],
        ['-C', spf, '--events', 'check', '--lint'],
        ['-C', spf, '--events', 'check', '-p', 'soma'],
      ]);
      assert.deepEqual(
        runs.map((r) => r.status),
        ['succeeded', 'succeeded', 'succeeded', 'succeeded'],
      );
      const lint = runs[2] as { result: CheckReport };
      assert.ok(lint.result.check.diagnostics.some((d) => d.tool === 'verilator'));
    });

    it('check que falha: código 1, com o erro de sintaxe', async () => {
      const run = await client.check(broken);
      assert.equal(run.status, 'failed');
      const errors = (run as { result: CheckReport }).result.check.diagnostics.filter((d) => d.severity === 'error');
      assert.equal(errors.length, 1);
      assert.equal(errors[0].line, 5);
    });

    it('wave: a do projeto, a de um processador e um arquivo', async () => {
      const fst = join(demo, 'contador_tb.fst');
      const runs = [
        await client.wave(spf, { kind: 'project' }),
        await client.wave(spf, { kind: 'processor', name: 'soma' }),
        await client.wave(spf, { kind: 'file', path: fst }),
      ];
      assert.deepEqual(auroraCalls(log), [
        ['-C', spf, '--json', 'wave'],
        ['-C', spf, '--json', 'wave', '-p', 'soma'],
        ['-C', spf, '--json', 'wave', fst],
      ]);
      const waveforms = runs.map((r) => (r as { result: WaveReport }).result.waveform);
      assert.ok(waveforms[0].endsWith('contador_tb.fst'));
      assert.ok(waveforms[1].endsWith('soma_tb.fst'));
    });

    it('wave de um arquivo sem projeto não passa -C', async () => {
      await client.wave(undefined, { kind: 'file', path: join(base, 'onda.vcd') });
      assert.deepEqual(auroraCalls(log), [['--json', 'wave', join(base, 'onda.vcd')]]);
    });

    it('wave sem testbench: o erro com a dica', async () => {
      const run = await client.wave(broken, { kind: 'project' });
      assert.equal(run.status === 'error' && run.error.code, 'no_testbench');
      assert.match(run.status === 'error' ? (run.error.hint ?? '') : '', /aurora add/);
    });

    it('add: os arquivos e, com testbench, o --tb', async () => {
      const sobra = join(demo, 'rtl', 'sobra.v');
      const velho = join(demo, 'rtl', 'velho_tb.v');
      const added = await client.add(spf, [sobra]);
      const bench = await client.add(spf, [velho], { testbench: true });
      assert.deepEqual(auroraCalls(log), [
        ['-C', spf, '--json', 'add', sobra],
        ['-C', spf, '--json', 'add', velho, '--tb'],
      ]);
      assert.equal(added.status === 'succeeded' && added.result.files[0].role, 'synthesizable');
      assert.equal(bench.status === 'succeeded' && bench.result.files[0].role, 'testbench');
    });
  });

  describe('sim e synth', () => {
    it('sim: o projeto, a rápida, um processador e um testbench, pelo --events', async () => {
      const tb = join(demo, 'tb', 'somador_tb.v');
      const project = await client.sim(spf);
      await client.sim(spf, { fast: true });
      const soma = await client.sim(spf, { processor: 'soma' });
      await client.sim(spf, { testbench: tb });
      assert.deepEqual(auroraCalls(log), [
        ['-C', spf, '--events', 'sim'],
        ['-C', spf, '--events', 'sim', '--fast'],
        ['-C', spf, '--events', 'sim', '-p', 'soma'],
        ['-C', spf, '--events', 'sim', tb],
      ]);
      const sim = (project as { result: SimReport }).result.simulation;
      assert.equal(sim?.status, 'succeeded');
      assert.ok(sim?.waveform?.path.endsWith('contador_tb.fst'));
      assert.deepEqual((soma as { result: SimReport }).result.outputs.map((o) => o.port), [0]);
    });

    it('sim que passa do --timeout: código 1, timed_out', async () => {
      const run = await client.sim(endless, { timeoutSeconds: 1 });
      assert.deepEqual(auroraCalls(log), [['-C', endless, '--events', 'sim', '--timeout', '1']]);
      assert.equal(run.status, 'failed');
      assert.equal((run as { result: SimReport }).result.simulation?.status, 'timed_out');
    });

    it('synth: com e sem esquemático, e um processador', async () => {
      const svg = await client.synth(spf, { svg: true });
      await client.synth(spf);
      const soma = await client.synth(spf, { processor: 'soma' });
      assert.deepEqual(auroraCalls(log), [
        ['-C', spf, '--events', 'synth', '--svg'],
        ['-C', spf, '--events', 'synth'],
        ['-C', spf, '--events', 'synth', '-p', 'soma'],
      ]);
      assert.ok((svg as { result: SynthReport }).result.schematic?.svg?.endsWith('contador.svg'));
      assert.ok(((soma as { result: SynthReport }).result.synthesis?.statistics?.cells ?? 0) > 0);
    });
  });

  describe('SAPHO', () => {
    it('build: todos, ou cada -p, pelo --events', async () => {
      const all = await client.build(spf);
      await client.build(spf, { processors: ['soma'] });
      assert.deepEqual(auroraCalls(log), [
        ['-C', spf, '--events', 'build'],
        ['-C', spf, '--events', 'build', '-p', 'soma'],
      ]);
      const results = (all as { result: BuildReport }).result.results;
      assert.deepEqual(results.map((b) => [b.processor, b.status]), [
        ['soma', 'succeeded'],
        ['filtro', 'succeeded'],
      ]);
    });

    it('build que falha: código 1, com o erro do YANC no fonte', async () => {
      writeFileSync(process.env.FAKE_AURORA_STATE ?? '', 'soma-quebrado');
      const run = await client.build(spf, { processors: ['soma'] });
      assert.equal(run.status, 'failed');
      const [soma] = (run as { result: BuildReport }).result.results;
      assert.equal(soma.failed_step, 'compile');
      // O aurora falso troca só o ${ROOT}; o resto do caminho é o gravado.
      assert.equal(soma.diagnostics[0].file, `${demo}/soma/soma.cmm`);
      assert.equal(soma.diagnostics[0].line, 14);
    });

    it('proc add e proc set: o nome, a linguagem e só o que muda', async () => {
      const existing = await client.addProcessor(spf, 'soma', 'cmm');
      await client.configureProcessor(spf, 'soma', { frequencyMhz: 50 });
      await client.configureProcessor(spf, 'soma', { clocks: 5000 });
      await client.configureProcessor(spf, 'soma', { showArrays: true });
      assert.deepEqual(auroraCalls(log), [
        ['-C', spf, '--json', 'proc', 'add', 'soma', '--lang', 'cmm'],
        ['-C', spf, '--json', 'proc', 'set', 'soma', '--freq', '50'],
        ['-C', spf, '--json', 'proc', 'set', 'soma', '--clocks', '5000'],
        ['-C', spf, '--json', 'proc', 'set', 'soma', '--arrays', 'true'],
      ]);
      assert.equal(existing.status === 'error' && existing.error.code, 'processor_exists');
    });
  });

  describe('remove, order e move', () => {
    const rtl = (name: string) => join(demo, 'rtl', name);

    it('remove: os arquivos; um que não está no projeto é erro', async () => {
      const two = await client.remove(spf, [rtl('somador.v'), rtl('mux.sv')]);
      const outside = await client.remove(spf, [rtl('sobra.v')]);
      assert.deepEqual(auroraCalls(log), [
        ['-C', spf, '--json', 'remove', rtl('somador.v'), rtl('mux.sv')],
        ['-C', spf, '--json', 'remove', rtl('sobra.v')],
      ]);
      assert.equal((two as { result: RemoveReport }).result.removed.length, 2);
      assert.equal(outside.status, 'error');
    });

    it('order: antes, depois, primeiro e último', async () => {
      const up = await client.order(spf, rtl('somador.v'), { kind: 'before', other: rtl('contador.v') });
      writeFileSync(process.env.FAKE_AURORA_STATE ?? '', 'initial');
      await client.order(spf, rtl('somador.v'), { kind: 'after', other: rtl('mux.sv') });
      await client.order(spf, rtl('mux.sv'), { kind: 'first' });
      await client.order(spf, rtl('contador.v'), { kind: 'last' });
      assert.deepEqual(auroraCalls(log), [
        ['-C', spf, '--json', 'order', rtl('somador.v'), '--before', rtl('contador.v')],
        ['-C', spf, '--json', 'order', rtl('somador.v'), '--after', rtl('mux.sv')],
        ['-C', spf, '--json', 'order', rtl('mux.sv'), '--first'],
        ['-C', spf, '--json', 'order', rtl('contador.v'), '--last'],
      ]);
      const order = (up as { result: OrderReport }).result.files.map((f) => basename(f.path));
      assert.deepEqual(order, ['somador.v', 'contador.v', 'mux.sv', 'soma.v']);
    });

    it('move --after: só o .spf; com o arquivo ainda no lugar, a AURORA recusa', async () => {
      const moved = await client.move(spf, rtl('somador.v'), rtl('adder.v'), { after: true });
      const refused = await client.move(spf, rtl('mux.sv'), rtl('adder.v'), { after: true });
      assert.deepEqual(auroraCalls(log), [
        ['-C', spf, '--json', 'move', '--after', rtl('somador.v'), rtl('adder.v')],
        ['-C', spf, '--json', 'move', '--after', rtl('mux.sv'), rtl('adder.v')],
      ]);
      assert.equal((moved as { result: MoveReport }).result.moved[0].files[0].path, `${demo}/rtl/adder.v`);
      assert.equal(refused.status, 'error');
    });
  });

  describe('cancelamento', () => {
    it('--events: o aurora recebe SIGTERM e sai com 130 e o resultado cancelado', async () => {
      const controller = new AbortController();
      const events: Event[] = [];
      const run = await client.runEvents<SimReport>(['sim'], {
        project: endless,
        signal: controller.signal,
        onEvent: (event) => {
          events.push(event);
          if (event.event === 'output' && !event.diagnostic) {
            controller.abort();
          }
        },
      });
      assert.equal(run.status, 'cancelled');
      assert.ok(events.some((e) => e.event === 'output' && e.line === 'comecou'));
      if (process.platform === 'win32') {
        // O kill do Windows encerra o aurora antes de ele escrever o resultado.
        assert.equal(run.status === 'cancelled' && run.result, null);
      } else {
        // O resultado só sai se o aurora tratou o sinal: um SIGKILL não deixaria.
        assert.equal(run.status === 'cancelled' && run.result?.simulation?.status, 'cancelled');
      }
    });

    it('--json: o mesmo, com o objeto no fim', async () => {
      const controller = new AbortController();
      const running = client.runJson<SimReport>(['sim'], { project: endless, signal: controller.signal });
      await until(() => auroraWaiting(log), 'o aurora falso esperar o sinal');
      controller.abort();
      const run = await running;
      assert.equal(run.status, 'cancelled');
      if (process.platform !== 'win32') {
        assert.equal(run.status === 'cancelled' && run.result?.simulation?.status, 'cancelled');
      }
    });
  });
});
