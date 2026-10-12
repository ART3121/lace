// "Lace: Definir como topo" dentro do VS Code, com o lace falso: o comando
// roda `lace top` no projeto ativo e relê o projeto.

import * as assert from 'node:assert/strict';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { LaceExtensionApi } from '../../src/extension';
import type { LaceRun } from '../../src/lace/client';
import type { TopReport } from '../../src/lace/lace-types';
import type { SourceNode } from '../../src/sources/provider';
import { activate, calls, lastCall, resetFake, run, spf, workspace } from './helpers';

describe('Lace: Definir como topo', () => {
  let api: LaceExtensionApi;

  before(async () => {
    api = await activate();
    await resetFake(api);
    assert.equal(api.status?.top_module, 'contador');
  });

  it('pelo explorador: roda lace top com -C e o caminho, e relê o projeto', async () => {
    const file = join(workspace, 'rtl', 'somador.v');
    const result = await run<LaceRun<TopReport>>('lace.setTop', vscode.Uri.file(file));
    assert.equal(result?.status, 'succeeded');
    // O caminho tem espaço ("projeto demo") e vai inteiro num argumento.
    assert.deepEqual(lastCall('top'), ['-C', spf, '--json', 'top', file]);
    // O comando só volta depois de reler o projeto.
    assert.equal(api.status?.top_module, 'somador');
  });

  it('pela vista de fontes: recebe o item da árvore', async () => {
    const file = join(workspace, 'rtl', 'contador.v');
    const node = { kind: 'entry', entry: { role: 'design', path: file } } as SourceNode;
    const result = await run<LaceRun<TopReport>>('lace.setTop', node);
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('top'), ['-C', spf, '--json', 'top', file]);
    assert.equal(api.status?.top_module, 'contador');
  });

  it('com um testbench: devolve o erro do Lace e o topo não muda', async () => {
    const result = await run<LaceRun<TopReport>>('lace.setTop', vscode.Uri.file(join(workspace, 'tb', 'contador_tb.v')));
    assert.equal(result?.status === 'error' && result.error.code, 'invalid_name');
    assert.equal(api.status?.top_module, 'contador');
  });

  it('sem arquivo: não roda o lace', async () => {
    const before = calls().length;
    await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    assert.equal(await run('lace.setTop', undefined), undefined);
    assert.equal(calls().length, before);
  });
});
