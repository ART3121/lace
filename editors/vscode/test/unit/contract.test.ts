// O contrato com a AURORA: as respostas que o aurora falso devolve são as que o
// aurora de verdade deu, e têm de continuar valendo contra docs/schema/. Se o
// schema muda, este teste quebra, e a correção é gravar de novo
// (npm run record:fixtures), gerar os tipos (npm run gen:types) e conferir
// o que a extensão usa do que mudou.

import * as assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

import Ajv2020, { type ValidateFunction } from 'ajv/dist/2020';

import { EXTENSION_ROOT, AURORA_REPO, recordedCases, RESPONSES } from '../support/fixtures';

const SCHEMAS = join(AURORA_REPO, 'docs', 'schema');

// Os formatos que o schemars põe nos inteiros. Um formato novo nos schemas
// faz o Ajv recusar o schema, e o teste diz qual.
const INTEGER_FORMATS: Record<string, [number, number]> = {
  uint8: [0, 2 ** 8 - 1],
  uint16: [0, 2 ** 16 - 1],
  uint32: [0, 2 ** 32 - 1],
  uint64: [0, Number.MAX_SAFE_INTEGER],
  uint: [0, Number.MAX_SAFE_INTEGER],
  int32: [-(2 ** 31), 2 ** 31 - 1],
  int64: [Number.MIN_SAFE_INTEGER, Number.MAX_SAFE_INTEGER],
};

const ajv = new Ajv2020({ allErrors: true, strict: true, allowUnionTypes: true });
for (const [name, [min, max]] of Object.entries(INTEGER_FORMATS)) {
  ajv.addFormat(name, { type: 'number', validate: (n: number) => Number.isInteger(n) && n >= min && n <= max });
}
for (const name of ['double', 'float']) {
  ajv.addFormat(name, { type: 'number', validate: () => true });
}

const validators = new Map<string, ValidateFunction>();

/**
 * O validador do schema, mais estrito que ele: um objeto com `properties`
 * não aceita campo fora delas. Assim, um campo que a AURORA renomeia ou tira
 * quebra o teste, em vez de passar calado.
 */
function validator(file: string): ValidateFunction {
  let validate = validators.get(file);
  if (!validate) {
    const schema = closeObjects(JSON.parse(readFileSync(join(SCHEMAS, file), 'utf8')));
    validate = ajv.compile(schema as object);
    validators.set(file, validate);
  }
  return validate;
}

function closeObjects(node: unknown): unknown {
  if (Array.isArray(node)) {
    return node.map(closeObjects);
  }
  if (typeof node !== 'object' || node === null) {
    return node;
  }
  const copy: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(node)) {
    // `properties` é um mapa de nomes: os nomes não são schemas.
    copy[key] =
      key === 'properties'
        ? Object.fromEntries(Object.entries(value as object).map(([k, v]) => [k, closeObjects(v)]))
        : closeObjects(value);
  }
  if ('properties' in copy && !('additionalProperties' in copy)) {
    copy.additionalProperties = false;
  }
  return copy;
}

function assertValid(schema: string, value: unknown, where: string): void {
  const validate = validator(schema);
  if (!validate(value)) {
    assert.fail(`${where} não confere com ${schema}:\n${ajv.errorsText(validate.errors, { separator: '\n' })}`);
  }
}

describe('contrato com docs/schema/', () => {
  const { cases } = recordedCases();

  it('toda resposta gravada tem os arquivos e um schema que existe', () => {
    assert.ok(existsSync(SCHEMAS), `sem ${SCHEMAS}: defina AURORA_REPO`);
    for (const c of cases) {
      for (const file of [c.stdout, c.stderr].filter((f): f is string => f !== undefined)) {
        assert.ok(existsSync(join(RESPONSES, file)), `falta ${file}`);
      }
      for (const schema of [c.schema, c.resultSchema].filter((s): s is string => s !== undefined)) {
        assert.ok(existsSync(join(SCHEMAS, schema)), `falta o schema ${schema}`);
      }
    }
  });

  for (const c of cases.filter((c) => c.schema !== undefined)) {
    const name = `${c.args.join(' ')} (código ${c.exit})`;
    const text = readFileSync(join(RESPONSES, c.stdout ?? ''), 'utf8');

    if (c.schema === 'events.json') {
      it(`--events: ${name}`, () => {
        const lines = text.split('\n').filter((line) => line !== '');
        lines.forEach((line, i) => assertValid('events.json', JSON.parse(line), `${c.stdout}:${i + 1}`));
        const last = JSON.parse(lines.at(-1) ?? '{}');
        assert.equal(last.event, 'result', 'a última linha do --events é o resultado');
        assert.ok(c.resultSchema, 'falta o resultSchema');
        assertValid(c.resultSchema, last.result, `${c.stdout}, linha result`);
      });
    } else {
      it(`--json: ${name}`, () => {
        assert.equal(c.exit === 2, c.schema === 'error.json', 'o código 2, e só ele, escreve o error.json');
        assertValid(c.schema ?? '', JSON.parse(text), c.stdout ?? '');
      });
    }
  }

  it('src/aurora/aurora-types.ts é o que docs/schema/ gera', function () {
    this.timeout(60000);
    const result = spawnSync(process.execPath, [join(EXTENSION_ROOT, 'scripts', 'gen-aurora-types.mjs'), '--check'], {
      env: { ...process.env, AURORA_REPO },
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr);
  });
});
