// Os diagnósticos do check gravado, organizados para o painel Problemas.

import * as assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, resolve, sep } from 'node:path';

import type { CheckReport, Diagnostic } from '../../src/lace/lace-types';
import { countProblems, describeCount, groupProblems } from '../../src/problems';
import { RESPONSES } from '../support/fixtures';

// Absoluto também no Windows (com a unidade), como a raiz que o aurora status dá.
const ROOT = resolve('/', 'projetos', 'projeto demo');
const HOME = join('/', 'home', 'usuario');
const inRoot = (file: string) => file.startsWith(ROOT + sep) || file.startsWith(`${ROOT}/`);

function diagnostics(file: string): Diagnostic[] {
  const lines = readFileSync(join(RESPONSES, file), 'utf8').trim().split('\n');
  const escape = (path: string) => JSON.stringify(path).slice(1, -1);
  const text = lines.at(-1)?.replaceAll('${ROOT}', escape(ROOT)).replaceAll('${HOME}', escape(HOME)) ?? '{}';
  return (JSON.parse(text).result as CheckReport).check.diagnostics;
}

describe('problemas do check', () => {
  it('tira as repetições de cada elaboração', () => {
    const all = diagnostics('check.events.jsonl');
    const { byFile, outside } = groupProblems(all, ROOT, inRoot);
    const kept = [...byFile.values(), ...outside.values()].flat().length;
    assert.ok(kept < all.length, `${kept} de ${all.length}`);
    for (const list of byFile.values()) {
      const keys = list.map((p) => JSON.stringify([p.line, p.column, p.severity, p.message]));
      assert.equal(new Set(keys).size, keys.length);
    }
  });

  it('separa a biblioteca SAPHO do bundle, que fica fora do painel', () => {
    const { byFile, outside } = groupProblems(diagnostics('check-lint.events.jsonl'), ROOT, inRoot);
    assert.ok([...byFile.keys()].every(inRoot));
    assert.ok([...outside.keys()].some((file) => file.includes('SAPHO')));
    // O Verilator dá a coluna; o Icarus, não.
    const soma = byFile.get(`${ROOT}/soma/soma/soma.v`) ?? [];
    assert.ok(soma.some((p) => p.tool === 'verilator' && p.column !== null));
  });

  it('o erro de sintaxe vai para o arquivo; a linha sem arquivo, para a saída', () => {
    const { byFile, unplaced } = groupProblems(diagnostics('erro-check.events.jsonl'), ROOT, inRoot);
    const [[file, problems]] = [...byFile];
    assert.ok(file.endsWith('quebrado.v'));
    assert.deepEqual(countProblems(problems), { errors: 1, warnings: 0 });
    assert.equal(problems[0].line, 5);
    assert.deepEqual(
      unplaced.map((d) => d.message),
      ['I give up.'],
    );
  });

  it('um caminho relativo é da pasta do projeto', () => {
    const relative: Diagnostic = {
      tool: 'iverilog',
      severity: 'error',
      message: 'x',
      file: 'rtl/a.v',
      line: 1,
      column: null,
      raw: 'rtl/a.v:1: error: x',
    };
    const { byFile } = groupProblems([relative], ROOT, inRoot);
    assert.deepEqual([...byFile.keys()], [join(ROOT, 'rtl', 'a.v')]);
  });

  it('conta por extenso', () => {
    assert.equal(describeCount({ errors: 1, warnings: 1 }), '1 erro, 1 aviso');
    assert.equal(describeCount({ errors: 0, warnings: 3 }), '0 erros, 3 avisos');
  });
});
