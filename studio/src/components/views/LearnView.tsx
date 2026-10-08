// O enunciado do exercício atual do `lace learn`, as dicas pedidas e a
// última correção: o veredito, cada saída que errou, os erros de compilação
// (que levam à linha) e o que a correção percebeu. Os botões corrigem,
// mostram a próxima dica, abrem a onda e o esquemático, restauram o arquivo
// e, depois de resolvido, abrem a solução e passam ao próximo.

import {
  Activity,
  ArrowRight,
  CircleCheck,
  CircuitBoard,
  FileCheck,
  Lightbulb,
  ListChecks,
  RotateCcw,
} from 'lucide-react';
import { useMemo } from 'react';

import { useT } from '../../i18n';
import type { Finding, Grade } from '../../ipc/lace-types';
import { useEditor } from '../../state/editor';
import { useJobs } from '../../state/jobs';
import { currentExercise, useLearn } from '../../state/learn';
import { useProject } from '../../state/project';
import { markdownToHtml } from '../../util/markdown';
import { baseName, relativeTo, samePath } from '../../util/paths';
import { Badge, Button, Empty, Kbd, Spinner, Switch } from '../common';

function Markdown({ text }: { text: string }) {
  const html = useMemo(() => markdownToHtml(text), [text]);
  return <div className="markdown" dangerouslySetInnerHTML={{ __html: html }} />;
}

function findingText(t: ReturnType<typeof useT>, finding: Finding): string {
  switch (finding.kind) {
    case 'interface_changed':
      return t('learn.finding.interface');
    case 'undriven_output':
      return t('learn.finding.undriven', { output: finding.output });
    case 'reset_only':
      return t('learn.finding.reset');
  }
}

function Result({ grade, dir, solution }: { grade: Grade; dir: string; solution: string | null }) {
  const t = useT();
  const open = (file: string, line?: number | null, column?: number | null) =>
    void useEditor.getState().openFile(file, { line: line ?? undefined, column: column ?? undefined });
  let headline: { text: string; tone: string };
  switch (grade.verdict) {
    case 'solved':
      headline = { text: t('learn.verdict.solved'), tone: 'ok' };
      break;
    case 'compile_error':
      headline = { text: t('learn.verdict.compileError'), tone: 'error' };
      break;
    case 'mismatch':
      headline = { text: t('learn.verdict.mismatch', { wrong: grade.mismatched, samples: grade.samples }), tone: 'error' };
      break;
    case 'timed_out':
      headline = { text: t('learn.verdict.timedOut'), tone: 'error' };
      break;
    case 'cancelled':
      headline = { text: t('learn.verdict.cancelled'), tone: 'warn' };
      break;
    default:
      headline = { text: t('learn.verdict.incomplete'), tone: 'error' };
  }
  const wrong = grade.outputs.filter((o) => o.mismatches > 0);
  return (
    <section className={`learn-result learn-result--${headline.tone}`}>
      <p className="learn-result__headline">
        {grade.verdict === 'solved' && <CircleCheck size={16} />}
        {headline.text}
      </p>
      {grade.verdict === 'solved' && solution && (
        <p className="learn-result__line">
          {t('learn.solutionAt')}{' '}
          <button type="button" className="link mono" onClick={() => open(solution)}>
            {baseName(solution)}
          </button>
        </p>
      )}
      {wrong.length > 0 && (
        <table className="learn-result__outputs">
          <thead>
            <tr>
              <th>{t('learn.output')}</th>
              <th>{t('learn.wrongSamples')}</th>
              <th>{t('learn.firstMismatch')}</th>
              <th>{t('learn.unknown')}</th>
            </tr>
          </thead>
          <tbody>
            {wrong.map((o) => (
              <tr key={o.name}>
                <td className="mono">{o.name}</td>
                <td>{o.mismatches}</td>
                <td>{o.first_ns === null ? '-' : `${o.first_ns} ns`}</td>
                <td>{o.unknown}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {grade.diagnostics.length > 0 && (
        <ul className="learn-result__diagnostics">
          {grade.diagnostics.map((d, i) => (
            <li key={i} className={`learn-result__diagnostic learn-result__diagnostic--${d.severity}`}>
              {d.file ? (
                <button type="button" className="link mono" onClick={() => open(d.file!, d.line, d.column)}>
                  {relativeTo(d.file, dir)}
                  {d.line ? `:${d.line}` : ''}
                </button>
              ) : null}{' '}
              <span>{d.message}</span>
            </li>
          ))}
        </ul>
      )}
      {grade.findings.map((f, i) => (
        <p key={i} className="learn-result__finding">
          {findingText(t, f)}
        </p>
      ))}
      {grade.output.length > 0 && (
        <pre className="learn-result__output">{grade.output.join('\n')}</pre>
      )}
      {grade.verdict === 'mismatch' && grade.waveform && <p className="muted">{t('learn.waveHint')}</p>}
    </section>
  );
}

export function LearnView() {
  const t = useT();
  const snapshot = useLearn((s) => s.snapshot);
  const exercise = currentExercise(snapshot);
  const grade = useLearn((s) => (exercise ? s.grades[exercise.name] : undefined));
  const hints = useLearn((s) => (exercise ? s.hints[exercise.name] ?? 0 : 0));
  const auto = useLearn((s) => s.auto);
  const running = useJobs((s) => s.running);
  const spf = useProject((s) => s.snapshot?.spf ?? null);

  if (!snapshot || !exercise) {
    return (
      <div className="view-page">
        <Empty>{t('learn.noExercise')}</Empty>
      </div>
    );
  }
  const chapter = snapshot.chapters.find((c) => c.exercises.some((e) => e.name === exercise.name));
  const all = snapshot.chapters.flatMap((c) => c.exercises);
  const position = all.findIndex((e) => e.name === exercise.name) + 1;
  const checking = running?.flow === 'learn';
  const busy = running !== null;
  const opened = samePath(spf, exercise.spf);
  const dir = exercise.file.slice(0, exercise.file.length - baseName(exercise.file).length);
  const allSolved = snapshot.solved === snapshot.total;

  return (
    <div className="view-page learn-view">
      <div className="learn-view__crumbs muted">
        {chapter?.title} · {t('learn.position', { n: position, total: all.length })}
      </div>
      <div className="view-page__title">
        <h1>
          {exercise.title}
          {exercise.solved && <Badge tone="ok">{t('learn.solved')}</Badge>}
        </h1>
        <span className="mono muted">{baseName(exercise.file)}</span>
      </div>

      <div className="learn-view__actions">
        <Button
          variant="primary"
          icon={checking ? <Spinner /> : <ListChecks size={14} />}
          disabled={busy || !opened}
          onClick={() => void useLearn.getState().check(exercise.name)}
        >
          {t('learn.check')}
        </Button>
        <Button icon={<Lightbulb size={14} />} disabled={hints >= exercise.hints.length} onClick={() => useLearn.getState().showHint(exercise.name)}>
          {t('learn.hint', { shown: hints, total: exercise.hints.length })}
        </Button>
        <Button icon={<Activity size={14} />} disabled={!grade?.waveform} onClick={() => useLearn.getState().openWave(exercise.name)}>
          {t('learn.wave')}
        </Button>
        <Button
          icon={<CircuitBoard size={14} />}
          disabled={busy || !opened}
          onClick={() => void useJobs.getState().run({ flow: 'synthesize', schematic: true })}
        >
          {t('learn.schematic')}
        </Button>
        <Button icon={<RotateCcw size={14} />} disabled={busy} onClick={() => void useLearn.getState().reset(exercise.name)}>
          {t('learn.reset')}
        </Button>
        {exercise.solution && (
          <Button icon={<FileCheck size={14} />} onClick={() => void useEditor.getState().openFile(exercise.solution!)}>
            {t('learn.solution')}
          </Button>
        )}
        {exercise.solved && !allSolved && (
          <Button variant="primary" icon={<ArrowRight size={14} />} disabled={busy} onClick={() => void useLearn.getState().next()}>
            {t('learn.next')}
          </Button>
        )}
      </div>
      <div className="learn-view__auto">
        <span className="learn-view__switch">
          <Switch checked={auto} onChange={(value) => useLearn.getState().setAuto(value)} label={t('learn.auto')} />
          <span>{t('learn.auto')}</span>
        </span>
        <span className="muted">
          <Kbd keys="Ctrl+Alt+L" /> {t('learn.checkShortcut')}
        </span>
      </div>

      {!opened && (
        <p className="learn-view__notice">
          {t('learn.notOpen')}{' '}
          <button type="button" className="link" onClick={() => void useLearn.getState().select(exercise.name)}>
            {t('learn.openExercise')}
          </button>
        </p>
      )}

      <Markdown text={exercise.prompt} />

      {exercise.hints.slice(0, hints).map((hint, i) => (
        <aside key={i} className="learn-hint">
          <p className="learn-hint__title">
            <Lightbulb size={14} /> {t('learn.hintTitle', { n: i + 1 })}
          </p>
          <Markdown text={hint} />
        </aside>
      ))}

      {grade && <Result grade={grade} dir={dir} solution={exercise.solution} />}
      {allSolved && snapshot.farewell && (
        <section className="learn-result learn-result--ok">
          <Markdown text={snapshot.farewell} />
        </section>
      )}
    </div>
  );
}
