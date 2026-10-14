// "Lace: Verificar" dentro do VS Code, com o lace falso: o comando roda
// `lace check` pelo --events e põe os diagnósticos no painel Problemas.

import * as assert from 'node:assert/strict';
import { join, sep } from 'node:path';

import * as vscode from 'vscode';

import type { LaceRun } from '../../src/lace/client';
import type { CheckReport } from '../../src/lace/lace-types';
import type { SourceNode } from '../../src/sources/provider';
import { activate, lastCall, resetFake, run, spf, workspace } from './helpers';

/** Os diagnósticos da extensão (as fontes são as ferramentas do Lace). */
function laceDiagnostics(): [vscode.Uri, vscode.Diagnostic[]][] {
  return vscode.languages
    .getDiagnostics()
    .filter(([, list]) => list.some((d) => d.source === 'iverilog' || d.source === 'verilator'));
}

describe('Lace: Verificar', () => {
  before(async () => {
    await resetFake(await activate());
  });

  it('o projeto: lace check pelo --events, e os avisos no painel Problemas', async () => {
    const result = await run<LaceRun<CheckReport>>('lace.check');
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('check'), ['-C', spf, '--events', 'check']);
    const soma = vscode.languages.getDiagnostics(vscode.Uri.file(join(workspace, 'soma', 'soma', 'soma.v')));
    assert.ok(soma.length > 0, 'sem os avisos de soma.v');
    assert.ok(soma.every((d) => d.severity === vscode.DiagnosticSeverity.Warning));
  });

  it('a biblioteca SAPHO do bundle fica fora do painel', async () => {
    await run('lace.check');
    const files = laceDiagnostics().map(([uri]) => uri.fsPath);
    assert.ok(files.length > 0);
    assert.deepEqual(
      files.filter((file) => !file.startsWith(workspace + sep)),
      [],
    );
  });

  it('com lint: passa --lint, e o Verilator dá a coluna', async () => {
    await run('lace.checkLint');
    assert.deepEqual(lastCall('check'), ['-C', spf, '--events', 'check', '--lint']);
    const columns = laceDiagnostics()
      .flatMap(([, list]) => list)
      .filter((d) => d.source === 'verilator' && d.range.start.character > 0);
    assert.ok(columns.length > 0);
  });

  it('um arquivo e um processador', async () => {
    const file = join(workspace, 'rtl', 'somador.v');
    await run('lace.checkFile', vscode.Uri.file(file));
    assert.deepEqual(lastCall('check'), ['-C', spf, '--events', 'check', file]);
    const processor = { kind: 'entry', entry: { role: 'processor', label: 'soma', path: null } } as unknown as SourceNode;
    await run('lace.checkProcessor', processor);
    assert.deepEqual(lastCall('check'), ['-C', spf, '--events', 'check', '-p', 'soma']);
  });
});
