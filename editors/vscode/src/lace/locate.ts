// Onde está o executável lace: o da configuração `lace.path` ou, sem ela, o
// primeiro do PATH. Sem shell, então quem procura no PATH somos nós.

import { accessSync, constants, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { delimiter, join } from 'node:path';

import type { LaceCommand } from './cliClient';

/** `lace.path` aponta para um arquivo que não existe. */
export class LacePathError extends Error {
  constructor(readonly configured: string) {
    super(`lace.path aponta para ${configured}, que não existe`);
    this.name = 'LacePathError';
  }
}

/**
 * O comando que roda o lace, ou `undefined` se não há `lace.path` e o PATH
 * não tem um lace. Lança `LacePathError` se `lace.path` aponta para nada.
 */
export function locateLace(
  configured: string | undefined,
  env: NodeJS.ProcessEnv = process.env,
): LaceCommand | undefined {
  const setting = configured?.trim();
  if (setting) {
    const file = expandHome(setting);
    if (!isFile(file)) {
      throw new LacePathError(setting);
    }
    return { file, args: [] };
  }
  // No Windows só o .exe: um .cmd ou .bat não roda sem shell.
  const name = process.platform === 'win32' ? 'lace.exe' : 'lace';
  for (const dir of pathEntries(env)) {
    const file = join(dir, name);
    if (isFile(file) && isExecutable(file)) {
      return { file, args: [] };
    }
  }
  return undefined;
}

function pathEntries(env: NodeJS.ProcessEnv): string[] {
  // O process.env do Windows não diferencia caixa, mas um objeto comum
  // (os testes) sim, e lá a variável costuma se chamar Path.
  const key = Object.keys(env).find((k) => k.toUpperCase() === 'PATH');
  const value = key ? env[key] : undefined;
  return (value ?? '').split(delimiter).filter((dir) => dir !== '');
}

function expandHome(path: string): string {
  return /^~[/\\]/.test(path) ? join(homedir(), path.slice(2)) : path;
}

function isFile(path: string): boolean {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

function isExecutable(path: string): boolean {
  if (process.platform === 'win32') {
    return true;
  }
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}
