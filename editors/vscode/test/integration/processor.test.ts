// Criar e configurar processadores dentro do VS Code, com o aurora falso:
// `aurora proc add` (o fonte-modelo abre no editor) e `aurora proc set`.

import * as assert from 'node:assert/strict';
import { existsSync, rmSync } from 'node:fs';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { AuroraExtensionApi } from '../../src/extension';
import type { AuroraRun } from '../../src/aurora/client';
import type { Processor } from '../../src/aurora/aurora-types';
import type { SourceNode } from '../../src/sources/provider';
import { activate, lastCall, resetFake, run, spf, workspace } from './helpers';

const soma = { kind: 'entry', entry: { role: 'processor', label: 'soma', path: null } } as unknown as SourceNode;

describe('AURORA: processadores', () => {
  let api: AuroraExtensionApi;

  beforeEach(async () => {
    api = await activate();
    await resetFake(api);
  });

  it('Novo processador: aurora proc add, o fonte abre e a vista o mostra', async () => {
    const source = join(workspace, 'novo', 'novo.cmm');
    try {
      const result = await run<AuroraRun<Processor>>('aurora.newProcessor', { name: 'novo', language: 'cmm' });
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

  it('um nome que já existe: o erro da AURORA', async () => {
    const result = await run<AuroraRun<Processor>>('aurora.newProcessor', { name: 'soma', language: 'cmm' });
    assert.equal(result?.status === 'error' && result.error.code, 'processor_exists');
  });

  it('Configurar: aurora proc set só com o que muda', async () => {
    const result = await run<AuroraRun<Processor>>('aurora.configureProcessor', soma, { changes: { frequencyMhz: 50 } });
    assert.equal(result?.status === 'succeeded' && result.result.frequency_mhz, 50);
    assert.deepEqual(lastCall('proc'), ['-C', spf, '--json', 'proc', 'set', 'soma', '--freq', '50']);
    await run('aurora.configureProcessor', soma, { changes: { showArrays: true } });
    assert.deepEqual(lastCall('proc'), ['-C', spf, '--json', 'proc', 'set', 'soma', '--arrays', 'true']);
  });
});
