// Criar e configurar processadores dentro do VS Code, com o lace falso:
// `lace proc add` (o fonte-modelo abre no editor) e `lace proc set`.

import * as assert from 'node:assert/strict';
import { existsSync, rmSync } from 'node:fs';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { LaceExtensionApi } from '../../src/extension';
import type { LaceRun } from '../../src/lace/client';
import type { Processor } from '../../src/lace/lace-types';
import type { SourceNode } from '../../src/sources/provider';
import { activate, lastCall, resetFake, run, spf, workspace } from './helpers';

const soma = { kind: 'entry', entry: { role: 'processor', label: 'soma', path: null } } as unknown as SourceNode;

describe('Lace: processadores', () => {
  let api: LaceExtensionApi;

  beforeEach(async () => {
    api = await activate();
    await resetFake(api);
  });

  it('Novo processador: lace proc add, o fonte abre e a vista o mostra', async () => {
    const source = join(workspace, 'novo', 'Software', 'novo.cmm');
    try {
      const result = await run<LaceRun<Processor>>('lace.newProcessor', { name: 'novo', language: 'cmm' });
      assert.equal(result?.status, 'succeeded');
      assert.deepEqual(lastCall('proc'), ['-C', spf, '--json', 'proc', 'add', 'novo', '--lang', 'cmm']);
      assert.ok(existsSync(source));
      assert.equal(vscode.window.activeTextEditor?.document.uri.fsPath, source);
      assert.equal(vscode.window.activeTextEditor?.document.languageId, 'cmm');
      assert.ok(api.status?.processors.some((p) => p.name === 'novo'));
    } finally {
      await vscode.commands.executeCommand('workbench.action.closeAllEditors');
      rmSync(join(workspace, 'novo'), { recursive: true, force: true });
    }
  });

  it('um nome que já existe: o erro do Lace', async () => {
    const result = await run<LaceRun<Processor>>('lace.newProcessor', { name: 'soma', language: 'cmm' });
    assert.equal(result?.status === 'error' && result.error.code, 'processor_exists');
  });

  it('Configurar: lace proc set só com o que muda', async () => {
    const result = await run<LaceRun<Processor>>('lace.configureProcessor', soma, { changes: { frequencyMhz: 50 } });
    assert.equal(result?.status === 'succeeded' && result.result.frequency_mhz, 50);
    assert.deepEqual(lastCall('proc'), ['-C', spf, '--json', 'proc', 'set', 'soma', '--freq', '50']);
    await run('lace.configureProcessor', soma, { changes: { showArrays: true } });
    assert.deepEqual(lastCall('proc'), ['-C', spf, '--json', 'proc', 'set', 'soma', '--arrays', 'true']);
  });
});
