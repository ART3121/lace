// A vista Exercícios: a pasta de exercícios do `lace learn` aberta, os
// capítulos e o que já foi resolvido. Sem pasta, criar uma ou abrir uma que
// já existe. Antes disso, a vista confere de onde vêm as trilhas: sem o
// componente `lace-learn` (e sem a preferência da pasta das trilhas), ela
// oferece instalá-lo ou escolher a pasta, e confere de novo quando a
// preferência ou o bundle mudam.

import { open as openDialogNative } from '@tauri-apps/plugin-dialog';
import { CircleCheck, Circle, FolderOpen, GraduationCap, ListChecks, Package, RefreshCw, X } from 'lucide-react';
import { useEffect, useState } from 'react';

import { errorText, useT } from '../../i18n';
import { api } from '../../ipc/api';
import type { IpcError, LearnTracks } from '../../ipc/types';
import { useApp } from '../../state/app';
import { openDialog } from '../../state/dialogs';
import { useJobs } from '../../state/jobs';
import { useLearn } from '../../state/learn';
import { useProject } from '../../state/project';
import { joinPath, samePath } from '../../util/paths';
import { Button, Empty, IconButton, Section, Spinner } from '../common';
import { ViewActions } from '../layout/ViewActions';

/** A pasta que "Criar" põe dentro da pasta escolhida. */
const FOLDER = 'lace-learn';

type Tracks =
  | { status: 'checking' }
  | { status: 'ok'; value: LearnTracks }
  | { status: 'missing' }
  | { status: 'error'; error: IpcError };

async function createWorkspace() {
  const parent = await openDialogNative({ directory: true, multiple: false });
  if (typeof parent !== 'string') return;
  const target = joinPath(parent, FOLDER);
  const learn = useLearn.getState();
  if (!(await learn.create(target)) && useLearn.getState().error?.code === 'learn_workspace_exists') {
    // Já existe: se for uma pasta de exercícios, abre.
    if (await learn.openRoot(target)) {
      const snapshot = useLearn.getState().snapshot;
      if (snapshot) await learn.select(snapshot.current);
    }
  }
}

async function openWorkspace() {
  const folder = await openDialogNative({ directory: true, multiple: false });
  if (typeof folder !== 'string') return;
  if (await useLearn.getState().openRoot(folder)) {
    const snapshot = useLearn.getState().snapshot;
    if (snapshot) await useLearn.getState().select(snapshot.current);
  }
}

/** A preferência `learn_dir`: a pasta das trilhas no lugar do componente. */
async function chooseTracks() {
  const folder = await openDialogNative({ directory: true, multiple: false });
  if (typeof folder !== 'string') return;
  await useApp.getState().updateSettings((s) => ({ ...s, learn_dir: folder }));
}

/** De onde vêm as trilhas, conferido ao abrir a vista e quando a
 * preferência ou o bundle mudam. */
function useTracks(enabled: boolean): Tracks {
  const learnDir = useApp((s) => s.settings?.learn_dir ?? null);
  const toolchain = useApp((s) => s.toolchain);
  const [tracks, setTracks] = useState<Tracks>({ status: 'checking' });
  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    setTracks({ status: 'checking' });
    api.learn
      .tracks()
      .then((value) => alive && setTracks({ status: 'ok', value }))
      .catch((error: IpcError) => {
        if (!alive) return;
        setTracks(error.code === 'learn_component_missing' ? { status: 'missing' } : { status: 'error', error });
      });
    // A pasta lembrada que não abriu antes (sem trilhas, naquela hora)
    // tenta de novo.
    const { root, error } = useLearn.getState();
    if (root && error) void useLearn.getState().openRoot(root);
    return () => {
      alive = false;
    };
  }, [enabled, learnDir, toolchain]);
  return tracks;
}

function Start({ tracks }: { tracks: Tracks }) {
  const t = useT();
  const loading = useLearn((s) => s.loading);
  const error = useLearn((s) => s.error);
  const learnDir = useApp((s) => s.settings?.learn_dir ?? null);

  if (tracks.status === 'checking') {
    return (
      <div className="sidebar__body learn-start">
        <Spinner />
      </div>
    );
  }

  const intro = (
    <Empty icon={<GraduationCap size={28} />}>
      <p>{t('learn.intro')}</p>
    </Empty>
  );

  if (tracks.status !== 'ok' || tracks.value.tracks.length === 0) {
    const problem =
      tracks.status === 'missing'
        ? t('learn.missing')
        : tracks.status === 'error'
          ? errorText(tracks.error)
          : t('learn.noTracks', { dir: tracks.value.dir });
    return (
      <div className="sidebar__body learn-start">
        {intro}
        <p className="learn-start__error">{problem}</p>
        <div className="learn-start__actions">
          {tracks.status === 'missing' && (
            <Button variant="primary" icon={<Package size={14} />} onClick={() => openDialog({ kind: 'install', components: ['lace-learn'] })}>
              {t('learn.install')}
            </Button>
          )}
          <Button icon={<FolderOpen size={14} />} onClick={() => void chooseTracks()}>
            {t('learn.chooseTracks')}
          </Button>
          {learnDir && (
            <Button onClick={() => void useApp.getState().updateSettings((s) => ({ ...s, learn_dir: null }))}>
              {t('learn.useComponent')}
            </Button>
          )}
        </div>
        <p className="muted learn-start__hint">{t('learn.tracksHint')}</p>
      </div>
    );
  }

  return (
    <div className="sidebar__body learn-start">
      {intro}
      {loading && <Spinner />}
      {error && <p className="learn-start__error">{errorText(error)}</p>}
      <div className="learn-start__actions">
        <Button variant="primary" icon={<GraduationCap size={14} />} disabled={loading} onClick={() => void createWorkspace()}>
          {t('learn.create')}
        </Button>
        <Button icon={<FolderOpen size={14} />} disabled={loading} onClick={() => void openWorkspace()}>
          {t('learn.open')}
        </Button>
      </div>
      <p className="muted learn-start__hint" title={tracks.value.dir}>
        {t('learn.tracksFrom', { dir: tracks.value.dir })}
      </p>
    </div>
  );
}

export function LearnPanel() {
  const t = useT();
  const snapshot = useLearn((s) => s.snapshot);
  const spf = useProject((s) => s.snapshot?.spf ?? null);
  const running = useJobs((s) => (s.running?.flow === 'learn' ? s.running.statusKey : null));
  const tracks = useTracks(!snapshot);

  useEffect(() => {
    void useLearn.getState().load();
  }, []);

  if (!snapshot) return <Start tracks={tracks} />;

  const percent = snapshot.total ? Math.round((snapshot.solved * 100) / snapshot.total) : 0;
  return (
    <>
      <ViewActions>
        <IconButton label={t('learn.check')} disabled={!!running} onClick={() => void useLearn.getState().check()}>
          <ListChecks size={14} />
        </IconButton>
        <IconButton label={t('common.refresh')} onClick={() => void useLearn.getState().openRoot(snapshot.root)}>
          <RefreshCw size={14} />
        </IconButton>
        <IconButton label={t('learn.closeFolder')} onClick={() => useLearn.getState().forget()}>
          <X size={14} />
        </IconButton>
      </ViewActions>
      <div className="learn-progress" title={snapshot.root}>
        <div className="learn-progress__text">
          <span className="learn-progress__title">{snapshot.title}</span>
          <span className="sidebar__subtitle">{t('learn.progress', { solved: snapshot.solved, total: snapshot.total })}</span>
        </div>
        <div className="learn-progress__bar" aria-hidden>
          <div style={{ width: `${percent}%` }} />
        </div>
      </div>
      <div className="sidebar__body">
        {snapshot.chapters.map((chapter) => {
          const solved = chapter.exercises.filter((e) => e.solved).length;
          const open = chapter.exercises.some((e) => e.name === snapshot.current);
          return (
            <Section key={chapter.id} title={chapter.title} count={solved} defaultOpen={open || solved < chapter.exercises.length}>
              <ul className="learn-list">
                {chapter.exercises.map((exercise) => {
                  const current = exercise.name === snapshot.current;
                  const busy = running === `learn:${exercise.name}`;
                  const opened = samePath(spf, exercise.spf);
                  return (
                    <li key={exercise.name}>
                      <button
                        type="button"
                        className={`learn-list__item${current ? ' is-current' : ''}${opened ? ' is-open' : ''}`}
                        title={exercise.name}
                        onClick={() => void useLearn.getState().select(exercise.name)}
                      >
                        {busy ? (
                          <Spinner size={13} />
                        ) : exercise.solved ? (
                          <CircleCheck size={13} className="learn-list__done" />
                        ) : (
                          <Circle size={13} className="learn-list__todo" />
                        )}
                        <span className="learn-list__title">{exercise.title}</span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            </Section>
          );
        })}
      </div>
    </>
  );
}
