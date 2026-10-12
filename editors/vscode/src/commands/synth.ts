// "Lace: Sintetizar": `lace synth` no topo do projeto ou num processador, no
// Yosys, e o RTL viewer com o netlist que ela grava (SynthesisResult.netlist).
// Com lace.synth.schematic, também o esquemático do Graphviz (`--svg`). As
// estatísticas são as células genéricas do Yosys, sem mapeamento para FPGA.

import { existsSync } from 'node:fs';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { LaceRun } from '../lace/client';
import type { SynthReport } from '../lace/lace-types';
import { formatDuration, formatLaceError } from '../messages';
import type { RunResult } from '../runs';
import { describeBuild, writeRegistered } from './build';
import { chooseProject, processorOf } from './common';
import { runOperation, setting, type RunDeps } from './run';

export type SynthTarget = { kind: 'project' } | { kind: 'processor'; name: string };

/** O netlist da última síntese de cada alvo nesta sessão, pelo JSON do synth. */
const lastNetlist = new Map<string, string>();
const targetKey = (spf: string, target: SynthTarget) => `${spf}|${target.kind === 'processor' ? target.name : ''}`;

export async function synthesize(deps: RunDeps, target: SynthTarget): Promise<LaceRun<SynthReport> | undefined> {
  const svg = setting('lace.synth', 'schematic', false);
  let summary: RunResult | undefined;
  const done = await runOperation(deps, 'synth', 'synth', undefined, (client, project) => ({
    title:
      `Sintetizando ${target.kind === 'processor' ? `o processador ${target.name}` : `o topo de ${project.name}`}` +
      (svg ? ', com o esquemático' : ''),
    reveal: true,
    restart: false,
    start: (signal, onEvent) =>
      client.synth(project.spf, {
        signal,
        onEvent,
        svg,
        processor: target.kind === 'processor' ? target.name : undefined,
      }),
    summarize: (run) => {
      deps.builds.apply(run.status !== 'error' ? (run.result?.builds ?? []) : [], project);
      return (summary = summarizeSynth(deps.runs.output, run));
    },
  }));
  if (!done) {
    return undefined;
  }
  // O synth compila os processadores antes.
  await deps.status.refresh();
  const { run } = done;
  if (run.status === 'error') {
    void vscode.window.showErrorMessage(`Lace: ${formatLaceError(run.error)}`);
  } else if (run.status === 'cancelled') {
    vscode.window.setStatusBarMessage('Lace: síntese cancelada', 5000);
  } else if (run.status === 'failed') {
    const output = 'Saída';
    void vscode.window
      .showErrorMessage(`Lace: a síntese falhou: ${summary?.summary}.`, output)
      .then((choice) => choice === output && deps.runs.output.show());
  } else {
    vscode.window.setStatusBarMessage(`$(pass) Lace: ${summary?.summary}`, 8000);
    const netlist = run.result.synthesis?.netlist;
    if (netlist) {
      lastNetlist.set(targetKey(done.project.spf, target), netlist);
      if (setting('lace.synth', 'openViewer', true)) {
        await deps.viewer.show(netlist, target);
      }
    }
    const schematic = run.result.schematic?.svg;
    if (schematic) {
      await vscode.commands.executeCommand('vscode.open', vscode.Uri.file(schematic));
    }
  }
  return run;
}

/**
 * "Lace: Abrir o RTL viewer": o netlist da última síntese do alvo. Nesta
 * sessão, o que o JSON do synth disse; de antes, o que ficou no disco, em
 * `.lace/Temp/synth/<alvo>/hierarchy.json` (onde o Lace grava, e onde o
 * Studio também procura, em SchematicView); sem nenhum, sintetiza.
 */
export async function openRtlViewer(deps: RunDeps, target: SynthTarget): Promise<void> {
  const project = await chooseProject(deps.projects);
  if (!project) {
    return;
  }
  const name = target.kind === 'processor' ? target.name : deps.status.report?.top_module;
  const onDisk = name ? join(project.root, '.lace', 'Temp', 'synth', name, 'hierarchy.json') : undefined;
  const netlist = lastNetlist.get(targetKey(project.spf, target)) ?? (onDisk && existsSync(onDisk) ? onDisk : undefined);
  if (netlist) {
    await deps.viewer.show(netlist, target);
    return;
  }
  const run = await synthesize(deps, target);
  if (run?.status === 'succeeded' && !setting('lace.synth', 'openViewer', true) && run.result.synthesis?.netlist) {
    await deps.viewer.show(run.result.synthesis.netlist, target);
  }
}

/** Escreve as estatísticas na saída e devolve o resumo para o botão. */
export function summarizeSynth(output: vscode.OutputChannel, run: LaceRun<SynthReport>): RunResult {
  if (run.status === 'error') {
    output.appendLine(`Erro: ${formatLaceError(run.error)}`);
    return { outcome: 'error', summary: formatLaceError(run.error) };
  }
  if (run.result === null) {
    output.appendLine('Cancelado.');
    return { outcome: 'cancelled', summary: 'cancelada' };
  }
  const { builds, synthesis, schematic, report } = run.result;
  writeRegistered(output, run.result.registered);
  if (synthesis === null) {
    const broken = builds.find((b) => b.status !== 'succeeded');
    if (broken) {
      output.appendLine(`=== ${describeBuild(broken)}`);
    }
    const why = broken ? `o build de ${broken.processor} terminou como ${broken.status}` : 'não rodou';
    output.appendLine(`=== Síntese não rodou: ${why}`);
    return { outcome: run.status, summary: why };
  }
  const stats = synthesis.statistics;
  if (stats) {
    const count = (n: number | null) => (n === null ? 'não informado' : String(n));
    const cells = stats.cell_types
      .slice(0, 8)
      .map((c) => `${c.cell_type} ${c.count}`)
      .join(', ');
    output.appendLine(`Células: ${count(stats.cells)}${cells ? ` (${cells})` : ''}`);
    output.appendLine(`Fios: ${count(stats.wires)} (${count(stats.wire_bits)} bits)`);
    output.appendLine(`Memórias: ${count(stats.memories)} (${count(stats.memory_bits)} bits)`);
    output.appendLine(`Processos: ${count(stats.processes)}`);
  }
  if (schematic) {
    output.appendLine(
      schematic.svg ? `Esquemático: ${schematic.svg}` : `Esquemático de ${schematic.module}: ${schematic.status}`,
    );
  }
  const time = formatDuration(synthesis.duration_ms);
  const summary =
    synthesis.status === 'succeeded'
      ? `${synthesis.top} sintetizado em ${time}` + (stats?.cells != null ? `: ${stats.cells} células` : '')
      : `a síntese de ${synthesis.top} terminou como ${synthesis.status}`;
  output.appendLine(`=== ${summary}${report ? `; relatório ${report}` : ''}`);
  return { outcome: run.status, summary };
}

export function synthesizeProcessor(deps: RunDeps, target?: unknown): Promise<LaceRun<SynthReport> | undefined> {
  const name = processorOf(target);
  if (name === undefined) {
    void vscode.window.showWarningMessage('Lace: escolha um processador na vista Fontes.');
    return Promise.resolve(undefined);
  }
  return synthesize(deps, { kind: 'processor', name });
}
