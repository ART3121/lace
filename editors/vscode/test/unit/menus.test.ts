// As cláusulas `when` do package.json repetem, como expressão regular, regras
// do Core (o nome de testbench, as extensões de Verilog) e o formato do
// contextValue da vista. Este teste confere que elas batem com
// src/verilog.ts e src/sources/model.ts, as cópias testáveis.

import * as assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import type { StatusReport } from '../../src/lace/lace-types';
import { sourceGroups } from '../../src/sources/model';
import { isTestbenchName, isVerilogFile } from '../../src/verilog';
import { EXTENSION_ROOT, RESPONSES } from '../support/fixtures';

interface MenuItem {
  command: string;
  when?: string;
  group?: string;
}

const manifest = JSON.parse(readFileSync(join(EXTENSION_ROOT, 'package.json'), 'utf8'));
const menus: Record<string, MenuItem[]> = manifest.contributes.menus;
const FILE_MENUS = ['commandPalette', 'explorer/context', 'editor/context', 'editor/title/context'];

function when(menu: string, command: string, group?: string): string {
  const item = menus[menu].find((i) => i.command === command && (group === undefined || i.group === group));
  assert.ok(item?.when, `${menu} não tem ${command}`);
  return item.when;
}

function regexFor(clause: string, key: string): RegExp {
  const match = new RegExp(`${key} =~ /(.+?)/([a-z]*)(?: |$)`).exec(clause);
  assert.ok(match, `a cláusula não testa ${key}: ${clause}`);
  return new RegExp(match[1], match[2]);
}

// Os exemplos do doc de is_testbench_name e os casos de borda.
const NAMES = [
  'alu.v',
  'alu_tb.v',
  'tb_alu.v',
  'TB_alu.sv',
  'tb.v',
  'TB.SV',
  'tbuf.v',
  'test_alu.v',
  'contest.v',
  'latest.v',
  'a.b_tb.v',
  'tb_.v',
  '_tb.v',
  'meu tb.v',
  'x_Tb.sv',
];

// Os contextValues que a vista gera de verdade, do status gravado.
const status: StatusReport = JSON.parse(readFileSync(join(RESPONSES, 'status.json'), 'utf8'));
const CONTEXT_VALUES = sourceGroups(status).flatMap((g) => g.entries.map((e) => e.contextValue));

describe('menus', () => {
  for (const command of ['lace.setTop', 'lace.checkFile', 'lace.add']) {
    it(`${command}: a mesma cláusula em todos os menus de arquivo`, () => {
      const clauses = new Set(
        FILE_MENUS.filter((menu) => menus[menu].some((i) => i.command === command)).map((m) => when(m, command)),
      );
      assert.equal(clauses.size, 1, [...clauses].join('\n'));
    });
  }

  describe('Definir como topo', () => {
    const clause = when('explorer/context', 'lace.setTop');

    it('esconde o nome de testbench pela regra do Core', () => {
      const allowed = regexFor(clause, 'resourceFilename');
      for (const name of NAMES) {
        assert.equal(allowed.test(name), !isTestbenchName(name), name);
      }
    });

    it('só aceita .v e .sv, com a caixa que o Core aceita', () => {
      const extension = regexFor(clause, 'resourceExtname');
      for (const name of ['a.v', 'a.sv', 'a.V', 'a.SV', 'a.vh', 'a.svh', 'a.py', 'a.vhd']) {
        const ext = name.slice(name.lastIndexOf('.'));
        assert.equal(extension.test(ext), isVerilogFile(name), name);
      }
    });

    it('esconde o topo e os testbenches registrados, e pede projeto e versão', () => {
      assert.match(clause, /resourcePath not in lace\.topFiles/);
      assert.match(clause, /resourcePath not in lace\.testbenchFiles/);
      assert.match(clause, /lace\.feature\.setTop && lace\.projectActive/);
    });

    it('na vista, aparece só no item marcado com .settop', () => {
      const viewItem = regexFor(when('view/item/context', 'lace.setTop', 'lace@1'), 'viewItem');
      const matched = CONTEXT_VALUES.filter((v) => viewItem.test(v));
      assert.deepEqual(matched, CONTEXT_VALUES.filter((v) => v.split('.').includes('settop')));
      assert.ok(matched.length > 0);
    });
  });

  it('Verificar este arquivo: na vista, só design e testbench em Verilog', () => {
    const viewItem = regexFor(when('view/item/context', 'lace.checkFile'), 'viewItem');
    for (const value of [...CONTEXT_VALUES, 'lace.testbench', 'lace.testbench.selected']) {
      const expected = /^lace\.(design|testbench)\./.test(value) && value.endsWith('.verilog');
      assert.equal(viewItem.test(value), expected, value);
    }
  });

  it('Adicionar ao projeto: só .v e .sv que ainda não estão no projeto', () => {
    const clause = when('explorer/context', 'lace.add');
    assert.match(clause, /resourcePath not in lace\.registeredFiles/);
    const extension = regexFor(clause, 'resourceExtname');
    assert.ok(extension.test('.v') && extension.test('.sv') && !extension.test('.py'));
    const viewItem = regexFor(when('view/item/context', 'lace.add', 'lace@1'), 'viewItem');
    assert.deepEqual(
      CONTEXT_VALUES.filter((v) => viewItem.test(v)),
      CONTEXT_VALUES.filter((v) => v.startsWith('lace.unregistered')),
    );
  });

  it('Abrir no surfer-aurora: os formatos que o lace wave abre', () => {
    const extension = regexFor(when('explorer/context', 'lace.waveFile'), 'resourceExtname');
    for (const ext of ['.vcd', '.fst', '.ghw', '.VCD']) {
      assert.ok(extension.test(ext), ext);
    }
    assert.ok(!extension.test('.v'));
  });

  it('os botões de build, check, sim, wave e synth ficam fora da vista (estão na barra de status)', () => {
    const functions = ['lace.build', 'lace.check', 'lace.checkLint', 'lace.sim', 'lace.simFast', 'lace.wave', 'lace.synth'];
    assert.deepEqual(
      menus['view/title'].filter((i) => functions.includes(i.command)),
      [],
    );
    const inline = menus['view/item/context'].filter((i) => i.group === 'inline').map((i) => i.command);
    assert.deepEqual(
      inline.filter((c) => /^lace\.(build|check|sim|wave|synth)/.test(c)),
      [],
    );
  });

  it('Simular este testbench: só num testbench registrado', () => {
    assert.match(when('explorer/context', 'lace.simTestbench'), /resourcePath in lace\.testbenchFiles/);
    const viewItem = regexFor(when('view/item/context', 'lace.simTestbench'), 'viewItem');
    assert.deepEqual(
      CONTEXT_VALUES.filter((v) => viewItem.test(v)),
      CONTEXT_VALUES.filter((v) => v.startsWith('lace.testbench')),
    );
  });

  describe('a ordem de compilação', () => {
    // Os contextValues do status gravado e os de listas com um item só e
    // sem .verilog (um testbench cocotb no meio da lista).
    const values = [...CONTEXT_VALUES, 'lace.testbench', 'lace.testbench.first.last', 'lace.design.first.last.verilog'];
    const parts = (v: string) => v.split('.');
    const listed = (v: string) => /^lace\.(design|testbench)(\.|$)/.test(v);

    for (const [command, edge] of [
      ['lace.moveUp', 'first'],
      ['lace.moveFirst', 'first'],
      ['lace.moveDown', 'last'],
      ['lace.moveLast', 'last'],
    ]) {
      it(`${command}: no RTL e nos testbenches, menos no ${edge === 'first' ? 'primeiro' : 'último'}`, () => {
        const viewItem = regexFor(when('view/item/context', command), 'viewItem');
        for (const value of values) {
          assert.equal(viewItem.test(value), listed(value) && !parts(value).includes(edge), value);
        }
      });
    }
  });

  it('Tirar do projeto: só os registrados', () => {
    assert.match(when('explorer/context', 'lace.remove'), /resourcePath in lace\.registeredFiles/);
    const viewItem = regexFor(when('view/item/context', 'lace.remove'), 'viewItem');
    assert.deepEqual(
      CONTEXT_VALUES.filter((v) => viewItem.test(v)),
      CONTEXT_VALUES.filter((v) => /^lace\.(design|testbench)\./.test(v)),
    );
  });

  it('todo comando dos menus está declarado', () => {
    const declared = new Set((manifest.contributes.commands as { command: string }[]).map((c) => c.command));
    for (const [menu, items] of Object.entries(menus)) {
      for (const item of items) {
        assert.ok(declared.has(item.command), `${menu}: ${item.command}`);
      }
    }
  });
});
