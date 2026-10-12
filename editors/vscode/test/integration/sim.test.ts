// "Lace: Simular" dentro do VS Code, com o lace falso: o botão Sim da barra
// de status roda `lace sim` pelo --events e guarda como terminou.

import * as assert from 'node:assert/strict';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { LaceExtensionApi } from '../../src/extension';
import type { LaceRun } from '../../src/lace/client';
import type { SimReport } from '../../src/lace/lace-types';
import type { SourceNode } from '../../src/sources/provider';
import { activate, lastCall, resetFake, run, spf, workspace } from './helpers';

describe('Lace: Simular', () => {
  let api: LaceExtensionApi;

  before(async () => {
    api = await activate();
    await resetFake(api);
  });

  it('o projeto: lace sim pelo --events, e o botão guarda o resultado', async () => {
    const result = await run<LaceRun<SimReport>>('lace.sim');
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('sim'), ['-C', spf, '--events', 'sim']);
    const state = api.runs.state('sim');
    assert.equal(state.running, false);
    assert.equal(state.last?.outcome, 'succeeded');
    assert.match(state.last?.summary ?? '', /contador_tb simulado/);
  });

  it('outro testbench: passa a ser o do projeto, e a vista acompanha', async () => {
    const somador = join(workspace, 'tb', 'somador_tb.v');
    await run('lace.simTestbench', vscode.Uri.file(somador));
    assert.deepEqual(lastCall('sim'), ['-C', spf, '--events', 'sim', somador]);
    assert.equal(api.status?.selected_testbench, somador);
    const contador = join(workspace, 'tb', 'contador_tb.v');
    await run('lace.simTestbench', vscode.Uri.file(contador));
    assert.equal(api.status?.selected_testbench, contador);
  });

  it('um processador e a simulação rápida', async () => {
    const processor = { kind: 'entry', entry: { role: 'processor', label: 'soma', path: null } } as unknown as SourceNode;
    const soma = await run<LaceRun<SimReport>>('lace.simProcessor', processor);
    assert.deepEqual(lastCall('sim'), ['-C', spf, '--events', 'sim', '-p', 'soma']);
    assert.equal(soma?.status === 'succeeded' && soma.result.outputs.length, 1);
    await run('lace.simFast');
    assert.deepEqual(lastCall('sim'), ['-C', spf, '--events', 'sim', '--fast']);
  });
});
