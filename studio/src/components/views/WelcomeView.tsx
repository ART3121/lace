// A tela inicial: começar, recentes e o estado da instalação da AURORA.

import { useId } from 'react';
import { Compass, Cpu, FileCode, FolderOpen, FolderPlus, Wrench, X } from 'lucide-react';

import { openProjectDialog } from '../../actions';
import { useT } from '../../i18n';
import { useApp } from '../../state/app';
import { openDialog } from '../../state/dialogs';
import { useEditor } from '../../state/editor';
import { useProject } from '../../state/project';
import { useTour } from '../../state/tour';
import { guarded } from '../../state/toasts';
import { api } from '../../ipc/api';
import { dirName } from '../../util/paths';
import { AuroraMark, Kbd } from '../common';

export function WelcomeView() {
  const t = useT();
  const recent = useApp((s) => s.recent);
  const toolchain = useApp((s) => s.toolchain);

  const forget = async (spf: string) => {
    await guarded(() => api.app.forgetRecent(spf));
    await useApp.getState().refreshRecent();
  };

  return (
    <>
      <AuroraWaves />
      <div className="view-page welcome">
        <header className="welcome__header">
          <AuroraMark className="welcome__logo" />
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
              <li>
                <button type="button" className="link" onClick={() => useTour.getState().start()}>
                  <Compass size={15} /> {t('welcome.tour')}
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
    </>
  );
}

/** A largura de cada linha no desenho: o dobro da que aparece, para a linha
 * correr metade e recomeçar no mesmo ponto. Os períodos dividem a metade. */
const WAVE_WIDTH = 2400;
const WAVE_HEIGHT = 120;

interface Wave {
  period: number;
  amplitude: number;
  /** A altura do eixo da onda na faixa. */
  y: number;
  /** As duas cores do degradê ao longo da linha (variáveis do .welcome-aurora). */
  from: string;
  to: string;
  /** Quanto leva para correr meia linha. */
  seconds: number;
  reverse?: boolean;
  opacity: number;
}

const WAVES: Wave[] = [
  { period: 1200, amplitude: 18, y: 84, from: '--aurora-1', to: '--aurora-2', seconds: 56, opacity: 0.7 },
  { period: 600, amplitude: 11, y: 92, from: '--aurora-2', to: '--aurora-3', seconds: 44, reverse: true, opacity: 0.55 },
  { period: 400, amplitude: 7, y: 78, from: '--aurora-3', to: '--aurora-1', seconds: 72, opacity: 0.4 },
];

function wavePath({ period, amplitude, y }: Wave): string {
  const step = period / 32;
  const points: string[] = [];
  for (let x = 0; x <= WAVE_WIDTH + step / 2; x += step) {
    points.push(`${x.toFixed(1)} ${(y - amplitude * Math.sin((2 * Math.PI * x) / period)).toFixed(2)}`);
  }
  return `M${points.join(' L')}`;
}

const WAVE_PATHS = WAVES.map(wavePath);

/** A aurora da tela inicial: três senoides finas no pé da área do editor,
 * correndo devagar, uma para cada lado, nas cores do tema. */
function AuroraWaves() {
  const id = useId();
  return (
    <div className="welcome-aurora" aria-hidden="true">
      {WAVES.map((wave, i) => (
        <svg
          key={i}
          className="welcome-aurora__line"
          viewBox={`0 0 ${WAVE_WIDTH} ${WAVE_HEIGHT}`}
          preserveAspectRatio="none"
          style={{
            animationDuration: `${wave.seconds}s`,
            animationDirection: wave.reverse ? 'reverse' : 'normal',
            opacity: wave.opacity,
          }}
        >
          <defs>
            {/* Um degradê de ida e volta a cada 1200 unidades: a meia linha
                que corre leva junto um número inteiro de voltas. */}
            <linearGradient
              id={`${id}-${i}`}
              gradientUnits="userSpaceOnUse"
              x1="0"
              x2={WAVE_WIDTH / 4}
              y1="0"
              y2="0"
              spreadMethod="reflect"
            >
              <stop offset="0" style={{ stopColor: `var(${wave.from})` }} />
              <stop offset="1" style={{ stopColor: `var(${wave.to})` }} />
            </linearGradient>
          </defs>
          <path d={WAVE_PATHS[i]} stroke={`url(#${id}-${i})`} />
        </svg>
      ))}
    </div>
  );
}
