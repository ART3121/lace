// O build do viewer para fora do Studio, em dist/:
//
//   rtl-viewer.js          o viewer num arquivo só (React e o worker do ELK
//                          dentro), com `LaceRtlViewer.mount(elemento, opções)`
//   rtl-viewer.css         o CSS do viewer
//   theme-vscode.css       as variáveis ligadas ao tema do VS Code
//   theme-standalone.css   as variáveis com as cores do Studio, claro ou escuro
//   rtl-viewer.html        o modelo do HTML autocontido do `lace synth --open`:
//                          tudo acima embutido, e dois lugares que o Core
//                          preenche, @@LACE_NETLIST@@ (o hierarchy.json) e
//                          @@LACE_CONFIG@@ (o caminho e a data dele)
//
// O Studio não usa nada disto: compila o fonte direto (studio/vite.config.ts).
//
// Uso: npm run build

import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';

import { build } from 'esbuild';

const root = join(import.meta.dirname, '..');
const dist = join(root, 'dist');
mkdirSync(dist, { recursive: true });

const require = createRequire(import.meta.url);
const elkWorker = readFileSync(require.resolve('elkjs/lib/elk-worker.min.js'), 'utf8');

await build({
  entryPoints: [join(root, 'src', 'standalone.tsx')],
  outfile: join(dist, 'rtl-viewer.js'),
  bundle: true,
  format: 'iife',
  globalName: 'LaceRtlViewer',
  platform: 'browser',
  target: 'es2022',
  jsx: 'automatic',
  minify: true,
  legalComments: 'none',
  define: {
    __ELK_WORKER__: JSON.stringify(elkWorker),
    'process.env.NODE_ENV': '"production"',
  },
  logLevel: 'warning',
});

for (const file of ['rtl-viewer.css', 'theme-vscode.css', 'theme-standalone.css']) {
  copyFileSync(join(root, 'src', file), join(dist, file));
}

// Dentro de <script>, um "</script" no meio do código fecharia o elemento.
const inline = (code) => code.replaceAll('</script', '<\\/script');
const js = readFileSync(join(dist, 'rtl-viewer.js'), 'utf8');
const css = readFileSync(join(dist, 'rtl-viewer.css'), 'utf8') + readFileSync(join(dist, 'theme-standalone.css'), 'utf8');

const html = `<!doctype html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="Lace RTL viewer">
<title>RTL viewer</title>
<style>
${css}
#lace-rtl-viewer { height: 100vh; }
</style>
</head>
<body>
<div id="lace-rtl-viewer"></div>
<script id="lace-netlist" type="application/json">@@LACE_NETLIST@@</script>
<script id="lace-config" type="application/json">@@LACE_CONFIG@@</script>
<script>
${inline(js)}
</script>
<script>
(() => {
  const config = JSON.parse(document.getElementById('lace-config').textContent);
  if (config.title) document.title = config.title;
  LaceRtlViewer.mount(document.getElementById('lace-rtl-viewer'), {
    netlist: { path: config.path, text: document.getElementById('lace-netlist').textContent, modifiedMs: config.modifiedMs },
    storageKey: 'lace-rtl-viewer',
  });
})();
</script>
</body>
</html>
`;
writeFileSync(join(dist, 'rtl-viewer.html'), html);

const kb = (file) => `${Math.round(readFileSync(join(dist, file)).length / 1024)} KB`;
console.log(`dist/rtl-viewer.js ${kb('rtl-viewer.js')}, dist/rtl-viewer.html ${kb('rtl-viewer.html')}`);
