import { readdir } from 'node:fs/promises';
import { basename, dirname, join } from 'node:path';

import * as vscode from 'vscode';

import { ActionBar } from './actionBar';
import { BuildResults } from './buildResults';
import { CheckResults } from './check';
import { addFiles, addSelected, newFile, offerToAdd } from './commands/add';
import { build, buildProcessor } from './commands/build';
import { check, checkFile, checkProcessor } from './commands/check';
import { dropInList, ExplorerFollower, removeFromProject, renameInProject, reorder } from './commands/files';
import { newProject } from './commands/newProject';
import { configureProcessor, newProcessor } from './commands/processor';
import { setTop } from './commands/setTop';
import { simulate, simulateProcessor, simulateTestbench } from './commands/sim';
import { openRtlViewer, synthesize, synthesizeProcessor } from './commands/synth';
import { openProcessorWave, openWave, openWaveFile } from './commands/wave';
import { AuroraConnection } from './connection';
import { syncContext } from './context';
import type { StatusReport } from './aurora/aurora-types';
import { formatAuroraError } from './messages';
import { ProjectStatus } from './projectStatus';
import { Projects } from './projects';
import { ChangePoller } from './poller';
import { RtlViewerPanel } from './rtlViewer';
import { RUN_KINDS, Runs, type RunKind } from './runs';
import { SourceDecorations } from './sources/decorations';
import { SourcesDragAndDrop } from './sources/dragAndDrop';
import { SourcesProvider } from './sources/provider';
import { AuroraStatusBar } from './statusBar';
import { registerSaphoAssembly } from './asmLanguage';
import { processorOf } from './commands/common';
import { isVerilogFile } from './verilog';

/**
 * O que `activate` devolve (o `exports` da extensão). Existe para os testes
 * de integração esperarem a leitura do projeto; não é uma API estável.
 */
export interface AuroraExtensionApi {
  /** O último `aurora status` do projeto ativo. */
  readonly status: StatusReport | undefined;
  /** Cada leitura do projeto terminada, com ou sem erro. */
  readonly onDidRefresh: vscode.Event<unknown>;
  /** Os .v e .sv que passaram a aparecer fora do projeto (o aviso de arquivo novo). */
  readonly onDidFindNewFiles: vscode.Event<string[]>;
  /** O estado das operações, como os botões da barra de status o mostram. */
  readonly runs: Runs;
  /** A aba do RTL viewer. */
  readonly viewer: RtlViewerPanel;
  /** Termina quando o que o explorador renomeou ou apagou já foi para o .spf. */
  explorerIdle(): Promise<unknown>;
  refresh(): Promise<void>;
}

/** Quanto esperar depois de gravar antes do check (aurora.check.onSave): junta um "gravar tudo". */
const CHECK_ON_SAVE_DELAY_MS = 300;

/** A conexão desta ativação, para o `deactivate` encerrar o `aurora serve`. */
let activeConnection: AuroraConnection | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<AuroraExtensionApi> {
  registerSaphoAssembly(context);
  const log = vscode.window.createOutputChannel('Lace', { log: true });
  const tools = vscode.window.createOutputChannel('Lace: ferramentas');
  const connection = new AuroraConnection(log, String(context.extension.packageJSON.version ?? '0.0.0'));
  activeConnection = connection;
  const projects = new Projects(context.workspaceState);
  const status = new ProjectStatus(connection, projects, log);
  const runs = new Runs(tools);
  const checks = new CheckResults(tools);
  const builds = new BuildResults();
  const sources = new SourcesProvider(status);
  const decorations = new SourceDecorations();
  const dragAndDrop = new SourcesDragAndDrop({
    order: (dragged, target) => dropInList(deps, dragged, target),
    add: (files, testbench) => addFiles(deps, files, testbench),
  });
  const view = vscode.window.createTreeView('aurora.sources', {
    treeDataProvider: sources,
    canSelectMany: true,
    dragAndDropController: dragAndDrop,
  });
  const statusBar = new AuroraStatusBar();
  const actions = new ActionBar();

  const sync = () => {
    const state = status.current;
    syncContext(connection.state, projects, state);
    statusBar.update(connection.state, projects);
    actions.update(connection.state, projects, runs, status.report);
    view.message = state.kind === 'error' ? formatAuroraError(state.error) : undefined;
  };
  // O fonte de um processador só tem problemas do build; o Verilog, do check.
  const redecorate = () =>
    decorations.update(sources.entries, (path) => checks.problemCount(path) ?? builds.problemCount(path));

  const viewer: RtlViewerPanel = new RtlViewerPanel(
    context.extensionUri,
    {
      resynthesize: (target) => void synthesize(deps, target),
      projectRoot: () => projects.active?.root,
    },
    log,
  );
  const deps = { connection, projects, status, log, runs, checks, builds, viewer };
  const explorer = new ExplorerFollower(deps);
  let activeSpf = projects.active?.spf;
  let saveTimer: NodeJS.Timeout | undefined;
  const command = (id: string, handler: (...args: unknown[]) => unknown) =>
    vscode.commands.registerCommand(id, handler);

  // No terminal integrado, o `aurora synth --open` não abre o navegador: só
  // grava o rtl-viewer.html (AURORA_OPENER), e a extensão, que confere esse
  // arquivo de cada alvo, abre o netlist ao lado dele numa aba. Quem confere
  // é o ChangePoller, e não o vigia do VS Code, que não vê a pasta que a
  // síntese cria (poller.ts).
  context.environmentVariableCollection.replace('AURORA_OPENER', 'vscode');
  context.environmentVariableCollection.description = 'O aurora synth --open abre o RTL viewer numa aba do VS Code';
  const synthDir = () => (projects.active ? join(projects.active.root, '.aurora8', 'Temp', 'synth') : undefined);
  const viewerPages = new ChangePoller(
    async () => {
      const dir = synthDir();
      if (!dir) {
        return [];
      }
      try {
        const targets = await readdir(dir, { withFileTypes: true });
        return targets.filter((t) => t.isDirectory()).map((t) => join(dir, t.name, 'rtl-viewer.html'));
      } catch {
        return [];
      }
    },
    (html) => {
      // A pasta da síntese tem o nome do alvo: um processador ou o topo.
      const dir = dirname(html);
      const name = basename(dir);
      const processor = status.report?.processors.some((p) => p.name === name);
      void viewer.show(join(dir, 'hierarchy.json'), processor ? { kind: 'processor', name } : { kind: 'project' });
    },
  );
  const openNetlist = new ChangePoller(
    async () => (viewer.current ? [viewer.current] : []),
    () => void viewer.reload(),
  );
  viewerPages.start();
  openNetlist.start();

  context.subscriptions.push(
    viewerPages,
    openNetlist,
    log,
    tools,
    connection,
    projects,
    status,
    runs,
    checks,
    builds,
    explorer,
    viewer,
    sources,
    decorations,
    view,
    statusBar,
    actions,
    connection.onDidChange(sync),
    projects.onDidChange(() => {
      sync();
      // Os problemas e os resultados eram do outro projeto.
      if (projects.active?.spf !== activeSpf) {
        activeSpf = projects.active?.spf;
        checks.clear();
        builds.clear();
        // Os rtl-viewer.html que já estavam no outro projeto não são novos.
        viewerPages.reset();
        RUN_KINDS.forEach((kind) => runs.forget(kind));
      }
    }),
    status.onDidChange(() => {
      sync();
      redecorate();
    }),
    runs.onDidChange(() => {
      actions.update(connection.state, projects, runs, status.report);
      viewer.setResynthesizing(runs.isRunning('synth'));
      const running = RUN_KINDS.some((kind) => runs.isRunning(kind));
      void vscode.commands.executeCommand('setContext', 'aurora.running', running);
    }),
    checks.onDidChange(redecorate),
    builds.onDidChange(redecorate),
    status.onDidFindNewFiles((paths) => void offerToAdd(deps, paths)),
    vscode.workspace.onDidSaveTextDocument((document) => {
      const active = projects.active;
      const onSave = vscode.workspace.getConfiguration('aurora.check').get<boolean>('onSave', false);
      if (!onSave || !active || document.uri.scheme !== 'file' || !isVerilogFile(basename(document.uri.fsPath))) {
        return;
      }
      if (projects.owner(document.uri.fsPath)?.spf !== active.spf) {
        return;
      }
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => void check(deps, { kind: 'project' }, { interactive: false }), CHECK_ON_SAVE_DELAY_MS);
    }),
    { dispose: () => clearTimeout(saveTimer) },
    command('aurora.setTop', (target) => setTop(deps, target)),
    command('aurora.build', () => build(deps, { kind: 'all' })),
    command('aurora.buildProcessor', (target) => buildProcessor(deps, target)),
    command('aurora.newProcessor', (args) => newProcessor(deps, args)),
    command('aurora.configureProcessor', (target, args) => configureProcessor(deps, target, args)),
    command('aurora.remove', (first, all) => removeFromProject(deps, first, all)),
    command('aurora.moveUp', (target) => reorder(deps, target, 'up')),
    command('aurora.moveDown', (target) => reorder(deps, target, 'down')),
    command('aurora.moveFirst', (target) => reorder(deps, target, 'first')),
    command('aurora.moveLast', (target) => reorder(deps, target, 'last')),
    command('aurora.rename', (target, args) => renameInProject(deps, target, args)),
    command('aurora.check', () => check(deps, { kind: 'project' })),
    command('aurora.checkLint', () => check(deps, { kind: 'project' }, { lint: true })),
    command('aurora.checkFile', (target) => checkFile(deps, target)),
    command('aurora.checkProcessor', (target) => checkProcessor(deps, target)),
    command('aurora.sim', () => simulate(deps, { kind: 'project' })),
    command('aurora.simFast', () => simulate(deps, { kind: 'project' }, { fast: true })),
    command('aurora.simTestbench', (target) => simulateTestbench(deps, target)),
    command('aurora.simProcessor', (target) => simulateProcessor(deps, target)),
    command('aurora.synth', () => synthesize(deps, { kind: 'project' })),
    command('aurora.synthProcessor', (target) => synthesizeProcessor(deps, target)),
    command('aurora.rtlViewer', (target) => {
      const name = processorOf(target);
      return openRtlViewer(deps, name === undefined ? { kind: 'project' } : { kind: 'processor', name });
    }),
    command('aurora.wave', () => openWave(deps, { kind: 'project' })),
    command('aurora.waveProcessor', (target) => openProcessorWave(deps, target)),
    command('aurora.waveFile', (target) => openWaveFile(deps, target)),
    command('aurora.cancel', () => runs.cancelAll()),
    command('aurora.cancelRun', (kind) => runs.cancel(kind as RunKind)),
    command('aurora.add', (first, all) => addSelected(deps, first, all)),
    command('aurora.addTestbench', (first, all) => addSelected(deps, first, all, true)),
    command('aurora.newFile', (target) => newFile(deps, target)),
    command('aurora.newProject', (target) => newProject(deps, target)),
    command('aurora.refresh', () => status.refresh()),
    command('aurora.selectProject', () => projects.select()),
    command('aurora.showTools', () => tools.show()),
  );
  sync();
  await Promise.all([connection.connect(), projects.discover()]);
  activeSpf = projects.active?.spf;

  return {
    get status() {
      return status.report;
    },
    onDidRefresh: status.onDidChange,
    onDidFindNewFiles: status.onDidFindNewFiles,
    runs,
    viewer,
    explorerIdle: () => explorer.idle(),
    refresh: () => status.refresh(),
  };
}

/**
 * Encerra o `aurora serve`, se a conexão o abriu. Roda depois dos `dispose`
 * de `context.subscriptions`, então o cancelamento das operações (`Runs`)
 * já chegou ao servidor antes do `shutdown`.
 */
export function deactivate(): Promise<void> | undefined {
  const connection = activeConnection;
  activeConnection = undefined;
  return connection?.shutdown();
}
