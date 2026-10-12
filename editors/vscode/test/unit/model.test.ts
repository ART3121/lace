// A vista de fontes a partir do `lace status --json` gravado.

import * as assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import type { StatusReport } from '../../src/lace/lace-types';
import { formatLaceError } from '../../src/messages';
import { decorationFor, sourceGroups, type SourceEntry } from '../../src/sources/model';
import { RESPONSES } from '../support/fixtures';

const ROOT = join('/', 'projetos', 'projeto demo');

function status(file: string): StatusReport {
  const text = readFileSync(join(RESPONSES, file), 'utf8');
  return JSON.parse(text.replaceAll('${ROOT}', JSON.stringify(ROOT).slice(1, -1)));
}

function byLabel(entries: SourceEntry[]): Record<string, Pick<SourceEntry, 'description' | 'contextValue'>> {
  return Object.fromEntries(
    entries.map((e) => [e.label, { description: e.description, contextValue: e.contextValue }]),
  );
}

function entry(file: string, label: string): SourceEntry {
  const found = sourceGroups(status(file))
    .flatMap((g) => g.entries)
    .find((e) => e.label === label);
  assert.ok(found, label);
  return found;
}

describe('vista de fontes', () => {
  it('os grupos, com o design na ordem do .spf', () => {
    const groups = sourceGroups(status('status.json'));
    assert.deepEqual(
      groups.map((g) => g.label),
      ['RTL', 'Testbenches', 'Processadores', 'Não registrados'],
    );
    // O build do soma registrou o .v e o testbench que ele gera.
    assert.deepEqual(
      groups[0].entries.map((e) => e.label),
      ['contador.v', 'somador.v', 'mux.sv', 'soma.v'],
    );
    assert.deepEqual(
      groups[1].entries.map((e) => e.label),
      ['contador_tb.v', 'somador_tb.v', 'soma_tb.v'],
    );
  });

  it('a descrição em maiúsculas e o contextValue de cada papel', () => {
    const [design, testbenches, processors, unregistered] = sourceGroups(status('status.json'));
    assert.deepEqual(byLabel(design.entries), {
      'contador.v': { description: 'TOPO · rtl', contextValue: 'lace.design.top.first.verilog' },
      'somador.v': { description: 'rtl', contextValue: 'lace.design.settop.verilog' },
      'mux.sv': { description: 'rtl', contextValue: 'lace.design.settop.verilog' },
      'soma.v': { description: 'soma/Hardware', contextValue: 'lace.design.settop.last.verilog' },
    });
    assert.deepEqual(byLabel(testbenches.entries), {
      'contador_tb.v': { description: 'SIMULADO · tb', contextValue: 'lace.testbench.selected.first.verilog' },
      'somador_tb.v': { description: 'tb', contextValue: 'lace.testbench.verilog' },
      'soma_tb.v': { description: 'soma/Simulation', contextValue: 'lace.testbench.last.verilog' },
    });
    assert.deepEqual(byLabel(processors.entries), {
      soma: { description: 'COMPILADO', contextValue: 'lace.processor.built' },
      filtro: { description: 'NÃO COMPILADO', contextValue: 'lace.processor' },
    });
    // velho_tb.v tem nome de testbench: lace top o recusaria.
    assert.deepEqual(byLabel(unregistered.entries), {
      'sobra.v': { description: 'rtl', contextValue: 'lace.unregistered.settop.verilog' },
      'velho_tb.v': { description: 'rtl', contextValue: 'lace.unregistered.verilog' },
    });
  });

  it('a ordem do .spf: o primeiro não sobe, o último não desce', () => {
    const order = (file: string) => sourceGroups(status(file))[0].entries.map((e) => e.label);
    assert.deepEqual(order('status-order-somador.json'), ['somador.v', 'contador.v', 'mux.sv', 'soma.v']);
    assert.match(entry('status-order-somador.json', 'somador.v').contextValue, /\.first\./);
    assert.doesNotMatch(entry('status-order-somador.json', 'contador.v').contextValue, /\.(first|last)\./);
  });

  it('o tooltip do selo de um processador fala do build', () => {
    assert.equal(decorationFor(entry('status.json', 'soma'), { errors: 1, warnings: 0 })?.tooltip, 'Último build: 1 erro, 0 avisos');
  });

  it('o clique do processador abre o programa', () => {
    assert.ok(entry('status.json', 'soma').path?.endsWith('soma.cmm'));
  });

  it('acompanha a troca do topo e o arquivo que entrou no projeto', () => {
    assert.equal(entry('status-top-somador.json', 'somador.v').contextValue, 'lace.design.top.verilog');
    assert.equal(entry('status-top-somador.json', 'contador.v').contextValue, 'lace.design.settop.first.verilog');
    assert.equal(entry('status-added.json', 'sobra.v').role, 'design');
  });

  it('um testbench cocotb não leva .verilog (o check de arquivo o recusa)', () => {
    const report = status('status.json');
    report.testbench.push({ role: 'testbench', path: `${ROOT}/tb/test_alu.py`, top_level: false });
    assert.equal(byLabel(sourceGroups(report)[1].entries)['test_alu.py'].contextValue, 'lace.testbench.last');
  });

  it('os avisos do .spf vêm primeiro', () => {
    const report = status('status.json');
    report.issues.push({
      kind: 'duplicate_file',
      path: `${ROOT}/rtl/somador.v`,
      detail: null,
      message: 'rtl/somador.v is listed twice',
    });
    const [issues] = sourceGroups(report);
    assert.equal(issues.label, 'Avisos do projeto');
    assert.equal(issues.entries[0].label, 'rtl/somador.v is listed twice');
    assert.equal(issues.entries[0].tone, 'issue');
  });

  it('não mostra grupo vazio', () => {
    const report = { ...status('status.json'), processors: [], unregistered: [] };
    assert.deepEqual(
      sourceGroups(report).map((g) => g.role),
      ['design', 'testbench'],
    );
  });
});

describe('o destaque de cada item', () => {
  it('cor e selo pelo papel', () => {
    assert.deepEqual(decorationFor(entry('status.json', 'contador.v')), {
      badge: 'T',
      color: 'lace.topForeground',
      tooltip: undefined,
    });
    assert.equal(decorationFor(entry('status.json', 'contador_tb.v'))?.badge, 'S');
    assert.equal(decorationFor(entry('status.json', 'soma'))?.badge, 'C');
    assert.deepEqual(decorationFor(entry('status.json', 'filtro')), {
      badge: undefined,
      color: 'lace.notBuiltForeground',
      tooltip: undefined,
    });
    assert.equal(decorationFor(entry('status.json', 'sobra.v'))?.badge, 'N');
    assert.equal(decorationFor(entry('status.json', 'somador.v')), undefined);
  });

  it('um erro do check passa na frente do papel', () => {
    assert.deepEqual(decorationFor(entry('status.json', 'contador.v'), { errors: 12, warnings: 1 }), {
      badge: '9+',
      color: 'list.errorForeground',
      tooltip: 'Último check: 12 erros, 1 aviso',
    });
  });

  it('avisos só no selo de quem não tem outro', () => {
    assert.equal(decorationFor(entry('status.json', 'contador.v'), { errors: 0, warnings: 2 })?.badge, 'T');
    assert.deepEqual(decorationFor(entry('status.json', 'somador.v'), { errors: 0, warnings: 2 }), {
      badge: '2',
      color: 'list.warningForeground',
      tooltip: 'Último check: 0 erros, 2 avisos',
    });
  });
});

describe('formatLaceError', () => {
  it('põe a dica depois da mensagem', () => {
    assert.equal(
      formatLaceError({ code: 'project_not_found', message: 'No Lace project here', hint: 'Create one with: lace new <name>' }),
      'No Lace project here. Create one with: lace new <name>',
    );
    assert.equal(formatLaceError({ code: 'invalid_name', message: 'Invalid name', hint: null }), 'Invalid name');
  });
});
