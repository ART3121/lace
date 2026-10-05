// Caminhos na interface. O backend devolve caminhos absolutos do sistema
// (com \ no Windows); estas funções aceitam os dois separadores.

export function separator(path: string): string {
  return path.includes('\\') && !path.includes('/') ? '\\' : '/';
}

export function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

export function dirName(path: string): string {
  const index = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return index <= 0 ? path.slice(0, index + 1) : path.slice(0, index);
}

export function joinPath(base: string, ...parts: string[]): string {
  const sep = separator(base);
  return [base.replace(/[\\/]+$/, ''), ...parts.map((p) => p.replace(/^[\\/]+|[\\/]+$/g, ''))]
    .filter((p) => p.length > 0)
    .join(sep);
}

export function extension(path: string): string {
  const name = baseName(path);
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : '';
}

function normalized(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+$/, '');
}

export function samePath(a: string | null | undefined, b: string | null | undefined): boolean {
  return !!a && !!b && normalized(a) === normalized(b);
}

/** `path` está dentro de `dir` (ou é ele). */
export function isInside(path: string, dir: string): boolean {
  const p = normalized(path);
  const d = normalized(dir);
  return p === d || p.startsWith(`${d}/`);
}

/** O caminho relativo a `root`, com `/`, ou o próprio caminho se estiver
 * fora. */
export function relativeTo(path: string, root: string | null | undefined): string {
  if (!root) return path;
  const p = normalized(path);
  const r = normalized(root);
  if (p === r) return '.';
  return p.startsWith(`${r}/`) ? p.slice(r.length + 1) : path;
}

/** `path` relativo a `root` mesmo quando está fora dele, com `..`
 * (`../../rtl/adc.v`), como o `.spf` do HITS grava. Em outro disco do Windows
 * não há caminho relativo, e volta o absoluto. */
export function relativeUp(path: string, root: string | null | undefined): string {
  if (!root) return path;
  const p = normalized(path).split('/');
  const r = normalized(root).split('/');
  if (p[0].toLowerCase() !== r[0].toLowerCase()) return path;
  let same = 0;
  while (same < p.length && same < r.length && p[same] === r[same]) same++;
  if (same === p.length && same === r.length) return '.';
  return [...Array<string>(r.length - same).fill('..'), ...p.slice(same)].join('/');
}

/** Resolve um caminho que pode ser relativo a `root`. */
export function resolveFrom(root: string, path: string): string {
  if (/^([A-Za-z]:[\\/]|\/|\\\\)/.test(path)) return path;
  return joinPath(root, path);
}
