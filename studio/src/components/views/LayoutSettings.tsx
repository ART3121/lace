// Preferências > Layout: o layout em uso (trocar, salvar, restaurar,
// renomear, excluir) e o arranjo da janela agora: as barras, as regiões, em
// que região fica cada vista e os itens das barras de ferramentas e de
// status. As mudanças valem na hora para a janela; "Salvar layout" as grava
// no layout em uso, como no resto do Studio.

import { ArrowDown, ArrowUp } from 'lucide-react';

import { useT } from '../../i18n';
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
import { applyLayout, deleteLayout, namedLayouts, renameLayout, restoreLayout, saveLayout, saveLayoutAs, useLayoutStatus } from '../../state/savedLayouts';
import { Button, Checkbox, IconButton } from '../common';

function ViewRow({ view, region, index, count }: { view: ViewId; region: RegionId; index: number; count: number }) {
  const t = useT();
  const hidden = useLayout((s) => s.live.hidden.views.includes(view));
  const layout = useLayout.getState;
  return (
    <tr>
      <td>{t(VIEW_INFO[view].label)}</td>
      <td>
        <select
          className="select select--small"
          aria-label={t('settings.layout.viewRegion', { view: t(VIEW_INFO[view].label) })}
          value={region}
          onChange={(e) => layout().moveView(view, e.target.value as RegionId)}
        >
          {REGION_IDS.map((id) => (
            <option key={id} value={id}>
              {t(REGION_LABELS[id])}
            </option>
          ))}
        </select>
      </td>
      <td>
        <label className="check check--small" title={t('settings.layout.shown')}>
          <input
            type="checkbox"
            checked={!hidden}
            aria-label={t('settings.layout.viewShown', { view: t(VIEW_INFO[view].label) })}
            onChange={(e) => layout().setViewHidden(view, !e.target.checked)}
          />
          <span className="check__box" aria-hidden />
        </label>
      </td>
      <td className="layout-settings__order">
        <IconButton label={t('settings.layout.up')} disabled={index === 0} onClick={() => layout().moveView(view, region, index - 1)}>
          <ArrowUp size={14} />
        </IconButton>
        <IconButton label={t('settings.layout.down')} disabled={index === count - 1} onClick={() => layout().moveView(view, region, index + 1)}>
          <ArrowDown size={14} />
        </IconButton>
      </td>
    </tr>
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

  const bar = (key: Exclude<keyof LayoutBars, 'activitybar'>, label: string) => (
    <Checkbox checked={live.bars[key]} onChange={(value) => state().setBar(key, value)} label={label} />
  );

  return (
    <section className="form-section" id="settings-layout">
      <h2>{t('settings.layout')}</h2>
      <p className="muted">{t('settings.layout.hint')}</p>
      {zen && <p className="text-warn">{t('settings.layout.zen')}</p>}
      <fieldset className="layout-settings" disabled={zen}>
        {/* Não são Fields: o <label> passaria o clique no título ao primeiro botão. */}
        <div className="field">
          <span className="field__label">{t('settings.layout.active')}</span>
          <div className="input-group layout-settings__actions">
            <select className="select" value={layout.id} onChange={(e) => void applyLayout(e.target.value)}>
              {layouts.map((candidate) => (
                <option key={candidate.id} value={candidate.id}>
                  {candidate.preset ? `${candidate.name} (${t('layout.tag.preset')})` : candidate.name}
                </option>
              ))}
            </select>
            <Button variant="primary" disabled={!dirty && !layout.readOnly} onClick={() => void saveLayout()}>
              {t('action.saveLayout')}
            </Button>
            <Button onClick={() => void saveLayoutAs()}>{t('settings.layout.saveAs')}</Button>
            <Button disabled={!dirty} onClick={restoreLayout}>
              {t('action.restoreLayout')}
            </Button>
            <Button disabled={layout.readOnly} onClick={() => void renameLayout(layout.id)}>
              {t('settings.layout.rename')}
            </Button>
            <Button variant="danger" disabled={layout.readOnly} onClick={() => void deleteLayout(layout.id)}>
              {t('settings.layout.delete')}
            </Button>
          </div>
          <span className="field__hint">
            {dirty ? t('settings.layout.modified') : layout.readOnly ? t('settings.layout.readOnly') : t('settings.layout.saved')}
          </span>
        </div>

        <h3 className="layout-settings__subtitle">{t('settings.layout.bars')}</h3>
        <div className="layout-settings__grid">
          {bar('menubar', t('layout.bar.menubar'))}
          {bar('toolbar', t('layout.bar.toolbar'))}
          {bar('statusbar', t('layout.bar.statusbar'))}
        </div>
        <div className="form-row">
          <label className="field">
            <span className="field__label">{t('layout.bar.activitybar')}</span>
            <select
              className="select"
              value={live.bars.activitybar}
              onChange={(e) => state().setBar('activitybar', e.target.value as ActivityBarSide)}
            >
              <option value="left">{t('settings.layout.side.left')}</option>
              <option value="right">{t('settings.layout.side.right')}</option>
              <option value="hidden">{t('settings.layout.side.hidden')}</option>
            </select>
          </label>
          <label className="field">
            <span className="field__label">{t('settings.layout.panelPosition')}</span>
            <select
              className="select"
              value={live.panelPosition}
              onChange={(e) => state().setPanelPosition(e.target.value as PanelPosition)}
            >
              <option value="bottom">{t('settings.layout.panel.bottom')}</option>
              <option value="right">{t('settings.layout.panel.right')}</option>
            </select>
          </label>
        </div>

        <h3 className="layout-settings__subtitle">{t('settings.layout.regions')}</h3>
        <div className="layout-settings__grid">
          {REGION_IDS.map((id) => (
            <Checkbox
              key={id}
              checked={live.regions[id].visible}
              onChange={(value) => state().setRegionVisible(id, value)}
              label={t(REGION_LABELS[id])}
            />
          ))}
        </div>

        <h3 className="layout-settings__subtitle">{t('settings.layout.views')}</h3>
        <table className="table layout-settings__views">
          <thead>
            <tr>
              <th>{t('settings.layout.view')}</th>
              <th>{t('settings.layout.region')}</th>
              <th>{t('settings.layout.visibility')}</th>
              <th>{t('settings.layout.order')}</th>
            </tr>
          </thead>
          <tbody>
            {REGION_IDS.flatMap((region) =>
              live.regions[region].views.map((view, index, views) => (
                <ViewRow key={view} view={view} region={region} index={index} count={views.length} />
              )),
            )}
          </tbody>
        </table>

        <h3 className="layout-settings__subtitle">{t('settings.layout.toolbarItems')}</h3>
        <div className="layout-settings__grid">
          {TOOLBAR_ITEMS.map((item) => (
            <Checkbox
              key={item.id}
              checked={!live.hidden.toolbar.includes(item.id)}
              onChange={(shown) => state().setItemHidden('toolbar', item.id, !shown)}
              label={t(item.label)}
            />
          ))}
        </div>

        <h3 className="layout-settings__subtitle">{t('settings.layout.statusItems')}</h3>
        <div className="layout-settings__grid">
          {STATUS_ITEMS.map((item) => (
            <Checkbox
              key={item.id}
              checked={!live.hidden.statusbar.includes(item.id)}
              onChange={(shown) => state().setItemHidden('statusbar', item.id, !shown)}
              label={t(item.label)}
            />
          ))}
        </div>
      </fieldset>
    </section>
  );
}
