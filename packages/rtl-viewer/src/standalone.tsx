// O viewer fora do Studio, num arquivo só (dist/rtl-viewer.js, com o React
// e o worker do ELK dentro): `LaceRtlViewer.mount(elemento, opções)`. É o
// que a extensão do VS Code carrega no webview e o que vai dentro do HTML do
// `lace synth --open`. Lá não há servidor, então o worker do layout sai do
// código embutido (__ELK_WORKER__, que o scripts/build.mjs põe aqui).

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { setLayoutWorker, workerFromSource } from './engine';
import type { Lang } from './i18n';
import { RtlViewer } from './RtlViewer';
import { createRtlViewerStore } from './store';
import type { SourceRef } from './yosys';

declare const __ELK_WORKER__: string;

export interface MountOptions {
  lang?: Lang;
  /** O netlist para mostrar já ao montar. */
  netlist?: { path: string; text: string; modifiedMs?: number };
  onOpenSource?: (src: SourceRef) => void;
  onExport?: (fileName: string, svg: string) => void;
  onResynthesize?: () => void;
  /** Guarda as preferências no localStorage com esta chave. */
  storageKey?: string;
}

export interface MountedViewer {
  /** Mostra um netlist (o texto do `hierarchy.json`); um igual ao atual mantém o lugar na hierarquia. */
  show(path: string, text: string, modifiedMs?: number): void;
  /** Uma síntese começou ou terminou: o botão de sintetizar gira e fica desligado. */
  setResynthesizing(running: boolean): void;
  setLang(lang: Lang): void;
  unmount(): void;
}

let workerReady = false;

export function mount(element: HTMLElement, options: MountOptions = {}): MountedViewer {
  if (!workerReady) {
    setLayoutWorker(workerFromSource(__ELK_WORKER__));
    workerReady = true;
  }
  const store = createRtlViewerStore({ storageKey: options.storageKey });
  const root = createRoot(element);
  let lang: Lang = options.lang ?? (navigator.language.toLowerCase().startsWith('pt') ? 'pt' : 'en');
  let resynthesizing = false;

  const render = () =>
    root.render(
      <StrictMode>
        <RtlViewer
          store={store}
          lang={lang}
          onOpenSource={options.onOpenSource}
          onExport={options.onExport}
          onResynthesize={options.onResynthesize}
          resynthesizing={resynthesizing}
          resynthesizeDisabled={resynthesizing}
        />
      </StrictMode>,
    );

  render();
  if (options.netlist) {
    store.getState().show(options.netlist.path, options.netlist.text, options.netlist.modifiedMs ?? Date.now());
  }

  return {
    show: (path, text, modifiedMs) => store.getState().show(path, text, modifiedMs ?? Date.now()),
    setResynthesizing: (running) => {
      resynthesizing = running;
      render();
    },
    setLang: (next) => {
      lang = next;
      render();
    },
    unmount: () => root.unmount(),
  };
}
