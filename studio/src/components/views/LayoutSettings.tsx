// Preferências > Layout da janela: o layout em uso e o arranjo da janela
// agora. As mudanças valem na hora; "Salvar" as grava no layout em uso, como
// no resto do Studio.
//
// Para não pesar: do layout em uso, só o que serve agora fica à vista (Salvar
// e Restaurar aparecem quando a janela está diferente dele; o resto vai no
// menu ⋯); as vistas aparecem agrupadas por região, como na janela, cada uma
// com o olho e o menu dela; e os itens das barras ficam recolhidos.

import { Eye, EyeOff, MoreHorizontal } from 'lucide-react';
import type { MouseEvent } from 'react';

import { useT, type Key } from '../../i18n';
import { useApp } from '../../state/app';
import { useLayout, type RegionId, type ViewId } from '../../state/layout';
import {
  REGION_IDS,
  REGION_LABELS,
  STATUS_ITEMS,
  TOOLBAR_ITEMS,
  VIEW_INFO,
  type ActivityBarSide,
  type LayoutBars,
  type PanelPosition,
} from '../../state/layoutModel';
import {
  applyLayout,
  deleteLayout,
  namedLayouts,
  renameLayout,
  resetLayout,
  restoreLayout,
  saveLayout,
  saveLayoutAs,
  useLayoutStatus,
} from '../../state/savedLayouts';
import { Button, Checkbox, IconButton, openContextMenu, Segmented, Switch, type MenuItem } from '../common';
import { moveItems } from '../layout/layoutMenus';
import { VIEW_ICONS } from '../layout/viewCatalog';
import { SettingRow, SettingsGroup } from './settingsParts';

/** Abre um menu logo abaixo do botão que o pediu. */
function menuBelow(e: MouseEvent<HTMLElement>, items: MenuItem[]) {
  const rect = e.currentTarget.getBoundingClientRect();
  openContextMenu({ clientX: rect.left, clientY: rect.bottom + 4, preventDefault: () => e.preventDefault() }, items);
}

function ViewItem({ view, region, index, count }: { view: ViewId; region: RegionId; index: number; count: number }) {
  const t = useT();
  const hidden = useLayout((s) => s.live.hidden.views.includes(view));
  const name = t(VIEW_INFO[view].label);
  const Icon = VIEW_ICONS[view];
  const move = (to: number) => useLayout.getState().moveView(view, region, to);
  return (
    <li className={`settings-view${hidden ? ' is-hidden' : ''}`}>
      <Icon size={14} strokeWidth={1.7} className="settings-view__icon" aria-hidden />
      <span className="settings-view__name">{name}</span>
      <IconButton
        label={t(hidden ? 'layout.showView' : 'layout.hideView', { view: name })}
        aria-pressed={!hidden}
        onClick={() => useLayout.getState().setViewHidden(view, !hidden)}
      >
        {hidden ? <EyeOff size={14} /> : <Eye size={14} />}
      </IconButton>
      <IconButton
        label={t('settings.layout.viewMenu', { view: name })}
        onClick={(e) =>
          menuBelow(e, [
            ...moveItems(view),
            { separator: true },
            { label: t('settings.layout.up'), disabled: index === 0, run: () => move(index - 1) },
            { label: t('settings.layout.down'), disabled: index === count - 1, run: () => move(index + 1) },
          ])
        }
      >
        <MoreHorizontal size={14} />
      </IconButton>
    </li>
  );
}

/** Os itens de uma barra, recolhidos: a lista só abre quando se quer mexer. */
function BarItems({ bar, title, items }: { bar: 'toolbar' | 'statusbar'; title: string; items: { id: string; label: Key }[] }) {
  const t = useT();
  const hidden = useLayout((s) => s.live.hidden[bar]);
  const shown = items.filter((item) => !hidden.includes(item.id)).length;
  return (
    <details className="settings-details">
      <summary>
        <span className="settings-details__title">{title}</span>
        <span className="settings-details__count">{t('settings.layout.itemsShown', { shown, total: items.length })}</span>
      </summary>
      <div className="settings-details__body">
        {items.map((item) => (
          <Checkbox
            key={item.id}
            checked={!hidden.includes(item.id)}
            onChange={(value) => useLayout.getState().setItemHidden(bar, item.id, !value)}
            label={t(item.label)}
          />
        ))}
      </div>
    </details>
  );
}

export function LayoutSettings() {
  const t = useT();
  const settings = useApp((s) => s.settings);
  const live = useLayout((s) => s.live);
  const zen = useLayout((s) => s.zen);
  const { layout, dirty } = useLayoutStatus();
  const layouts = namedLayouts(settings);
  const state = useLayout.getState;

  const status = dirty ? t('settings.layout.modified') : layout.readOnly ? t('settings.layout.readOnly') : t('settings.layout.saved');
  const bar = (key: Exclude<keyof LayoutBars, 'activitybar'>, label: string) => (
    <SettingRow label={label}>{(id) => <Switch id={id} checked={live.bars[key]} onChange={(value) => state().setBar(key, value)} />}</SettingRow>
  );
  const region = (id: RegionId) => (
    <SettingRow key={id} label={t(REGION_LABELS[id])}>
      {(control) => <Switch id={control} checked={live.regions[id].visible} onChange={(value) => state().setRegionVisible(id, value)} />}
    </SettingRow>
  );

  return (
    <>
      <p className="settings__intro">{t('settings.layout.hint')}</p>
      {zen && <p className="settings__note">{t('settings.layout.zen')}</p>}
      <fieldset className="settings-fieldset" disabled={zen}>
        <SettingsGroup>
          <SettingRow label={t('settings.layout.active')} hint={status} stack>
            {(id) => (
              <div className="settings-inline">
                <select id={id} className="select" value={layout.id} onChange={(e) => void applyLayout(e.target.value)}>
                  {layouts.map((candidate) => (
                    <option key={candidate.id} value={candidate.id}>
                      {candidate.preset ? `${candidate.name} (${t('layout.tag.preset')})` : candidate.name}
                    </option>
                  ))}
                </select>
                {dirty && (
                  <Button variant="primary" onClick={() => void saveLayout()}>
                    {layout.readOnly ? t('settings.layout.saveAs') : t('common.save')}
                  </Button>
                )}
                {dirty && <Button onClick={restoreLayout}>{t('settings.layout.restore')}</Button>}
                <IconButton
                  label={t('settings.layout.more')}
                  onClick={(e) =>
                    menuBelow(e, [
                      { label: t('settings.layout.saveAs'), run: () => void saveLayoutAs() },
                      { label: t('settings.layout.rename'), disabled: layout.readOnly, run: () => void renameLayout(layout.id) },
                      { label: t('settings.layout.delete'), disabled: layout.readOnly, danger: true, run: () => void deleteLayout(layout.id) },
                      { separator: true },
                      { label: t('action.resetLayout'), run: () => void resetLayout() },
                    ])
                  }
                >
                  <MoreHorizontal size={15} />
                </IconButton>
              </div>
            )}
          </SettingRow>
        </SettingsGroup>

        <SettingsGroup title={t('settings.layout.bars')}>
          {bar('menubar', t('layout.bar.menubar'))}
          {bar('toolbar', t('layout.bar.toolbar'))}
          {bar('statusbar', t('layout.bar.statusbar'))}
          <SettingRow label={t('layout.bar.activitybar')}>
            {() => (
              <Segmented
                label={t('layout.bar.activitybar')}
                value={live.bars.activitybar}
                options={[
                  { value: 'left' as ActivityBarSide, label: t('settings.layout.side.left') },
                  { value: 'right' as ActivityBarSide, label: t('settings.layout.side.right') },
                  { value: 'hidden' as ActivityBarSide, label: t('settings.layout.side.hidden') },
                ]}
                onChange={(value) => state().setBar('activitybar', value)}
              />
            )}
          </SettingRow>
        </SettingsGroup>

        <SettingsGroup title={t('settings.layout.regions')}>
          {REGION_IDS.map(region)}
          <SettingRow label={t('settings.layout.panelPosition')}>
            {() => (
              <Segmented
                label={t('settings.layout.panelPosition')}
                value={live.panelPosition}
                options={[
                  { value: 'bottom' as PanelPosition, label: t('settings.layout.panel.bottom') },
                  { value: 'right' as PanelPosition, label: t('settings.layout.panel.right') },
                ]}
                onChange={(value) => state().setPanelPosition(value)}
              />
            )}
          </SettingRow>
        </SettingsGroup>

        <SettingsGroup title={t('settings.layout.views')} hint={t('settings.layout.viewsHint')}>
          <div className="settings-views">
            {REGION_IDS.map((id) => {
              const views = live.regions[id].views;
              return (
                <section key={id} className="settings-views__region" aria-label={t(REGION_LABELS[id])}>
                  <h4 className="settings-views__title">{t(REGION_LABELS[id])}</h4>
                  {views.length === 0 ? (
                    <p className="settings-views__empty">{t('settings.layout.noViews')}</p>
                  ) : (
                    <ul className="settings-views__list">
                      {views.map((view, index) => (
                        <ViewItem key={view} view={view} region={id} index={index} count={views.length} />
                      ))}
                    </ul>
                  )}
                </section>
              );
            })}
          </div>
        </SettingsGroup>

        <SettingsGroup title={t('settings.layout.items')} hint={t('settings.layout.itemsHint')}>
          <BarItems bar="toolbar" title={t('layout.bar.toolbar')} items={TOOLBAR_ITEMS} />
          <BarItems bar="statusbar" title={t('layout.bar.statusbar')} items={STATUS_ITEMS} />
        </SettingsGroup>
      </fieldset>
    </>
  );
}
