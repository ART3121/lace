// "AURORA: Definir como topo" dentro do VS Code, com o aurora falso: o comando
// roda `aurora top` no projeto ativo e relê o projeto.

import * as assert from 'node:assert/strict';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { AuroraExtensionApi } from '../../src/extension';
import type { AuroraRun } from '../../src/aurora/client';
import type { TopReport } from '../../src/aurora/aurora-types';
import type { SourceNode } from '../../src/sources/provider';
import { activate, calls, lastCall, resetFake, run, spf, workspace } from './helpers';

describe('AURORA: Definir como topo', () => {
  let api: AuroraExtensionApi;

  before(async () => {
    api = await activate();
    await resetFake(api);
    assert.equal(api.status?.top_module, 'contador');
  });

  it('pelo explorador: roda aurora top com -C e o caminho, e relê o projeto', async () => {
    const file = join(workspace, 'rtl', 'somador.v');
    const result = await run<AuroraRun<TopReport>>('aurora.setTop', vscode.Uri.file(file));
    assert.equal(result?.status, 'succeeded');
    // O caminho tem espaço ("projeto demo") e vai inteiro num argumento.
    assert.deepEqual(lastCall('top'), ['-C', spf, '--json', 'top', file]);
    // O comando só volta depois de reler o projeto.
    assert.equal(api.status?.top_module, 'somador');
  });

  it('pela vista de fontes: recebe o item da árvore', async () => {
    const file = join(workspace, 'rtl', 'contador.v');
    const node = { kind: 'entry', entry: { role: 'design', path: file } } as SourceNode;
    const result = await run<AuroraRun<TopReport>>('aurora.setTop', node);
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('top'), ['-C', spf, '--json', 'top', file]);
    assert.equal(api.status?.top_module, 'contador');
  });

  it('com um testbench: devolve o erro da AURORA e o topo não muda', async () => {
    const result = await run<AuroraRun<TopReport>>('aurora.setTop', vscode.Uri.file(join(workspace, 'tb', 'contador_tb.v')));
    assert.equal(result?.status === 'error' && result.error.code, 'invalid_name');
    assert.equal(api.status?.top_module, 'contador');
  });

  it('sem arquivo: não roda o aurora', async () => {
    const before = calls().length;
    await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    assert.equal(await run('aurora.setTop', undefined), undefined);
    assert.equal(calls().length, before);
  });
});
