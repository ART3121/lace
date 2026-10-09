// O bundle do Lace: o equivalente gráfico do `lace tools`, com a conferência
// dos hashes, a busca de atualização (`lace update --check`), a atualização
// (`lace update --yes`, depois de confirmar) e a instalação de componentes
// (`lace install`). A saída da última instalação ou atualização aparece aqui.

import { CircleArrowUp, CircleCheck, CircleX, Download, RefreshCw, ShieldCheck } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { useT } from '../../i18n';
import { api } from '../../ipc/api';
import type { FileMismatch, UpdateReport } from '../../ipc/lace-types';
import { useApp } from '../../state/app';
import { confirm, openDialog } from '../../state/dialogs';
import { useEditor } from '../../state/editor';
import { useJobs } from '../../state/jobs';
import { guarded } from '../../state/toasts';
import { Badge, Button, Spinner } from '../common';

export function ToolchainView({ checkToken }: { checkToken: string | null }) {
  const t = useT();
  const info = useApp((s) => s.toolchain);
  const running = useJobs((s) => s.running);
  const cliLog = useJobs((s) => s.cliLog);
  const logRef = useRef<HTMLPreElement>(null);
  const windows = useApp((s) => s.info?.os.startsWith('windows') ?? false);
  const [mismatches, setMismatches] = useState<FileMismatch[] | null>(null);
  const [verifying, setVerifying] = useState(false);
  const [updates, setUpdates] = useState<UpdateReport | null>(null);
  const [checking, setChecking] = useState(false);

  const verify = async () => {
    setVerifying(true);
    setMismatches((await guarded(() => api.toolchain.verify())) ?? null);
    setVerifying(false);
  };

  const checkUpdates = async () => {
    setChecking(true);
    setUpdates(((await guarded(() => api.toolchain.updateCheck())) as UpdateReport | undefined) ?? null);
    setChecking(false);
  };

  // A CLI perguntaria no terminal; aqui a pergunta é o diálogo, e o `--yes`
  // vai depois do sim.
  const update = async (report: UpdateReport) => {
    const answer = await confirm({
      title: t('dialog.update.title'),
      message: t(windows ? 'dialog.update.messageWindows' : 'dialog.update.message', {
        installed: report.lace.installed,
        latest: report.lace.latest,
        prefix: report.prefix ?? '',
      }),
      buttons: [
        { label: t('toolchain.update', { version: report.lace.latest }), value: 'update', primary: true },
        { label: t('common.cancel'), value: 'cancel' },
      ],
    });
    if (answer !== 'update') return;
    const result = await useJobs.getState().update();
    // Atualizado, a tabela passa a vir do lace novo. Com o assistente do
    // Windows, ela espera o usuário terminar lá.
    if (result?.action === 'updated') {
      setUpdates(null);
      void checkUpdates();
    }
  };

  useEffect(() => {
    if (checkToken) void checkUpdates();
  }, [checkToken]);

  // A saída rola para o fim a cada linha nova.
  useEffect(() => {
    const log = logRef.current;
    if (log) log.scrollTop = log.scrollHeight;
  }, [cliLog?.lines.length]);

  if (!info) return <div className="view-page">{t('common.loading')}</div>;

  if (!info.found) {
    return (
      <div className="view-page">
        <h1>{t('toolchain.notFound')}</h1>
        <p className="muted">{info.error?.message}</p>
        <p>{t('toolchain.notFoundHint')}</p>
        <div className="button-row">
          <Button onClick={() => useEditor.getState().openView('settings')}>{t('toolchain.openSettings')}</Button>
          <Button icon={<RefreshCw size={14} />} onClick={() => void useApp.getState().refreshToolchain()}>
            {t('common.refresh')}
          </Button>
        </div>
      </div>
    );
  }

  const fileCount = info.components.reduce((n, c) => n + Object.keys(c.files ?? {}).length, 0);

  return (
    <div className="view-page toolchain">
      <div className="view-page__title">
        <h1>{t('toolchain.title')}</h1>
        <div className="button-row">
          <Button icon={<RefreshCw size={14} />} onClick={() => void useApp.getState().refreshToolchain()}>
            {t('common.refresh')}
          </Button>
          <Button icon={verifying ? <Spinner /> : <ShieldCheck size={14} />} disabled={verifying} onClick={() => void verify()}>
            {t('toolchain.verify')}
          </Button>
          <Button icon={checking ? <Spinner /> : <Download size={14} />} disabled={checking || !info.lace_cli} onClick={() => void checkUpdates()}>
            {t('toolchain.checkUpdates')}
          </Button>
        </div>
      </div>

      <dl className="kv">
        <dt>{t('toolchain.bundle')}</dt>
        <dd>{info.bundle}</dd>
        <dt>{t('toolchain.platform')}</dt>
        <dd>{info.platform}</dd>
        <dt>{t('toolchain.root')}</dt>
        <dd className="mono">{info.root}</dd>
        <dt>{t('toolchain.origin')}</dt>
        <dd>{info.origin ? t(`toolchain.origin.${info.origin}`) : '-'}</dd>
        <dt>{t('toolchain.laceCli')}</dt>
        <dd className="mono">{info.lace_cli ?? '-'}</dd>
      </dl>

      {cliLog && (
        <>
          <h2>{t('toolchain.output', { command: cliLog.command })}</h2>
          <pre className="cli-log" ref={logRef}>
            {cliLog.lines.length === 0 && running ? <Spinner /> : null}
            {cliLog.lines.map((line, index) => (
              <div key={index} className={`cli-log__line cli-log__line--${line.style}`}>
                {line.text}
              </div>
            ))}
          </pre>
        </>
      )}

      {mismatches && (
        <div className={`notice${mismatches.length ? ' notice--error' : ' notice--ok'}`}>
          {mismatches.length === 0 ? (
            t('toolchain.verifyOk', { count: fileCount })
          ) : (
            <>
              {t('toolchain.verifyBad', { count: mismatches.length })}
              <ul>
                {mismatches.map((m) => (
                  <li key={m.path} className="mono">
                    {m.component}: {m.path}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}

      {updates && (
        <>
          <h2>{t('toolchain.updates')}</h2>
          {updates.lace.newer ? (
            <div className="notice update-offer">
              <p>{t('toolchain.updateAvailable', { latest: updates.lace.latest, installed: updates.lace.installed })}</p>
              {updates.prefix ? (
                <Button
                  variant="primary"
                  icon={running?.flow === 'update' ? <Spinner /> : <CircleArrowUp size={14} />}
                  disabled={!!running}
                  onClick={() => void update(updates)}
                >
                  {t('toolchain.update', { version: updates.lace.latest })}
                </Button>
              ) : (
                <p className="muted">{t('toolchain.updateNoInstaller')}</p>
              )}
            </div>
          ) : (
            <p className="muted">{t('toolchain.upToDate', { latest: updates.lace.latest })}</p>
          )}
          <table className="table">
            <thead>
              <tr>
                <th />
                <th>{t('toolchain.version')}</th>
                <th>release</th>
                <th>upstream</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>Lace</td>
                <td className="mono">{updates.lace.installed}</td>
                <td className="mono">
                  {updates.lace.latest} {updates.lace.newer && <Badge tone="warn">new</Badge>}
                </td>
                <td />
              </tr>
              <tr>
                <td>bundle</td>
                <td className="mono">{updates.bundle.installed ?? '-'}</td>
                <td className="mono">
                  {updates.bundle.latest ?? '?'} {updates.bundle.newer && <Badge tone="warn">new</Badge>}
                </td>
                <td />
              </tr>
              {updates.components.map((c) => (
                <tr key={c.name}>
                  <td>{c.name}</td>
                  <td className="mono">{c.installed}</td>
                  <td className="mono">
                    {c.release ?? '?'} {c.release_newer && <Badge tone="warn">new</Badge>}
                  </td>
                  <td className="mono">
                    {c.upstream ?? '?'} {c.upstream_newer && <Badge>new</Badge>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {updates.components.some((c) => c.upstream_newer) && <p className="muted">{t('toolchain.upstreamNote')}</p>}
        </>
      )}

      <h2>{t('toolchain.components')}</h2>
      <table className="table">
        <thead>
          <tr>
            <th>{t('toolchain.component')}</th>
            <th>{t('toolchain.version')}</th>
            <th>{t('toolchain.source')}</th>
          </tr>
        </thead>
        <tbody>
          {info.components.map((c) => (
            <tr key={c.name}>
              <td>{c.name}</td>
              <td className="mono">{c.version}</td>
              <td className="mono truncate" title={c.source}>
                {c.source}
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      {info.not_installed.length > 0 && (
        <>
          <h2>{t('toolchain.notInstalled')}</h2>
          <div className="chip-row">
            {info.not_installed.map((name) => (
              <span key={name} className="chip">
                {name}
                <button
                  type="button"
                  className="link"
                  disabled={!!running || !info.lace_cli}
                  onClick={() => openDialog({ kind: 'install', components: [name] })}
                >
                  {t('toolchain.install')}
                </button>
              </span>
            ))}
          </div>
        </>
      )}

      <h2>{t('toolchain.tools')}</h2>
      <table className="table">
        <thead>
          <tr>
            <th>{t('toolchain.tool')}</th>
            <th>{t('toolchain.component')}</th>
            <th>{t('toolchain.path')}</th>
          </tr>
        </thead>
        <tbody>
          {info.tools.map((tool) => (
            <tr key={tool.name}>
              <td>
                <span className="inline-icon">
                  {tool.path ? <CircleCheck size={13} className="text-ok" /> : <CircleX size={13} className="text-muted" />}
                  {tool.name}
                </span>
              </td>
              <td>{tool.component ?? t('toolchain.system')}</td>
              <td className="mono truncate" title={tool.path ?? tool.error?.message}>
                {tool.path ?? <span className="text-muted">{tool.error?.message ?? t('toolchain.missing')}</span>}
                {tool.system && <Badge>{t('toolchain.system')}</Badge>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      <h2>{t('toolchain.compiler')}</h2>
      {info.system_compiler ? (
        <dl className="kv">
          <dt>C++</dt>
          <dd className="mono">
            {info.system_compiler.cxx}{' '}
            <Badge>{info.system_compiler.bundled ? t('toolchain.compilerBundled') : t('toolchain.compilerSystem')}</Badge>
          </dd>
          <dt>make</dt>
          <dd className="mono">{info.system_compiler.make}</dd>
          <dt>perl</dt>
          <dd className="mono">{info.system_compiler.perl}</dd>
        </dl>
      ) : (
        <p className="muted">{t('toolchain.compilerNone')}</p>
      )}
      {info.compiler_error && <p className="text-error">{info.compiler_error.message}</p>}

      <h2>{t('toolchain.quartus')}</h2>
      {info.quartus ? (
        <dl className="kv">
          <dt>{t('toolchain.quartusVersion')}</dt>
          <dd>
            {info.quartus.version ?? '?'} <Badge>{t('toolchain.system')}</Badge>
          </dd>
          <dt>{t('toolchain.quartusRoot')}</dt>
          <dd className="mono">{info.quartus.root}</dd>
        </dl>
      ) : (
        <p className="muted">{t('toolchain.quartusNone')}</p>
      )}
      {info.quartus_error && <p className="text-error">{info.quartus_error.message}</p>}

    </div>
  );
}
