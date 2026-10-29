// O RTL viewer do Lace numa aba do VS Code: o bundle de packages/rtl-viewer
// (media/rtl-viewer, copiado pelo esbuild.mjs) num webview, com as cores do
// tema do editor (theme-vscode.css). A extensão lê o netlist que o
// `aurora synth` gravou (SynthesisResult.netlist) e o manda ao webview; se ele
// muda no disco (um `aurora synth` no terminal), quem confere (o ChangePoller
// da extension.ts) chama reload e o desenho acompanha.
//
// O webview avisa quando montou (`ready`) e quando o primeiro desenho
// apareceu (`drawn`): é o que diz que o worker do layout rodou lá dentro.

import { randomBytes } from 'node:crypto';
import { join } from 'node:path';

import * as vscode from 'vscode';

/** O que o webview manda. */
type FromViewer =
  | { type: 'ready' }
  | { type: 'drawn' }
  | { type: 'openSource'; src: { file: string; line: number; column: number } }
  | { type: 'export'; fileName: string; svg: string }
  | { type: 'resynthesize' };

export interface RtlViewerHost {
  /** Sintetiza de novo o que está aberto (o botão do viewer). */
  resynthesize(target: RtlTarget): void;
  /** A pasta para onde a exportação do SVG aponta primeiro. */
  projectRoot(): string | undefined;
}

/** De onde veio o netlist: o topo do projeto ou um processador. */
export type RtlTarget = { kind: 'project' } | { kind: 'processor'; name: string };

export class RtlViewerPanel implements vscode.Disposable {
  private panel: vscode.WebviewPanel | undefined;
  private netlist: string | undefined;
  private target: RtlTarget = { kind: 'project' };
  private ready = false;
  private resynthesizing = false;
  private readonly drawn = new vscode.EventEmitter<void>();
  /** O primeiro desenho apareceu no webview. */
  readonly onDidDraw = this.drawn.event;

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly host: RtlViewerHost,
    private readonly log: vscode.LogOutputChannel,
  ) {}

  /** O netlist aberto, ou `undefined` sem aba. */
  get current(): string | undefined {
    return this.panel ? this.netlist : undefined;
  }

  /** Abre (ou traz para a frente) a aba com este netlist. */
  async show(netlist: string, target: RtlTarget): Promise<void> {
    this.target = target;
    this.netlist = netlist;
    if (!this.panel) {
      this.panel = this.createPanel();
    } else {
      this.panel.reveal(undefined, false);
    }
    this.panel.title = this.title();
    if (this.ready) {
      await this.post();
    }
  }

  /** Uma síntese começou ou terminou: o botão do viewer gira e fica desligado. */
  setResynthesizing(running: boolean): void {
    this.resynthesizing = running;
    void this.panel?.webview.postMessage({ type: 'busy', running });
  }

  private createPanel(): vscode.WebviewPanel {
    const media = vscode.Uri.joinPath(this.extensionUri, 'media', 'rtl-viewer');
    const panel = vscode.window.createWebviewPanel('aurora.rtlViewer', this.title(), vscode.ViewColumn.Active, {
      enableScripts: true,
      // O estado do desenho (onde está na hierarquia, o zoom) fica quando a
      // aba vai para trás.
      retainContextWhenHidden: true,
      localResourceRoots: [media],
    });
    panel.webview.html = this.html(panel.webview, media);
    panel.webview.onDidReceiveMessage((message: FromViewer) => void this.receive(message));
    panel.onDidDispose(() => {
      this.panel = undefined;
      this.ready = false;
      this.netlist = undefined;
    });
    return panel;
  }

  private async receive(message: FromViewer): Promise<void> {
    switch (message.type) {
      case 'ready':
        this.ready = true;
        if (this.resynthesizing) {
          this.setResynthesizing(true);
        }
        await this.post();
        break;
      case 'drawn':
        this.log.info(`RTL viewer: ${this.netlist ?? ''} desenhado`);
        this.drawn.fire();
        break;
      case 'openSource': {
        const { file, line, column } = message.src;
        const position = new vscode.Position(Math.max(line - 1, 0), Math.max(column - 1, 0));
        await vscode.window.showTextDocument(vscode.Uri.file(file), {
          selection: new vscode.Range(position, position),
          viewColumn: vscode.ViewColumn.Beside,
        });
        break;
      }
      case 'export': {
        const root = this.host.projectRoot();
        const target = await vscode.window.showSaveDialog({
          defaultUri: vscode.Uri.file(root ? join(root, message.fileName) : message.fileName),
          filters: { SVG: ['svg'] },
        });
        if (target) {
          await vscode.workspace.fs.writeFile(target, new TextEncoder().encode(message.svg));
          vscode.window.setStatusBarMessage(`Lace: SVG exportado em ${target.fsPath}`, 6000);
        }
        break;
      }
      case 'resynthesize':
        this.host.resynthesize(this.target);
        break;
    }
  }

  /** Lê o netlist e o manda ao webview. */
  private async post(): Promise<void> {
    const path = this.netlist;
    if (!this.panel || !path) {
      return;
    }
    try {
      const uri = vscode.Uri.file(path);
      const [bytes, stat] = await Promise.all([vscode.workspace.fs.readFile(uri), vscode.workspace.fs.stat(uri)]);
      await this.panel.webview.postMessage({
        type: 'netlist',
        path,
        text: new TextDecoder().decode(bytes),
        modifiedMs: stat.mtime,
      });
    } catch (error) {
      this.log.error(`RTL viewer: não deu para ler ${path}: ${(error as Error).message}`);
      void vscode.window.showErrorMessage(`Lace: não deu para ler o netlist ${path}.`);
    }
  }

  /** O netlist aberto mudou no disco (um `aurora synth` de fora): o desenho acompanha. */
  async reload(): Promise<void> {
    if (this.ready) {
      await this.post();
    }
  }

  private title(): string {
    return this.target.kind === 'processor' ? `RTL: ${this.target.name}` : 'RTL viewer';
  }

  private html(webview: vscode.Webview, media: vscode.Uri): string {
    const nonce = randomBytes(16).toString('base64');
    const asset = (file: string) => webview.asWebviewUri(vscode.Uri.joinPath(media, file)).toString();
    const lang = vscode.env.language.toLowerCase().startsWith('pt') ? 'pt' : 'en';
    // O worker do layout sai de um blob (workerFromSource); o React põe
    // estilo nos elementos, daí o 'unsafe-inline' no style-src.
    const csp = [
      "default-src 'none'",
      `style-src ${webview.cspSource} 'unsafe-inline'`,
      `script-src 'nonce-${nonce}'`,
      'worker-src blob:',
      `img-src ${webview.cspSource} data: blob:`,
      `font-src ${webview.cspSource}`,
    ].join('; ');
    return `<!doctype html>
<html lang="${lang}">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<link rel="stylesheet" href="${asset('rtl-viewer.css')}">
<link rel="stylesheet" href="${asset('theme-vscode.css')}">
<style>#root { height: 100vh; overflow: hidden; }</style>
</head>
<body>
<div id="root"></div>
<script nonce="${nonce}" src="${asset('rtl-viewer.js')}"></script>
<script nonce="${nonce}">
(() => {
  const vscode = acquireVsCodeApi();
  const root = document.getElementById('root');
  const viewer = AuroraRtlViewer.mount(root, {
    lang: ${JSON.stringify(lang)},
    storageKey: 'aurora-vscode-rtl-viewer',
    onOpenSource: (src) => vscode.postMessage({ type: 'openSource', src }),
    onExport: (fileName, svg) => vscode.postMessage({ type: 'export', fileName, svg }),
    onResynthesize: () => vscode.postMessage({ type: 'resynthesize' }),
  });
  window.addEventListener('message', (event) => {
    const message = event.data;
    if (message.type === 'netlist') viewer.show(message.path, message.text, message.modifiedMs);
    else if (message.type === 'busy') viewer.setResynthesizing(message.running);
  });
  // O primeiro desenho na tela: o worker do layout rodou.
  const observer = new MutationObserver(() => {
    if (root.querySelector('.sch-node')) {
      observer.disconnect();
      vscode.postMessage({ type: 'drawn' });
    }
  });
  observer.observe(root, { childList: true, subtree: true });
  vscode.postMessage({ type: 'ready' });
})();
</script>
</body>
</html>`;
  }

  dispose(): void {
    this.panel?.dispose();
    this.drawn.dispose();
  }
}
