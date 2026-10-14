// O fluxo SAPHO dentro do VS Code, com o lace falso: o botão Build da barra
// de status (`lace build`), o build de um processador e os erros do YANC no
// painel Problemas, na linha do fonte.

import * as assert from 'node:assert/strict';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { LaceExtensionApi } from '../../src/extension';
import type { LaceRun } from '../../src/lace/client';
import type { BuildReport } from '../../src/lace/lace-types';
import type { SourceNode } from '../../src/sources/provider';
import { activate, lastCall, resetFake, run, setFakeState, spf, workspace } from './helpers';

const soma = { kind: 'entry', entry: { role: 'processor', label: 'soma', path: null } } as unknown as SourceNode;

describe('Lace: Compilar', () => {
  let api: LaceExtensionApi;
  const source = vscode.Uri.file(join(workspace, 'soma', 'soma.cmm'));

  beforeEach(async () => {
    api = await activate();
    await resetFake(api);
  });

  it('todos os processadores: lace build pelo --events, e a vista mostra o filtro compilado', async () => {
    assert.equal(api.status?.processors.find((p) => p.name === 'filtro')?.built, false);
    const result = await run<LaceRun<BuildReport>>('lace.build');
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('build'), ['-C', spf, '--events', 'build']);
    assert.equal(api.runs.state('build').last?.summary, '2 processadores compilados');
    assert.equal(api.status?.processors.find((p) => p.name === 'filtro')?.built, true);
    // O Lace registrou o .v e o testbench que o filtro gerou.
    assert.ok(api.status?.synthesizable.some((f) => f.path.endsWith('filtro.v')));
    assert.deepEqual(
      result?.status === 'succeeded' && result.result.registered.map((f) => [f.role, f.selected]),
      [
        ['synthesizable', false],
        ['testbench', false],
      ],
    );
  });

  it('um processador com erro: o erro do YANC no fonte, e o build seguinte o apaga', async () => {
    setFakeState('soma-quebrado');
    const failed = await run<LaceRun<BuildReport>>('lace.buildProcessor', soma);
    assert.equal(failed?.status, 'failed');
    assert.deepEqual(lastCall('build'), ['-C', spf, '--events', 'build', '-p', 'soma']);
    const [error] = vscode.languages.getDiagnostics(source);
    assert.equal(error?.severity, vscode.DiagnosticSeverity.Error);
    assert.equal(error?.range.start.line, 14);
    assert.equal(error?.source, 'cmmcomp');
    assert.equal(api.runs.state('build').last?.outcome, 'failed');

    setFakeState('initial');
    await run('lace.buildProcessor', soma);
    assert.deepEqual(vscode.languages.getDiagnostics(source), []);
  });
});
