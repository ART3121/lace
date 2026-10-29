// Numa pasta sem .spf: a extensão abre sem projeto, e o Novo projeto roda
// `aurora new` sem -C e passa a mostrar o projeto criado.

import * as assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { AuroraExtensionApi } from '../../../src/extension';
import type { AuroraRun } from '../../../src/aurora/client';
import type { NewReport } from '../../../src/aurora/aurora-types';
import { until } from '../../support/fixtures';
import { lastCall, run, workspace } from '../helpers';

describe('Numa pasta sem .spf', () => {
  let api: AuroraExtensionApi;

  before(async () => {
    assert.ok(workspace, 'AURORA_TEST_WORKSPACE não definido: rode pelo npm run test:integration');
    const extension = vscode.extensions.getExtension<AuroraExtensionApi>('nipscernlab.aurora');
    assert.ok(extension, 'a extensão não foi carregada');
    api = await extension.activate();
  });

  it('a extensão ativa sem projeto', () => {
    assert.equal(api.status, undefined);
  });

  it('Novo projeto com um nome que o Lace recusa: o erro dele, e nada criado', async () => {
    const result = await run<AuroraRun<NewReport>>('aurora.newProject', { name: '9x', dir: workspace });
    assert.equal(result?.status === 'error' && result.error.code, 'invalid_name');
    assert.equal(api.status, undefined);
  });

  it('Novo projeto: aurora new sem -C, e o projeto criado vira o ativo', async () => {
    const result = await run<AuroraRun<NewReport>>('aurora.newProject', { name: 'novo', dir: workspace });
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('new'), ['--json', 'new', 'novo', '--dir', workspace]);
    assert.ok(existsSync(join(workspace, 'novo', 'novo.spf')));
    await until(() => api.status?.name === 'novo', 'o projeto novo na vista');
  });
});
