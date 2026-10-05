// A tela inicial: começar, recentes e o estado da instalação do Lace.

import { Cpu, FileCode, FolderOpen, FolderPlus, Wrench, X } from 'lucide-react';

import { openProjectDialog } from '../../actions';
import { useT } from '../../i18n';
import { useApp } from '../../state/app';
import { openDialog } from '../../state/dialogs';
import { useEditor } from '../../state/editor';
import { useProject } from '../../state/project';
import { guarded } from '../../state/toasts';
import { api } from '../../ipc/api';
import { dirName } from '../../util/paths';
import { Kbd, LaceMark } from '../common';

export function WelcomeView() {
  const t = useT();
  const recent = useApp((s) => s.recent);
  const toolchain = useApp((s) => s.toolchain);

  const forget = async (spf: string) => {
    await guarded(() => api.app.forgetRecent(spf));
    await useApp.getState().refreshRecent();
  };

  return (
    <div className="view-page welcome">
      <header className="welcome__header">
        <LaceMark className="welcome__logo" />
        <div>
          <h1>{t('welcome.title')}</h1>
          <p className="muted">{t('welcome.subtitle')}</p>
        </div>
      </header>

      <div className="welcome__columns">
        <div>
          <h2>{t('welcome.start')}</h2>
          <ul className="link-list">
            <li>
              <button type="button" className="link" onClick={() => openDialog({ kind: 'newProject' })}>
                <FolderPlus size={15} /> {t('action.newProject')}
              </button>
              <Kbd keys="Ctrl+Alt+N" />
            </li>
            <li>
              <button type="button" className="link" onClick={() => void openProjectDialog()}>
                <FolderOpen size={15} /> {t('action.openProject')}
              </button>
              <Kbd keys="Ctrl+Shift+O" />
            </li>
            <li>
              <button type="button" className="link" onClick={() => useEditor.getState().openView('toolchain')}>
                <Wrench size={15} /> {t('action.toolchain')}
              </button>
            </li>
          </ul>

          <h2>{t('welcome.recent')}</h2>
          {recent.length === 0 && <p className="muted">{t('welcome.recentEmpty')}</p>}
          <ul className="recent-list">
            {recent.map((project) => (
              <li key={project.spf} className={project.exists ? '' : 'is-missing'}>
                <button
                  type="button"
                  className="link"
                  disabled={!project.exists}
                  title={project.spf}
                  onClick={() => void useProject.getState().open(project.spf)}
                >
                  {project.name}
                </button>
                <span className="recent-list__path">
                  {project.exists ? dirName(project.spf) : t('welcome.missing')}
                </span>
                <button
                  type="button"
                  className="icon-btn icon-btn--small"
                  title={t('welcome.forget')}
                  aria-label={t('welcome.forget')}
                  onClick={() => void forget(project.spf)}
                >
                  <X size={13} />
                </button>
              </li>
            ))}
          </ul>
        </div>

        <div>
          <h2>{t('toolchain.title')}</h2>
          <div className={`notice${toolchain && !toolchain.found ? ' notice--warn' : ''}`}>
            {toolchain?.found
              ? t('welcome.toolchainOk', {
                  bundle: toolchain.bundle ?? '',
                  platform: toolchain.platform ?? '',
                  count: toolchain.components.length,
                })
              : toolchain
                ? t('welcome.toolchainMissing')
                : t('common.loading')}
          </div>

          <div className="guide">
            <h3>
              <FileCode size={15} /> {t('welcome.verilogTitle')}
            </h3>
            <p>{t('welcome.verilogSteps')}</p>
          </div>
          <div className="guide">
            <h3>
              <Cpu size={15} /> {t('welcome.saphoTitle')}
            </h3>
            <p>{t('welcome.saphoSteps')}</p>
          </div>
        </div>
      </div>
    </div>
  );
}
