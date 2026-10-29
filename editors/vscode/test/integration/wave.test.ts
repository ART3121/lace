// "Lace: Abrir a onda" dentro do VS Code, com o aurora falso (que não abre
// Surfer nenhum: devolve o JSON que o aurora de verdade deu).

import * as assert from 'node:assert/strict';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { AuroraRun } from '../../src/aurora/client';
import type { WaveReport } from '../../src/aurora/aurora-types';
import type { SourceNode } from '../../src/sources/provider';
import { activate, lastCall, resetFake, run, spf, workspace } from './helpers';

describe('Lace: Abrir a onda', () => {
  before(async () => {
    await resetFake(await activate());
  });

  it('a do projeto', async () => {
    const result = await run<AuroraRun<WaveReport>>('aurora.wave');
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('wave'), ['-C', spf, '--json', 'wave']);
  });

  it('a de um processador, pelo item da vista', async () => {
    const node = { kind: 'entry', entry: { role: 'processor', label: 'soma', path: null } } as unknown as SourceNode;
    const result = await run<AuroraRun<WaveReport>>('aurora.waveProcessor', node);
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('wave'), ['-C', spf, '--json', 'wave', '-p', 'soma']);
  });

  it('um arquivo de onda, pelo explorador', async () => {
    const fst = join(workspace, 'contador_tb.fst');
    const result = await run<AuroraRun<WaveReport>>('aurora.waveFile', vscode.Uri.file(fst));
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('wave'), ['-C', spf, '--json', 'wave', fst]);
  });
});
