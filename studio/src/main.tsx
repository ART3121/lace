// Ponto de entrada da interface.

import '@fontsource-variable/inter';
import '@fontsource-variable/jetbrains-mono';
import '@xterm/xterm/css/xterm.css';
import './styles/tokens.css';
import './styles/app.css';

import { invoke } from '@tauri-apps/api/core';
import { createRoot } from 'react-dom/client';

import { App } from './App';

// Erros da interface vão para o log do backend (o terminal de onde o Studio
// foi aberto), além do console da WebView. Três avisos que não são erro
// ficam de fora: o "ResizeObserver loop" (o navegador adiando um
// redimensionamento), o "Canceled" das promessas que o Monaco cancela de
// propósito quando o editor perde o foco e a leitura da área de transferência
// que o WebKit nega.
function forward(level: 'error' | 'warn', message: string) {
  void invoke('log_frontend', { level, message }).catch(() => undefined);
}
function describe(value: unknown): string {
  if (value instanceof Error) return `${value.name}: ${value.message}${value.stack ? `\n${value.stack}` : ''}`;
  if (typeof value === 'string') return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}
// O Monaco tenta ler a área de transferência ao ganhar o foco, e o WebKitGTK
// nega sem um gesto do usuário (NotAllowedError); colar continua funcionando.
const isCanceled = (value: unknown) => {
  const error = value as { name?: string; message?: string } | null;
  return error?.name === 'Canceled' || error?.message === 'Canceled' || error?.name === 'NotAllowedError';
};
window.addEventListener('error', (e) => {
  if (e.message?.startsWith('ResizeObserver loop')) return;
  forward('error', `${e.message} (${e.filename}:${e.lineno})`);
});
window.addEventListener('unhandledrejection', (e) => {
  if (isCanceled(e.reason)) {
    e.preventDefault();
    return;
  }
  forward('error', `unhandled rejection: ${describe(e.reason)}`);
});
const consoleError = console.error.bind(console);
console.error = (...args: unknown[]) => {
  consoleError(...args);
  if (args.some(isCanceled)) return;
  forward('error', args.map(describe).join(' '));
};

// O menu de contexto do navegador não serve num aplicativo de desktop; os
// campos de texto e o editor mantêm o deles.
document.addEventListener('contextmenu', (event) => {
  const target = event.target as HTMLElement | null;
  if (target?.closest('input, textarea, .monaco-editor, .xterm')) return;
  event.preventDefault();
});

// Quanto a abertura levou, para o log: até os módulos carregarem (o
// JavaScript, o CSS e o Monaco) e até o primeiro quadro desenhado. O tempo
// conta da navegação da WebView.
const modulesMs = Math.round(performance.now());
createRoot(document.getElementById('root')!).render(<App />);
requestAnimationFrame(() =>
  requestAnimationFrame(() => {
    const frameMs = Math.round(performance.now());
    void invoke('log_frontend', {
      level: 'info',
      message: `Startup: modules loaded in ${modulesMs} ms, first frame at ${frameMs} ms`,
    }).catch(() => undefined);
  }),
);
