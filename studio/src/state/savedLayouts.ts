// Os layouts com nome: os prontos (layoutModel.ts, PRESETS) e os que o
// usuário gravou (settings.json, `layouts`). Cada um é uma foto da janela.
// Mexer na janela não muda a foto: "Salvar layout" grava a janela nela,
// "Restaurar layout" volta a ela, "Salvar como" cria outra. Os prontos e os
// gravados por uma versão mais nova do Studio são só para leitura: salvar
// sobre eles vira "Salvar como".

import { useMemo } from 'react';

import { t, useLang } from '../i18n';
import type { LayoutSettings, Settings } from '../ipc/types';
import { useApp } from './app';
import { confirm, prompt } from './dialogs';
import { effectiveLive, useLayout } from './layout';
import {
  bodyOf,
  DEFAULT_LAYOUT_ID,
  LAYOUT_VERSION,
  parseLayoutDef,
  PRESETS,
  sameBody,
  type LayoutBody,
  type LayoutSizes,
} from './layoutModel';

export interface NamedLayout {
  id: string;
  name: string;
  /** Pronto (do Studio) ou gravado por uma versão mais nova: não se
   * sobrescreve, não se renomeia, não se exclui. */
  readOnly: boolean;
  preset: boolean;
  body: LayoutBody;
}

const NO_LAYOUTS: LayoutSettings = { active: DEFAULT_LAYOUT_ID, saved: [] };

function layoutSettings(settings: Settings | null): LayoutSettings {
  return settings?.layouts ?? NO_LAYOUTS;
}

/** Os prontos e os gravados, nesta ordem. Um gravado com o id de outro que
 * veio antes fica de fora. */
export function namedLayouts(settings: Settings | null): NamedLayout[] {
  const list: NamedLayout[] = PRESETS.map((preset) => ({
    id: preset.id,
    name: t(preset.nameKey),
    readOnly: true,
    preset: true,
    body: preset.body,
  }));
  for (const raw of layoutSettings(settings).saved) {
    const def = parseLayoutDef(raw);
    if (!def || list.some((layout) => layout.id === def.id)) continue;
    list.push({ id: def.id, name: def.name, readOnly: def.newer, preset: false, body: bodyOf(def) });
  }
  return list;
}

/** O layout em uso; o Padrão, se o gravado sumiu. */
export function activeLayout(settings: Settings | null = useApp.getState().settings): NamedLayout {
  const list = namedLayouts(settings);
  const id = layoutSettings(settings).active;
  return list.find((layout) => layout.id === id) ?? list[0];
}

/** O layout em uso, se a janela está diferente dele e se ele é só para
 * leitura; para o indicador da barra de status e para as Preferências. */
export function useLayoutStatus(): { layout: NamedLayout; dirty: boolean } {
  // O nome dos prontos depende do idioma.
  const lang = useLang((s) => s.lang);
  const settings = useApp((s) => s.settings);
  const layout = useMemo(() => activeLayout(settings), [settings, lang]);
  const dirty = useLayout((s) => !sameBody(effectiveLive(s), layout.body));
  return { layout, dirty };
}

function writeLayouts(change: (current: LayoutSettings) => LayoutSettings): Promise<void> {
  return useApp.getState().updateSettings((settings) => ({ ...settings, layouts: change(layoutSettings(settings)) }));
}

/** O layout como vai para o settings.json. */
function serialize(id: string, name: string, body: LayoutBody): Record<string, unknown> {
  return { v: LAYOUT_VERSION, id, name, ...body };
}

/** Troca a janela pelo layout e o marca como em uso. */
export async function applyLayout(id: string): Promise<void> {
  const layout = namedLayouts(useApp.getState().settings).find((candidate) => candidate.id === id);
  if (!layout) return;
  useLayout.getState().applyBody(layout.body);
  await writeLayouts((current) => ({ ...current, active: layout.id }));
}

/** Volta a janela à foto do layout em uso. */
export function restoreLayout(): void {
  useLayout.getState().applyBody(activeLayout().body);
}

/** Volta ao Padrão. */
export function resetLayout(): Promise<void> {
  return applyLayout(DEFAULT_LAYOUT_ID);
}

/** O tamanho de uma região na foto do layout em uso: o duplo clique numa
 * divisão volta a ele. */
export function snapshotSize(size: keyof LayoutSizes): number {
  return activeLayout().body.sizes[size];
}

const MAX_NAME = 40;

/** Erro de um nome de layout, ou `null` se serve. */
function nameError(name: string, except?: string): string | null {
  const trimmed = name.trim();
  if (!trimmed) return t('layout.name.empty');
  if (trimmed.length > MAX_NAME) return t('layout.name.tooLong', { max: MAX_NAME });
  const taken = namedLayouts(useApp.getState().settings).some(
    (layout) => layout.id !== except && layout.name.toLowerCase() === trimmed.toLowerCase(),
  );
  return taken ? t('layout.name.taken') : null;
}

function newId(): string {
  return `u-${Date.now().toString(36)}${Math.floor(Math.random() * 1296).toString(36)}`;
}

/** Grava a janela num layout novo, com nome, e passa a usá-lo. */
export async function saveLayoutAs(): Promise<void> {
  const name = await prompt({
    title: t('layout.saveAs.title'),
    label: t('layout.name.label'),
    initial: activeLayout().preset ? '' : activeLayout().name,
    validate: (value) => nameError(value),
  });
  if (name === null) return;
  const id = newId();
  const body = effectiveLive(useLayout.getState());
  await writeLayouts((current) => ({ active: id, saved: [...current.saved, serialize(id, name.trim(), body)] }));
}

/** Grava a janela no layout em uso. Num pronto, ou num de versão mais nova,
 * pede um nome e cria outro. */
export async function saveLayout(): Promise<void> {
  const layout = activeLayout();
  if (layout.readOnly) {
    await saveLayoutAs();
    return;
  }
  const body = effectiveLive(useLayout.getState());
  await writeLayouts((current) => ({
    ...current,
    // Os outros vão de volta como vieram, sem passar pela validação.
    saved: current.saved.map((raw) => (parseLayoutDef(raw)?.id === layout.id ? serialize(layout.id, layout.name, body) : raw)),
  }));
}

export async function renameLayout(id: string): Promise<void> {
  const layout = namedLayouts(useApp.getState().settings).find((candidate) => candidate.id === id);
  if (!layout || layout.readOnly) return;
  const name = await prompt({
    title: t('layout.rename.title'),
    label: t('layout.name.label'),
    initial: layout.name,
    validate: (value) => nameError(value, id),
  });
  if (name === null) return;
  await writeLayouts((current) => ({
    ...current,
    saved: current.saved.map((raw) => (parseLayoutDef(raw)?.id === id ? { ...(raw as Record<string, unknown>), name: name.trim() } : raw)),
  }));
}

/** Exclui um layout gravado. Se era o em uso, o Padrão passa a ser, sem
 * mexer na janela. */
export async function deleteLayout(id: string): Promise<void> {
  const layout = namedLayouts(useApp.getState().settings).find((candidate) => candidate.id === id);
  if (!layout || layout.readOnly) return;
  const answer = await confirm({
    title: t('layout.delete.title'),
    message: t('layout.delete.message', { name: layout.name }),
    buttons: [
      { label: t('layout.delete.confirm'), value: 'delete', danger: true },
      { label: t('common.cancel'), value: 'cancel' },
    ],
  });
  if (answer !== 'delete') return;
  await writeLayouts((current) => ({
    active: current.active === id ? DEFAULT_LAYOUT_ID : current.active,
    saved: current.saved.filter((raw) => parseLayoutDef(raw)?.id !== id),
  }));
}
