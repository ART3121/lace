// O Monaco de um grupo da área central. Mostra o modelo do arquivo da aba
// ativa do grupo, guarda a posição do cursor e da rolagem de cada arquivo ao
// trocar de aba e aplica as preferências do editor. Dois grupos com o mesmo
// arquivo mostram o mesmo modelo: o que se digita num aparece no outro.

import { useEffect, useRef } from 'react';

import { setGroupEditor } from '../../editor/host';
import { getModel, monaco, setMonacoTheme } from '../../editor/monaco';
import { setVimMode, showVimStatus } from '../../editor/vim';
import { useApp } from '../../state/app';
import { fileTabId, useEditor } from '../../state/editor';
import { useLayout } from '../../state/layout';

const viewStates = new Map<string, monaco.editor.ICodeEditorViewState | null>();

export function MonacoHost({ path, group }: { path: string; group: string }) {
  const container = useRef<HTMLDivElement>(null);
  const editorRef = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const currentPath = useRef<string | null>(null);
  const settings = useApp((s) => s.settings?.editor);
  const zen = useLayout((s) => s.zen);
  const zenHideLines = useApp((s) => s.settings?.zen?.hide_line_numbers ?? false);
  const theme = useApp((s) => s.theme);
  const reveal = useEditor((s) => s.reveal);
  const focused = useEditor((s) => s.activeGroup === group);

  // Cria o editor uma vez.
  useEffect(() => {
    if (!container.current) return;
    const editor = monaco.editor.create(container.current, {
      automaticLayout: true,
      fontFamily: "'JetBrains Mono Variable', 'JetBrains Mono', ui-monospace, monospace",
      fontLigatures: false,
      renderWhitespace: 'selection',
      smoothScrolling: true,
      scrollBeyondLastLine: false,
      stickyScroll: { enabled: true },
      bracketPairColorization: { enabled: false },
      guides: { indentation: true },
      padding: { top: 8 },
      fixedOverflowWidgets: true,
      model: null,
    });
    editorRef.current = editor;
    setGroupEditor(group, editor);
    const report = () => {
      const position = editor.getPosition();
      useEditor.getState().setCursor(position?.lineNumber ?? 1, position?.column ?? 1, editor.getModel()?.getLanguageId() ?? null);
    };
    const cursor = editor.onDidChangeCursorPosition(() => {
      if (useEditor.getState().activeGroup === group) report();
    });
    // O foco num editor faz do grupo dele o ativo: os atalhos, a barra de
    // status e o Vim passam a falar dele.
    const focus = editor.onDidFocusEditorWidget(() => {
      useEditor.getState().focusGroup(group);
      showVimStatus(editor);
      report();
    });
    const blur = editor.onDidBlurEditorText(() => {
      if (useApp.getState().settings?.editor.auto_save && currentPath.current) {
        void useEditor.getState().save(fileTabId(currentPath.current));
      }
    });
    return () => {
      cursor.dispose();
      focus.dispose();
      blur.dispose();
      setVimMode(editor, false);
      if (currentPath.current) viewStates.set(currentPath.current, editor.saveViewState());
      setGroupEditor(group, null);
      editor.dispose();
      editorRef.current = null;
    };
  }, []);

  // Troca o modelo quando a aba muda.
  useEffect(() => {
    const editor = editorRef.current;
    if (!editor) return;
    if (currentPath.current && currentPath.current !== path) {
      viewStates.set(currentPath.current, editor.saveViewState());
      if (useApp.getState().settings?.editor.auto_save) void useEditor.getState().save(fileTabId(currentPath.current));
    }
    const model = getModel(path);
    editor.setModel(model);
    currentPath.current = path;
    const state = viewStates.get(path);
    if (state) editor.restoreViewState(state);
    // Só o grupo ativo informa o cursor e pega o foco: restaurar a sessão
    // ou abrir ao lado não tira o foco de onde ele está.
    if (useEditor.getState().activeGroup === group) {
      if (model) {
        const position = editor.getPosition();
        useEditor.getState().setCursor(position?.lineNumber ?? 1, position?.column ?? 1, model.getLanguageId());
      }
      editor.focus();
    }
  }, [path, group]);

  // O grupo virou o ativo por fora do editor (clique numa aba, Ctrl+2):
  // o foco vem para cá.
  useEffect(() => {
    const editor = editorRef.current;
    if (focused && editor && !editor.hasWidgetFocus() && editor.getContainerDomNode().offsetParent !== null) editor.focus();
  }, [focused]);

  // Preferências. No zen, sem minimapa e, se a preferência pedir, sem os
  // números de linha.
  useEffect(() => {
    if (!settings) return;
    editorRef.current?.updateOptions({
      fontSize: settings.font_size,
      tabSize: settings.tab_size,
      wordWrap: settings.word_wrap ? 'on' : 'off',
      minimap: { enabled: settings.minimap && !zen },
      lineNumbers: zen && zenHideLines ? 'off' : 'on',
      // Sem o minimapa, a borda da régua da direita vira uma linha de cima a
      // baixo no meio da tela.
      overviewRulerBorder: !zen,
    });
    editorRef.current?.getModel()?.updateOptions({ tabSize: settings.tab_size });
    setVimMode(editorRef.current, settings.vim_mode);
  }, [settings, zen, zenHideLines]);

  useEffect(() => setMonacoTheme(theme), [theme]);

  // Ir para a linha (Problemas, links do console, busca).
  useEffect(() => {
    const editor = editorRef.current;
    if (!editor || !reveal || reveal.path !== path || reveal.group !== group) return;
    const position = { lineNumber: reveal.line, column: reveal.column };
    editor.setPosition(position);
    editor.revealPositionInCenter(position);
    editor.focus();
  }, [reveal, path, group]);

  return <div ref={container} className="editor-host" />;
}
