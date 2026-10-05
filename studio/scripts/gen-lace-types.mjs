// Gera src/ipc/lace-types.ts a partir dos JSON Schemas do Lace
// (lace/docs/schema/*.json), que o próprio Lace gera dos tipos do Core
// (ADR 0008 do Lace). Assim a interface usa o mesmo contrato da CLI.
//
// Uso: npm run gen:types            (o Lace deste repositório, em ..)
//      LACE_REPO=/outro/lace npm run gen:types
//
// Rode de novo sempre que o Lace mudar um tipo público e confira o diff.

import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { compile } from 'json-schema-to-typescript';

const repo = resolve(process.env.LACE_REPO ?? join(import.meta.dirname, '..', '..'));
const dir = join(repo, 'docs', 'schema');
const out = join(import.meta.dirname, '..', 'src', 'ipc', 'lace-types.ts');

const defs = {};
const roots = {};
for (const file of readdirSync(dir).filter((f) => f.endsWith('.json')).sort()) {
  const schema = JSON.parse(readFileSync(join(dir, file), 'utf8'));
  for (const [name, def] of Object.entries(schema.$defs ?? {})) {
    defs[name] ??= def;
  }
  const { $defs, $schema, ...root } = schema;
  // O tipo raiz de cada comando ganha o nome do título do schema
  // (SimReport, StatusReport, RunComparison).
  const name = (root.title ?? file.replace('.json', '')).replace(/[^A-Za-z0-9]/g, '');
  roots[name] = root;
}
// O schema das linhas do --events descreve a linha final com o resultado;
// o Studio só usa o Event do Core, que já está em $defs.
delete roots.EventLine;

const combined = {
  title: 'LaceSchemas',
  type: 'object',
  additionalProperties: false,
  properties: {},
  $defs: { ...defs, ...roots },
};

let ts = await compile(combined, 'LaceSchemas', {
  bannerComment: '',
  unreachableDefinitions: true,
  additionalProperties: false,
  strictIndexSignatures: true,
  format: true,
  style: { singleQuote: true, printWidth: 100 },
});
ts = ts.replace(/export interface LaceSchemas \{\}\n?/, '');

const header = `// GERADO por scripts/gen-lace-types.mjs a partir de lace/docs/schema/.
// Não edite à mão: rode \`npm run gen:types\`.
//
// São os tipos do Core do Lace serializados (o mesmo JSON do \`--json\` da
// CLI). Os nomes seguem os tipos Rust: BuildResult, SimulationResult,
// Diagnostic, Event, RunComparison.

/* eslint-disable */
`;
writeFileSync(out, header + ts);
console.log(`Wrote ${out} (${Object.keys(defs).length} definitions, ${Object.keys(roots).length} command types)`);
