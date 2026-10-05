// A aba Sobre: o que é o Studio, as versões (do Studio, do Lace e dos
// componentes do bundle, do Tauri e do motor de páginas), onde ficam as
// preferências e o bundle, e os links. "Copiar informações" junta tudo num
// texto, para anexar a um relato de problema.

import { openUrl, revealItemInDir } from '@tauri-apps/plugin-opener';
import { BookOpen, Check, Copy, Download, ExternalLink, FolderOpen, Wrench } from 'lucide-react';
import { Fragment, useState } from 'react';

import { LACE_CLI_DOCS, NIPSCERN_GITHUB, NIPSCERN_SITE, runAction, SAPHO_MANUAL } from '../../actions';
import { t, useT } from '../../i18n';
import type { AppInfo, ToolchainInfo } from '../../ipc/types';
import { useApp } from '../../state/app';
import { useEditor } from '../../state/editor';
import { showError } from '../../state/toasts';
import { copyText } from '../../util/clipboard';
import { Button, IconButton } from '../common';

/** O texto de "Copiar informações", sempre em inglês, como as mensagens do
 * Lace: é para quem vai ler o relato. */
function diagnostics(info: AppInfo | null, toolchain: ToolchainInfo | null): string {
  const lines = [`Lace Studio ${info?.version ?? '?'}${info?.debug ? ' (development build)' : ''}`];
  lines.push(`System: ${info?.os ?? '?'}`);
  if (toolchain?.found) {
    lines.push(`Lace bundle: ${toolchain.bundle} (${toolchain.platform}), from ${toolchain.origin ?? '?'}: ${toolchain.root}`);
    lines.push(`Components: ${toolchain.components.map((c) => `${c.name} ${c.version}`).join(', ') || 'none'}`);
    if (toolchain.not_installed.length) lines.push(`Not installed: ${toolchain.not_installed.join(', ')}`);
    lines.push(`lace CLI: ${toolchain.lace_cli ?? 'not found'}`);
  } else {
    lines.push(`Lace bundle: not found${toolchain?.error ? ` (${toolchain.error.message})` : ''}`);
  }
  lines.push(`Tauri ${info?.tauri_version ?? '?'}, ${info ? engineName(info.os) : 'web engine'} ${info?.webview_version ?? 'unknown'}`);
  if (info?.config_dir) lines.push(`Settings: ${info.config_dir}`);
  return lines.join('\n');
}

/** O motor de páginas que o Tauri usa em cada sistema. */
function engineName(os: string): string {
  if (os.startsWith('windows')) return 'WebView2';
  if (os.startsWith('macos')) return 'WKWebView';
  return 'WebKitGTK';
}

function Link({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <button type="button" className="about__link" onClick={() => void openUrl(href).catch(showError)}>
      {children}
      <ExternalLink size={12} />
    </button>
  );
}

function PathValue({ path }: { path: string | null | undefined }) {
  if (!path) return <dd>-</dd>;
  return (
    <dd className="about__path">
      <span className="mono">{path}</span>
      <IconButton label={t('common.reveal')} onClick={() => void revealItemInDir(path).catch(showError)}>
        <FolderOpen size={13} />
      </IconButton>
    </dd>
  );
}

export function AboutView() {
  useT();
  const info = useApp((s) => s.info);
  const toolchain = useApp((s) => s.toolchain);
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await copyText(diagnostics(info, toolchain));
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch (error) {
      showError(error);
    }
  };

  const found = !!toolchain?.found;

  return (
    <div className="view-page about">
      <header className="about__header">
        <img src="/brand/lace-icon.svg" alt="" className="about__logo" />
        <div>
          <h1>Lace Studio</h1>
          <p className="muted">
            {t('about.version', { version: info?.version ?? '' })}
            {info?.debug ? ` (${t('about.debugBuild')})` : ''}
          </p>
        </div>
      </header>
      <p className="about__lead">{t('about.description')}</p>
      <div className="button-row">
        <Button icon={copied ? <Check size={14} /> : <Copy size={14} />} onClick={() => void copy()}>
          {copied ? t('about.copied') : t('about.copy')}
        </Button>
        <Button icon={<Wrench size={14} />} onClick={() => useEditor.getState().openView('toolchain')}>
          {t('toolchain.title')}
        </Button>
        <Button icon={<Download size={14} />} disabled={!toolchain?.lace_cli} onClick={() => runAction('checkUpdates')}>
          {t('toolchain.checkUpdates')}
        </Button>
      </div>

      <div className="about__columns">
        <section>
          <h2>{t('about.versions')}</h2>
          <dl className="kv">
            <dt>Lace Studio</dt>
            <dd className="mono">{info?.version}</dd>
            <dt>{t('about.lace')}</dt>
            <dd className="mono">{found ? `${toolchain!.bundle} (${toolchain!.platform})` : t('about.laceMissing')}</dd>
            <dt>Tauri</dt>
            <dd className="mono">{info?.tauri_version}</dd>
            <dt>{t('about.webview')}</dt>
            <dd className="mono">{info?.webview_version ? `${engineName(info.os)} ${info.webview_version}` : '-'}</dd>
          </dl>

          {found && (
            <>
              <h2>{t('about.components')}</h2>
              <dl className="kv">
                {toolchain!.components.map((c) => (
                  <Fragment key={c.name}>
                    <dt>{c.name}</dt>
                    <dd className="mono">{c.version}</dd>
                  </Fragment>
                ))}
                {toolchain!.not_installed.map((name) => (
                  <Fragment key={name}>
                    <dt>{name}</dt>
                    <dd className="muted">{t('about.notInstalled')}</dd>
                  </Fragment>
                ))}
              </dl>
            </>
          )}
        </section>

        <section>
          <h2>{t('about.system')}</h2>
          <dl className="kv">
            <dt>{t('about.os')}</dt>
            <dd className="mono">{info?.os}</dd>
            <dt>{t('about.config')}</dt>
            <PathValue path={info?.config_dir} />
            <dt>{t('toolchain.root')}</dt>
            <PathValue path={toolchain?.root} />
            <dt>{t('toolchain.laceCli')}</dt>
            <dd className="mono">{toolchain?.lace_cli ?? '-'}</dd>
          </dl>

          <h2>{t('about.links')}</h2>
          <ul className="about__links">
            <li>
              <BookOpen size={14} />
              <Link href={LACE_CLI_DOCS}>{t('about.laceDocs')}</Link>
            </li>
            <li>
              <BookOpen size={14} />
              <Link href={SAPHO_MANUAL}>{t('about.saphoManual')}</Link>
            </li>
            <li>
              <ExternalLink size={14} />
              <Link href={NIPSCERN_SITE}>nipscern.com</Link>
            </li>
            <li>
              <ExternalLink size={14} />
              <Link href={NIPSCERN_GITHUB}>github.com/nipscernlab</Link>
            </li>
          </ul>
        </section>
      </div>

      <p className="muted about__credit">{t('about.credit')}</p>
    </div>
  );
}
