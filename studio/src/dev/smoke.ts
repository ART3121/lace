// Roteiro de fumaça: roda uma sequência de ações pelo mesmo caminho dos
// botões, para conferir a interface de ponta a ponta sem mouse nem teclado.
// Só existe na build de desenvolvimento (App.tsx importa este módulo com
// import.meta.env.DEV, e o backend só devolve o roteiro em debug).
//
//   LACE_STUDIO_SMOKE="wait:2000;check;fastSim;panel:wave;synthesize" npm run tauri dev
//
// Passos, separados por ';':
//   <ação>         um id de actions.ts (check, fastSim, synthesize, build...)
//   wait:<ms>      espera
//   open:<caminho> abre um arquivo, relativo à pasta do projeto
//   aside:<caminho> abre um arquivo no grupo do editor à direita
//   view:<tipo>[:<nome>] abre uma vista (schematic, synthesis, processor:soma)
//   project:<.spf> abre outro projeto
//   add:<caminho>  registra um Verilog, como Projeto > Adicionar arquivos
//                  Verilog depois do diálogo (caminho absoluto, ou relativo à
//                  pasta do projeto)
//   panel:<aba>    mostra uma aba do painel inferior
//   target:<nome>  escolhe o alvo (vazio: o projeto)
//   sidebar:<vista> mostra uma vista da barra lateral
//   click:<seletor> clica no primeiro elemento que casa com o seletor CSS
//   focus:<seletor> põe o foco no elemento (para um key: depois)
//   key:<tecla>    manda a tecla (Escape, Enter) ao elemento com foco
//   explorer:<modo> sources, hierarchy ou files
//   editor         escreve no log os grupos do editor e as abas de cada um
//   rects:<seletor> escreve no log a posição e o tamanho de cada elemento
//                  que casa com o seletor
//   count:<seletor> escreve no log quantos elementos casam com o seletor
//   type:<seletor>|<texto> escreve num campo, como se fosse digitado
//   drag:<de>|<para> arrasta com eventos de ponteiro, do centro de um
//                  elemento ao centro do outro (seletores CSS)
//   osdrop:<seletor>|<caminho>[|<caminho>...] simula arquivos do sistema
//                  soltos no centro do elemento: emite o evento de arrastar
//                  do Tauri, o mesmo que o gerenciador de arquivos gera
//   menu:<seletor> abre o menu de contexto do elemento (o botão direito no
//                  centro dele) e escreve no log os itens, com `-` nos
//                  separadores e `(off)` nos desabilitados
//   choose:<rótulo> clica no item do menu aberto que tem esse rótulo
//
// Cada passo concluído vai para o log do backend como "smoke: <passo>", e o
// fim como "smoke: done".

import { invoke } from '@tauri-apps/api/core';
import { emitTo } from '@tauri-apps/api/event';

import { action, addVerilogFiles } from '../actions';
import { useDialogs } from '../state/dialogs';
import { useEditor, type ViewKind } from '../state/editor';
import { useLayout, type ExplorerMode, type PanelTab, type SidebarView } from '../state/layout';
import { useProject } from '../state/project';
import { resolveFrom } from '../util/paths';

function log(message: string) {
  return invoke('log_frontend', { level: 'info', message: `smoke: ${message}` });
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function element(selector: string): HTMLElement {
  const found = document.querySelector<HTMLElement>(selector);
  if (!found) throw new Error(`nothing matches ${selector}`);
  return found;
}

function center(el: HTMLElement): { x: number; y: number } {
  const r = el.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
}

export async function runSmokeScript(): Promise<void> {
  const script = await invoke<string | null>('dev_smoke_script');
  if (!script) return;
  for (const step of script.split(';').map((s) => s.trim()).filter(Boolean)) {
    const [kind, ...rest] = step.split(':');
    const arg = rest.join(':');
    try {
      switch (kind) {
        case 'wait':
          await sleep(Number(arg) || 1000);
          break;
        case 'open': {
          const root = useProject.getState().snapshot?.root;
          if (root) await useEditor.getState().openFile(resolveFrom(root, arg));
          break;
        }
        case 'aside': {
          const root = useProject.getState().snapshot?.root;
          if (root) await useEditor.getState().openFile(resolveFrom(root, arg), { side: true });
          break;
        }
        case 'view': {
          const [view, name] = arg.split(':');
          useEditor.getState().openView(view as ViewKind, name ? { name } : undefined);
          break;
        }
        case 'project':
          await useProject.getState().open(arg);
          break;
        case 'add': {
          const root = useProject.getState().snapshot?.root;
          if (root) await addVerilogFiles([resolveFrom(root, arg)]);
          break;
        }
        case 'panel':
          useLayout.getState().showPanel(arg as PanelTab);
          break;
        case 'sidebar':
          useLayout.setState({ sidebarView: arg as SidebarView, sidebarVisible: true });
          break;
        case 'target':
          useProject.getState().setTarget(arg || null);
          break;
        case 'click':
          element(arg).click();
          break;
        case 'focus':
          element(arg).focus();
          break;
        case 'editor': {
          const { groups, activeGroup } = useEditor.getState();
          // Só o nome do arquivo: o caminho inteiro não cabe numa linha de log.
          const short = (id: string) => id.replace(/^file:.*[\\/]/, '');
          const summary = groups
            .map((g) => `${g.id === activeGroup ? '*' : ''}${g.id}[${g.tabIds.map((id) => (id === g.activeId ? '>' : '') + short(id)).join(', ')}]`)
            .join(' ');
          await log(`editor ${summary}`);
          break;
        }
        case 'rects': {
          const found = [...document.querySelectorAll<HTMLElement>(arg)].map((el) => {
            const r = el.getBoundingClientRect();
            return `${el.className || el.tagName}@${Math.round(r.left)},${Math.round(r.top)} ${Math.round(r.width)}x${Math.round(r.height)}`;
          });
          await log(`rects ${arg}: ${found.join(' | ') || 'none'}`);
          break;
        }
        case 'count':
          await log(`count ${arg}: ${document.querySelectorAll(arg).length}`);
          break;
        case 'explorer':
          useLayout.setState({ sidebarView: 'explorer', sidebarVisible: true, explorerMode: arg as ExplorerMode });
          break;
        case 'type': {
          const [selector, text] = arg.split('|');
          const input = element(selector) as HTMLInputElement;
          input.focus();
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, text);
          input.dispatchEvent(new Event('input', { bubbles: true }));
          break;
        }
        case 'drag': {
          const [fromSelector, toSelector] = arg.split('|');
          const from = element(fromSelector);
          const a = center(from);
          const b = center(element(toSelector));
          const pointer = { bubbles: true, button: 0, pointerId: 1, isPrimary: true };
          from.dispatchEvent(new PointerEvent('pointerdown', { ...pointer, clientX: a.x, clientY: a.y }));
          for (let i = 1; i <= 6; i++) {
            const x = a.x + ((b.x - a.x) * i) / 6;
            const y = a.y + ((b.y - a.y) * i) / 6;
            window.dispatchEvent(new PointerEvent('pointermove', { ...pointer, clientX: x, clientY: y }));
            await sleep(40);
          }
          await sleep(300);
          window.dispatchEvent(new PointerEvent('pointerup', { ...pointer, clientX: b.x, clientY: b.y }));
          break;
        }
        case 'osdrop': {
          const [selector, ...paths] = arg.split('|');
          const { x, y } = center(element(selector));
          const ratio = window.devicePixelRatio || 1;
          const position = { x: x * ratio, y: y * ratio };
          const target = { kind: 'Webview' as const, label: 'main' };
          await emitTo(target, 'tauri://drag-over', { position });
          await sleep(500);
          await emitTo(target, 'tauri://drag-drop', { paths, position });
          break;
        }
        case 'framekey': {
          // Uma tecla como o cliente web da aba de onda a repassa (o script
          // que o wave_tab.rs injeta): uma mensagem vinda do iframe.
          const frame = element('.wave-view__frame') as HTMLIFrameElement;
          const [key, ...mods] = arg.split('+').reverse();
          window.dispatchEvent(
            new MessageEvent('message', {
              source: frame.contentWindow,
              data: { lace: 'key', key, code: key, ctrlKey: mods.includes('Ctrl'), shiftKey: mods.includes('Shift'), altKey: mods.includes('Alt'), metaKey: false },
            }),
          );
          break;
        }
        case 'key': {
          const target = document.activeElement ?? document.body;
          target.dispatchEvent(new KeyboardEvent('keydown', { key: arg, code: arg, bubbles: true, cancelable: true }));
          break;
        }
        case 'menu': {
          const { x, y } = center(element(arg));
          element(arg).dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 2 }));
          await sleep(200);
          const items = [...document.querySelectorAll<HTMLElement>('.menu__item, .menu__sep')].map((el) =>
            el.classList.contains('menu__sep')
              ? '-'
              : `${el.querySelector('.menu__label')?.textContent ?? ''}${(el as HTMLButtonElement).disabled ? ' (off)' : ''}`,
          );
          await log(`menu ${arg}: ${items.join(' | ') || 'none'}`);
          break;
        }
        case 'choose': {
          const item = [...document.querySelectorAll<HTMLButtonElement>('.menu__item')].find(
            (el) => el.querySelector('.menu__label')?.textContent === arg,
          );
          if (!item) throw new Error(`no menu item ${arg}`);
          item.click();
          break;
        }
        default: {
          // Espera a ação terminar, menos quando ela abre um diálogo: aí só
          // termina quando ele fechar, e os passos seguintes o preenchem.
          const result = action(kind).run();
          if (result instanceof Promise) {
            let settled = false;
            const done = () => (settled = true);
            result.then(done, done);
            while (!settled && !useDialogs.getState().dialog) await sleep(100);
          }
        }
      }
      await sleep(400);
      const dialog = useDialogs.getState().dialog?.kind;
      await log(dialog ? `${step} [${dialog}]` : step);
    } catch (error) {
      await log(`${step} failed: ${String((error as { message?: string })?.message ?? error)}`);
    }
  }
  await log('done');
}
