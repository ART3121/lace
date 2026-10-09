// Os diálogos modais. O que está aberto vem de state/dialogs.ts; um por vez.

import { homeDir } from '@tauri-apps/api/path';
import { open as openNative } from '@tauri-apps/plugin-dialog';
import { Check } from 'lucide-react';
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';

import { ACTIONS, isEnabled, runAction } from '../../actions';
import { useT } from '../../i18n';
import { api } from '../../ipc/api';
import type { Language, NewProcessorDefaults } from '../../ipc/types';
import { useApp } from '../../state/app';
import { useDialogs, type ConfirmOptions, type PromptOptions } from '../../state/dialogs';
import { useEditor } from '../../state/editor';
import { useJobs } from '../../state/jobs';
import { useProject } from '../../state/project';
import { removeReports } from '../../state/reports';
import { applyLayout, namedLayouts, useLayoutStatus, type NamedLayout } from '../../state/savedLayouts';
import { guarded } from '../../state/toasts';
import { SYSTEM_THEME, THEMES, themeById } from '../../themes';
import { baseName, joinPath, relativeTo } from '../../util/paths';
import { Button, Checkbox, Field, Kbd } from '../common';
import { WaveSignalsDialog } from './WaveSignalsDialog';

function close() {
  useDialogs.getState().close();
}

export function Dialog({
  title,
  children,
  footer,
  onSubmit,
  wide,
}: {
  title: string;
  children: ReactNode;
  footer?: ReactNode;
  onSubmit?: () => void;
  wide?: boolean;
}) {
  return (
    <div className="dialog-overlay" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <form
        className={`dialog${wide ? ' dialog--wide' : ''}`}
        role="dialog"
        aria-label={title}
        onSubmit={(e) => {
          e.preventDefault();
          onSubmit?.();
        }}
        onKeyDown={(e) => e.key === 'Escape' && close()}
      >
        <header className="dialog__header">{title}</header>
        <div className="dialog__body">{children}</div>
        {footer && <footer className="dialog__footer">{footer}</footer>}
      </form>
    </div>
  );
}

function Footer({ submit, disabled }: { submit: string; disabled?: boolean }) {
  const t = useT();
  return (
    <>
      <Button onClick={close}>{t('common.cancel')}</Button>
      <Button variant="primary" type="submit" disabled={disabled}>
        {submit}
      </Button>
    </>
  );
}

const NAME_RE = /^[A-Za-z_][A-Za-z0-9_]*$/;

// Projeto ---------------------------------------------------------------------

function NewProjectDialog() {
  const t = useT();
  const [name, setName] = useState('');
  const [parent, setParent] = useState(() => localStorage.getItem('lace-studio:newProjectParent') ?? '');

  useEffect(() => {
    if (!parent) void homeDir().then(setParent).catch(() => undefined);
  }, [parent]);

  // A regra de nome é do Core (`validate_project_name`): o diálogo pergunta
  // a ele enquanto o usuário digita, e mostra o motivo ao passar o mouse.
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    if (!name) {
      setProblem(null);
      return;
    }
    let cancelled = false;
    api.project
      .checkName(name)
      .then((error) => !cancelled && setProblem(error ? error.message : null))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [name]);
  const invalid = !name || problem !== null;
  const submit = async () => {
    if (invalid || !parent) return;
    try {
      localStorage.setItem('lace-studio:newProjectParent', parent);
    } catch {
      // Só não lembra a pasta.
    }
    close();
    await useProject.getState().create(parent, name);
  };

  return (
    <Dialog title={t('dialog.newProject.title')} onSubmit={() => void submit()} footer={<Footer submit={t('common.create')} disabled={invalid} />}>
      <Field label={t('dialog.newProject.name')} error={problem ? t('dialog.newProject.invalid') : null} hint={t('dialog.newProject.rule')}>
        <input
          className="input"
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="contador"
          spellCheck={false}
          aria-invalid={problem !== null}
          title={problem ?? undefined}
        />
      </Field>
      <Field label={t('dialog.newProject.location')} hint={name && !problem ? t('dialog.newProject.hint', { path: joinPath(parent, name, `${name}.spf`) }) : undefined}>
        <div className="input-group">
          <input className="input" value={parent} onChange={(e) => setParent(e.target.value)} />
          <Button
            onClick={async () => {
              const chosen = await openNative({ directory: true, multiple: false, defaultPath: parent || undefined });
              if (typeof chosen === 'string') setParent(chosen);
            }}
          >
            {t('common.browse')}
          </Button>
        </div>
      </Field>
    </Dialog>
  );
}

function NewProcessorDialog() {
  const t = useT();
  const [defaults, setDefaults] = useState<NewProcessorDefaults | null>(null);
  const [name, setName] = useState('');
  const [language, setLanguage] = useState<Language>('cmm');
  const [fields, setFields] = useState<Record<string, string>>({});

  useEffect(() => {
    void api.project.processorDefaults().then((d) => {
      setDefaults(d);
      setFields({
        input_ports: String(d.input_ports),
        output_ports: String(d.output_ports),
        nubits: String(d.nubits),
        nbmant: String(d.nbmant),
        nbexpo: String(d.nbexpo),
        nugain: String(d.nugain),
        ndstac: String(d.ndstac),
        sdepth: String(d.sdepth),
      });
    });
  }, []);

  // A regra do nome é do Core (`validate_processor_name`): além de letras,
  // números e _, recusa palavras do C± e do Verilog, módulos da biblioteca
  // SAPHO e nomes longos. O motivo aparece ao passar o mouse.
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    if (!name) {
      setProblem(null);
      return;
    }
    let cancelled = false;
    api.project
      .checkProcessorName(name)
      .then((error) => !cancelled && setProblem(error ? error.message : null))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [name]);
  const nameError = name && (problem !== null || !NAME_RE.test(name)) ? t('dialog.newProcessor.invalid') : null;
  const num = (key: string) => {
    const value = Number.parseInt(fields[key] ?? '', 10);
    return Number.isFinite(value) && value >= 0 ? value : undefined;
  };

  const submit = async () => {
    if (!name || nameError) return;
    const processor = await guarded(() =>
      api.project.addProcessor({
        name,
        language,
        input_ports: num('input_ports'),
        output_ports: num('output_ports'),
        ...(language === 'cmm'
          ? {
              nubits: num('nubits'),
              nbmant: num('nbmant'),
              nbexpo: num('nbexpo'),
              nugain: num('nugain'),
              ndstac: num('ndstac'),
              sdepth: num('sdepth'),
            }
          : {}),
      }),
    );
    if (!processor) return;
    close();
    await useProject.getState().refresh();
    useProject.getState().bumpTree();
    await useEditor.getState().openFile(processor.source);
  };

  const numberField = (key: string, label: string) => (
    <Field label={label}>
      <input
        className="input"
        type="number"
        min={0}
        value={fields[key] ?? ''}
        onChange={(e) => {
          const next = { ...fields, [key]: e.target.value };
          // A palavra acompanha a mantissa e o expoente: o asmcomp só aceita
          // #NUBITS = #NBMANT + #NBEXPO + 1.
          if (key === 'nbmant' || key === 'nbexpo') {
            const mant = Number.parseInt(next.nbmant ?? '', 10);
            const expo = Number.parseInt(next.nbexpo ?? '', 10);
            if (Number.isFinite(mant) && Number.isFinite(expo)) next.nubits = String(mant + expo + 1);
          }
          setFields(next);
        }}
      />
    </Field>
  );

  return (
    <Dialog
      title={t('dialog.newProcessor.title')}
      wide
      onSubmit={() => void submit()}
      footer={<Footer submit={t('common.create')} disabled={!name || !!nameError || !defaults} />}
    >
      <div className="form-grid">
        <Field label={t('dialog.newProcessor.name')} error={nameError} hint={t('dialog.newProcessor.nameHint')}>
          <input
            className="input"
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="filtro"
            spellCheck={false}
            aria-invalid={nameError !== null}
            title={problem ?? undefined}
          />
        </Field>
        <Field label={t('dialog.newProcessor.language')}>
          <select className="select" value={language} onChange={(e) => setLanguage(e.target.value as Language)}>
            <option value="cmm">{t('dialog.newProcessor.cmm')}</option>
            <option value="cpp">{t('dialog.newProcessor.cpp')}</option>
          </select>
        </Field>
        {numberField('input_ports', t('dialog.newProcessor.inputs'))}
        {numberField('output_ports', t('dialog.newProcessor.outputs'))}
      </div>
      <h3 className="dialog__section">{t('dialog.newProcessor.advanced')}</h3>
      {language === 'cmm' ? (
        <div className="form-grid form-grid--3">
          {numberField('nubits', t('dialog.newProcessor.nubits'))}
          {numberField('nbmant', t('dialog.newProcessor.nbmant'))}
          {numberField('nbexpo', t('dialog.newProcessor.nbexpo'))}
          {numberField('nugain', t('dialog.newProcessor.nugain'))}
          {numberField('ndstac', t('dialog.newProcessor.ndstac'))}
          {numberField('sdepth', t('dialog.newProcessor.sdepth'))}
        </div>
      ) : (
        <p className="muted">{t('dialog.newProcessor.cppNote')}</p>
      )}
    </Dialog>
  );
}

function NewVerilogDialog({
  testbench: initialTb,
  folder: initialFolder,
  cocotb: initialCocotb,
}: {
  testbench: boolean;
  folder?: string;
  cocotb?: boolean;
}) {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot)!;
  const [testbench, setTestbench] = useState(initialTb || !!initialCocotb);
  // O testbench em Verilog ou em Python (cocotb, que o Lace roda no Icarus).
  const [cocotb, setCocotb] = useState(!!initialCocotb);
  const [folder, setFolder] = useState(initialFolder ?? '');
  const suggested = (tb: boolean, py: boolean) => {
    if (!tb || !snapshot.top_module) return '';
    return py ? `test_${snapshot.top_module}.py` : `${snapshot.top_module}_tb.v`;
  };
  const [name, setName] = useState(() => suggested(initialTb || !!initialCocotb, !!initialCocotb));

  const python = testbench && cocotb;
  const fileName = !name
    ? name
    : python
      ? /\.py$/i.test(name)
        ? name
        : `${name}.py`
      : /\.(s?v|vh|svh|py)$/i.test(name)
        ? name
        : `${name}.v`;
  // O cocotb importa o .py como módulo Python: o nome precisa ser um
  // identificador (o Core recusa o resto).
  const badPythonName = /\.py$/i.test(fileName) && !/^[A-Za-z_][A-Za-z0-9_]*\.py$/i.test(baseName(fileName));
  const invalid = !fileName || /[\\:*?"<>|]/.test(fileName) || badPythonName;
  // O nome sugerido acompanha a linguagem enquanto o usuário não escreveu outro.
  const chooseCocotb = (py: boolean) => {
    if (name === suggested(testbench, cocotb)) setName(suggested(testbench, py));
    setCocotb(py);
  };
  const submit = async () => {
    if (invalid) return;
    const path = folder ? joinPath(snapshot.root, folder, fileName) : joinPath(snapshot.root, fileName);
    const added = await guarded(() => api.project.addVerilog(path, testbench));
    if (!added) return;
    close();
    await useProject.getState().refresh();
    useProject.getState().bumpTree();
    await useEditor.getState().openFile(added.path);
  };

  return (
    <Dialog
      title={
        python
          ? t('dialog.newVerilog.titleCocotb')
          : testbench
            ? t('dialog.newVerilog.titleTb')
            : t('dialog.newVerilog.title')
      }
      onSubmit={() => void submit()}
      footer={<Footer submit={t('common.create')} disabled={invalid} />}
    >
      <Field label={t('dialog.newVerilog.fileName')} error={badPythonName ? t('dialog.newVerilog.pythonName') : null}>
        <input
          className="input"
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={python ? 'test_alu.py' : testbench ? 'alu_tb.v' : 'alu.v'}
          aria-invalid={badPythonName}
        />
      </Field>
      <Field label={t('dialog.newVerilog.folder')}>
        <input className="input" value={folder} onChange={(e) => setFolder(e.target.value)} placeholder="rtl" />
      </Field>
      <Checkbox checked={testbench} onChange={setTestbench} label={t('dialog.newVerilog.testbench')} />
      {testbench && (
        <Field label={t('dialog.newVerilog.language')}>
          <select className="select" value={cocotb ? 'cocotb' : 'verilog'} onChange={(e) => chooseCocotb(e.target.value === 'cocotb')}>
            <option value="verilog">Verilog</option>
            <option value="cocotb">{t('dialog.newVerilog.cocotb')}</option>
          </select>
        </Field>
      )}
      <p className="muted">{python ? t('dialog.newVerilog.cocotbHint') : t('dialog.newVerilog.hint')}</p>
    </Dialog>
  );
}

function NewInputDialog({ processor }: { processor: string }) {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot)!;
  const p = snapshot.processors.find((x) => x.name === processor);
  const used = new Set(p?.inputs.map((i) => i.port) ?? []);
  const firstFree = [...Array(64).keys()].find((n) => !used.has(n)) ?? 0;
  const [port, setPort] = useState(String(firstFree));

  const submit = async () => {
    const n = Number.parseInt(port, 10);
    if (!p || !(n >= 0)) return;
    const path = joinPath(p.dir, 'Simulation', `input_${n}.txt`);
    if ((await guarded(() => api.fs.createFile(path, ''))) === undefined) return;
    close();
    await useProject.getState().refresh();
    await useEditor.getState().openFile(path);
  };

  return (
    <Dialog title={t('dialog.newInput.title')} onSubmit={() => void submit()} footer={<Footer submit={t('common.create')} />}>
      <Field label={t('dialog.newInput.port')} hint={p ? relativeTo(joinPath(p.dir, 'Simulation', `input_${port}.txt`), snapshot.root) : undefined}>
        <input className="input input--narrow" type="number" min={0} autoFocus value={port} onChange={(e) => setPort(e.target.value)} />
      </Field>
      <p className="muted">{t('processor.noInputs')}</p>
    </Dialog>
  );
}

function ChooseTopDialog() {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot)!;
  const [module, setModule] = useState('');
  const choose = async (target: string) => {
    if ((await guarded(() => api.project.setTop(target))) === undefined) return;
    close();
    await useProject.getState().refresh();
  };
  return (
    <Dialog title={t('dialog.chooseTop.title')} onSubmit={() => module && void choose(module)} footer={<Footer submit={t('common.ok')} disabled={!module} />}>
      {/* Qualquer Verilog do projeto, menos nome de testbench (a regra do
          Core): os registrados, o gerado de cada processador e os de fora do
          .spf, que passam a ser registrados. */}
      <ul className="choice-list">
        {snapshot.top_candidates.map((path) => {
          const builtBy = snapshot.processors.find((p) => p.generated.verilog === path);
          const note = builtBy
            ? t('dialog.chooseTop.generated', { name: builtBy.name })
            : snapshot.testbenches.some((f) => f.path === path)
              ? t('dialog.chooseTop.testbench')
              : snapshot.unregistered.includes(path)
                ? t('dialog.chooseTop.unregistered')
                : null;
          return (
            <li key={path}>
              <button type="button" className={`choice${path === snapshot.top_level ? ' is-active' : ''}`} onClick={() => void choose(path)}>
                {relativeTo(path, snapshot.root)}
                {note && <span className="choice__note">{note}</span>}
              </button>
            </li>
          );
        })}
      </ul>
      <Field label={t('dialog.chooseTop.hint')}>
        <input className="input" value={module} onChange={(e) => setModule(e.target.value)} placeholder={snapshot.top_module ?? 'top'} />
      </Field>
    </Dialog>
  );
}

function ChooseTestbenchDialog() {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot)!;
  const choose = async (path: string) => {
    if ((await guarded(() => api.project.setTestbench(path))) === undefined) return;
    close();
    await useProject.getState().refresh();
  };
  return (
    <Dialog title={t('dialog.chooseTestbench.title')} footer={<Button onClick={close}>{t('common.close')}</Button>}>
      {snapshot.testbenches.length === 0 && <p className="muted">{t('explorer.emptyTestbenches')}</p>}
      <ul className="choice-list">
        {snapshot.testbenches.map((file) => (
          <li key={file.path}>
            <button
              type="button"
              className={`choice${file.path === snapshot.selected_testbench ? ' is-active' : ''}`}
              onClick={() => void choose(file.path)}
            >
              {relativeTo(file.path, snapshot.root)}
            </button>
          </li>
        ))}
      </ul>
    </Dialog>
  );
}

// Ferramentas --------------------------------------------------------------------

function InstallDialog({ components }: { components?: string[] }) {
  const t = useT();
  const toolchain = useApp((s) => s.toolchain);
  const available = toolchain?.not_installed ?? [];
  const [chosen, setChosen] = useState<Set<string>>(new Set(components ?? []));
  const submit = () => {
    if (chosen.size === 0) return;
    close();
    void useJobs.getState().install([...chosen]);
  };
  return (
    <Dialog title={t('dialog.install.title')} onSubmit={submit} footer={<Footer submit={t('toolchain.install')} disabled={chosen.size === 0} />}>
      <p className="muted">{t('dialog.install.hint')}</p>
      {available.map((name) => (
        <Checkbox
          key={name}
          checked={chosen.has(name)}
          label={name}
          onChange={(value) => {
            const next = new Set(chosen);
            if (value) next.add(name);
            else next.delete(name);
            setChosen(next);
          }}
        />
      ))}
    </Dialog>
  );
}

// Paleta e abrir rápido ------------------------------------------------------------

function ListPicker<T>({
  placeholder,
  items,
  filter,
  render,
  onPick,
}: {
  placeholder: string;
  items: T[];
  filter: (item: T, query: string) => boolean;
  render: (item: T) => ReactNode;
  onPick: (item: T) => void;
}) {
  const t = useT();
  const [query, setQuery] = useState('');
  const [index, setIndex] = useState(0);
  const list = useRef<HTMLUListElement>(null);
  const visible = useMemo(() => items.filter((item) => filter(item, query.toLowerCase())).slice(0, 200), [items, query, filter]);

  useEffect(() => setIndex(0), [query]);
  useEffect(() => {
    list.current?.children[index]?.scrollIntoView({ block: 'nearest' });
  }, [index]);

  return (
    <div className="dialog-overlay dialog-overlay--top" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="dialog palette" role="dialog">
        <input
          className="input palette__input"
          autoFocus
          placeholder={placeholder}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') close();
            else if (e.key === 'ArrowDown') {
              e.preventDefault();
              setIndex(Math.min(index + 1, visible.length - 1));
            } else if (e.key === 'ArrowUp') {
              e.preventDefault();
              setIndex(Math.max(index - 1, 0));
            } else if (e.key === 'Enter' && visible[index]) {
              e.preventDefault();
              close();
              onPick(visible[index]);
            }
          }}
        />
        <ul className="palette__list" ref={list}>
          {visible.length === 0 && <li className="palette__empty">{t('dialog.noMatches')}</li>}
          {visible.map((item, i) => (
            <li key={i}>
              <button
                type="button"
                className={`palette__item${i === index ? ' is-active' : ''}`}
                onMouseEnter={() => setIndex(i)}
                onClick={() => {
                  close();
                  onPick(item);
                }}
              >
                {render(item)}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

function CommandPalette() {
  const t = useT();
  const items = ACTIONS.filter((a) => isEnabled(a));
  return (
    <ListPicker
      placeholder={t('dialog.palette.placeholder')}
      items={items}
      filter={(a, q) => q.split(/\s+/).every((word) => `${t(`menu.${a.category}` as never)} ${t(a.label)}`.toLowerCase().includes(word))}
      render={(a) => (
        <>
          <span className="palette__category">{t(`menu.${a.category}` as never)}</span>
          <span className="palette__label">{t(a.label)}</span>
          {a.keys && <Kbd keys={a.keys} />}
        </>
      )}
      onPick={(a) => runAction(a.id)}
    />
  );
}

/** `query` aparece em `text` nessa ordem, não necessariamente junto. */
function subsequence(text: string, query: string): boolean {
  let i = 0;
  for (const char of text) if (char === query[i]) i++;
  return i === query.length;
}

function QuickOpen() {
  const t = useT();
  const root = useProject((s) => s.snapshot?.root ?? '');
  const [files, setFiles] = useState<string[]>([]);
  useEffect(() => {
    void api.fs.listFiles().then(setFiles).catch(() => setFiles([]));
  }, []);
  return (
    <ListPicker
      placeholder={t('dialog.quickOpen.placeholder')}
      items={files}
      filter={(path, q) => subsequence(relativeTo(path, root).toLowerCase(), q.replace(/\s+/g, ''))}
      render={(path) => (
        <>
          <span className="palette__label">{baseName(path)}</span>
          <span className="palette__category">{relativeTo(path, root)}</span>
        </>
      )}
      onPick={(path) => void useEditor.getState().openFile(path)}
    />
  );
}

/** "Selecionar tema": os temas da paleta, com o esquema ao lado. Escolher
 * grava a preferência, como nas Preferências. */
function ThemePicker() {
  const t = useT();
  const current = useApp((s) => s.settings?.theme);
  const items = [SYSTEM_THEME, ...THEMES.map((theme) => theme.id)];
  const name = (id: string) => (id === SYSTEM_THEME ? t('settings.theme.system') : (themeById(id)?.name ?? id));
  const detail = (id: string) => {
    if (id === SYSTEM_THEME) return t('settings.theme.systemHint');
    return themeById(id)?.scheme === 'light' ? t('settings.theme.light') : t('settings.theme.dark');
  };
  return (
    <ListPicker
      placeholder={t('dialog.theme.placeholder')}
      items={items}
      filter={(id, q) => name(id).toLowerCase().includes(q.trim())}
      render={(id) => (
        <>
          <span className="palette__label">{name(id)}</span>
          <span className="palette__category">{detail(id)}</span>
          {id === current && <Check size={14} className="palette__check" />}
        </>
      )}
      onPick={(id) => void useApp.getState().updateSettings((s) => ({ ...s, theme: id }))}
    />
  );
}

/** Os layouts com nome: os prontos primeiro, depois os gravados. O em uso
 * tem a marca, e "modificado" se a janela está diferente da foto dele. */
function LayoutPicker() {
  const t = useT();
  const settings = useApp((s) => s.settings);
  const { layout: active, dirty } = useLayoutStatus();
  const items = namedLayouts(settings);
  const detail = (layout: NamedLayout) => {
    if (layout.id === active.id && dirty) return t('layout.tag.modified');
    if (layout.preset) return t('layout.tag.preset');
    return layout.readOnly ? t('layout.tag.newer') : '';
  };
  return (
    <ListPicker
      placeholder={t('dialog.layout.placeholder')}
      items={items}
      filter={(layout, q) => layout.name.toLowerCase().includes(q.trim())}
      render={(layout) => (
        <>
          <span className="palette__label">{layout.name}</span>
          <span className="palette__category">{detail(layout)}</span>
          {layout.id === active.id && <Check size={14} className="palette__check" />}
        </>
      )}
      onPick={(layout) => void applyLayout(layout.id)}
    />
  );
}

/** O alvo dos botões do fluxo, como o seletor da barra de ferramentas, que
 * pode estar escondido pelo layout. */
function TargetPicker() {
  const t = useT();
  const processors = useProject((s) => s.snapshot?.processors);
  const target = useProject((s) => s.target);
  const items = useMemo(() => ['', ...(processors ?? []).map((p) => p.name)], [processors]);
  const name = (id: string) => id || t('toolbar.targetProject');
  return (
    <ListPicker
      placeholder={t('dialog.target.placeholder')}
      items={items}
      filter={(id, q) => name(id).toLowerCase().includes(q.trim())}
      render={(id) => (
        <>
          <span className="palette__label">{name(id)}</span>
          <span className="palette__category">{id ? t('dialog.target.processor') : t('dialog.target.project')}</span>
          {id === (target ?? '') && <Check size={14} className="palette__check" />}
        </>
      )}
      onPick={(id) => useProject.getState().setTarget(id || null)}
    />
  );
}

// Informação ---------------------------------------------------------------------

function ShortcutsDialog() {
  const t = useT();
  return (
    <Dialog title={t('shortcuts.title')} wide footer={<Button onClick={close}>{t('common.close')}</Button>}>
      <table className="table">
        <thead>
          <tr>
            <th>{t('shortcuts.action')}</th>
            <th>{t('shortcuts.keys')}</th>
          </tr>
        </thead>
        <tbody>
          {ACTIONS.filter((a) => a.keys).map((a) => (
            <tr key={a.id}>
              <td>{t(a.label)}</td>
              <td>
                <Kbd keys={a.keys!} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Dialog>
  );
}

// Perguntas ------------------------------------------------------------------------

function PromptDialog({ options, resolve }: { options: PromptOptions; resolve: (v: string | null) => void }) {
  const t = useT();
  const [value, setValue] = useState(options.initial ?? '');
  const input = useRef<HTMLInputElement>(null);
  const error = options.validate?.(value) ?? null;

  useEffect(() => {
    const el = input.current;
    if (!el) return;
    el.focus();
    const dot = options.selectStem ? value.lastIndexOf('.') : -1;
    el.setSelectionRange(0, dot > 0 ? dot : value.length);
    // Só ao abrir: a seleção inicial não acompanha o que se digita.
  }, []);

  return (
    <Dialog
      title={options.title}
      onSubmit={() => !error && value.trim() && resolve(value.trim())}
      footer={<Footer submit={t('common.ok')} disabled={!!error || !value.trim()} />}
    >
      <Field label={options.label} error={value ? error : null}>
        <input ref={input} className="input" value={value} placeholder={options.placeholder} onChange={(e) => setValue(e.target.value)} />
      </Field>
    </Dialog>
  );
}

// Relatórios ----------------------------------------------------------------

/** `lace report clean`: todos, ou todos menos os mais novos. Mostra o que vai
 * sair (o plano do Core) antes de apagar. */
function CleanReportsDialog() {
  const t = useT();
  const [mode, setMode] = useState<'all' | 'keep'>('keep');
  const [keep, setKeep] = useState('10');
  const [plan, setPlan] = useState<string[] | null>(null);
  const keepCount = Number.parseInt(keep, 10);
  const keepValid = Number.isFinite(keepCount) && keepCount >= 0;

  useEffect(() => {
    if (mode === 'keep' && !keepValid) {
      setPlan(null);
      return;
    }
    let cancelled = false;
    const request = mode === 'all' ? ({ kind: 'all' } as const) : ({ kind: 'keep_latest', keep: keepCount } as const);
    api.history
      .planCleanup(request)
      .then((ids) => !cancelled && setPlan(ids))
      .catch(() => !cancelled && setPlan([]));
    return () => {
      cancelled = true;
    };
  }, [mode, keepCount, keepValid]);

  const count = plan?.length ?? 0;
  const submit = async () => {
    if (!plan || count === 0) return;
    close();
    await removeReports(plan);
  };

  const preview = !plan
    ? ''
    : count === 0
      ? t('reports.cleanNothing')
      : count <= 6
        ? plan.join(', ')
        : `${plan[0]} … ${plan[count - 1]}`;

  return (
    <Dialog
      title={t('reports.cleanTitle')}
      onSubmit={() => void submit()}
      footer={
        <>
          <Button onClick={close}>{t('common.cancel')}</Button>
          <Button variant="danger" type="submit" disabled={count === 0}>
            {t('reports.cleanSubmit', { count })}
          </Button>
        </>
      }
    >
      <div className="radio-list" role="radiogroup">
        <label className="radio">
          <input type="radio" name="clean" checked={mode === 'keep'} onChange={() => setMode('keep')} />
          <span>{t('reports.cleanKeep')}</span>
          <input
            className="input input--narrow"
            type="number"
            min={0}
            value={keep}
            onFocus={() => setMode('keep')}
            onChange={(e) => setKeep(e.target.value)}
            aria-label={t('reports.cleanKeep')}
          />
        </label>
        <label className="radio">
          <input type="radio" name="clean" checked={mode === 'all'} onChange={() => setMode('all')} />
          <span>{t('reports.cleanAll')}</span>
        </label>
      </div>
      <p className="dialog__note">
        {plan && count > 0 && <strong>{t('reports.cleanCount', { count })}</strong>} {preview}
      </p>
      <p className="dialog__note dialog__note--muted">{t('reports.cleanHint')}</p>
    </Dialog>
  );
}

function ConfirmDialog({ options, resolve }: { options: ConfirmOptions; resolve: (v: string | null) => void }) {
  const primary = options.buttons.find((b) => b.primary) ?? options.buttons[0];
  return (
    <Dialog
      title={options.title}
      onSubmit={() => resolve(primary.value)}
      footer={options.buttons.map((b) => (
        <Button key={b.value} variant={b.danger ? 'danger' : b.primary ? 'primary' : 'default'} autoFocus={b === primary} onClick={() => resolve(b.value)}>
          {b.label}
        </Button>
      ))}
    >
      <p>{options.message}</p>
    </Dialog>
  );
}

export function Dialogs() {
  const dialog = useDialogs((s) => s.dialog);
  const hasProject = useProject((s) => s.snapshot !== null);
  if (!dialog) return null;
  switch (dialog.kind) {
    case 'newProject':
      return <NewProjectDialog />;
    case 'cleanReports':
      return hasProject ? <CleanReportsDialog /> : null;
    case 'newProcessor':
      return hasProject ? <NewProcessorDialog /> : null;
    case 'newVerilog':
      return hasProject ? (
        <NewVerilogDialog testbench={dialog.testbench} folder={dialog.folder} cocotb={dialog.cocotb} />
      ) : null;
    case 'newInput':
      return hasProject ? <NewInputDialog processor={dialog.processor} /> : null;
    case 'chooseTop':
      return hasProject ? <ChooseTopDialog /> : null;
    case 'chooseTestbench':
      return hasProject ? <ChooseTestbenchDialog /> : null;
    case 'waveSignals':
      return hasProject ? <WaveSignalsDialog /> : null;
    case 'install':
      return <InstallDialog components={dialog.components} />;
    case 'palette':
      return <CommandPalette />;
    case 'quickOpen':
      return hasProject ? <QuickOpen /> : null;
    case 'theme':
      return <ThemePicker />;
    case 'target':
      return hasProject ? <TargetPicker /> : null;
    case 'layout':
      return <LayoutPicker />;
    case 'shortcuts':
      return <ShortcutsDialog />;
    case 'prompt':
      return <PromptDialog options={dialog.options} resolve={dialog.resolve} />;
    case 'confirm':
      return <ConfirmDialog options={dialog.options} resolve={dialog.resolve} />;
  }
}
