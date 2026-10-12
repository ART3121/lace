import * as assert from 'node:assert/strict';
import { chmodSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';

import { LacePathError, locateLace } from '../../src/lace/locate';

const exe = process.platform === 'win32' ? 'lace.exe' : 'lace';

function fakeExecutable(dir: string, mode = 0o755): string {
  mkdirSync(dir, { recursive: true });
  const file = join(dir, exe);
  writeFileSync(file, '');
  chmodSync(file, mode);
  return file;
}

describe('locateLace', () => {
  let base: string;

  beforeEach(() => {
    base = mkdtempSync(join(tmpdir(), 'lace locate '));
  });

  it('usa lace.path quando ele está preenchido, sem olhar o PATH', () => {
    const configured = fakeExecutable(join(base, 'meu lace'));
    const inPath = fakeExecutable(join(base, 'bin'));
    assert.deepEqual(locateLace(configured, { PATH: join(base, 'bin') }), { file: configured, args: [] });
    assert.deepEqual(locateLace('', { PATH: join(base, 'bin') }), { file: inPath, args: [] });
  });

  it('recusa um lace.path que não existe, em vez de cair no PATH', () => {
    fakeExecutable(join(base, 'bin'));
    assert.throws(() => locateLace(join(base, 'nada', exe), { PATH: join(base, 'bin') }), LacePathError);
  });

  it('procura no PATH na ordem, pulando pastas sem o lace', () => {
    const second = fakeExecutable(join(base, 'segunda'));
    fakeExecutable(join(base, 'terceira'));
    const path = [join(base, 'vazia'), join(base, 'segunda'), join(base, 'terceira')].join(delimiter);
    assert.deepEqual(locateLace(undefined, { PATH: path }), { file: second, args: [] });
  });

  it('acha a variável do PATH com outra caixa (Path, no Windows)', () => {
    const file = fakeExecutable(join(base, 'bin'));
    assert.deepEqual(locateLace(undefined, { Path: join(base, 'bin') }), { file, args: [] });
  });

  it('sem lace no PATH, devolve undefined', () => {
    assert.equal(locateLace(undefined, { PATH: join(base, 'vazia') }), undefined);
    assert.equal(locateLace(undefined, {}), undefined);
  });

  if (process.platform !== 'win32') {
    it('pula um lace sem permissão de execução', () => {
      fakeExecutable(join(base, 'bin'), 0o644);
      assert.equal(locateLace(undefined, { PATH: join(base, 'bin') }), undefined);
    });
  }
});
