// O modo Vim do editor, com o monaco-vim (as teclas do Vim do CodeMirror,
// adaptadas para o Monaco). Liga e desliga pela preferência
// `editor.vim_mode`, sem recriar o editor.
//
// Cada editor (um por grupo da área central) tem o seu adaptador. O modo
// atual (NORMAL, INSERT, VISUAL) e a linha de comando (`:`, `/`) aparecem
// num elemento da barra de status, que ela registra aqui; dentro dele, cada
// adaptador escreve no seu próprio pedaço, e só o do editor com o foco
// aparece.
//
// Comandos `:` além dos do Vim:
//   :w   grava a aba        :wa  grava todas
//   :q   fecha a aba        :wq  e :x  gravam e fecham

import { initVimMode, VimMode, type VimAdapterInstance } from 'monaco-vim';

import { saveActive, useEditor } from '../state/editor';
import type { monaco } from './monaco';

interface Instance {
  adapter: VimAdapterInstance;
  /** O pedaço da barra de status deste editor. O monaco-vim mexe no
   * `display` do elemento que recebe, por isso ele fica dentro de outro,
   * que é o que mostramos e escondemos. */
  outer: HTMLElement;
}

const instances = new Map<monaco.editor.IStandaloneCodeEditor, Instance>();
let statusNode: HTMLElement | null = null;
let shown: monaco.editor.IStandaloneCodeEditor | null = null;
let exDefined = false;

function closeActive() {
  const { activeId, activeGroup, closeTab } = useEditor.getState();
  if (activeId) void closeTab(activeId, activeGroup);
}

function defineEx() {
  if (exDefined) return;
  exDefined = true;
  // `VimMode.Vim` existe em tempo de execução, mas não nos tipos do pacote.
  const vim = (VimMode as unknown as { Vim: { defineEx: (name: string, prefix: string, fn: () => void) => void } }).Vim;
  vim.defineEx('write', 'w', () => void saveActive());
  vim.defineEx('wall', 'wa', () => void useEditor.getState().saveAll());
  vim.defineEx('quit', 'q', closeActive);
  vim.defineEx('wq', 'wq', () => void saveActive().then(closeActive));
  vim.defineEx('xit', 'x', () => void saveActive().then(closeActive));
}

/** O elemento da barra de status onde o modo aparece. */
export function setVimStatusNode(node: HTMLElement | null): void {
  statusNode = node;
  if (node) for (const { outer } of instances.values()) node.appendChild(outer);
}

/** Liga ou desliga o Vim num editor. */
export function setVimMode(editor: monaco.editor.IStandaloneCodeEditor | null, enabled: boolean): void {
  if (!editor) return;
  const current = instances.get(editor);
  if (!enabled) {
    if (!current) return;
    current.adapter.dispose();
    current.outer.remove();
    instances.delete(editor);
    if (shown === editor) shown = null;
    return;
  }
  if (current) return;
  defineEx();
  const outer = document.createElement('span');
  const inner = document.createElement('span');
  outer.appendChild(inner);
  outer.hidden = shown !== null && shown !== editor;
  statusNode?.appendChild(outer);
  instances.set(editor, { adapter: initVimMode(editor, inner), outer });
  if (shown === null) shown = editor;
}

/** Mostra na barra de status o modo do editor que ganhou o foco. */
export function showVimStatus(editor: monaco.editor.IStandaloneCodeEditor): void {
  shown = editor;
  for (const [owner, { outer }] of instances) outer.hidden = owner !== editor;
}

/** O Vim está ligado agora. */
export function vimActive(): boolean {
  return instances.size > 0;
}
