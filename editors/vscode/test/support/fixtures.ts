// Caminhos e auxiliares que os testes unitários e os de integração dividem.

import { existsSync, mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

/** editors/vscode, visto de out/test/support. */
export const EXTENSION_ROOT = resolve(__dirname, '..', '..', '..');
export const FIXTURES = join(EXTENSION_ROOT, 'test', 'fixtures');
export const RESPONSES = join(FIXTURES, 'responses');
/** O aurora falso, que devolve as respostas gravadas. */
export const FAKE_AURORA = join(FIXTURES, 'fake-aurora', 'aurora.mjs');
/** O repositório da AURORA, de onde vêm os schemas. */
export const AURORA_REPO = resolve(process.env.AURORA_REPO ?? join(EXTENSION_ROOT, '..', '..'));

export interface RecordedCase {
  args: string[];
  project?: string;
  state?: string;
  next?: string;
  exit: number;
  stdout?: string;
  stderr?: string;
  schema?: string;
  resultSchema?: string;
  cancelAt?: number;
}

export function recordedCases(): { recordedWith: string; cases: RecordedCase[] } {
  return JSON.parse(readFileSync(join(RESPONSES, 'cases.json'), 'utf8'));
}

/** Uma pasta temporária, com espaço no nome, para o log e o estado do aurora falso. */
export function fakeAuroraDir(): { log: string; state: string } {
  const dir = mkdtempSync(join(tmpdir(), 'aurora fake '));
  return { log: join(dir, 'aurora.log'), state: join(dir, 'aurora.state') };
}

/** Os argumentos de cada chamada ao aurora falso, na ordem. */
export function laceCalls(log: string): string[][] {
  return logEntries(log).flatMap((entry) => (entry.argv ? [entry.argv] : []));
}

/** O aurora falso já espera o SIGTERM de uma resposta gravada com cancelamento. */
export function laceWaiting(log: string): boolean {
  return logEntries(log).some((entry) => entry.waiting === true);
}

function logEntries(log: string): { argv?: string[]; waiting?: boolean }[] {
  if (!existsSync(log)) {
    return [];
  }
  return readFileSync(log, 'utf8')
    .split('\n')
    .filter((line) => line !== '')
    .map((line) => JSON.parse(line));
}

/** Espera a condição valer, conferindo a cada 20 ms. */
export async function until(condition: () => boolean, what: string, timeoutMs = 10000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!condition()) {
    if (Date.now() > deadline) {
      throw new Error(`Esperei ${timeoutMs} ms por ${what}`);
    }
    await new Promise((r) => setTimeout(r, 20));
  }
}
