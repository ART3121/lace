// As preferências, gravadas no settings.json do backend a cada mudança.

import { open } from '@tauri-apps/plugin-dialog';
import { useEffect, useState, type CSSProperties, type ReactNode } from 'react';

import { restartShell } from '../../console/shell';
import { useT } from '../../i18n';
import type { Settings } from '../../ipc/types';
import { useApp } from '../../state/app';
import { DEFAULT_LIGHT_THEME, DEFAULT_THEME, SYSTEM_THEME, THEMES, themeById, type SyntaxColors, type Theme } from '../../themes';
import { Button, Checkbox, Field } from '../common';

export function SettingsView() {
  const t = useT();
  const settings = useApp((s) => s.settings);
  const toolchain = useApp((s) => s.toolchain);
  const update = useApp((s) => s.updateSettings);
  // O `os` do backend: `windows-x86_64`, `linux-x86_64`, `macos-aarch64`.
  const windows = useApp((s) => s.info?.os.startsWith('windows') ?? false);
  const [timeout, setTimeoutText] = useState('');

  useEffect(() => {
    setTimeoutText(settings?.sim_timeout_s ? String(settings.sim_timeout_s) : '');
  }, [settings?.sim_timeout_s]);

  if (!settings) return null;

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => update((s) => ({ ...s, [key]: value }));
  const setEditor = <K extends keyof Settings['editor']>(key: K, value: Settings['editor'][K]) =>
    update((s) => ({ ...s, editor: { ...s.editor, [key]: value } }));
  const setZen = <K extends keyof Settings['zen']>(key: K, value: Settings['zen'][K]) =>
    update((s) => ({ ...s, zen: { ...s.zen, [key]: value } }));

  const browse = async (key: 'toolchain_dir' | 'compiler_dir') => {
    const chosen = await open({ directory: true, multiple: false });
    if (typeof chosen === 'string') set(key, chosen);
  };

  const commitTimeout = () => {
    const value = Number.parseInt(timeout, 10);
    set('sim_timeout_s', Number.isFinite(value) && value > 0 ? value : null);
  };

  return (
    <div className="view-page settings">
      <h1>{t('settings.title')}</h1>

      <section className="form-section">
        <h2>{t('settings.general')}</h2>
        <Field label={t('settings.language')}>
          <select className="select" value={settings.language} onChange={(e) => set('language', e.target.value as Settings['language'])}>
            <option value="system">{t('settings.language.system')}</option>
            <option value="pt">Português</option>
            <option value="en">English</option>
          </select>
        </Field>
        <Checkbox
          checked={settings.restore_last_project}
          onChange={(v) => set('restore_last_project', v)}
          label={t('settings.restoreLast')}
        />
      </section>

      <section className="form-section">
        <h2>{t('settings.appearance')}</h2>
        {/* Não é um Field: o <label> passaria o clique no título ao primeiro cartão. */}
        <div className="field">
          <span className="field__label">{t('settings.theme')}</span>
          <ThemeGrid value={settings.theme} onChange={(id) => set('theme', id)} />
        </div>
      </section>

      <section className="form-section">
        <h2>{t('settings.terminal')}</h2>
        {windows ? (
          <Field label={t('settings.terminalShell')} hint={t('settings.terminalShellHint')}>
            <select
              className="select"
              value={settings.terminal_shell}
              onChange={(e) => {
                // Gravada a preferência, o terminal aberto passa ao shell novo.
                void set('terminal_shell', e.target.value as Settings['terminal_shell']).then(() => restartShell());
              }}
            >
              <option value="powershell">PowerShell</option>
              <option value="cmd">{t('settings.terminalShell.cmd')}</option>
            </select>
          </Field>
        ) : (
          <p className="muted">{t('settings.terminalShellUnix')}</p>
        )}
      </section>

      <section className="form-section">
        <h2>{t('settings.editor')}</h2>
        <div className="form-row">
          <Field label={t('settings.fontSize')}>
            <input
              className="input input--narrow"
              type="number"
              min={8}
              max={32}
              value={settings.editor.font_size}
              onChange={(e) => setEditor('font_size', Math.max(8, Math.min(32, Number(e.target.value) || 13)))}
            />
          </Field>
          <Field label={t('settings.tabSize')}>
            <input
              className="input input--narrow"
              type="number"
              min={1}
              max={8}
              value={settings.editor.tab_size}
              onChange={(e) => setEditor('tab_size', Math.max(1, Math.min(8, Number(e.target.value) || 4)))}
            />
          </Field>
        </div>
        <Checkbox checked={settings.editor.word_wrap} onChange={(v) => setEditor('word_wrap', v)} label={t('settings.wordWrap')} />
        <Checkbox checked={settings.editor.minimap} onChange={(v) => setEditor('minimap', v)} label={t('settings.minimap')} />
        <Checkbox checked={settings.editor.auto_save} onChange={(v) => setEditor('auto_save', v)} label={t('settings.autoSave')} />
        <Checkbox checked={settings.editor.vim_mode} onChange={(v) => setEditor('vim_mode', v)} label={t('settings.vimMode')} />
      </section>

      <section className="form-section">
        <h2>{t('settings.zen')}</h2>
        <p className="muted">{t('settings.zenHint')}</p>
        <Checkbox checked={settings.zen.fullscreen} onChange={(v) => setZen('fullscreen', v)} label={t('settings.zen.fullscreen')} />
        <Checkbox checked={settings.zen.center_layout} onChange={(v) => setZen('center_layout', v)} label={t('settings.zen.center')} />
        <Checkbox checked={settings.zen.show_tabs} onChange={(v) => setZen('show_tabs', v)} label={t('settings.zen.tabs')} />
        <Checkbox
          checked={settings.zen.hide_line_numbers}
          onChange={(v) => setZen('hide_line_numbers', v)}
          label={t('settings.zen.hideLineNumbers')}
        />
      </section>

      <section className="form-section">
        <h2>{t('settings.simulation')}</h2>
        <Field label={t('settings.simulator')}>
          <select className="select" value={settings.simulator} onChange={(e) => set('simulator', e.target.value as Settings['simulator'])}>
            <option value="icarus">Icarus Verilog</option>
            <option value="verilator">Verilator</option>
          </select>
        </Field>
        <Field label={t('settings.timeout')} hint={t('settings.timeoutHint')}>
          <input
            className="input input--narrow"
            type="number"
            min={1}
            value={timeout}
            onChange={(e) => setTimeoutText(e.target.value)}
            onBlur={commitTimeout}
            onKeyDown={(e) => e.key === 'Enter' && commitTimeout()}
          />
        </Field>
        <Field
          label={t('settings.waveViewer')}
          hint={toolchain?.found && !toolchain.surfer_web ? t('settings.waveViewerNoWeb') : t('settings.waveViewerHint')}
        >
          <select
            className="select"
            value={settings.wave_viewer}
            onChange={(e) => set('wave_viewer', e.target.value as Settings['wave_viewer'])}
          >
            <option value="tab">{t('settings.waveTab')}</option>
            <option value="window">{t('settings.waveWindow')}</option>
          </select>
        </Field>
        <Checkbox checked={settings.open_wave_after_sim} onChange={(v) => set('open_wave_after_sim', v)} label={t('settings.openWave')} />
        <Checkbox checked={settings.verbose} onChange={(v) => set('verbose', v)} label={t('settings.verbose')} />
      </section>

      <section className="form-section">
        <h2>{t('settings.toolchain')}</h2>
        <Field label={t('settings.toolchainDir')} hint={t('settings.toolchainDirHint')}>
          <div className="input-group">
            <input
              className="input"
              value={settings.toolchain_dir ?? ''}
              placeholder={toolchain?.root ?? ''}
              onChange={(e) => set('toolchain_dir', e.target.value || null)}
            />
            <Button onClick={() => void browse('toolchain_dir')}>{t('common.browse')}</Button>
          </div>
        </Field>
        <Field label={t('settings.compilerDir')} hint={t('settings.compilerDirHint')}>
          <div className="input-group">
            <input
              className="input"
              value={settings.compiler_dir ?? ''}
              onChange={(e) => set('compiler_dir', e.target.value || null)}
            />
            <Button onClick={() => void browse('compiler_dir')}>{t('common.browse')}</Button>
          </div>
        </Field>
        {toolchain?.compiler_error && <p className="text-error">{toolchain.compiler_error.message}</p>}
      </section>
    </div>
  );
}

/**
 * A escolha do tema: um cartão por tema, com uma prévia desenhada nas cores
 * dele, e o "Do sistema" primeiro. Clicar aplica e grava na hora.
 */
function ThemeGrid({ value, onChange }: { value: string; onChange: (id: string) => void }) {
  const t = useT();
  const selected = themeById(value) ? value : value === SYSTEM_THEME ? SYSTEM_THEME : DEFAULT_THEME;
  return (
    <div className="theme-grid" role="radiogroup" aria-label={t('settings.theme')}>
      <ThemeCard
        name={t('settings.theme.system')}
        hint={t('settings.theme.systemHint')}
        selected={selected === SYSTEM_THEME}
        onSelect={() => onChange(SYSTEM_THEME)}
      >
        <ThemePreview theme={themeById(DEFAULT_THEME)!} />
        <ThemePreview theme={themeById(DEFAULT_LIGHT_THEME)!} half />
      </ThemeCard>
      {THEMES.map((theme) => (
        <ThemeCard
          key={theme.id}
          name={theme.name}
          note={theme.id === DEFAULT_THEME ? t('settings.theme.default') : undefined}
          selected={selected === theme.id}
          onSelect={() => onChange(theme.id)}
        >
          <ThemePreview theme={theme} />
        </ThemeCard>
      ))}
    </div>
  );
}

/** Um cartão da grade. `note` aparece ao lado do nome (o "padrão" do Atlas);
 * `hint`, só ao passar o mouse. */
function ThemeCard({
  name,
  note,
  hint,
  selected,
  onSelect,
  children,
}: {
  name: string;
  note?: string;
  hint?: string;
  selected: boolean;
  onSelect: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      className={`theme-card${selected ? ' is-selected' : ''}`}
      title={hint ? `${name}: ${hint}` : name}
      onClick={onSelect}
    >
      <span className="theme-card__preview">{children}</span>
      <span className="theme-card__name">
        {name}
        {note && <span className="theme-card__note">{note}</span>}
      </span>
    </button>
  );
}

/** As linhas de código da prévia: cada trecho é um papel da sintaxe e uma largura. */
const PREVIEW_LINES: [Exclude<keyof SyntaxColors, 'style'>, number][][] = [
  [['directive', 7], ['constant', 5], ['number', 2]],
  [['type', 4], ['fg', 3], ['delimiter', 1], ['type', 4], ['fg', 2], ['delimiter', 1]],
  [['keyword', 3], ['fg', 2], ['operator', 1], ['func', 5], ['delimiter', 1], ['string', 6]],
  [['comment', 15]],
  [['control', 4], ['fg', 2], ['operator', 1], ['number', 3]],
];

/**
 * Um Studio em miniatura nas cores do tema: barra de atividades, barra
 * lateral com o item ativo, aba com a linha de destaque, código e barra de
 * status. As cores vêm do tema que ela mostra, não do tema em uso.
 */
function ThemePreview({ theme, half = false }: { theme: Theme; half?: boolean }) {
  const { ui, syntax } = theme;
  const color = (value: string): CSSProperties => ({ background: value });
  return (
    <span className={`theme-preview${half ? ' theme-preview--half' : ''}`} style={{ background: ui.bgEditor }} aria-hidden>
      <span className="theme-preview__activity" style={color(ui.bg0)}>
        <span style={color(ui.brand)} />
      </span>
      <span className="theme-preview__side" style={{ background: ui.bg1, borderColor: ui.border }}>
        {[0, 1, 2].map((i) => (
          <span key={i} style={color(i === 1 ? ui.text1 : ui.text3)} />
        ))}
      </span>
      <span className="theme-preview__main">
        <span className="theme-preview__tabs" style={{ background: ui.bg0, borderColor: ui.border }}>
          <span style={{ background: ui.bgEditor, borderColor: ui.brand }} />
        </span>
        <span className="theme-preview__code">
          {PREVIEW_LINES.map((line, i) => (
            <span key={i} className="theme-preview__line">
              {line.map(([role, width], j) => (
                <span key={j} style={{ background: syntax[role], width: width * 3 }} />
              ))}
            </span>
          ))}
        </span>
        <span className="theme-preview__status" style={{ background: ui.bg0, borderColor: ui.border }}>
          <span style={color(ui.brandFill)} />
        </span>
      </span>
    </span>
  );
}
