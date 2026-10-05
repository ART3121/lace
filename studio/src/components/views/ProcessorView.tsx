// Um processador SAPHO: a configuração da simulação (frequência, clocks,
// arrays, o painel do botão C± da AURORA), as entradas e as saídas da
// última simulação.

import { Activity, CircuitBoard, FileCode, Hammer, Play, Plus } from 'lucide-react';
import { useEffect, useState } from 'react';

import { openWave, runBuild, runSimulation, runSynthesis } from '../../actions';
import { useLang, useT } from '../../i18n';
import { api } from '../../ipc/api';
import { openDialog } from '../../state/dialogs';
import { useEditor } from '../../state/editor';
import { useJobs } from '../../state/jobs';
import { useProject } from '../../state/project';
import { guarded, useToasts } from '../../state/toasts';
import { relativeTo } from '../../util/paths';
import { Badge, Button, Checkbox, Empty, Field } from '../common';

/** O tempo simulado de `clocks` ciclos a `mhz` MHz, legível, com o separador
 * decimal do idioma. */
function simulatedTime(clocks: number, mhz: number, lang: string): string {
  if (!clocks || !mhz) return '-';
  const format = (value: number, digits: number) =>
    value.toLocaleString(lang === 'pt' ? 'pt-BR' : 'en', { minimumFractionDigits: digits, maximumFractionDigits: digits });
  const us = clocks / mhz;
  if (us < 1) return `${format(us * 1000, 1)} ns`;
  if (us < 1000) return `${format(us, us < 10 ? 2 : 1)} µs`;
  return `${format(us / 1000, 2)} ms`;
}

export function ProcessorView({ name }: { name: string }) {
  const t = useT();
  const lang = useLang((s) => s.lang);
  const snapshot = useProject((s) => s.snapshot);
  const running = useJobs((s) => s.running);
  const lastFinished = useJobs((s) => s.last?.finishedAt);
  const processor = snapshot?.processors.find((p) => p.name === name) ?? null;
  const [freq, setFreq] = useState('');
  const [clocks, setClocks] = useState('');
  const [values, setValues] = useState<Record<number, number[] | string>>({});

  useEffect(() => {
    if (!processor) return;
    setFreq(String(processor.frequency_mhz));
    setClocks(String(processor.clocks));
  }, [processor?.frequency_mhz, processor?.clocks]);

  useEffect(() => {
    if (!processor) return;
    let cancelled = false;
    Promise.all(
      processor.outputs.map(async (o) => {
        try {
          return [o.port, await api.project.outputValues(name, o.port)] as const;
        } catch (error) {
          return [o.port, (error as { message: string }).message] as const;
        }
      }),
    ).then((entries) => !cancelled && setValues(Object.fromEntries(entries)));
    return () => {
      cancelled = true;
    };
  }, [name, processor?.outputs.map((o) => o.path).join('|'), lastFinished]);

  if (!processor || !snapshot) {
    return (
      <div className="view-page">
        <Empty>{t('error.processor_not_found')}</Empty>
      </div>
    );
  }

  const save = async (config: { frequency_mhz?: number; clocks?: number; show_arrays?: boolean }) => {
    const done = await guarded(() => api.project.configureProcessor(name, config));
    if (done) {
      useToasts.getState().push({ kind: 'success', title: t('processor.saved') }, 2000);
      await useProject.getState().refresh();
    }
  };

  const commit = () => {
    const f = Number.parseInt(freq, 10);
    const c = Number.parseInt(clocks, 10);
    const change: { frequency_mhz?: number; clocks?: number } = {};
    if (f > 0 && f !== processor.frequency_mhz) change.frequency_mhz = f;
    if (c > 0 && c !== processor.clocks) change.clocks = c;
    if (Object.keys(change).length) void save(change);
  };

  const busy = !!running;
  const root = snapshot.root;

  return (
    <div className="view-page processor">
      <div className="view-page__title">
        <h1>
          {processor.name} <Badge>{processor.language === 'cpp' ? 'C' : 'C±'}</Badge>{' '}
          <Badge tone={processor.built ? 'ok' : 'muted'}>{processor.built ? t('explorer.built') : t('explorer.notBuilt')}</Badge>
        </h1>
        <div className="button-row">
          <Button icon={<FileCode size={14} />} onClick={() => void useEditor.getState().openFile(processor.source)}>
            {t('explorer.source')}
          </Button>
          <Button icon={<Hammer size={14} />} disabled={busy} onClick={() => void runBuild(name)}>
            {t('toolbar.build')}
          </Button>
          <Button icon={<Play size={14} />} disabled={busy} onClick={() => void runSimulation(true, name)}>
            {t('toolbar.simulate')}
          </Button>
          <Button icon={<Activity size={14} />} disabled={!processor.waveform} onClick={() => void openWave(name)}>
            {t('toolbar.openWave')}
          </Button>
          <Button icon={<CircuitBoard size={14} />} disabled={busy} onClick={() => void runSynthesis(name)}>
            {t('toolbar.synthesize')}
          </Button>
        </div>
      </div>

      <section className="form-section">
        <h2>{t('processor.settings')}</h2>
        <div className="form-row">
          <Field label={t('processor.frequency')}>
            <input
              className="input input--narrow"
              type="number"
              min={1}
              value={freq}
              onChange={(e) => setFreq(e.target.value)}
              onBlur={commit}
              onKeyDown={(e) => e.key === 'Enter' && commit()}
            />
          </Field>
          <Field label={t('processor.clocks')}>
            <input
              className="input input--narrow"
              type="number"
              min={1}
              value={clocks}
              onChange={(e) => setClocks(e.target.value)}
              onBlur={commit}
              onKeyDown={(e) => e.key === 'Enter' && commit()}
            />
          </Field>
        </div>
        <Checkbox
          checked={processor.show_arrays}
          onChange={(value) => void save({ show_arrays: value })}
          label={t('processor.arrays')}
        />
        <p className="muted">
          {t('processor.estimated', {
            time: simulatedTime(Number.parseInt(clocks, 10), Number.parseInt(freq, 10), lang),
          })}{' '}
          {t('processor.appliesNext')}
        </p>
      </section>

      <section className="form-section">
        <div className="form-section__title">
          <h2>{t('processor.inputs')}</h2>
          <Button small icon={<Plus size={13} />} onClick={() => openDialog({ kind: 'newInput', processor: name })}>
            {t('explorer.newInput')}
          </Button>
        </div>
        {processor.inputs.length === 0 && <p className="muted">{t('processor.noInputs')}</p>}
        <ul className="plain-list">
          {processor.inputs.map((input) => (
            <li key={input.path}>
              <button type="button" className="link mono" onClick={() => void useEditor.getState().openFile(input.path)}>
                {relativeTo(input.path, root)}
              </button>
            </li>
          ))}
        </ul>
        {processor.missing_inputs.length > 0 && (
          <p className="text-warn">
            {t('processor.missingInputs', { list: processor.missing_inputs.map((p) => relativeTo(p, root)).join(', ') })}
          </p>
        )}
      </section>

      <section className="form-section">
        <h2>{t('processor.outputs')}</h2>
        {processor.outputs.length === 0 && <p className="muted">{t('processor.noOutputs')}</p>}
        {processor.outputs.map((output) => {
          const v = values[output.port];
          return (
            <div key={output.path} className="output-port">
              <div className="output-port__head">
                <button type="button" className="link" onClick={() => void useEditor.getState().openFile(output.path)}>
                  {t('processor.port', { port: output.port })}
                </button>
                {Array.isArray(v) && <span className="muted">{t('processor.values', { count: v.length })}</span>}
              </div>
              {typeof v === 'string' ? (
                <p className="text-error">{v}</p>
              ) : (
                <div className="output-port__values mono">{(v ?? []).slice(0, 512).join('  ')}</div>
              )}
            </div>
          );
        })}
      </section>
    </div>
  );
}
