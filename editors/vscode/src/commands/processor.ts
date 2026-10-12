// Os processadores SAPHO do projeto: "Novo processador" (`lace proc add`,
// que cria a pasta, o fonte-modelo e registra no .spf) e "Configurar
// processador" (`lace proc set`: frequência, clocks e exportação de arrays,
// que valem a partir do próximo build).

import * as vscode from 'vscode';

import type { LaceRun, ProcessorChanges } from '../lace/client';
import type { Processor, ProcessorStatus } from '../lace/lace-types';
import { chooseProject, clientFor, processorOf, showLaceError, showProcessError, type CommandDeps } from './common';

/** A regra do Lace (`lace proc add`): letras, dígitos e _, começando por letra ou _. */
export const PROCESSOR_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;

type Language = 'cmm' | 'cpp';

/** Para os testes, e para quem chama o comando com o processador pronto. */
export interface NewProcessorArgs {
  name: string;
  language: Language;
}

export async function newProcessor(deps: CommandDeps, args?: unknown): Promise<LaceRun<Processor> | undefined> {
  const client = clientFor(deps, 'processors');
  const project = client && (await chooseProject(deps.projects));
  if (!client || !project) {
    return undefined;
  }
  const existing = new Set(deps.status.report?.processors.map((p) => p.name) ?? []);
  const given = isNewProcessorArgs(args) ? args : undefined;
  const name =
    given?.name ??
    (await vscode.window.showInputBox({
      title: `Novo processador SAPHO em ${project.name}`,
      prompt: 'O nome é a pasta, o #PRNAME do programa e o nome do módulo Verilog.',
      placeHolder: 'proc',
      validateInput: (value) => validateName(value, existing),
    }));
  if (name === undefined) {
    return undefined;
  }
  const language = given?.language ?? (await pickLanguage());
  if (language === undefined) {
    return undefined;
  }
  let run: LaceRun<Processor>;
  try {
    run = await client.addProcessor(project.spf, name, language);
  } catch (error) {
    showProcessError(deps, error);
    return undefined;
  }
  if (run.status === 'error') {
    showLaceError(run.error);
    return run;
  }
  await deps.status.refresh();
  if (run.result) {
    await vscode.window.showTextDocument(vscode.Uri.file(run.result.source));
    vscode.window.setStatusBarMessage(`$(pass) Lace: processador ${run.result.name} criado`, 6000);
  }
  return run;
}

export function validateName(value: string, existing: ReadonlySet<string>): string | undefined {
  if (value === '') {
    return 'Dê um nome ao processador.';
  }
  if (!PROCESSOR_NAME.test(value)) {
    return 'Só letras sem acento, dígitos e _, começando por letra ou _.';
  }
  if (existing.has(value)) {
    return `O projeto já tem o processador ${value}.`;
  }
  return undefined;
}

async function pickLanguage(): Promise<Language | undefined> {
  const items: (vscode.QuickPickItem & { language: Language })[] = [
    { label: 'C±', description: 'cmm', detail: 'A linguagem do SAPHO, com número complexo nativo', language: 'cmm' },
    { label: 'C', description: 'cpp', detail: 'C, pelo pré-processador e o compilador C do YANC', language: 'cpp' },
  ];
  const picked = await vscode.window.showQuickPick(items, { title: 'A linguagem do programa' });
  return picked?.language;
}

function isNewProcessorArgs(value: unknown): value is NewProcessorArgs {
  const args = value as NewProcessorArgs | undefined;
  return typeof args?.name === 'string' && (args.language === 'cmm' || args.language === 'cpp');
}

/** Para os testes: o processador e o que mudar, sem perguntar. */
export interface ConfigureProcessorArgs {
  changes: ProcessorChanges;
}

export async function configureProcessor(
  deps: CommandDeps,
  target?: unknown,
  args?: unknown,
): Promise<LaceRun<Processor> | undefined> {
  const client = clientFor(deps, 'processors');
  const project = client && (await chooseProject(deps.projects));
  if (!client || !project) {
    return undefined;
  }
  const processors = deps.status.report?.processors ?? [];
  const name = processorOf(target) ?? (await pickProcessor(processors));
  const processor = processors.find((p) => p.name === name);
  if (name === undefined || processor === undefined) {
    if (name !== undefined) {
      void vscode.window.showWarningMessage(`Lace: o projeto não tem o processador ${name}.`);
    }
    return undefined;
  }
  const changes = (args as ConfigureProcessorArgs | undefined)?.changes ?? (await askChanges(processor));
  if (changes === undefined) {
    return undefined;
  }
  let run: LaceRun<Processor>;
  try {
    run = await client.configureProcessor(project.spf, name, changes);
  } catch (error) {
    showProcessError(deps, error);
    return undefined;
  }
  if (run.status === 'error') {
    showLaceError(run.error);
    return run;
  }
  await deps.status.refresh();
  if (run.result) {
    vscode.window.setStatusBarMessage(
      `$(pass) Lace: ${run.result.name} com ${describeSettings(run.result)}; vale a partir do próximo build`,
      8000,
    );
  }
  return run;
}

async function pickProcessor(processors: readonly ProcessorStatus[]): Promise<string | undefined> {
  if (processors.length === 0) {
    void vscode.window.showWarningMessage('Lace: o projeto não tem processador. Crie um com Novo processador.');
    return undefined;
  }
  if (processors.length === 1) {
    return processors[0].name;
  }
  const picked = await vscode.window.showQuickPick(
    processors.map((p) => ({ label: p.name, description: describeSettings(p) })),
    { title: 'Configurar qual processador?' },
  );
  return picked?.label;
}

/** Um ajuste por vez: o que mudar, e o valor novo. */
async function askChanges(processor: ProcessorStatus): Promise<ProcessorChanges | undefined> {
  type Setting = 'frequency' | 'clocks' | 'arrays';
  const items: (vscode.QuickPickItem & { setting: Setting })[] = [
    {
      label: 'Frequência',
      description: `${processor.frequency_mhz} MHz`,
      detail: 'A frequência de operação, que o asmcomp usa (-f)',
      setting: 'frequency',
    },
    {
      label: 'Clocks',
      description: String(processor.clocks),
      detail: 'Quantos clocks o testbench do processador simula (-c)',
      setting: 'clocks',
    },
    {
      label: processor.show_arrays ? 'Parar de exportar os arrays' : 'Exportar os arrays',
      description: processor.show_arrays ? 'exporta hoje' : 'não exporta hoje',
      detail: 'Os arrays do programa na simulação, para ver na onda (-A do cmmcomp)',
      setting: 'arrays',
    },
  ];
  const picked = await vscode.window.showQuickPick(items, { title: `Configurar ${processor.name}` });
  if (!picked) {
    return undefined;
  }
  if (picked.setting === 'arrays') {
    return { showArrays: !processor.show_arrays };
  }
  const frequency = picked.setting === 'frequency';
  const value = await vscode.window.showInputBox({
    title: `${frequency ? 'Frequência' : 'Clocks'} de ${processor.name}`,
    prompt: frequency ? 'Em MHz.' : 'Quantos clocks o testbench simula.',
    value: String(frequency ? processor.frequency_mhz : processor.clocks),
    validateInput: validatePositive,
  });
  if (value === undefined) {
    return undefined;
  }
  return frequency ? { frequencyMhz: Number(value) } : { clocks: Number(value) };
}

/** Os dois são u32 no Lace, e zero não faz sentido em nenhum. */
export function validatePositive(value: string): string | undefined {
  const n = Number(value);
  return /^\d+$/.test(value.trim()) && n > 0 && n <= 0xffffffff ? undefined : 'Um número inteiro maior que zero.';
}

function describeSettings(p: Pick<Processor, 'frequency_mhz' | 'clocks' | 'show_arrays'>): string {
  return `${p.frequency_mhz} MHz, ${p.clocks} clocks${p.show_arrays ? ', exportando os arrays' : ''}`;
}
