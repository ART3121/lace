// Onde fica o assembly do SAPHO. Desde o YANC 7.1 o `.asm` sai na pasta de
// saída do processador, `<proc>/<proc>/<proc>.asm`. Sem o vscode, para o
// teste de unidade.

import * as path from 'node:path';

/** `<proc>/<proc>/<proc>.asm`: o `.asm` que o YANC 7.1 gera. */
export function isSaphoAssembly(fileName: string): boolean {
  const parsed = path.parse(fileName);
  if (parsed.ext.toLowerCase() !== '.asm' || !parsed.name) {
    return false;
  }
  const output = parsed.dir;
  return path.basename(output) === parsed.name && path.basename(path.dirname(output)) === parsed.name;
}
