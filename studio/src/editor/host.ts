// Os editores que estão na tela, um por grupo da área central, para as ações
// de "Editar" (desfazer, localizar, ir para a linha) chegarem ao do grupo
// ativo sem passar pelo React.

import { useEditor } from '../state/editor';
import type { monaco } from './monaco';

interface Entry {
  editor: monaco.editor.IStandaloneCodeEditor;
  /** A aba ativa do grupo é um arquivo de texto (o editor não está atrás de uma vista). */
  visible: boolean;
}

const editors = new Map<string, Entry>();

export function setGroupEditor(group: string, editor: monaco.editor.IStandaloneCodeEditor | null): void {
  if (editor) editors.set(group, { editor, visible: editors.get(group)?.visible ?? true });
  else editors.delete(group);
}

export function setEditorVisible(group: string, value: boolean): void {
  const entry = editors.get(group);
  if (entry) entry.visible = value;
}

/** O editor do grupo ativo, se a aba ativa dele for um arquivo. */
export function activeEditor(): monaco.editor.IStandaloneCodeEditor | null {
  const entry = editors.get(useEditor.getState().activeGroup);
  return entry?.visible ? entry.editor : null;
}
