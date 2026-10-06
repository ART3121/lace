// Arrastar e soltar no explorador.
//
// Dentro da janela, o arrasto é feito com eventos de ponteiro, não com o
// drag-and-drop do HTML: no Windows o Tauri desliga o do HTML quando recebe
// arquivos do sistema, e com ponteiro o comportamento é o mesmo nas três
// plataformas. Os arquivos que chegam do gerenciador de arquivos do sistema
// vêm pelo evento de arrastar do Tauri (App.tsx), e caem no mesmo destino.
//
// Os destinos se marcam no DOM:
//   data-drop-dir="<pasta>"        a pasta que recebe (linhas da árvore de
//                                  arquivos; numa linha de arquivo, a pasta dele)
//   data-drop-section="modules"    as seções de módulos e testbenches das
//                                  fontes
//   data-drop-order="<arquivo>"    uma linha das fontes: soltar ali põe o
//                                  arquivo antes dela, na mesma lista
//
// O que soltar faz:
//   arquivo ou pasta do projeto em pasta     move (Project::move_path do Core,
//                                            que mantém o .spf)
//   arquivo do sistema em pasta              copia (fs_copy)
//   Verilog sobre um da mesma seção          muda de lugar na lista (a ordem
//                                            em que os compiladores leem;
//                                            Project::reorder_file do Core)
//   Verilog em Módulos ou Testbenches        registra com esse papel, no lugar
//                                            (de fora da pasta, o Core grava
//                                            relativo quando o arquivo está no
//                                            mesmo repositório git do projeto,
//                                            e absoluto quando não, como o menu
//                                            Adicionar)

import { create } from 'zustand';

import { t } from '../../i18n';
import { api } from '../../ipc/api';
import type { IpcError, ListPosition } from '../../ipc/types';
import { confirm } from '../../state/dialogs';
import { useEditor } from '../../state/editor';
import { useProject } from '../../state/project';
import { showError, useToasts } from '../../state/toasts';
import { baseName, dirName, extension, isInside, joinPath, samePath } from '../../util/paths';

export type DropSection = 'modules' | 'testbenches';

export type DropTarget =
  | { kind: 'dir'; path: string }
  | { kind: 'section'; section: DropSection }
  | { kind: 'order'; path: string; section: DropSection };

/** O que está sendo arrastado de dentro da janela. */
export interface DragSource {
  path: string;
  isDir: boolean;
  /** Pode ir para uma pasta (a árvore de arquivos). */
  movable: boolean;
  /** É Verilog e pode trocar de seção (as fontes). */
  verilog: boolean;
  /** A seção das fontes de onde ele saiu: soltar sobre outro arquivo da
   * mesma seção muda a ordem; de outra, troca o papel. */
  section?: DropSection;
}

interface DragState {
  source: DragSource | null;
  /** Arquivos do sistema passando sobre a janela. */
  external: boolean;
  target: DropTarget | null;
  allowed: boolean;
  x: number;
  y: number;
  set: (patch: Partial<DragState>) => void;
}

export const useDrag = create<DragState>((set) => ({
  source: null,
  external: false,
  target: null,
  allowed: false,
  x: 0,
  y: 0,
  set: (patch) => set(patch),
}));

/** O destino debaixo de um ponto da janela, em pixels CSS. */
export function targetAt(x: number, y: number): DropTarget | null {
  const element = document.elementFromPoint(x, y);
  const row = element?.closest<HTMLElement>('[data-drop-order]');
  const rowSection = row?.closest<HTMLElement>('[data-drop-section]')?.dataset.dropSection;
  if (row?.dataset.dropOrder && rowSection) {
    return { kind: 'order', path: row.dataset.dropOrder, section: rowSection as DropSection };
  }
  const section = element?.closest<HTMLElement>('[data-drop-section]');
  if (section?.dataset.dropSection) return { kind: 'section', section: section.dataset.dropSection as DropSection };
  const dir = element?.closest<HTMLElement>('[data-drop-dir]');
  if (dir?.dataset.dropDir) return { kind: 'dir', path: dir.dataset.dropDir };
  return null;
}

export function sameTarget(a: DropTarget | null, b: DropTarget | null): boolean {
  if (!a || !b) return a === b;
  if (a.kind === 'dir' && b.kind === 'dir') return samePath(a.path, b.path);
  if (a.kind === 'order' && b.kind === 'order') return samePath(a.path, b.path);
  return a.kind === 'section' && b.kind === 'section' && a.section === b.section;
}

/** O que entra nas listas do projeto: Verilog e testbench cocotb (.py). */
const isVerilog = (path: string) => ['v', 'sv', 'py'].includes(extension(path));

/** Pastas e arquivos que não se movem: o `.spf`, `.lace/` e as pastas dos
 * processadores (o backend recusa também). */
function isProtected(path: string): boolean {
  const snapshot = useProject.getState().snapshot;
  if (!snapshot) return true;
  if (samePath(path, snapshot.spf) || isInside(path, `${snapshot.root}/.lace`)) return true;
  return snapshot.processors.some((p) => isInside(path, p.dir));
}

/** Se soltar `source` em `target` faz alguma coisa. */
function accepts(source: DragSource, target: DropTarget): boolean {
  if (target.kind === 'section') return source.verilog;
  if (target.kind === 'order') return source.verilog && !samePath(source.path, target.path);
  if (!source.movable || isProtected(source.path)) return false;
  const snapshot = useProject.getState().snapshot;
  if (snapshot && isInside(target.path, `${snapshot.root}/.lace`)) return false;
  if (samePath(dirName(source.path), target.path)) return false;
  if (source.isDir && isInside(target.path, source.path)) return false;
  return true;
}

// Arrastar dentro da janela --------------------------------------------------

const THRESHOLD = 4;

/** Começa a seguir um arrasto a partir de `pointerdown` numa linha. Só vira
 * arrasto depois de alguns pixels; antes disso é um clique comum. */
export function beginDrag(event: React.PointerEvent, source: DragSource): void {
  if (event.button !== 0) return;
  const startX = event.clientX;
  const startY = event.clientY;
  let active = false;
  const state = useDrag.getState();

  const move = (e: PointerEvent) => {
    if (!active) {
      if (Math.abs(e.clientX - startX) < THRESHOLD && Math.abs(e.clientY - startY) < THRESHOLD) return;
      active = true;
      document.body.classList.add('is-dragging');
    }
    const target = targetAt(e.clientX, e.clientY);
    state.set({ source, target, allowed: target ? accepts(source, target) : false, x: e.clientX, y: e.clientY });
  };
  const finish = (drop: boolean) => {
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', up);
    window.removeEventListener('keydown', key, true);
    document.body.classList.remove('is-dragging');
    const { target, allowed } = useDrag.getState();
    state.set({ source: null, target: null, allowed: false });
    if (!active) return;
    // O clique que o navegador dispara depois do arrasto não abre o arquivo.
    const swallow = (c: MouseEvent) => c.stopPropagation();
    window.addEventListener('click', swallow, { capture: true, once: true });
    setTimeout(() => window.removeEventListener('click', swallow, true), 0);
    if (drop && target && allowed) void dropInternal(source, target);
  };
  const up = () => finish(true);
  const key = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      finish(false);
    }
  };
  window.addEventListener('pointermove', move);
  window.addEventListener('pointerup', up);
  window.addEventListener('keydown', key, true);
}

async function dropInternal(source: DragSource, target: DropTarget): Promise<void> {
  if (target.kind === 'order' && source.section === target.section) {
    await reorder(source.path, { kind: 'before', path: target.path });
    return;
  }
  if (target.kind === 'section' || target.kind === 'order') {
    await register([source.path], target.section);
    return;
  }
  await moveInto(source.path, target.path);
}

/** Pergunta se substitui o que já existe com o mesmo nome. */
async function askReplace(name: string, dir: string): Promise<boolean> {
  const answer = await confirm({
    title: t('dnd.existsTitle'),
    message: t('dnd.exists', { name, dir: baseName(dir) || dir }),
    buttons: [
      { label: t('dnd.replace'), value: 'replace', danger: true },
      { label: t('common.cancel'), value: 'cancel' },
    ],
  });
  return answer === 'replace';
}

// `exists` vem do Studio (copiar); `path_exists`, do Core (mover).
const isExists = (error: unknown) => ['exists', 'path_exists'].includes((error as IpcError)?.code);

/** Move um arquivo ou pasta do projeto para dentro de `dir`. */
export function moveInto(path: string, dir: string): Promise<string | null> {
  return movePath(path, joinPath(dir, baseName(path)));
}

/** Move ou renomeia pelo Core, que mantém o `.spf` em dia; pergunta antes de
 * substituir o que já existe em `to`. Devolve o caminho novo. */
export async function movePath(path: string, to: string): Promise<string | null> {
  let moved: string;
  try {
    moved = (await api.project.move(path, to)).to;
  } catch (error) {
    if (!isExists(error) || !(await askReplace(baseName(to), dirName(to)))) {
      if (!isExists(error)) showError(error);
      return null;
    }
    try {
      moved = (await api.project.move(path, to, true)).to;
    } catch (again) {
      showError(again);
      return null;
    }
  }
  useEditor.getState().renamed(path, moved);
  useProject.getState().bumpTree();
  await useProject.getState().refresh();
  return moved;
}

/** Copia arquivos de fora para dentro de `dir`. */
export async function copyInto(paths: string[], dir: string): Promise<string[]> {
  const copied: string[] = [];
  for (const path of paths) {
    try {
      copied.push(await api.fs.copy(path, dir));
    } catch (error) {
      if (!isExists(error)) {
        showError(error);
        continue;
      }
      if (!(await askReplace(baseName(path), dir))) continue;
      try {
        copied.push(await api.fs.copy(path, dir, true));
      } catch (again) {
        showError(again);
      }
    }
  }
  if (copied.length) {
    useProject.getState().bumpTree();
    useToasts.getState().push({
      kind: 'success',
      title: t('dnd.copied', { count: copied.length }),
      detail: baseName(dir) || dir,
    });
  }
  return copied;
}

/** Registra arquivos Verilog com o papel da seção, no lugar onde estão. Um
 * arquivo de fora da pasta do projeto não é copiado: uma cópia divergiria
 * do original (o `rtl/` de um repositório como o HITS). O Core grava o
 * caminho relativo quando o arquivo está no mesmo repositório git (ADR 0012
 * do Lace). */
export async function register(paths: string[], section: DropSection): Promise<void> {
  const snapshot = useProject.getState().snapshot;
  if (!snapshot) return;
  const files = paths.filter(isVerilog);
  if (files.length < paths.length) {
    useToasts.getState().push({ kind: 'warning', title: t('dnd.onlyVerilog') });
  }
  for (const file of files) {
    try {
      const added = await api.project.addVerilog(file, section === 'testbenches');
      const role = added.role === 'testbench' ? t('explorer.testbenches') : t('explorer.modules');
      const wanted = section === 'testbenches' ? 'testbench' : 'synthesizable';
      useToasts.getState().push({
        kind: added.role === wanted ? 'success' : 'warning',
        title: baseName(added.path),
        detail: added.role === wanted ? role : t('dnd.roleFromContent', { role }),
      });
    } catch (error) {
      showError(error);
    }
  }
  await useProject.getState().refresh();
}

// Arquivos do sistema -------------------------------------------------------

/** Converte a posição do evento do Tauri (pixels físicos da janela) para
 * pixels CSS. */
export function cssPoint(position: { x: number; y: number }): { x: number; y: number } {
  const ratio = window.devicePixelRatio || 1;
  return { x: position.x / ratio, y: position.y / ratio };
}

/** Arquivos do sistema sobre a janela: marca o destino. */
export function externalOver(position: { x: number; y: number }): void {
  const { x, y } = cssPoint(position);
  const target = targetAt(x, y);
  useDrag.getState().set({ external: true, target, allowed: target !== null, x, y });
}

export function externalLeave(): void {
  useDrag.getState().set({ external: false, target: null, allowed: false });
}

/** Arquivos do sistema soltos sobre a janela. Devolve `false` se não
 * caíram num destino do explorador (quem chamou decide o que fazer). */
export async function externalDrop(paths: string[], position: { x: number; y: number }): Promise<boolean> {
  const { x, y } = cssPoint(position);
  const target = targetAt(x, y);
  externalLeave();
  if (!target || !useProject.getState().snapshot) return false;
  if (target.kind === 'section' || target.kind === 'order') await register(paths, target.section);
  else await copyInto(paths, target.path);
  return true;
}

/** Muda a posição de um arquivo na lista dele (`Project::reorder_file`): a
 * ordem em que os compiladores leem, que decide quem vê cada `define. */
export async function reorder(path: string, position: ListPosition): Promise<void> {
  try {
    await api.project.reorder(path, position);
  } catch (error) {
    showError(error);
  }
  await useProject.getState().refresh();
}
