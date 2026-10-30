// As preferências, gravadas no settings.json do backend a cada mudança.
// Uma página por assunto, com a lista delas à esquerda (numa área estreita,
// em cima). A página aberta é lembrada (state/settingsPage.ts). Cada
// preferência é uma linha: o nome e a descrição à esquerda, o controle à
// direita (settingsParts.tsx). A página Experimental é outra coisa: liga os
// recursos experimentais da AURORA, que o Core guarda fora do settings.json e
// que valem também para a CLI (state/experimental.ts).

import { open } from '@tauri-apps/plugin-dialog';
import {
  Activity,
  FileCode,
  FlaskConical,
  LayoutTemplate,
  Package,
  Palette,
  SlidersHorizontal,
  type LucideIcon,
  Bot,
} from 'lucide-react';
import { useEffect, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from 'react';

import { restartShell } from '../../console/shell';
import { useT, type Key } from '../../i18n';
import type { Settings } from '../../ipc/types';
import { prefersDark, useApp } from '../../state/app';
import { useAssistant } from '../../state/assistant';
import { useExperimental, useFeature } from '../../state/experimental';
import { SETTINGS_PAGES, useSettingsPage, type SettingsPage } from '../../state/settingsPage';
import { DEFAULT_THEME, PALETTES, paletteName, paletteOf, paletteTheme, schemeOf, type Scheme, type SyntaxColors, type Theme } from '../../themes';
import { Button, Segmented, Switch } from '../common';
import { LayoutSettings } from './LayoutSettings';
import { SettingRow, SettingsGroup } from './settingsParts';

const PAGES: Record<SettingsPage, { label: Key; icon: LucideIcon }> = {
  general: { label: 'settings.general', icon: SlidersHorizontal },
  appearance: { label: 'settings.appearance', icon: Palette },
  layout: { label: 'settings.layout', icon: LayoutTemplate },
  editor: { label: 'settings.editor', icon: FileCode },
  simulation: { label: 'settings.simulation', icon: Activity },
  toolchain: { label: 'settings.toolchain', icon: Package },
  assistant: { label: 'settings.assistant', icon: Bot },
  experimental: { label: 'settings.experimental', icon: FlaskConical },
};

/** Abaixo desta largura, a lista de páginas vai para cima do conteúdo. */
const COMPACT_WIDTH = 640;

type Update = ReturnType<typeof useApp.getState>['updateSettings'];

/** Muda uma preferência e grava. */
function setter(update: Update) {
  return <K extends keyof Settings>(key: K, value: Settings[K]) => update((s) => ({ ...s, [key]: value }));
}

export function SettingsView() {
  const t = useT();
  const settings = useApp((s) => s.settings);
  const current = useSettingsPage((s) => s.page);
  const [root, setRoot] = useState<HTMLDivElement | null>(null);
  const [compact, setCompact] = useState(false);

  useEffect(() => {
    if (!root) return;
    const observer = new ResizeObserver(([entry]) => setCompact(entry.contentRect.width < COMPACT_WIDTH));
    observer.observe(root);
    return () => observer.disconnect();
  }, [root]);

  // A página Assistente é do assistente de IA, experimental: desligado, ela
  // some da lista, e quem estava nela cai na Geral.
  const assistant = useFeature('assistant');
  const pages = SETTINGS_PAGES.filter((id) => id !== 'assistant' || assistant);
  const page = pages.includes(current) ? current : 'general';

  if (!settings) return null;

  // As setas andam pela lista, como nas abas.
  const onKey = (e: KeyboardEvent<HTMLButtonElement>) => {
    const index = pages.indexOf(page);
    const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
    let next: number | null = step === undefined ? null : (index + step + pages.length) % pages.length;
    if (e.key === 'Home') next = 0;
    if (e.key === 'End') next = pages.length - 1;
    if (next === null) return;
    e.preventDefault();
    useSettingsPage.getState().setPage(pages[next]);
    document.getElementById(`settings-tab-${pages[next]}`)?.focus();
  };

  return (
    <div ref={setRoot} className={`settings${compact ? ' settings--compact' : ''}`}>
      <div className="settings__side">
        <h1 className="settings__title">{t('settings.title')}</h1>
        <div className="settings__nav" role="tablist" aria-orientation={compact ? 'horizontal' : 'vertical'} aria-label={t('settings.pages')}>
          {pages.map((id) => {
            const { label, icon: Icon } = PAGES[id];
            return (
              <button
                key={id}
                id={`settings-tab-${id}`}
                type="button"
                role="tab"
                aria-selected={page === id}
                aria-controls="settings-page"
                tabIndex={page === id ? 0 : -1}
                className={`settings__tab${page === id ? ' is-active' : ''}`}
                onClick={() => useSettingsPage.getState().setPage(id)}
                onKeyDown={onKey}
              >
                <Icon size={15} strokeWidth={1.7} />
                <span>{t(label)}</span>
              </button>
            );
          })}
        </div>
      </div>
      <div id="settings-page" className="settings__page" role="tabpanel" aria-labelledby={`settings-tab-${page}`}>
        <h2 className="settings__page-title">{t(PAGES[page].label)}</h2>
        {page === 'general' && <GeneralPage settings={settings} />}
        {page === 'appearance' && <AppearancePage settings={settings} />}
        {page === 'layout' && <LayoutSettings />}
        {page === 'editor' && <EditorPage settings={settings} />}
        {page === 'simulation' && <SimulationPage settings={settings} />}
        {page === 'toolchain' && <ToolchainPage settings={settings} />}
        {page === 'assistant' && <AssistantPage settings={settings} />}
        {page === 'experimental' && <ExperimentalPage />}
      </div>
    </div>
  );
}

function GeneralPage({ settings }: { settings: Settings }) {
  const t = useT();
  const set = setter(useApp((s) => s.updateSettings));
  // O `os` do backend: `windows-x86_64`, `linux-x86_64`, `macos-aarch64`.
  const windows = useApp((s) => s.info?.os.startsWith('windows') ?? false);
  return (
    <SettingsGroup>
      <SettingRow label={t('settings.language')}>
        {(id) => (
          <select id={id} className="select" value={settings.language} onChange={(e) => set('language', e.target.value as Settings['language'])}>
            <option value="system">{t('settings.language.system')}</option>
            <option value="pt">Português</option>
            <option value="en">English</option>
          </select>
        )}
      </SettingRow>
      <SettingRow label={t('settings.restoreLast')}>
        {(id) => <Switch id={id} checked={settings.restore_last_project} onChange={(v) => set('restore_last_project', v)} />}
      </SettingRow>
      <SettingRow label={t('settings.checkUpdates')} hint={t('settings.checkUpdatesHint')}>
        {(id) => <Switch id={id} checked={settings.check_updates} onChange={(v) => set('check_updates', v)} />}
      </SettingRow>
      {/* Nos outros sistemas o terminal abre o shell do usuário ($SHELL). */}
      {windows && (
        <SettingRow label={t('settings.terminalShell')} hint={t('settings.terminalShellHint')}>
          {(id) => (
            <select
              id={id}
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
          )}
        </SettingRow>
      )}
    </SettingsGroup>
  );
}

function AppearancePage({ settings }: { settings: Settings }) {
  const t = useT();
  const set = setter(useApp((s) => s.updateSettings));
  // O tema em uso muda quando o sistema troca de esquema: com ele, as prévias
  // do "Do sistema" acompanham.
  useApp((s) => s.theme);
  const scheme = schemeOf(settings.theme_mode, prefersDark());
  return (
    <SettingsGroup title={t('settings.theme')}>
      <SettingRow label={t('settings.theme.mode')}>
        {() => (
          <Segmented
            label={t('settings.theme.mode')}
            value={settings.theme_mode}
            options={[
              { value: 'dark', label: t('settings.theme.dark') },
              { value: 'light', label: t('settings.theme.light') },
              { value: 'system', label: t('settings.theme.system') },
            ]}
            onChange={(value) => set('theme_mode', value)}
          />
        )}
      </SettingRow>
      <div className="settings-group__block">
        <ThemeGrid value={settings.theme} scheme={scheme} onChange={(id) => set('theme', id)} />
      </div>
    </SettingsGroup>
  );
}

function EditorPage({ settings }: { settings: Settings }) {
  const t = useT();
  const update = useApp((s) => s.updateSettings);
  const setEditor = <K extends keyof Settings['editor']>(key: K, value: Settings['editor'][K]) =>
    update((s) => ({ ...s, editor: { ...s.editor, [key]: value } }));
  const setZen = <K extends keyof Settings['zen']>(key: K, value: Settings['zen'][K]) =>
    update((s) => ({ ...s, zen: { ...s.zen, [key]: value } }));
  return (
    <>
      <SettingsGroup>
        <SettingRow label={t('settings.fontSize')}>
          {(id) => (
            <input
              id={id}
              className="input input--narrow"
              type="number"
              min={8}
              max={32}
              value={settings.editor.font_size}
              onChange={(e) => setEditor('font_size', Math.max(8, Math.min(32, Number(e.target.value) || 13)))}
            />
          )}
        </SettingRow>
        <SettingRow label={t('settings.tabSize')}>
          {(id) => (
            <input
              id={id}
              className="input input--narrow"
              type="number"
              min={1}
              max={8}
              value={settings.editor.tab_size}
              onChange={(e) => setEditor('tab_size', Math.max(1, Math.min(8, Number(e.target.value) || 4)))}
            />
          )}
        </SettingRow>
        <SettingRow label={t('settings.wordWrap')}>
          {(id) => <Switch id={id} checked={settings.editor.word_wrap} onChange={(v) => setEditor('word_wrap', v)} />}
        </SettingRow>
        <SettingRow label={t('settings.minimap')}>
          {(id) => <Switch id={id} checked={settings.editor.minimap} onChange={(v) => setEditor('minimap', v)} />}
        </SettingRow>
        <SettingRow label={t('settings.autoSave')}>
          {(id) => <Switch id={id} checked={settings.editor.auto_save} onChange={(v) => setEditor('auto_save', v)} />}
        </SettingRow>
        <SettingRow label={t('settings.vimMode')}>
          {(id) => <Switch id={id} checked={settings.editor.vim_mode} onChange={(v) => setEditor('vim_mode', v)} />}
        </SettingRow>
      </SettingsGroup>
      <SettingsGroup title={t('settings.zen')} hint={t('settings.zenHint')}>
        <SettingRow label={t('settings.zen.fullscreen')}>
          {(id) => <Switch id={id} checked={settings.zen.fullscreen} onChange={(v) => setZen('fullscreen', v)} />}
        </SettingRow>
        <SettingRow label={t('settings.zen.center')}>
          {(id) => <Switch id={id} checked={settings.zen.center_layout} onChange={(v) => setZen('center_layout', v)} />}
        </SettingRow>
        <SettingRow label={t('settings.zen.tabs')}>
          {(id) => <Switch id={id} checked={settings.zen.show_tabs} onChange={(v) => setZen('show_tabs', v)} />}
        </SettingRow>
        <SettingRow label={t('settings.zen.hideLineNumbers')}>
          {(id) => <Switch id={id} checked={settings.zen.hide_line_numbers} onChange={(v) => setZen('hide_line_numbers', v)} />}
        </SettingRow>
      </SettingsGroup>
    </>
  );
}

function SimulationPage({ settings }: { settings: Settings }) {
  const t = useT();
  const set = setter(useApp((s) => s.updateSettings));
  const toolchain = useApp((s) => s.toolchain);
  const [timeout, setTimeoutText] = useState(settings.sim_timeout_s ? String(settings.sim_timeout_s) : '');

  useEffect(() => {
    setTimeoutText(settings.sim_timeout_s ? String(settings.sim_timeout_s) : '');
  }, [settings.sim_timeout_s]);

  const commitTimeout = () => {
    const value = Number.parseInt(timeout, 10);
    set('sim_timeout_s', Number.isFinite(value) && value > 0 ? value : null);
  };

  return (
    <SettingsGroup>
      <SettingRow label={t('settings.simulator')}>
        {() => (
          <Segmented
            label={t('settings.simulator')}
            value={settings.simulator}
            options={[
              { value: 'icarus', label: 'Icarus Verilog' },
              { value: 'verilator', label: 'Verilator' },
            ]}
            onChange={(value) => set('simulator', value)}
          />
        )}
      </SettingRow>
      <SettingRow
        label={t('settings.waveViewer')}
        hint={toolchain?.found && !toolchain.surfer_web ? t('settings.waveViewerNoWeb') : t('settings.waveViewerHint')}
      >
        {() => (
          <Segmented
            label={t('settings.waveViewer')}
            value={settings.wave_viewer}
            options={[
              { value: 'tab', label: t('settings.waveTab') },
              { value: 'window', label: t('settings.waveWindow') },
            ]}
            onChange={(value) => set('wave_viewer', value)}
          />
        )}
      </SettingRow>
      <SettingRow label={t('settings.openWave')}>
        {(id) => <Switch id={id} checked={settings.open_wave_after_sim} onChange={(v) => set('open_wave_after_sim', v)} />}
      </SettingRow>
      <SettingRow label={t('settings.timeout')} hint={t('settings.timeoutHint')}>
        {(id) => (
          <input
            id={id}
            className="input input--narrow"
            type="number"
            min={1}
            value={timeout}
            onChange={(e) => setTimeoutText(e.target.value)}
            onBlur={commitTimeout}
            onKeyDown={(e) => e.key === 'Enter' && commitTimeout()}
          />
        )}
      </SettingRow>
      <SettingRow label={t('settings.verbose')}>
        {(id) => <Switch id={id} checked={settings.verbose} onChange={(v) => set('verbose', v)} />}
      </SettingRow>
    </SettingsGroup>
  );
}

function ToolchainPage({ settings }: { settings: Settings }) {
  const t = useT();
  const set = setter(useApp((s) => s.updateSettings));
  const toolchain = useApp((s) => s.toolchain);
  // O Quartus só serve à placa FPGA, experimental: desligada, a linha some.
  const fpga = useFeature('fpga');

  const browse = async (key: 'toolchain_dir' | 'compiler_dir' | 'quartus_dir' | 'learn_dir') => {
    const chosen = await open({ directory: true, multiple: false });
    if (typeof chosen === 'string') set(key, chosen);
  };
  const browseAurora = async () => {
    const chosen = await open({ directory: false, multiple: false });
    if (typeof chosen === 'string') set('aurora_path', chosen);
  };

  return (
    <SettingsGroup>
      <SettingRow label={t('settings.toolchainDir')} hint={t('settings.toolchainDirHint')} stack>
        {(id) => (
          <div className="input-group">
            <input
              id={id}
              className="input"
              value={settings.toolchain_dir ?? ''}
              placeholder={toolchain?.root ?? ''}
              onChange={(e) => set('toolchain_dir', e.target.value || null)}
            />
            <Button onClick={() => void browse('toolchain_dir')}>{t('common.browse')}</Button>
          </div>
        )}
      </SettingRow>
      <SettingRow label={t('settings.auroraPath')} hint={t('settings.auroraPathHint')} stack>
        {(id) => (
          <div className="input-group">
            <input id={id} className="input" value={settings.aurora_path ?? ''} onChange={(e) => set('aurora_path', e.target.value || null)} />
            <Button onClick={() => void browseAurora()}>{t('common.browse')}</Button>
          </div>
        )}
      </SettingRow>
      <SettingRow label={t('settings.compilerDir')} hint={t('settings.compilerDirHint')} stack>
        {(id) => (
          <>
            <div className="input-group">
              <input id={id} className="input" value={settings.compiler_dir ?? ''} onChange={(e) => set('compiler_dir', e.target.value || null)} />
              <Button onClick={() => void browse('compiler_dir')}>{t('common.browse')}</Button>
            </div>
            {toolchain?.compiler_error && <p className="text-error">{toolchain.compiler_error.message}</p>}
          </>
        )}
      </SettingRow>
      {fpga && (
        <SettingRow label={t('settings.quartusDir')} hint={t('settings.quartusDirHint')} stack>
          {(id) => (
            <>
              <div className="input-group">
                <input
                  id={id}
                  className="input"
                  value={settings.quartus_dir ?? ''}
                  placeholder={toolchain?.quartus?.root ?? ''}
                  onChange={(e) => set('quartus_dir', e.target.value || null)}
                />
                <Button onClick={() => void browse('quartus_dir')}>{t('common.browse')}</Button>
              </div>
              {toolchain?.quartus_error && <p className="text-error">{toolchain.quartus_error.message}</p>}
            </>
          )}
        </SettingRow>
      )}
      <SettingRow label={t('settings.learnDir')} hint={t('settings.learnDirHint')} stack>
        {(id) => (
          <div className="input-group">
            <input id={id} className="input" value={settings.learn_dir ?? ''} onChange={(e) => set('learn_dir', e.target.value || null)} />
            <Button onClick={() => void browse('learn_dir')}>{t('common.browse')}</Button>
          </div>
        )}
      </SettingRow>
    </SettingsGroup>
  );
}

/**
 * O assistente: onde está o Claude Code, o `aurora` das ferramentas, o modelo
 * e o que ele pode fazer nos arquivos. A AURORA não guarda chave: quem
 * autentica é o Claude Code (o login da conta, ou ANTHROPIC_API_KEY).
 */
function AssistantPage({ settings }: { settings: Settings }) {
  const t = useT();
  const set = setter(useApp((s) => s.updateSettings));
  const info = useAssistant((s) => s.info);
  const browse = async (key: 'assistant_claude' | 'assistant_aurora') => {
    const picked = await open({ multiple: false });
    if (typeof picked === 'string') {
      set(key, picked);
      void useAssistant.getState().refreshInfo();
    }
  };
  return (
    <SettingsGroup hint={t('settings.assistantHint')}>
      <SettingRow label={t('settings.assistantClaude')} hint={t('settings.assistantClaudeHint')} stack>
        {(id) => (
          <>
            <div className="input-group">
              <input
                id={id}
                className="input"
                value={settings.assistant_claude ?? ''}
                placeholder={info?.claude ?? ''}
                onChange={(e) => set('assistant_claude', e.target.value || null)}
                onBlur={() => void useAssistant.getState().refreshInfo()}
              />
              <Button onClick={() => void browse('assistant_claude')}>{t('common.browse')}</Button>
            </div>
            {info && (info.claude ? <p className="muted">{t('assistant.found', { version: info.version ?? '' })}</p> : <p className="text-warn">{t('assistant.missing')}</p>)}
          </>
        )}
      </SettingRow>
      <SettingRow label={t('settings.assistantModel')} hint={t('settings.assistantModelHint')} stack>
        {(id) => <input id={id} className="input" value={settings.assistant_model ?? ''} placeholder="sonnet" onChange={(e) => set('assistant_model', e.target.value || null)} />}
      </SettingRow>
      <SettingRow label={t('settings.assistantAllowEdits')} hint={t('settings.assistantAllowEditsHint')}>
        {(id) => <Switch id={id} checked={settings.assistant_allow_edits} onChange={(v) => set('assistant_allow_edits', v)} />}
      </SettingRow>
      <SettingRow label={t('settings.assistantAurora')} hint={t('settings.assistantAuroraHint')} stack>
        {(id) => (
          <div className="input-group">
            <input
              id={id}
              className="input"
              value={settings.assistant_aurora ?? ''}
              placeholder={info?.aurora ?? ''}
              onChange={(e) => set('assistant_aurora', e.target.value || null)}
              onBlur={() => void useAssistant.getState().refreshInfo()}
            />
            <Button onClick={() => void browse('assistant_aurora')}>{t('common.browse')}</Button>
          </div>
        )}
      </SettingRow>
    </SettingsGroup>
  );
}

/**
 * Os recursos experimentais da AURORA, um interruptor por recurso. Ligar ou
 * desligar grava no experimental.json do Core e vale na hora: o que é do
 * recurso aparece ou some da janela, e a CLI passa a aceitar ou recusar os
 * comandos dele.
 */
function ExperimentalPage() {
  const t = useT();
  const components = useExperimental((s) => s.components);
  const path = useExperimental((s) => s.path);
  return (
    <SettingsGroup
      hint={
        <>
          {t('settings.experimental.hint')}
          {path && (
            <span className="settings-experimental__file">
              {t('settings.experimental.file')} <code>{path}</code>
            </span>
          )}
        </>
      }
    >
      {components.length === 0 && <div className="settings-group__block muted">{t('settings.experimental.none')}</div>}
      {components.map(({ feature, enabled }) => (
        <SettingRow key={feature} label={t(`experimental.${feature}`)} hint={t(`experimental.${feature}.hint`)}>
          {(id) => <Switch id={id} checked={enabled} onChange={(value) => void useExperimental.getState().set(feature, value)} />}
        </SettingRow>
      ))}
    </SettingsGroup>
  );
}

/**
 * A escolha da paleta: um cartão por paleta, com uma prévia desenhada no
 * esquema do modo; a paleta só escura mostra a prévia escura e diz isso.
 * Clicar aplica e grava na hora.
 */
function ThemeGrid({ value, scheme, onChange }: { value: string; scheme: Scheme; onChange: (id: string) => void }) {
  const t = useT();
  const selected = paletteOf(value).id;
  return (
    <div className="theme-grid" role="radiogroup" aria-label={t('settings.theme')}>
      {PALETTES.map((palette) => (
        <ThemeCard
          key={palette.id}
          name={paletteName(palette)}
          note={
            palette.id === DEFAULT_THEME
              ? t('settings.theme.default')
              : palette.pair
                ? undefined
                : t('settings.theme.darkOnly')
          }
          selected={selected === palette.id}
          onSelect={() => onChange(palette.id)}
        >
          <ThemePreview theme={paletteTheme(palette, scheme)} />
        </ThemeCard>
      ))}
    </div>
  );
}

/** Um cartão da grade. `note` aparece ao lado do nome (o "padrão" do Boreal,
 * o "só escuro" do Dracula). */
function ThemeCard({
  name,
  note,
  selected,
  onSelect,
  children,
}: {
  name: string;
  note?: string;
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
      title={name}
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
function ThemePreview({ theme }: { theme: Theme }) {
  const { ui, syntax } = theme;
  const color = (value: string): CSSProperties => ({ background: value });
  return (
    <span className="theme-preview" style={{ background: ui.bgEditor }} aria-hidden>
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
