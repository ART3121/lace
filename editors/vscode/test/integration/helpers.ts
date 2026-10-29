// O que os testes de integração dividem: a extensão ativada, o projeto da
// cópia e o aurora falso.

import * as assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

import * as vscode from 'vscode';

import type { AuroraExtensionApi } from '../../src/extension';
import { laceCalls, until } from '../support/fixtures';

export const workspace = process.env.AURORA_TEST_WORKSPACE ?? '';
export const spf = join(workspace, 'demo.spf');
const log = process.env.FAKE_AURORA_LOG ?? '';
const state = process.env.FAKE_AURORA_STATE ?? '';

/** A extensão ativada, com o projeto lido. */
export async function activate(): Promise<AuroraExtensionApi> {
  assert.ok(workspace, 'AURORA_TEST_WORKSPACE não definido: rode pelo npm run test:integration');
  const extension = vscode.extensions.getExtension<AuroraExtensionApi>('nipscernlab.aurora');
  assert.ok(extension, 'a extensão não foi carregada');
  const api = await extension.activate();
  await until(() => api.status !== undefined, 'a primeira leitura do projeto');
  return api;
}

/** Volta o aurora falso ao estado inicial e relê o projeto. */
export async function resetFake(api: AuroraExtensionApi): Promise<void> {
  writeFileSync(state, 'initial');
  await api.refresh();
}

/** Muda o estado do aurora falso (o que um comando de fora do VS Code faria). */
export function setFakeState(name: string): void {
  writeFileSync(state, name);
}

/** As chamadas ao aurora falso, na ordem. */
export function calls(): string[][] {
  return laceCalls(log);
}

/** A última chamada que tem este comando. */
export function lastCall(command: string): string[] | undefined {
  return calls()
    .filter((argv) => argv.includes(command))
    .at(-1);
}

export function run<T>(command: string, ...args: unknown[]): Thenable<T | undefined> {
  return vscode.commands.executeCommand<T | undefined>(command, ...args);
}
