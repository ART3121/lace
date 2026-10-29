// As operações: começar um fluxo, acompanhar, cancelar, e o que fica
// depois (problemas, último resultado de cada fluxo, estado de cada ação no
// navegador de fluxo).
//
// As mensagens de uma operação (jobs.rs) chegam em ordem: started, phase,
// events (em lotes), build (um por processador), e por fim finished ou
// failed. Cada operação é uma rodada no INFO (console/consoles.ts), com
// cabeçalho e fim; cada linha leva a etapa que a escreveu, e o terminal
// dessa etapa, se estiver aberto, também a mostra. O resultado final vira o
// resumo da rodada, os problemas e os marcadores no editor.

import { create } from 'zustand';

import { beginRun, endRun, noteStepDir, revealRunError, writeLine, type LineStyle } from '../console/consoles';
import * as fmt from '../console/format';
import { setDiagnostics } from '../editor/monaco';
import { formatDuration, t, type Key } from '../i18n';
import { api } from '../ipc/api';
import type {
  BuildResult,
  Diagnostic,
  Event,
  Invocation,
  Status,
  Step,
  SynthesisResult,
  UpdateReport,
} from '../ipc/lace-types';
import type {
  CliFlow,
  CliOutcome,
  FlowName,
  FlowOutcome,
  FlowRequest,
  IpcError,
  JobMessage,
  Phase,
} from '../ipc/types';
import { relativeTo } from '../util/paths';
import { useApp } from './app';
import { useEditor } from './editor';
import { useHierarchy } from './hierarchy';
import { useLayout, type ConsoleChannel, type ViewId } from './layout';
import { learnSession } from './learnSession';
import { isConsoleChannel, regionOf } from './layoutModel';
import { useProject } from './project';
import { useSchematic } from './schematic';
import { showError, useToasts } from './toasts';
import { openWaveTab } from './waves';

export type ActionStatus = 'ok' | 'failed' | 'running';

export interface RunningJob {
  id: number | null;
  flow: FlowName | CliFlow;
  phase: Phase | null;
  startedAt: number;
  command: string;
  statusKey: string;
}

export interface LastRun {
  flow: FlowName | CliFlow;
  /** Foi a simulação rápida. */
  fast?: boolean;
  succeeded: boolean;
  status: Status | 'error';
  durationMs: number;
  finishedAt: number;
}

/** Uma linha da saída do `aurora install` ou do `aurora update`. */
export interface CliLine {
  text: string;
  style: LineStyle;
}

/** A última instalação ou atualização: o comando e o que ele escreveu. A
 * tela do bundle mostra. */
export interface CliLog {
  flow: CliFlow;
  command: string;
  lines: CliLine[];
}

/** Quantas linhas do `aurora install` e do `aurora update` guardar. */
const CLI_LOG_LINES = 2000;

interface JobsState {
  running: RunningJob | null;
  /** A última instalação ou atualização. */
  cliLog: CliLog | null;
  last: LastRun | null;
  outcomes: Partial<Record<FlowName, FlowOutcome>>;
  statusByKey: Record<string, ActionStatus>;
  problems: Diagnostic[];
  /** A última síntese que rodou (de qualquer fluxo), para as vistas. */
  synthesis: SynthesisResult | null;

  run: (request: FlowRequest, statusKey?: string) => Promise<FlowOutcome | null>;
  cancel: () => Promise<void>;
  install: (components: string[]) => Promise<boolean>;
  /** `aurora update --yes`, depois de a interface confirmar. Devolve o
   * relatório da CLI, ou null se falhou. */
  update: () => Promise<UpdateReport | null>;
}

/** A etapa de cada passo: além do INFO, o terminal dela mostra a linha. */
const STEP_CHANNEL: Record<Step, ConsoleChannel> = {
  preprocess: 'cmm',
  compile: 'cmm',
  pre_assemble: 'asm',
  assemble: 'asm',
  check_syntax: 'verilog',
  lint: 'verilog',
  elaborate: 'wave',
  verilate: 'wave',
  simulate: 'wave',
  synthesize: 'rtlViewer',
  graph: 'rtlViewer',
  render: 'rtlViewer',
  // Os passos do Quartus (`aurora fpga build` e `program`). O `synthesize`
  // dele vai para o mesmo console pela ferramenta (`channelOf`).
  fit: 'fpga',
  bitstream: 'fpga',
  timing: 'fpga',
  program: 'fpga',
};

/** A etapa de um passo: o do Quartus vai todo para a da placa, inclusive o
 * `synthesize`, que na síntese do Yosys é do RTLViewer. */
function channelOf(step: Step, tool: string): ConsoleChannel {
  return tool === 'quartus' ? 'fpga' : STEP_CHANNEL[step];
}

/** A etapa do título de cada fase. */
const PHASE_CHANNEL: Record<Phase, ConsoleChannel> = {
  build: 'cmm',
  check: 'verilog',
  hierarchy: 'verilog',
  simulate: 'wave',
  synthesize: 'rtlViewer',
  schematic: 'rtlViewer',
  rtl_viewer: 'rtlViewer',
  wave: 'wave',
  learn: 'verilog',
  fpga: 'fpga',
  program: 'fpga',
};

/** A etapa onde cada operação começa: o comando dela e os avisos (falha ao
 * iniciar, pedido de cancelamento) também vão para o terminal dela. */
const START_CHANNEL: Record<FlowName, ConsoleChannel> = {
  build: 'cmm',
  check: 'verilog',
  simulate: 'wave',
  synthesize: 'rtlViewer',
  schematic: 'rtlViewer',
  learn: 'verilog',
  fpga_build: 'fpga',
  fpga_program: 'fpga',
};

/** Uma linha no registro da instalação ou da atualização. */
function cliLine(text: string, style: LineStyle = 'plain'): void {
  const log = useJobs.getState().cliLog;
  if (!log) return;
  const lines = [...log.lines, { text, style }];
  useJobs.setState({ cliLog: { ...log, lines: lines.slice(-CLI_LOG_LINES) } });
}

/** A etapa do resumo de cada fluxo. */
const FLOW_CHANNEL: Record<FlowName, ConsoleChannel> = {
  build: 'asm',
  check: 'verilog',
  simulate: 'wave',
  synthesize: 'rtlViewer',
  schematic: 'rtlViewer',
  learn: 'verilog',
  fpga_build: 'fpga',
  fpga_program: 'fpga',
};

function verbose(): boolean {
  return useApp.getState().settings?.verbose ?? false;
}

function root(): string | null {
  return useProject.getState().snapshot?.root ?? null;
}

function rel(path: string): string {
  return relativeTo(path, root());
}

/** Traz o INFO para a frente da região dele, uma vez por rodada (no começo
 * e numa falha), sem trocar de aba no meio. Fica onde está quem escolheu o
 * shell ou um terminal de etapa nessa região, e um INFO que o usuário fechou
 * não reabre sozinho: nos dois casos, a aba ganha o ponto de saída nova. */
function showInfo(): void {
  const layout = useLayout.getState();
  // Numa sessão de exercícios o painel só aparece quando o aluno o chama.
  if (learnSession.active()) {
    layout.markUnread('info');
    return;
  }
  const region = layout.live.regions[regionOf(layout.live, 'info')];
  if (region.visible && (region.active === 'terminal' || isConsoleChannel(region.active))) {
    layout.markUnread('info');
    return;
  }
  if (!layout.revealView('info')) layout.markUnread('info');
}

/** O ponto de saída nova numa aba de console que está aberta. */
function markUnread(view: ViewId): void {
  const layout = useLayout.getState();
  if (!layout.live.hidden.views.includes(view)) layout.markUnread(view);
}

/** Uma linha já formatada (console/format.ts) no INFO e no terminal da
 * etapa. `error` marca onde o INFO para se a rodada falhar. */
function write(stage: ConsoleChannel | null, text: string, error = false): void {
  writeLine(stage, text, error);
  markUnread('info');
  if (stage) markUnread(stage);
}

/** Quem escreveu as últimas linhas da rodada: uma linha de outro autor
 * (outra ferramenta, o testbench) vem depois do nome dele. */
let lastBlock: string | null = null;

/** O nome do autor das próximas linhas, se ele mudou. */
function writeBlock(stage: ConsoleChannel, key: string, title: string): void {
  if (lastBlock === key) return;
  lastBlock = key;
  write(stage, fmt.block(title));
}

function commandLine(command: Invocation): string {
  const quote = (s: string) => (/[\s"']/.test(s) || s === '' ? `"${s}"` : s);
  return [command.program, ...command.args].map(quote).join(' ');
}

function statusText(status: Status): string {
  return t(`console.status.${status}` as Key);
}

/** O resultado de uma etapa: o marcador, e o erro marcado para o INFO. */
function writeResult(stage: ConsoleChannel, status: Status, text: string): void {
  const outcome = fmt.outcomeOf(status);
  write(stage, fmt.result(outcome, text), outcome === 'error');
}

/** "Simulação rápida · 14:02:31": o cabeçalho da rodada. */
function runTitle(request: FlowRequest): string {
  const name = t(request.flow === 'simulate' && request.fast ? 'flowName.fastSimulate' : (`flowName.${request.flow}` as Key));
  const time = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  return t('console.runStart', { flow: capitalized(name), time });
}

function capitalized(text: string): string {
  return text.charAt(0).toLocaleUpperCase() + text.slice(1);
}

/** O rótulo de uma fase. A simulação rápida (a chave `fastSim`) tem o
 * dela: a fase que o backend manda é a mesma da simulação com onda. */
export function phaseKey(phase: Phase, statusKey: string): Key {
  if (phase === 'simulate' && statusKey.split(':')[0] === 'fastSim') return 'console.phase.fastSimulate';
  return `console.phase.${phase}` as Key;
}

/** O nome da última operação, para a barra de status. */
export function lastFlowKey(last: LastRun): Key {
  return last.fast ? 'flowName.fastSimulate' : (`flowName.${last.flow}` as Key);
}

/** Uma mensagem de evento do Core: a linha vai recuada embaixo do nome de
 * quem a escreveu, a ferramenta ou, na simulação, o testbench. */
function handleEvent(event: Event): void {
  switch (event.event) {
    case 'step_started': {
      // Os caminhos relativos que a ferramenta escrever valem a partir do
      // CWD dela: o link do console os procura ali também.
      noteStepDir((event.command as Invocation).cwd);
      // Com o modo detalhado, o comando exato abre o bloco da ferramenta.
      if (verbose()) {
        const stage = channelOf(event.step as Step, event.tool as string);
        lastBlock = `${event.step}:${event.tool}`;
        write(stage, fmt.block(`$ ${commandLine(event.command as Invocation)}`));
      }
      break;
    }
    case 'output': {
      const stage = channelOf(event.step as Step, event.tool as string);
      // Na simulação, o que não é mensagem da ferramenta é o testbench: o
      // $display, o log do cocotb.
      const program = event.step === 'simulate' && !event.diagnostic;
      if (program) writeBlock(stage, 'program', t('console.programOutput'));
      else writeBlock(stage, `${event.step}:${event.tool}`, event.tool as string);
      if (event.diagnostic) write(stage, fmt.toolMessage(event.line), fmt.isErrorMessage(event.line));
      else write(stage, fmt.output(event.line));
      break;
    }
    case 'step_finished': {
      const stage = channelOf(event.step as Step, event.tool as string);
      const termination = event.termination as { kind: string; value?: number };
      const ok = termination.kind === 'exited' && termination.value === 0;
      const time = formatDuration(event.duration_ms);
      if (ok) {
        if (verbose()) write(stage, fmt.block(`${event.tool} ${t('console.step.done')} · ${time}`));
        break;
      }
      // Cancelar não é falha da ferramenta: marcador e rótulo próprios.
      const cancelled = termination.kind === 'cancelled';
      const how =
        termination.kind === 'exited'
          ? t('console.termination.code', { code: termination.value ?? '?' })
          : t(`console.termination.${termination.kind}` as Key);
      const label = cancelled ? t('console.step.cancelled') : t('console.step.failed');
      write(stage, fmt.attention(cancelled ? 'warning' : 'error', `${event.tool} ${label}: ${how} · ${time}`), !cancelled);
      lastBlock = null;
      break;
    }
  }
}

/** O resumo de um build, como a linha de título da CLI. Os diagnósticos vão
 * junto porque a mensagem do YANC não traz o arquivo (`Error on line 16`); o
 * Lace o preenche, e a linha com `arquivo:linha` vira link. Nas outras fases a
 * ferramenta já escreveu o arquivo e a linha ao vivo, e o resumo não repete. */
function writeBuild(result: BuildResult): void {
  const failedStep = result.failed_step as Step | null;
  const stage = failedStep ? STEP_CHANNEL[failedStep] : 'asm';
  lastBlock = null;
  writeResult(
    stage,
    result.status,
    t('console.buildTitle', {
      name: result.processor,
      lang: result.language === 'cpp' ? 'C' : 'C±',
      mhz: result.frequency_mhz,
      clocks: result.clocks,
      status: statusText(result.status),
      time: formatDuration(result.duration_ms),
    }),
  );
  writeDiagnostics(stage, result.diagnostics);
}

function writeDiagnostics(stage: ConsoleChannel, diagnostics: Diagnostic[]): void {
  for (const d of diagnostics) {
    if (d.severity === 'info' && !verbose()) continue;
    const where = d.file ? `${rel(d.file)}${d.line ? `:${d.line}` : ''}${d.column ? `:${d.column}` : ''}: ` : '';
    const label = t(`panel.severity.${d.severity}` as Key);
    write(stage, fmt.diagnostic(where, d.severity, label, d.message), d.severity === 'error');
  }
}

/** Todos os diagnósticos de um resultado. */
function collectDiagnostics(outcome: FlowOutcome): Diagnostic[] {
  return [
    ...outcome.builds.flatMap((b) => b.diagnostics),
    ...(outcome.check?.diagnostics ?? []),
    ...(outcome.simulation?.diagnostics ?? []),
    ...(outcome.synthesis?.diagnostics ?? []),
    ...(outcome.schematic?.diagnostics ?? []),
    ...(outcome.learn?.diagnostics ?? []),
    ...(outcome.fpga?.diagnostics ?? []),
    ...(outcome.fpga_program?.diagnostics ?? []),
  ];
}

/** O resumo final de um fluxo: o resultado de cada etapa com o marcador, e
 * embaixo dele o que ela produziu. */
function writeOutcome(outcome: FlowOutcome): void {
  let stage = FLOW_CHANNEL[outcome.flow];
  lastBlock = null;
  const failedBuild = outcome.builds.find((b) => b.status !== 'succeeded');
  if (failedBuild && outcome.flow !== 'build') {
    const phase = {
      check: 'flowName.check',
      simulate: 'flowName.simulate',
      synthesize: 'flowName.synthesize',
      fpga_build: 'flowName.fpga_build',
    }[outcome.flow as 'check' | 'simulate' | 'synthesize' | 'fpga_build'];
    stage = failedBuild.failed_step ? STEP_CHANNEL[failedBuild.failed_step as Step] : 'cmm';
    if (phase) write(stage, fmt.attention('warning', t('console.notRun', { phase: t(phase as Key), name: failedBuild.processor })));
  }
  if (outcome.check) {
    const c = outcome.check;
    writeResult(
      stage,
      c.status,
      t('console.checkTitle', {
        targets: c.targets.join(', ') || '-',
        status: statusText(c.status),
        time: formatDuration(c.duration_ms),
      }),
    );
  }
  if (outcome.simulation) {
    const s = outcome.simulation;
    writeResult(
      stage,
      s.status,
      t(s.fast ? 'console.fastSimTitle' : 'console.simTitle', {
        top: s.top,
        simulator: s.simulator === 'icarus' ? 'Icarus' : 'Verilator',
        status: statusText(s.status),
        time: formatDuration(s.duration_ms),
      }),
    );
    for (const port of outcome.outputs) {
      const text = t('console.output', { port: port.port, values: port.error ? port.error.message : port.values.join(' ') });
      write(stage, port.error ? fmt.item('error', text) : fmt.detail(text), !!port.error);
    }
    // cocotb: um teste por linha e quantos passaram; a falha vem com a linha
    // do .py, que também está nos problemas.
    if (s.tests) {
      for (const c of s.tests.cases) {
        const where = c.status === 'failed' && c.file ? ` (${rel(c.file)}${c.line ? `:${c.line}` : ''})` : '';
        const [key, mark]: [Key, fmt.Outcome] =
          c.status === 'passed'
            ? ['console.testPassed', 'ok']
            : c.status === 'failed'
              ? ['console.testFailed', 'error']
              : ['console.testSkipped', 'skipped'];
        write(stage, fmt.item(mark, `${t(key, { name: c.name })}${where}`), mark === 'error');
      }
      const total = s.tests.passed + s.tests.failed + s.tests.skipped;
      write(stage, fmt.detail(t('console.testsSummary', { passed: s.tests.passed, total })));
    }
    for (const missing of s.missing_inputs) write(stage, fmt.attention('warning', t('console.missingInput', { path: rel(missing) })));
    if (s.status === 'timed_out') write(stage, fmt.attention('warning', t('console.timedOutHint')));
    if (s.waveform) write(stage, fmt.note(t('console.waveformAt', { path: rel(s.waveform.path) })));
  }
  if (outcome.synthesis) {
    const s = outcome.synthesis;
    writeResult(stage, s.status, t('console.synthTitle', { top: s.top, status: statusText(s.status), time: formatDuration(s.duration_ms) }));
  }
  if (outcome.schematic) {
    const s = outcome.schematic;
    writeResult(
      stage,
      s.status,
      t('console.schematicTitle', { module: s.module, status: statusText(s.status), time: formatDuration(s.duration_ms) }),
    );
  }
  if (outcome.fpga) {
    const f = outcome.fpga;
    writeResult(
      stage,
      f.status,
      t('console.fpgaTitle', { top: f.top, board: f.board, status: statusText(f.status), time: formatDuration(f.duration_ms) }),
    );
    for (const note of f.notes) write(stage, fmt.attention('warning', t('console.note', { note })));
    // Os erros e avisos do Quartus já passaram ao vivo; o tempo não
    // atendido não reprova a compilação, mas precisa ser visto.
    if (f.timing && !f.timing.met) write(stage, fmt.attention('warning', t('console.fpgaTimingNotMet')));
    if (f.bitstream) write(stage, fmt.note(t('console.fpgaBitstream', { path: rel(f.bitstream) })));
  }
  if (outcome.fpga_program) {
    const p = outcome.fpga_program;
    writeResult(
      stage,
      p.status,
      t('console.programTitle', { board: p.board, cable: p.cable, status: statusText(p.status), time: formatDuration(p.duration_ms) }),
    );
    // O aviso de .sof velho não vem das linhas ao vivo do Quartus.
    writeDiagnostics(stage, p.diagnostics.filter((d) => !d.raw));
  }
  if (outcome.wave) {
    write(stage, fmt.note(t('console.waveOpened', { pid: outcome.wave.pid, path: rel(outcome.wave.waveform) })));
    for (const processor of outcome.wave.outdated) {
      write(stage, fmt.attention('warning', `${processor}: ${t('wave.outdated')}`));
    }
  }
  if (outcome.wave_tab) {
    write(stage, fmt.note(t('console.waveTab', { path: rel(outcome.wave_tab) })));
  }
  if (outcome.wave_error) {
    write(stage, fmt.attention('error', t('console.waveError', { error: outcome.wave_error.message })), true);
  }
  if (outcome.flow === 'build' && outcome.builds.length === 0) {
    stage = 'cmm';
    write(stage, fmt.detail(t('console.noProcessors')));
  }
  if (outcome.learn) {
    const g = outcome.learn;
    const verdict: fmt.Outcome = g.verdict === 'solved' ? 'ok' : g.verdict === 'cancelled' ? 'warning' : 'error';
    write(
      stage,
      fmt.result(
        verdict,
        t('console.learnTitle', { exercise: g.exercise, verdict: t(`learn.short.${g.verdict}` as Key), time: formatDuration(g.duration_ms) }),
      ),
      verdict === 'error',
    );
    for (const o of g.outputs.filter((o) => o.mismatches > 0)) {
      const text = t('console.learnOutput', { output: o.name, wrong: o.mismatches, samples: g.samples, first: o.first_ns ?? '-' });
      write(stage, fmt.item('error', text.trim()), true);
    }
    // O contraexemplo: as entradas e as saídas que erraram na primeira amostra errada.
    const first = g.counterexample;
    if (first) {
      const inputs = first.inputs.map((i) => `${i.name} = ${i.text}`).join(', ');
      write(stage, fmt.detail(t('console.learnFirst', { time: first.time_ns, inputs }).trim()), true);
      for (const o of first.outputs.filter((o) => o.wrong)) {
        const text = t('console.learnFirstOutput', { output: o.name, expected: o.expected, actual: o.actual });
        write(stage, fmt.item('error', text.trim()), true);
      }
    }
    writeDiagnostics(stage, g.diagnostics);
  }
  if (outcome.report) write(stage, fmt.note(t('console.report', { id: outcome.report })));
  if (outcome.report_error) write(stage, fmt.attention('warning', t('console.reportError', { error: outcome.report_error })));
}

/** O status que resume um fluxo, para a barra de status. */
function outcomeStatus(outcome: FlowOutcome): Status {
  const failedBuild = outcome.builds.find((b) => b.status !== 'succeeded');
  if (failedBuild) return failedBuild.status as Status;
  if (outcome.learn) return outcome.learn.verdict === 'cancelled' ? 'cancelled' : outcome.succeeded ? 'succeeded' : 'failed';
  return (outcome.fpga_program?.status ??
    outcome.fpga?.status ??
    outcome.schematic?.status ??
    outcome.synthesis?.status ??
    outcome.simulation?.status ??
    outcome.check?.status ??
    (outcome.succeeded ? 'succeeded' : 'failed')) as Status;
}

/** O que a CLI escreve no JSON quando o comando não roda. */
interface CliFailure {
  error?: { code?: string; message?: string; hint?: string | null };
}

/** Roda uma operação da CLI (`aurora install`, `aurora update`): a tela do
 * bundle abre e mostra cada linha (`cliLog`), o fim vai para `last`, e o
 * bundle é relido, porque a operação mexe nele. O erro que a CLI descreve no
 * JSON vira o aviso. Devolve o resultado, ou null se a operação não chegou ao
 * fim. */
function runCli(flow: CliFlow, start: (onMessage: (m: JobMessage) => void) => Promise<number>): Promise<CliOutcome | null> {
  if (useJobs.getState().running) {
    showError({ code: 'busy', message: 'Another operation is running' } satisfies IpcError);
    return Promise.resolve(null);
  }
  const startedAt = Date.now();
  useJobs.setState({
    running: { id: null, flow, phase: null, startedAt, command: '', statusKey: flow },
    cliLog: { flow, command: `aurora ${flow}`, lines: [] },
  });
  useEditor.getState().openView('toolchain');
  return new Promise((resolve) => {
    const done = (outcome: CliOutcome | null, error?: IpcError, hint?: string | null) => {
      const ok = outcome?.succeeded ?? false;
      useJobs.setState({
        running: null,
        last: { flow, succeeded: ok, status: ok ? 'succeeded' : 'failed', durationMs: Date.now() - startedAt, finishedAt: Date.now() },
      });
      if (error) {
        cliLine(error.message, 'error');
        if (hint) cliLine(hint, 'dim');
        showError(error);
      }
      void useApp.getState().refreshToolchain();
      resolve(outcome);
    };
    start((message) => {
      switch (message.type) {
        case 'started': {
          useJobs.setState({ running: { ...useJobs.getState().running!, id: message.job, command: message.command } });
          const log = useJobs.getState().cliLog;
          if (log) useJobs.setState({ cliLog: { ...log, command: message.command } });
          break;
        }
        case 'cli_output':
          cliLine(message.line, message.stream === 'stderr' ? 'plain' : 'dim');
          break;
        case 'finished': {
          const outcome = message.outcome as CliOutcome;
          const failure = outcome.succeeded ? undefined : (outcome.result as CliFailure | null)?.error;
          if (failure?.message) done(outcome, { code: failure.code ?? 'cli', message: failure.message }, failure.hint);
          else done(outcome);
          break;
        }
        case 'failed':
          done(null, message.error);
          break;
        default:
          break;
      }
    }).catch((error: IpcError) => done(null, error));
  });
}

export const useJobs = create<JobsState>((set, get) => ({
  running: null,
  cliLog: null,
  last: null,
  outcomes: {},
  statusByKey: {},
  problems: [],
  synthesis: null,

  run: async (request, statusKey) => {
    if (get().running) {
      showError({ code: 'busy', message: 'Another operation is running' } satisfies IpcError);
      return null;
    }
    // Os arquivos abertos são gravados antes, como na AURORA: compilar o
    // que está na tela, não o que estava no disco.
    if (!(await useEditor.getState().saveAll())) return null;

    const key = statusKey ?? request.flow;
    const startedAt = Date.now();
    set({
      running: { id: null, flow: request.flow, phase: null, startedAt, command: '', statusKey: key },
      statusByKey: { ...get().statusByKey, [key]: 'running' },
    });
    // Uma rodada nova no INFO, que vem para a frente agora e não troca de
    // aba até o fim.
    beginRun(runTitle(request));
    lastBlock = null;
    showInfo();

    return new Promise<FlowOutcome | null>((resolve) => {
      const finish = (outcome: FlowOutcome | null, error?: IpcError) => {
        const durationMs = Date.now() - startedAt;
        if (outcome) {
          const diagnostics = collectDiagnostics(outcome);
          setDiagnostics(diagnostics.filter((d) => d.severity === 'error' || d.severity === 'warning'));
          set({
            running: null,
            problems: diagnostics,
            outcomes: { ...get().outcomes, [outcome.flow]: outcome },
            synthesis: outcome.synthesis ?? get().synthesis,
            statusByKey: { ...get().statusByKey, [key]: outcome.succeeded ? 'ok' : 'failed' },
            last: {
              flow: outcome.flow,
              fast: request.flow === 'simulate' && !!request.fast,
              succeeded: outcome.succeeded,
              status: outcomeStatus(outcome),
              durationMs,
              finishedAt: Date.now(),
            },
          });
          writeOutcome(outcome);
          if (outcome.schematic_error) {
            write('rtlViewer', fmt.attention('warning', outcome.schematic_error.message));
          }
          const status = outcomeStatus(outcome);
          const errors = diagnostics.filter((d) => d.severity === 'error').length;
          const warnings = diagnostics.filter((d) => d.severity === 'warning').length;
          endRun(
            fmt.footer(
              fmt.outcomeOf(status),
              `${capitalized(t('console.runEnd', { status: statusText(status), time: formatDuration(durationMs) }))} · ${t('console.runProblems', { errors, warnings })}`,
            ),
          );
          if (!outcome.succeeded) {
            showInfo();
            revealRunError();
          }
          if (outcome.wave_error) showError(outcome.wave_error);
          if (outcome.schematic_error) showError(outcome.schematic_error);
          // A onda numa aba: abre, ou recarrega a que já está aberta, porque
          // a simulação acabou de regravar o arquivo.
          if (outcome.wave_tab) openWaveTab(outcome.wave_tab, true);
          // Depois da síntese o RTLViewer abre sozinho, como na AURORA, e
          // desenha o netlist que ela gravou.
          if (outcome.flow === 'synthesize' && outcome.synthesis?.netlist) {
            useEditor.getState().openView('schematic');
          }
          // A compilação para a placa mostra os recursos e o tempo na tela
          // da placa.
          if (outcome.flow === 'fpga_build' && outcome.fpga) {
            useEditor.getState().openView('board');
          }
          void useProject.getState().refresh();
          useProject.getState().bumpTree();
          // Compilou: a hierarquia elaborada pelo Icarus acompanha.
          if (outcome.succeeded && ['check', 'simulate', 'build', 'synthesize'].includes(outcome.flow)) {
            void useHierarchy.getState().refresh();
          }
        } else {
          set({
            running: null,
            statusByKey: { ...get().statusByKey, [key]: 'failed' },
            last: {
              flow: request.flow,
              fast: request.flow === 'simulate' && !!request.fast,
              succeeded: false,
              status: 'error',
              durationMs,
              finishedAt: Date.now(),
            },
          });
          if (error) {
            write(START_CHANNEL[request.flow], fmt.result('error', t('console.failedToRun', { message: error.message })), true);
          }
          endRun(
            fmt.footer(
              'error',
              capitalized(t('console.runEnd', { status: statusText('failed'), time: formatDuration(durationMs) })),
            ),
          );
          showInfo();
          if (error) showError(error);
        }
        resolve(outcome);
      };

      const onMessage = (message: JobMessage) => {
        switch (message.type) {
          case 'started':
            set({ running: { ...get().running!, id: message.job, command: message.command } });
            write(START_CHANNEL[request.flow], fmt.command(message.command));
            break;
          case 'phase': {
            set({ running: { ...get().running!, phase: message.phase } });
            lastBlock = null;
            write(PHASE_CHANNEL[message.phase], fmt.phase(t(phaseKey(message.phase, key))));
            break;
          }
          case 'events':
            message.events.forEach(handleEvent);
            break;
          case 'output_omitted':
            write(
              channelOf(message.step, message.tool),
              fmt.note(t('console.outputOmitted', { count: message.lines.toLocaleString() })),
            );
            break;
          case 'build':
            writeBuild(message.result);
            break;
          case 'finished':
            finish(message.outcome as FlowOutcome);
            break;
          case 'failed':
            finish(null, message.error);
            break;
          case 'cli_output':
            write(START_CHANNEL[request.flow], fmt.output(message.line));
            break;
        }
      };

      api.flow.start(request, onMessage).catch((error: IpcError) => finish(null, error));
    });
  },

  cancel: async () => {
    const running = get().running;
    if (!running || running.flow === 'update') return;
    if (running.flow === 'install') cliLine(t('console.cancelRequested'), 'warning');
    else write(START_CHANNEL[running.flow], fmt.attention('warning', t('console.cancelRequested')));
    try {
      await api.flow.cancel(running.id);
    } catch (error) {
      showError(error);
    }
  },

  install: async (components) => {
    const outcome = await runCli('install', (onMessage) => api.toolchain.install(components, onMessage));
    if (!outcome) return false;
    cliLine(t('console.installDone', { code: outcome.exit_code ?? '-' }), outcome.succeeded ? 'success' : 'error');
    if (outcome.succeeded) {
      useToasts.getState().push({ kind: 'success', title: t('console.installDone', { code: 0 }) });
    }
    return outcome.succeeded;
  },

  update: async () => {
    const outcome = await runCli('update', api.toolchain.update);
    if (!outcome?.succeeded) {
      if (outcome) cliLine(t('console.updateFailed', { code: outcome.exit_code ?? '-' }), 'error');
      return null;
    }
    const report = outcome.result as UpdateReport;
    const version = report.aurora.latest;
    if (report.action === 'updated') {
      const changes = report.method === 'components' ? report.changes : null;
      const text = changes
        ? t('console.updateDoneParts', {
            version,
            size: `${(changes.download_bytes / 1048576).toFixed(1)} MiB`,
            components: changes.components.join(', '),
          })
        : t('console.updateDone', { version });
      cliLine(text, 'success');
      useToasts.getState().push({ kind: 'success', title: text });
      // O Studio que está aberto é o antigo: ele reinicia (state/updates.ts).
      cliLine(t('console.updateRestart'), 'info');
    } else if (report.action === 'wizard_opened') {
      // O diálogo que fecha o Studio vem em seguida (state/updates.ts).
      cliLine(t('console.updateWizard', { version }), 'info');
    } else {
      const text = t('console.updateCurrent', { version });
      cliLine(text, 'info');
      useToasts.getState().push({ kind: 'info', title: text });
    }
    return report;
  },
}));

// Outro projeto (ou nenhum): o que as operações deixaram era do anterior.
// Problemas, marcadores, o "Último" da barra e o estado dos botões do fluxo
// recomeçam; uma operação ainda rodando segue até o fim.
useProject.subscribe((state, previous) => {
  if (state.snapshot?.spf === previous.snapshot?.spf) return;
  useJobs.setState({ last: null, outcomes: {}, statusByKey: {}, problems: [], synthesis: null });
  useSchematic.getState().clear();
  setDiagnostics([]);
});
