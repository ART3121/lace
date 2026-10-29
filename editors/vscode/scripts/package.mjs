// Empacota a extensão em aurora-<versão>.vsix, nesta pasta. O vsce exige um
// LICENSE ao lado do package.json: o da raiz do repositório é copiado para
// cá antes (e fica fora do git). O vsce roda o vscode:prepublish, que gera o
// dist/extension.js minificado.
//
// Uso: npm run package

import { spawnSync } from 'node:child_process';
import { copyFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');
copyFileSync(join(root, '..', '..', 'LICENSE'), join(root, 'LICENSE'));

// As dependências são só de desenvolvimento: o esbuild já pôs tudo no dist/.
const vsce = join(root, 'node_modules', '@vscode', 'vsce', 'vsce');
const result = spawnSync(process.execPath, [vsce, 'package', '--no-dependencies'], { cwd: root, stdio: 'inherit' });
process.exitCode = result.status ?? 1;
