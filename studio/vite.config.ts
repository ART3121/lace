import { fileURLToPath } from 'node:url';

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// O Tauri abre a interface em http://localhost:1420 no desenvolvimento
// (src-tauri/tauri.conf.json, build.devUrl). Porta fixa: se estiver em uso,
// o Vite falha em vez de escolher outra que o Tauri não conhece.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  resolve: {
    alias: [
      // O monaco-vim declara primeiro, para navegador, um UMD que chama
      // `require('monaco-editor/...')`, e o Vite escolhe esse; no navegador
      // não há `require` e a interface não abre. A versão ESM serve.
      {
        find: /^monaco-vim$/,
        replacement: fileURLToPath(new URL('./node_modules/monaco-vim/dist/index.mjs', import.meta.url)),
      },
      // E ela importa `monaco-editor/esm/vs/...`, caminho que o Monaco 0.57
      // não exporta mais (o `exports` dele mapeia `monaco-editor/<x>` para
      // `esm/vs/<x>.js`). Redirecionado, é o mesmo Monaco do resto do Studio.
      { find: /^monaco-editor\/esm\/vs\/(.*)$/, replacement: 'monaco-editor/$1' },
    ],
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    target: 'es2022',
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // O Monaco sozinho passa de 3 MB; num aplicativo de desktop isso não
    // pesa no carregamento como pesaria na web.
    chunkSizeWarningLimit: 8000,
  },
  worker: { format: 'es' },
});
