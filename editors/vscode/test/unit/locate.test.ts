import * as assert from 'node:assert/strict';
import { chmodSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';

import { AuroraPathError, locateAurora } from '../../src/aurora/locate';

const exe = process.platform === 'win32' ? 'aurora.exe' : 'aurora';

function fakeExecutable(dir: string, mode = 0o755): string {
  mkdirSync(dir, { recursive: true });
  const file = join(dir, exe);
  writeFileSync(file, '');
  chmodSync(file, mode);
  return file;
}

describe('locateAurora', () => {
  let base: string;

  beforeEach(() => {
    base = mkdtempSync(join(tmpdir(), 'aurora locate '));
  });

  it('usa aurora.path quando ele está preenchido, sem olhar o PATH', () => {
    const configured = fakeExecutable(join(base, 'meu aurora'));
    const inPath = fakeExecutable(join(base, 'bin'));
    assert.deepEqual(locateAurora(configured, { PATH: join(base, 'bin') }), { file: configured, args: [] });
    assert.deepEqual(locateAurora('', { PATH: join(base, 'bin') }), { file: inPath, args: [] });
  });

  it('recusa um aurora.path que não existe, em vez de cair no PATH', () => {
    fakeExecutable(join(base, 'bin'));
    assert.throws(() => locateAurora(join(base, 'nada', exe), { PATH: join(base, 'bin') }), AuroraPathError);
  });

  it('procura no PATH na ordem, pulando pastas sem o aurora', () => {
    const second = fakeExecutable(join(base, 'segunda'));
    fakeExecutable(join(base, 'terceira'));
    const path = [join(base, 'vazia'), join(base, 'segunda'), join(base, 'terceira')].join(delimiter);
    assert.deepEqual(locateAurora(undefined, { PATH: path }), { file: second, args: [] });
  });

  it('acha a variável do PATH com outra caixa (Path, no Windows)', () => {
    const file = fakeExecutable(join(base, 'bin'));
    assert.deepEqual(locateAurora(undefined, { Path: join(base, 'bin') }), { file, args: [] });
  });

  it('sem aurora no PATH, devolve undefined', () => {
    assert.equal(locateAurora(undefined, { PATH: join(base, 'vazia') }), undefined);
    assert.equal(locateAurora(undefined, {}), undefined);
  });

  if (process.platform !== 'win32') {
    it('pula um aurora sem permissão de execução', () => {
      fakeExecutable(join(base, 'bin'), 0o644);
      assert.equal(locateAurora(undefined, { PATH: join(base, 'bin') }), undefined);
    });
  }
});
