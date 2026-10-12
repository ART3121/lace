// As ferramentas de arquivo dentro do VS Code, com o lace falso: a ordem de
// compilação (`lace order`), tirar do projeto (`lace remove`), renomear
// (`lace move`) e o que o explorador faz com um arquivo do projeto
// (`lace move --after`, `lace remove`).

import * as assert from 'node:assert/strict';
import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';

import * as vscode from 'vscode';

import type { LaceExtensionApi } from '../../src/extension';
import type { SourceNode } from '../../src/sources/provider';
import { until } from '../support/fixtures';
import { activate, calls, lastCall, resetFake, run, spf, workspace } from './helpers';

const rtl = (name: string) => join(workspace, 'rtl', name);
const node = (role: string, path: string) =>
  ({ kind: 'entry', entry: { role, label: basename(path), path } }) as unknown as SourceNode;
const order = (api: LaceExtensionApi) => api.status?.synthesizable.map((f) => basename(f.path));
const count = (command: string) => calls().filter((argv) => argv.includes(command)).length;

describe('Lace: arquivos do projeto', () => {
  let api: LaceExtensionApi;

  beforeEach(async () => {
    api = await activate();
    await resetFake(api);
  });

  afterEach(async () => {
    await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    // O que o teste moveu no disco volta ao lugar.
    if (existsSync(rtl('adder.v')) && !existsSync(rtl('somador.v'))) {
      renameSync(rtl('adder.v'), rtl('somador.v'));
    }
  });

  it('Subir: lace order --before o de cima, e a vista mostra a ordem nova', async () => {
    await run('lace.moveUp', node('design', rtl('somador.v')));
    assert.deepEqual(lastCall('order'), ['-C', spf, '--json', 'order', rtl('somador.v'), '--before', rtl('contador.v')]);
    assert.deepEqual(order(api), ['somador.v', 'contador.v', 'mux.sv', 'soma.v']);
  });

  it('Descer, Primeiro e Último; o primeiro não sobe', async () => {
    await run('lace.moveDown', node('design', rtl('somador.v')));
    assert.deepEqual(lastCall('order'), ['-C', spf, '--json', 'order', rtl('somador.v'), '--after', rtl('mux.sv')]);
    await run('lace.moveFirst', node('design', rtl('mux.sv')));
    assert.deepEqual(lastCall('order'), ['-C', spf, '--json', 'order', rtl('mux.sv'), '--first']);
    await run('lace.moveLast', node('design', rtl('contador.v')));
    assert.deepEqual(lastCall('order'), ['-C', spf, '--json', 'order', rtl('contador.v'), '--last']);
    const orders = count('order');
    assert.equal(await run('lace.moveUp', node('design', rtl('contador.v'))), undefined);
    assert.equal(count('order'), orders);
  });

  it('Tirar do projeto: lace remove, e o arquivo fica no disco', async () => {
    await run('lace.remove', node('design', rtl('somador.v')));
    assert.deepEqual(lastCall('remove'), ['-C', spf, '--json', 'remove', rtl('somador.v')]);
    assert.ok(!api.status?.synthesizable.some((f) => f.path === rtl('somador.v')));
    assert.ok(existsSync(rtl('somador.v')));
  });

  it('Renomear: lace move, e a aba aberta passa ao nome novo', async () => {
    await vscode.window.showTextDocument(vscode.Uri.file(rtl('somador.v')));
    await run('lace.rename', node('design', rtl('somador.v')), { to: 'rtl/adder.v' });
    assert.deepEqual(lastCall('move'), ['-C', spf, '--json', 'move', rtl('somador.v'), rtl('adder.v')]);
    assert.ok(existsSync(rtl('adder.v')));
    assert.equal(vscode.window.activeTextEditor?.document.uri.fsPath, rtl('adder.v'));
    assert.ok(api.status?.synthesizable.some((f) => f.path === rtl('adder.v')));
  });

  it('renomeado no explorador: lace move --after', async () => {
    const moves = count('move');
    const edit = new vscode.WorkspaceEdit();
    edit.renameFile(vscode.Uri.file(rtl('somador.v')), vscode.Uri.file(rtl('adder.v')));
    assert.ok(await vscode.workspace.applyEdit(edit));
    await until(() => count('move') > moves, 'o lace move --after');
    await api.explorerIdle();
    assert.deepEqual(lastCall('move'), ['-C', spf, '--json', 'move', '--after', rtl('somador.v'), rtl('adder.v')]);
    assert.ok(api.status?.synthesizable.some((f) => f.path === rtl('adder.v')));
  });

  it('um arquivo fora do projeto renomeado no explorador: nada no Lace', async () => {
    const moves = count('move');
    const edit = new vscode.WorkspaceEdit();
    edit.renameFile(vscode.Uri.file(rtl('sobra.v')), vscode.Uri.file(rtl('sobra2.v')));
    assert.ok(await vscode.workspace.applyEdit(edit));
    await api.explorerIdle();
    renameSync(rtl('sobra2.v'), rtl('sobra.v'));
    assert.equal(count('move'), moves);
  });

  it('apagado no explorador: lace remove', async () => {
    const text = readFileSync(rtl('somador.v'));
    const removes = count('remove');
    try {
      const edit = new vscode.WorkspaceEdit();
      edit.deleteFile(vscode.Uri.file(rtl('somador.v')));
      assert.ok(await vscode.workspace.applyEdit(edit));
      await until(() => count('remove') > removes, 'o lace remove');
      await api.explorerIdle();
      assert.deepEqual(lastCall('remove'), ['-C', spf, '--json', 'remove', rtl('somador.v')]);
    } finally {
      writeFileSync(rtl('somador.v'), text);
    }
  });
});
