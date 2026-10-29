// "Lace: Sintetizar" dentro do VS Code, com o lace falso: `lace synth`, o RTL
// viewer numa aba (o webview tem de desenhar: é o que prova que o worker do
// layout roda lá dentro) e, com lace.synth.schematic, o SVG do Graphviz.

import * as assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { LaceExtensionApi } from '../../src/extension';
import type { LaceRun } from '../../src/lace/client';
import type { SynthReport } from '../../src/lace/lace-types';
import type { SourceNode } from '../../src/sources/provider';
import { RESPONSES, until } from '../support/fixtures';
import { activate, calls, lastCall, resetFake, run, spf, workspace } from './helpers';

function openTabs(): string[] {
  return vscode.window.tabGroups.all
    .flatMap((group) => group.tabs)
    .flatMap((tab) => {
      const input = tab.input as { uri?: vscode.Uri } | undefined;
      return input?.uri ? [input.uri.fsPath] : [];
    });
}

function within<T>(promise: Promise<T>, ms: number, what: string): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) => setTimeout(() => reject(new Error(`Esperei ${ms} ms por ${what}`)), ms)),
  ]);
}

const processor = { kind: 'entry', entry: { role: 'processor', label: 'soma', path: null } } as unknown as SourceNode;

describe('Lace: Sintetizar', () => {
  let api: LaceExtensionApi;

  before(async () => {
    api = await activate();
    await resetFake(api);
  });

  it('o topo: lace synth, e o RTL viewer abre e desenha', async () => {
    const drawn = new Promise<void>((resolve) => api.viewer.onDidDraw(() => resolve()));
    const result = await run<LaceRun<SynthReport>>('lace.synth');
    assert.equal(result?.status, 'succeeded');
    assert.deepEqual(lastCall('synth'), ['-C', spf, '--events', 'synth']);
    const netlist = result?.status === 'succeeded' ? result.result.synthesis?.netlist : undefined;
    assert.ok(netlist);
    assert.equal(api.viewer.current, netlist);
    await within(drawn, 20000, 'o desenho no webview');
  });

  it('Abrir o RTL viewer: o netlist da última síntese, sem sintetizar de novo', async () => {
    const before = calls().length;
    await run('lace.rtlViewer');
    assert.equal(calls().length, before);
    assert.ok(api.viewer.current?.endsWith('hierarchy.json'));
  });

  it('com lace.synth.schematic: passa --svg e abre também o esquemático do Graphviz', async () => {
    const config = vscode.workspace.getConfiguration('lace.synth');
    await config.update('schematic', true, vscode.ConfigurationTarget.Global);
    try {
      const result = await run<LaceRun<SynthReport>>('lace.synth');
      assert.deepEqual(lastCall('synth'), ['-C', spf, '--events', 'synth', '--svg']);
      const svg = result?.status === 'succeeded' ? result.result.schematic?.svg : undefined;
      assert.ok(svg);
      assert.ok(openTabs().includes(svg), `${svg} não abriu`);
    } finally {
      await config.update('schematic', undefined, vscode.ConfigurationTarget.Global);
    }
  });

  it('um lace synth --open no terminal (o rtl-viewer.html aparece): o netlist abre na aba', async () => {
    // O que o --open grava com LACE_OPENER=vscode, que a extensão põe no terminal.
    assert.equal(process.env.LACE_OPENER ?? 'vscode', 'vscode');
    // A pasta e os arquivos no mesmo instante, como na primeira síntese de um
    // alvo: o vigia do VS Code não veria (poller.ts); a extensão confere.
    const dir = join(workspace, '.aurora8', 'Temp', 'synth', 'soma');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'hierarchy.json'), readFileSync(join(RESPONSES, 'synth-soma.created.json')));
    writeFileSync(join(dir, 'rtl-viewer.html'), '<html></html>');
    await until(() => api.viewer.current === join(dir, 'hierarchy.json'), 'a aba com o netlist do soma');
  });

  it('um processador: lace synth -p, e o RTL viewer com o netlist dele', async () => {
    const result = await run<LaceRun<SynthReport>>('lace.synthProcessor', processor);
    assert.deepEqual(lastCall('synth'), ['-C', spf, '--events', 'synth', '-p', 'soma']);
    assert.ok(((result?.status === 'succeeded' && result.result.synthesis?.statistics?.cells) || 0) > 0);
    assert.ok(api.viewer.current?.includes('soma'));
  });
});
