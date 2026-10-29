// Pôr arquivos no projeto dentro do VS Code, com o lace falso: o + dos não
// registrados, o testbench, o arquivo novo do modelo e a detecção de um .v
// que aparece fora do projeto.

import * as assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { LaceExtensionApi } from '../../src/extension';
import type { LaceRun } from '../../src/lace/client';
import type { AddReport } from '../../src/lace/lace-types';
import { activate, lastCall, resetFake, run, setFakeState, spf, workspace } from './helpers';

describe('Lace: Adicionar ao projeto', () => {
  let api: LaceExtensionApi;

  beforeEach(async () => {
    api = await activate();
    await resetFake(api);
  });

  it('um .v de fora: aurora add, e o projeto relido já o tem no design', async () => {
    const sobra = join(workspace, 'rtl', 'sobra.v');
    assert.ok(api.status?.unregistered.includes(sobra));
    const result = await run<LaceRun<AddReport>>('lace.add', vscode.Uri.file(sobra), [vscode.Uri.file(sobra)]);
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('add'), ['-C', spf, '--json', 'add', sobra]);
    assert.ok(api.status?.synthesizable.some((f) => f.path === sobra));
    assert.ok(!api.status?.unregistered.includes(sobra));
  });

  it('como testbench: passa --tb', async () => {
    const velho = join(workspace, 'rtl', 'velho_tb.v');
    const result = await run<LaceRun<AddReport>>('lace.addTestbench', vscode.Uri.file(velho));
    assert.equal(result?.status === 'succeeded' && result.result.files[0].role, 'testbench');
    assert.deepEqual(lastCall('add'), ['-C', spf, '--json', 'add', velho, '--tb']);
  });

  it('Novo arquivo Verilog: o Lace cria do modelo, e o arquivo abre no editor', async () => {
    const novo = join(workspace, 'rtl', 'novo.v');
    const result = await run<LaceRun<AddReport>>('lace.newFile', { path: 'rtl/novo.v' });
    assert.equal(result?.status === 'succeeded' && result.result.files[0].created, true);
    assert.deepEqual(lastCall('add'), ['-C', spf, '--json', 'add', novo]);
    assert.ok(existsSync(novo));
    assert.equal(vscode.window.activeTextEditor?.document.uri.fsPath, novo);
  });

  it('percebe um .v que passa a aparecer fora do projeto', async () => {
    const sobra = join(workspace, 'rtl', 'sobra.v');
    setFakeState('added');
    await api.refresh();
    const found = new Promise<string[]>((resolve) => api.onDidFindNewFiles(resolve));
    // Como se alguém tirasse sobra.v do projeto pelo terminal.
    setFakeState('initial');
    await api.refresh();
    assert.deepEqual(await found, [sobra]);
  });
});
