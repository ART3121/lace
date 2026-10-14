import * as assert from 'node:assert/strict';
import * as path from 'node:path';

import { isSaphoAssembly } from '../../src/asmPath';

describe('assembly do SAPHO pelo lugar', () => {
  it('reconhece o .asm da pasta de saída do YANC 7.1', () => {
    assert.ok(isSaphoAssembly(path.join('/p', 'soma', 'soma', 'soma.asm')));
    assert.ok(isSaphoAssembly(path.join('/p', 'soma', 'soma', 'soma.ASM')));
  });

  it('deixa os outros .asm', () => {
    assert.ok(!isSaphoAssembly(path.join('/p', 'soma', 'soma.asm')));
    assert.ok(!isSaphoAssembly(path.join('/p', 'soma', 'saida', 'soma.asm')));
    assert.ok(!isSaphoAssembly(path.join('/p', 'boot', 'boot', 'boot.s')));
  });
});
