// Empacota a extensão num arquivo só, dist/extension.js, que é o que o
// VS Code carrega, e copia para media/rtl-viewer/ o build do RTL viewer
// (packages/rtl-viewer/dist, que o `npm run build` gera antes). O esbuild não
// confere tipos: isso é o `npm run typecheck`.
//
// Uso: node esbuild.mjs                (com source map, para depurar)
//      node esbuild.mjs --production   (minificado, o do .vsix)
//      node esbuild.mjs --watch

import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

import { build, context } from 'esbuild';

const viewerDist = join(import.meta.dirname, '..', '..', 'packages', 'rtl-viewer', 'dist');
const viewerMedia = join(import.meta.dirname, 'media', 'rtl-viewer');
if (!existsSync(join(viewerDist, 'rtl-viewer.js'))) {
  console.error(`Falta o build do RTL viewer em ${viewerDist}: rode npm ci e npm run build em packages/rtl-viewer.`);
  process.exit(1);
}
mkdirSync(viewerMedia, { recursive: true });
for (const file of ['rtl-viewer.js', 'rtl-viewer.css', 'theme-vscode.css']) {
  copyFileSync(join(viewerDist, file), join(viewerMedia, file));
}

const production = process.argv.includes('--production');
const watch = process.argv.includes('--watch');

const options = {
  entryPoints: ['src/extension.ts'],
  outfile: 'dist/extension.js',
  bundle: true,
  // O módulo vscode é dado pelo VS Code em tempo de execução.
  external: ['vscode'],
  format: 'cjs',
  platform: 'node',
  // O Node do VS Code 1.90 (Electron 29), o mínimo de engines.vscode.
  target: 'node20',
  sourcemap: !production,
  minify: production,
  logLevel: 'info',
};

if (watch) {
  const ctx = await context(options);
  await ctx.watch();
} else {
  await build(options);
}
