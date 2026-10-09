// As operações: começar um fluxo, acompanhar, cancelar, e o que fica
// depois (problemas, último resultado de cada fluxo, estado de cada ação no
// navegador de fluxo).
//
// As mensagens de uma operação (jobs.rs) chegam em ordem: started, phase,
// events (em lotes), build (um por processador), e por fim finished ou
// failed. Cada linha de ferramenta vai para o console do passo que a
// escreveu (console/consoles.ts); o resultado final vira o resumo no
// console, os problemas e os marcadores no editor.

import { create } from 'zustand';

import { writeLine, styleForToolLine, type LineStyle } from '../console/consoles';
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
import { isOnScreen, useLayout, type ConsoleChannel } from './layout';
import { learnSession } from './learnSession';
import { regionOf } from './layoutModel';
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

/** Uma linha da saída do `lace install` ou do `lace update`. */
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

/** Quantas linhas do `lace install` e do `lace update` guardar. */
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
  /** `lace update --yes`, depois de a interface confirmar. Devolve o
   * relatório da CLI, ou null se falhou. */
  update: () => Promise<UpdateReport | null>;
}

/** O console de cada passo. */
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
  synthesize: 'prism',
  graph: 'prism',
  render: 'prism',
  // Os passos do Quartus (`lace fpga build` e `program`). O `synthesize`
  // dele vai para o mesmo console pela ferramenta (`channelOf`).
  fit: 'fpga',
  bitstream: 'fpga',
  timing: 'fpga',
  program: 'fpga',
};

/** O console de um passo: o do Quartus vai todo para o da placa, inclusive
 * o `synthesize`, que na síntese do Yosys é do PRISM. */
function channelOf(step: Step, tool: string): ConsoleChannel {
  return tool === 'quartus' ? 'fpga' : STEP_CHANNEL[step];
}

const PHASE_CHANNEL: Record<Phase, ConsoleChannel> = {
  build: 'cmm',
  check: 'verilog',
  simulate: 'wave',
  synthesize: 'prism',
  schematic: 'prism',
  wave: 'wave',
  learn: 'verilog',
  fpga: 'fpga',
  program: 'fpga',
};

/** O console onde cada operação começa: o comando dela e os avisos (falha
 * ao iniciar, pedido de cancelamento) vão para lá. */
const START_CHANNEL: Record<FlowName, ConsoleChannel> = {
  build: 'cmm',
  check: 'verilog',
  simulate: 'wave',
  synthesize: 'prism',
  schematic: 'prism',
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

const FLOW_CHANNEL: Record<FlowName, ConsoleChannel> = {
  build: 'asm',
  check: 'verilog',
  simulate: 'wave',
  synthesize: 'prism',
  schematic: 'prism',
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

/** Traz o console da etapa para a frente da região dele. Quem está no
 * terminal de shell, na mesma região, não é tirado de lá; e um console que o
 * layout escondeu não reaparece sozinho. Nos dois casos, a aba ganha o ponto
 * de saída nova. */
function show(channel: ConsoleChannel): void {
  const layout = useLayout.getState();
  // Numa sessão de exercícios o painel só aparece quando o aluno o chama.
  if (learnSession.active()) {
    layout.markUnread(channel);
    return;
  }
  if (isOnScreen(layout, 'terminal') && regionOf(layout.live, 'terminal') === regionOf(layout.live, channel)) {
    layout.markUnread(channel);
    return;
  }
  if (!layout.revealView(channel)) layout.markUnread(channel);
}

function write(channel: ConsoleChannel, text: string, style: LineStyle = 'plain'): void {
  writeLine(channel, text, style);
  useLayout.getState().markUnread(channel);
}

function commandLine(command: Invocation): string {
  const quote = (s: string) => (/[\s"']/.test(s) || s === '' ? `"${s}"` : s);
  return [command.program, ...command.args].map(quote).join(' ');
}

function statusStyle(status: Status): LineStyle {
  if (status === 'succeeded') return 'success';
  if (status === 'cancelled' || status === 'timed_out') return 'warning';
  return 'error';
}

function statusText(status: Status): string {
  return t(`console.status.${status}` as Key);
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

/** Uma mensagem de evento do Core no console do passo. */
function handleEvent(event: Event): void {
  switch (event.event) {
    case 'step_started': {
      const channel = channelOf(event.step as Step, event.tool as string);
      if (verbose()) write(channel, `$ ${commandLine(event.command as Invocation)}`, 'command');
      break;
    }
    case 'output': {
      const channel = channelOf(event.step as Step, event.tool as string);
      write(channel, event.line, styleForToolLine(event.line, event.diagnostic, event.stream === 'stderr'));
      break;
    }
    case 'step_finished': {
      const channel = channelOf(event.step as Step, event.tool as string);
      const termination = event.termination as { kind: string; value?: number };
      const ok = termination.kind === 'exited' && termination.value === 0;
      // Cancelar não é falha da ferramenta: rótulo e cor próprios.
      const cancelled = termination.kind === 'cancelled';
      if (verbose() || !ok) {
        const how =
          termination.kind === 'exited'
            ? t('console.termination.code', { code: termination.value ?? '?' })
            : t(`console.termination.${termination.kind}` as Key);
        const label = ok ? t('console.step.done') : cancelled ? t('console.step.cancelled') : t('console.step.failed');
        write(
          channel,
          `  ${label}  ${event.step}  ${event.tool}  ${formatDuration(event.duration_ms)}${ok ? '' : `  (${how})`}`,
          ok ? 'dim' : cancelled ? 'warning' : 'error',
        );
      }
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
  const channel = failedStep ? STEP_CHANNEL[failedStep] : 'asm';
  write(
    channel,
    t('console.buildTitle', {
      name: result.processor,
      lang: result.language === 'cpp' ? 'C' : 'C±',
      mhz: result.frequency_mhz,
      clocks: result.clocks,
      status: statusText(result.status),
      time: formatDuration(result.duration_ms),
    }),
    statusStyle(result.status),
  );
  writeDiagnostics(channel, result.diagnostics);
}

function writeDiagnostics(channel: ConsoleChannel, diagnostics: Diagnostic[]): void {
  for (const d of diagnostics) {
    if (d.severity === 'info' && !verbose()) continue;
    const where = d.file ? `${rel(d.file)}${d.line ? `:${d.line}` : ''}${d.column ? `:${d.column}` : ''}: ` : '';
    const style: LineStyle = d.severity === 'error' ? 'error' : d.severity === 'warning' ? 'warning' : 'dim';
    write(channel, `  ${where}${d.severity}: ${d.message}`, style);
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

/** O resumo final de um fluxo, no console dele. */
function writeOutcome(outcome: FlowOutcome): ConsoleChannel {
  let channel = FLOW_CHANNEL[outcome.flow];
  const failedBuild = outcome.builds.find((b) => b.status !== 'succeeded');
  if (failedBuild && outcome.flow !== 'build') {
    const phase = {
      check: 'flowName.check',
      simulate: 'flowName.simulate',
      synthesize: 'flowName.synthesize',
      fpga_build: 'flowName.fpga_build',
    }[outcome.flow as 'check' | 'simulate' | 'synthesize' | 'fpga_build'];
    channel = failedBuild.failed_step ? STEP_CHANNEL[failedBuild.failed_step as Step] : 'cmm';
    if (phase) write(channel, t('console.notRun', { phase: t(phase as Key), name: failedBuild.processor }), 'warning');
  }
  if (outcome.check) {
    const c = outcome.check;
    write(
      channel,
      t('console.checkTitle', {
        targets: c.targets.join(', ') || '-',
        status: statusText(c.status),
        time: formatDuration(c.duration_ms),
      }),
      statusStyle(c.status),
    );
  }
  if (outcome.simulation) {
    const s = outcome.simulation;
    write(
      channel,
      t(s.fast ? 'console.fastSimTitle' : 'console.simTitle', {
        top: s.top,
        simulator: s.simulator === 'icarus' ? 'Icarus' : 'Verilator',
        status: statusText(s.status),
        time: formatDuration(s.duration_ms),
      }),
      statusStyle(s.status),
    );
    for (const port of outcome.outputs) {
      write(
        channel,
        t('console.output', { port: port.port, values: port.error ? port.error.message : port.values.join(' ') }),
        port.error ? 'error' : 'success',
      );
    }
    // cocotb: um teste por linha e quantos passaram; a falha vem com a linha
    // do .py, que também está nos problemas.
    if (s.tests) {
      for (const c of s.tests.cases) {
        const where = c.status === 'failed' && c.file ? ` (${rel(c.file)}${c.line ? `:${c.line}` : ''})` : '';
        const [key, style]: [Key, LineStyle] =
          c.status === 'passed'
            ? ['console.testPassed', 'success']
            : c.status === 'failed'
              ? ['console.testFailed', 'error']
              : ['console.testSkipped', 'dim'];
        write(channel, `  ${t(key, { name: c.name })}${where}`, style);
      }
      const total = s.tests.passed + s.tests.failed + s.tests.skipped;
      write(
        channel,
        t('console.testsSummary', { passed: s.tests.passed, total }),
        s.tests.failed > 0 || total === 0 ? 'error' : 'success',
      );
    }
    for (const missing of s.missing_inputs) write(channel, t('console.missingInput', { path: rel(missing) }), 'warning');
    if (s.status === 'timed_out') write(channel, t('console.timedOutHint'), 'warning');
    if (s.waveform) write(channel, t('console.waveformAt', { path: rel(s.waveform.path) }), 'dim');
  }
  if (outcome.synthesis) {
    const s = outcome.synthesis;
    write(
      channel,
      t('console.synthTitle', { top: s.top, status: statusText(s.status), time: formatDuration(s.duration_ms) }),
      statusStyle(s.status),
    );
  }
  if (outcome.schematic) {
    const s = outcome.schematic;
    write(
      channel,
      t('console.schematicTitle', { module: s.module, status: statusText(s.status), time: formatDuration(s.duration_ms) }),
      statusStyle(s.status),
    );
  }
  if (outcome.fpga) {
    const f = outcome.fpga;
    write(
      channel,
      t('console.fpgaTitle', { top: f.top, board: f.board, status: statusText(f.status), time: formatDuration(f.duration_ms) }),
      statusStyle(f.status),
    );
    for (const note of f.notes) write(channel, `  ${t('console.note', { note })}`, 'warning');
    // Os erros e avisos do Quartus já passaram ao vivo; o tempo não
    // atendido não reprova a compilação, mas precisa ser visto.
    if (f.timing && !f.timing.met) write(channel, t('console.fpgaTimingNotMet'), 'warning');
    if (f.bitstream) write(channel, t('console.fpgaBitstream', { path: rel(f.bitstream) }), 'dim');
  }
  if (outcome.fpga_program) {
    const p = outcome.fpga_program;
    write(
      channel,
      t('console.programTitle', { board: p.board, cable: p.cable, status: statusText(p.status), time: formatDuration(p.duration_ms) }),
      statusStyle(p.status),
    );
    // O aviso de .sof velho não vem das linhas ao vivo do Quartus.
    writeDiagnostics(channel, p.diagnostics.filter((d) => !d.raw));
  }
  if (outcome.wave) {
    write(channel, t('console.waveOpened', { pid: outcome.wave.pid, path: rel(outcome.wave.waveform) }), 'info');
    for (const processor of outcome.wave.outdated) {
      write(channel, `${processor}: ${t('wave.outdated')}`, 'warning');
    }
  }
  if (outcome.wave_tab) {
    write(channel, t('console.waveTab', { path: rel(outcome.wave_tab) }), 'info');
  }
  if (outcome.wave_error) {
    write(channel, t('console.waveError', { error: outcome.wave_error.message }), 'error');
  }
  if (outcome.flow === 'build' && outcome.builds.length === 0) {
    write('cmm', t('console.noProcessors'), 'info');
    channel = 'cmm';
    show('cmm');
  }
  if (outcome.learn) {
    const g = outcome.learn;
    const style: LineStyle = g.verdict === 'solved' ? 'success' : g.verdict === 'cancelled' ? 'warning' : 'error';
    write(
      channel,
      t('console.learnTitle', { exercise: g.exercise, verdict: t(`learn.short.${g.verdict}` as Key), time: formatDuration(g.duration_ms) }),
      style,
    );
    for (const o of g.outputs.filter((o) => o.mismatches > 0)) {
      write(channel, t('console.learnOutput', { output: o.name, wrong: o.mismatches, samples: g.samples, first: o.first_ns ?? '-' }), 'error');
    }
    writeDiagnostics(channel, g.diagnostics);
  }
  if (outcome.report) write(channel, t('console.report', { id: outcome.report }), 'dim');
  if (outcome.report_error) write(channel, t('console.reportError', { error: outcome.report_error }), 'warning');
  return channel;
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

/** Roda uma operação da CLI (`lace install`, `lace update`): a tela do
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
    cliLog: { flow, command: `lace ${flow}`, lines: [] },
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
          const channel = writeOutcome(outcome);
          if (!outcome.succeeded) show(channel);
          if (outcome.wave_error) showError(outcome.wave_error);
          if (outcome.schematic_error) {
            write('prism', outcome.schematic_error.message, 'warning');
            showError(outcome.schematic_error);
          }
          // A onda numa aba: abre, ou recarrega a que já está aberta, porque
          // a simulação acabou de regravar o arquivo.
          if (outcome.wave_tab) openWaveTab(outcome.wave_tab, true);
          // Depois da síntese o PRISM abre sozinho, como na AURORA, e
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
            write(START_CHANNEL[request.flow], t('console.failedToRun', { message: error.message }), 'error');
            showError(error);
          }
        }
        resolve(outcome);
      };

      const onMessage = (message: JobMessage) => {
        switch (message.type) {
          case 'started':
            set({ running: { ...get().running!, id: message.job, command: message.command } });
            write(START_CHANNEL[request.flow], `> ${message.command}`, 'command');
            break;
          case 'phase': {
            set({ running: { ...get().running!, phase: message.phase } });
            const channel = PHASE_CHANNEL[message.phase];
            write(channel, `${t(phaseKey(message.phase, key))}...`, 'title');
            show(channel);
            break;
          }
          case 'events':
            message.events.forEach(handleEvent);
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
            write(START_CHANNEL[request.flow], message.line, message.stream === 'stderr' ? 'dim' : 'plain');
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
    else write(START_CHANNEL[running.flow], t('console.cancelRequested'), 'warning');
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
    const version = report.lace.latest;
    if (report.action === 'updated') {
      const text = t('console.updateDone', { version });
      cliLine(text, 'success');
      useToasts.getState().push({ kind: 'success', title: text });
    } else if (report.action === 'wizard_opened') {
      const text = t('console.updateWizard', { version });
      cliLine(text, 'info');
      useToasts.getState().push({ kind: 'info', title: text }, 10000);
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
