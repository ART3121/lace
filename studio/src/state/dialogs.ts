// Os diálogos modais. Um por vez. `prompt` e `confirm` devolvem promessas,
// para o código que pede ficar linear:
//
//   const name = await prompt({ title: t('dialog.rename.title'), initial: 'a.v' });
//   if (name === null) return;

import { create } from 'zustand';

export interface PromptOptions {
  title: string;
  label?: string;
  initial?: string;
  placeholder?: string;
  /** Seleciona só o nome, sem a extensão. */
  selectStem?: boolean;
  /** Devolve a mensagem de erro, ou null se o valor serve. */
  validate?: (value: string) => string | null;
}

export interface ConfirmButton {
  label: string;
  value: string;
  primary?: boolean;
  danger?: boolean;
}

export interface ConfirmOptions {
  title: string;
  message: string;
  buttons: ConfirmButton[];
}

export type DialogSpec =
  | { kind: 'newProject' }
  | { kind: 'newProcessor' }
  | { kind: 'newVerilog'; testbench: boolean; folder?: string; cocotb?: boolean }
  | { kind: 'newInput'; processor: string }
  | { kind: 'install'; components?: string[] }
  | { kind: 'palette' }
  | { kind: 'quickOpen' }
  | { kind: 'theme' }
  | { kind: 'layout' }
  | { kind: 'target' }
  | { kind: 'chooseTop' }
  | { kind: 'chooseTestbench' }
  | { kind: 'cleanReports' }
  | { kind: 'waveSignals' }
  | { kind: 'shortcuts' }
  | { kind: 'prompt'; options: PromptOptions; resolve: (value: string | null) => void }
  | { kind: 'confirm'; options: ConfirmOptions; resolve: (value: string | null) => void };

interface DialogState {
  dialog: DialogSpec | null;
  open: (dialog: DialogSpec) => void;
  close: () => void;
}

export const useDialogs = create<DialogState>((set, get) => ({
  dialog: null,
  open: (dialog) => {
    // Um prompt ou confirm aberto antes é respondido com "cancelar".
    const current = get().dialog;
    if (current && (current.kind === 'prompt' || current.kind === 'confirm')) current.resolve(null);
    set({ dialog });
  },
  close: () => {
    const current = get().dialog;
    if (current && (current.kind === 'prompt' || current.kind === 'confirm')) current.resolve(null);
    set({ dialog: null });
  },
}));

export function openDialog(dialog: DialogSpec): void {
  useDialogs.getState().open(dialog);
}

/** Pede um texto. `null` se o usuário cancelar. */
export function prompt(options: PromptOptions): Promise<string | null> {
  return new Promise((resolve) => {
    useDialogs.getState().open({
      kind: 'prompt',
      options,
      resolve: (value) => {
        useDialogs.setState({ dialog: null });
        resolve(value);
      },
    });
  });
}

/** Pergunta com botões. Devolve o `value` do botão, ou `null` ao cancelar. */
export function confirm(options: ConfirmOptions): Promise<string | null> {
  return new Promise((resolve) => {
    useDialogs.getState().open({
      kind: 'confirm',
      options,
      resolve: (value) => {
        useDialogs.setState({ dialog: null });
        resolve(value);
      },
    });
  });
}
