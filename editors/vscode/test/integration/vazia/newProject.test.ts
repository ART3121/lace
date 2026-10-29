// Numa pasta sem .spf: a extensão abre sem projeto, e o Novo projeto roda
// `aurora new` sem -C e passa a mostrar o projeto criado.

import * as assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { LaceExtensionApi } from '../../../src/extension';
import type { LaceRun } from '../../../src/lace/client';
import type { NewReport } from '../../../src/lace/lace-types';
import { until } from '../../support/fixtures';
import { lastCall, run, workspace } from '../helpers';

describe('Numa pasta sem .spf', () => {
  let api: LaceExtensionApi;

  before(async () => {
    assert.ok(workspace, 'LACE_TEST_WORKSPACE não definido: rode pelo npm run test:integration');
    const extension = vscode.extensions.getExtension<LaceExtensionApi>('nipscernlab.lace');
    assert.ok(extension, 'a extensão não foi carregada');
    api = await extension.activate();
  });

  it('a extensão ativa sem projeto', () => {
    assert.equal(api.status, undefined);
  });

  it('Novo projeto com um nome que o Lace recusa: o erro dele, e nada criado', async () => {
    const result = await run<LaceRun<NewReport>>('lace.newProject', { name: '9x', dir: workspace });
    assert.equal(result?.status === 'error' && result.error.code, 'invalid_name');
    assert.equal(api.status, undefined);
  });

  it('Novo projeto: aurora new sem -C, e o projeto criado vira o ativo', async () => {
    const result = await run<LaceRun<NewReport>>('lace.newProject', { name: 'novo', dir: workspace });
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('new'), ['--json', 'new', 'novo', '--dir', workspace]);
    assert.ok(existsSync(join(workspace, 'novo', 'novo.spf')));
    await until(() => api.status?.name === 'novo', 'o projeto novo na vista');
  });
});
