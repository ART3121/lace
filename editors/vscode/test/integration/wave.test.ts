// "Lace: Abrir a onda" dentro do VS Code, com o lace falso (que não abre
// Surfer nenhum: devolve o JSON que o lace de verdade deu).

import * as assert from 'node:assert/strict';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { LaceRun } from '../../src/lace/client';
import type { WaveReport } from '../../src/lace/lace-types';
import type { SourceNode } from '../../src/sources/provider';
import { activate, lastCall, resetFake, run, spf, workspace } from './helpers';

describe('Lace: Abrir a onda', () => {
  before(async () => {
    await resetFake(await activate());
  });

  it('a do projeto', async () => {
    const result = await run<LaceRun<WaveReport>>('lace.wave');
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('wave'), ['-C', spf, '--json', 'wave']);
  });

  it('a de um processador, pelo item da vista', async () => {
    const node = { kind: 'entry', entry: { role: 'processor', label: 'soma', path: null } } as unknown as SourceNode;
    const result = await run<LaceRun<WaveReport>>('lace.waveProcessor', node);
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('wave'), ['-C', spf, '--json', 'wave', '-p', 'soma']);
  });

  it('um arquivo de onda, pelo explorador', async () => {
    const fst = join(workspace, 'contador_tb.fst');
    const result = await run<LaceRun<WaveReport>>('lace.waveFile', vscode.Uri.file(fst));
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('wave'), ['-C', spf, '--json', 'wave', fst]);
  });
});
