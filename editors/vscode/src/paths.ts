// Comparar caminhos que vêm de dois lugares: o Lace devolve o caminho
// canônico (`C:\Users\...`), e o `Uri.fsPath` do VS Code, a letra do drive
// em minúscula (`c:\Users\...`). No Windows o sistema de arquivos não
// distingue a caixa, e um nome que chega com `/` é o mesmo de `\`. Sem
// `vscode` aqui, para os testes de unidade.

import { posix, win32 } from 'node:path';

const WINDOWS = process.platform === 'win32';

/** O caminho na forma de comparar: sem a barra do fim e, no Windows, com
 * `\` e em minúsculas. */
export function pathKey(path: string, windows = WINDOWS): string {
  if (!windows) {
    return path.length > 1 ? path.replace(/\/+$/, '') : path;
  }
  return path.replace(/\//g, '\\').replace(/\\+$/, '').toLowerCase();
}

/** Os dois são o mesmo caminho. */
export function samePath(a: string, b: string, windows = WINDOWS): boolean {
  return pathKey(a, windows) === pathKey(b, windows);
}

/** `path` é `dir` ou está dentro dele. */
export function isInside(path: string, dir: string, windows = WINDOWS): boolean {
  const p = pathKey(path, windows);
  const d = pathKey(dir, windows);
  const sep = windows ? win32.sep : posix.sep;
  return p === d || p.startsWith(d.endsWith(sep) ? d : d + sep);
}

/** `path` dentro de `dir`, relativo a ele (`rtl/a.v`); `''` se é o próprio
 * `dir`. Só para um `path` que [`isInside`] aceita. */
export function insidePart(path: string, dir: string, windows = WINDOWS): string {
  return (windows ? win32 : posix).relative(dir, path);
}
