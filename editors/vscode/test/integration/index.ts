// O ponto de entrada que o VS Code de teste chama: roda, dentro dele, os
// *.test.js desta pasta com o mocha.
//
// O mocha fica na 11, que é CommonJS: o 12 faz require() de módulo ESM, que
// o Node do Electron do VS Code 1.90 (o mínimo de engines) não aceita. As
// duas dependências dele com falha conhecida vão forçadas para as versões
// corrigidas (overrides, no package.json).

import { readdirSync } from 'node:fs';
import { join } from 'node:path';

import Mocha from 'mocha';

export async function run(): Promise<void> {
  const mocha = new Mocha({ ui: 'bdd', timeout: 30000, color: true });
  // Os testes da pasta sem .spf ficam em vazia/; os do projeto, aqui.
  const dir = process.env.LACE_TEST_SUITE === 'vazia' ? join(__dirname, 'vazia') : __dirname;
  for (const file of readdirSync(dir).filter((f) => f.endsWith('.test.js'))) {
    mocha.addFile(join(dir, file));
  }
  await mocha.loadFilesAsync();
  await new Promise<void>((resolve, reject) => {
    mocha.run((failures) => (failures > 0 ? reject(new Error(`${failures} teste(s) falharam`)) : resolve()));
  });
}
