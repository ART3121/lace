import * as assert from 'node:assert/strict';

import { insidePart, isInside, pathKey, samePath } from '../../src/paths';

describe('caminhos do Lace e do VS Code', () => {
  it('no Windows, a letra do drive, a caixa e a barra não importam', () => {
    // O Lace devolve o canônico; o Uri.fsPath, a letra em minúscula.
    assert.ok(samePath('C:\\Users\\Aluno\\p\\a.v', 'c:\\Users\\Aluno\\p\\a.v', true));
    assert.ok(samePath('C:\\Users\\Aluno\\P\\A.v', 'c:/users/aluno/p/a.v', true));
    assert.ok(samePath('C:\\p\\', 'c:\\p', true));
    assert.ok(!samePath('C:\\p\\a.v', 'C:\\p\\b.v', true));
    assert.equal(pathKey('C:\\Proj\\', true), 'c:\\proj');
  });

  it('fora do Windows, a caixa importa', () => {
    assert.ok(!samePath('/home/a/P/a.v', '/home/a/p/a.v', false));
    assert.ok(samePath('/home/a/p/', '/home/a/p', false));
    assert.equal(pathKey('/', false), '/');
  });

  it('dentro de uma pasta, com o nome que vier', () => {
    assert.ok(isInside('c:\\Users\\Aluno\\p\\rtl\\a.v', 'C:\\Users\\Aluno\\p', true));
    assert.ok(isInside('C:\\p', 'c:\\p\\', true));
    assert.ok(!isInside('C:\\proj2\\a.v', 'C:\\proj', true));
    assert.ok(isInside('/p/rtl/a.v', '/p', false));
    assert.ok(!isInside('/p2/a.v', '/p', false));
    assert.ok(!isInside('/P/a.v', '/p', false));
  });

  it('a parte de dentro guarda os nomes do caminho', () => {
    assert.equal(insidePart('c:\\p\\Rtl\\A.v', 'C:\\p', true), 'Rtl\\A.v');
    assert.equal(insidePart('/p/rtl/a.v', '/p', false), 'rtl/a.v');
  });
});
