// O assembly do SAPHO pelo lugar do arquivo: um padrão de nome de arquivo do
// package.json não diz "a pasta tem o nome do arquivo", então a extensão
// confere ao abrir (`isSaphoAssembly`) e troca a linguagem.

import * as vscode from 'vscode';

import { isSaphoAssembly } from './asmPath';

/** Abre os `.asm` gerados pelo YANC com a linguagem do assembly do SAPHO. */
export function registerSaphoAssembly(context: vscode.ExtensionContext): void {
  const apply = (document: vscode.TextDocument) => {
    if (document.languageId !== 'sapho-asm' && isSaphoAssembly(document.fileName)) {
      void vscode.languages.setTextDocumentLanguage(document, 'sapho-asm');
    }
  };
  vscode.workspace.textDocuments.forEach(apply);
  context.subscriptions.push(vscode.workspace.onDidOpenTextDocument(apply));
}
